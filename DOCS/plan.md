# Plan: VolumeWidget Subprocess Optimization

**Status:** Completed
**Date:** 2026-03-18
**Standards consulted:** RULE_OF_LAW.md, CODING_STANDARDS.md, WIDGET_API.md 1.7.0,
ARCHITECTURE.md, TESTING_GUIDE.md, PLATFORM_COMPAT.md

---

## Overview

`VolumeWidget` currently spawns **two** `pactl` subprocesses per poll cycle — one for
volume and one for mute state — creating 2× fork/exec + IPC overhead on every tick.

This plan delivers the optimization in two self-contained parts:

**Part 1 — Single subprocess (immediate):**
Replace `pactl get-sink-volume` + `pactl get-sink-mute` with a single
`pactl get-sink-info @DEFAULT_SINK@` call that returns both values in one multi-line
block. Zero architecture change; pure implementation improvement.

**Part 2 — Event-driven subscription (follow-on, in the same plan):**
Spawn `pactl subscribe` once as a persistent background thread. Push `VolumeData`
through an `mpsc` channel. `Widget::update()` drains the channel and returns cached
data when no events have arrived. Eliminates all per-poll subprocess spawning after
initial startup.

Both parts are confined entirely to `frames_core/src/widgets/volume.rs`. No GTK,
GDK, or display imports are involved. `frames_core`'s headless-test guarantee is
fully preserved.

**Design authority:** `DOCS/research/volume-event-driven.md` and the design note
produced from it (2026-03-18). Consult those documents for full rationale and
rejected alternatives.

---

## Affected Crates & Modules

| File | Change |
|------|--------|
| `crates/frames_core/src/widgets/volume.rs` | Primary: all Part 1 + Part 2 implementation |
| `standards/WIDGET_API.md` | §7.2 doc clarification + new §7.4; patch version bump |
| `DOCS/futures.md` | Mark `VolumeWidget double-pactl` entry as completed |

**No new modules added to `ARCHITECTURE.md §4.1`** — `volume.rs` already exists.
**No new workspace dependencies required.** The implementation uses only
`std::sync::mpsc`, `std::sync::Mutex`, `std::thread`, and `std::process` — all in
`std`.

---

## New Types & Signatures

### Part 1 — Rename + rewrite `read_volume()`

`fn read_volume() -> Option<(f32, bool)>` is replaced by:

```rust
/// Query `pactl get-sink-info` for the default sink and extract volume + mute.
///
/// Returns `(volume_pct, muted)` on success, `None` when `pactl` is absent,
/// exits non-zero, or produces unrecognisable output.
fn read_volume_info() -> Option<(f32, bool)>
```

Two private parse helpers are extracted so they can be unit-tested independently:

```rust
/// Extract volume percentage from `pactl get-sink-info` output.
///
/// Looks for the first `Volume:` line and parses the `/ NN% /` token.
/// Returns `None` if the line is absent or the percentage is not a valid float.
fn parse_volume_pct(text: &str) -> Option<f32>

/// Determine mute state from `pactl get-sink-info` output.
///
/// Returns `true` if a `Mute: yes` line is found (case-insensitive).
/// Returns `false` when the line is absent (conservative default matches
/// the existing `get-sink-mute` fallback behaviour).
fn parse_mute(text: &str) -> bool
```

Neither is `pub`. No public API change. `update()` calling convention is unchanged.

### Part 2 — Stateful `VolumeWidget`

Private helper struct (not exported, not re-exported from `lib.rs`):

```rust
/// Volume and mute snapshot sent from the background subscribe thread.
///
/// Implements `Copy` so it can be cheaply cloned through the channel.
#[derive(Debug, Clone, Copy)]
struct VolumeData {
    volume_pct: f32,
    muted:      bool,
}
```

Private background thread driver:

```rust
/// Drive the `pactl subscribe` subprocess and forward sink-change events.
///
/// Spawns `pactl subscribe`, reads stdout line by line, and sends a
/// [`VolumeData`] snapshot through `tx` whenever a sink change event is
/// detected. Returns when `pactl` exits or when `tx.send()` fails because
/// the receiver has been dropped.
///
/// # Errors
///
/// Does not return `Result`. All failures are logged via `tracing::warn!`
/// and cause an early return (which drops `tx` and signals the receiver).
fn subscribe_loop(tx: std::sync::mpsc::Sender<VolumeData>)
```

**`VolumeWidget` struct fields (Part 2 additions):**

| Field | Type | Role |
|-------|------|------|
| `cached` | `WidgetData` | Last confirmed volume state (replaces `last_data: Option<WidgetData>`) |
| `rx` | `std::sync::Mutex<std::sync::mpsc::Receiver<VolumeData>>` | Receives events from background thread; wrapped in `Mutex` to satisfy `Sync` |
| `_thread` | `std::thread::JoinHandle<()>` | Holds the thread handle; never joined explicitly (thread exits when `tx` is dropped or `pactl` exits) |

**`Send + Sync` analysis (WIDGET_API §3 requirement):**

| `VolumeWidget` field after Part 2 | `Send`? | `Sync`? |
|------------------------------------|---------|---------|
| `name: String` | ✅ | ✅ |
| `cached: WidgetData` | ✅ | ✅ |
| `rx: Mutex<Receiver<VolumeData>>` | ✅ (`Receiver<T>: Send where T: Send`) | ✅ (`Mutex<T>: Sync where T: Send`) |
| `_thread: JoinHandle<()>` | ✅ | ✅ |

The existing test `volume_widget_satisfies_send_sync` enforces this at compile time.

**`WIDGET_API_VERSION` bump:** `1.7.0` → `1.7.1` (patch — documentation-only
change to §7.2 and new §7.4; no `WidgetData` or trait signature change per
WIDGET_API §2 versioning table).

---

## Implementation Steps

> Verify `cargo build --workspace` exits 0 after each numbered step before
> proceeding. **Do not batch steps.** Each step must be independently buildable.

---

### Part 1 — Single `pactl get-sink-info` call

#### Step 1 — Extract parse helpers and rewrite `read_volume`

**File:** `crates/frames_core/src/widgets/volume.rs`

1. Add private `parse_volume_pct(text: &str) -> Option<f32>`:

   ```rust
   // "Volume: front-left: 45875 /  70% / -8.66 dB   front-right: ..."
   // Split on '/', take the second token, strip whitespace and '%'.
   fn parse_volume_pct(text: &str) -> Option<f32> {
       text.lines()
           .find(|l| l.trim_start().starts_with("Volume:"))
           .and_then(|l| l.split('/').nth(1))
           .map(|s| s.trim().trim_end_matches('%'))
           .and_then(|s| s.parse::<f32>().ok())
   }
   ```

2. Add private `parse_mute(text: &str) -> bool`:

   ```rust
   // "Mute: yes" or "Mute: no" — conservative false default when absent.
   fn parse_mute(text: &str) -> bool {
       text.lines()
           .find(|l| l.trim_start().starts_with("Mute:"))
           .map(|l| l.to_lowercase().contains("yes"))
           .unwrap_or(false)
   }
   ```

3. Replace `fn read_volume() -> Option<(f32, bool)>` with:

   ```rust
   fn read_volume_info() -> Option<(f32, bool)> {
       let out = Command::new("pactl")
           .args(["get-sink-info", "@DEFAULT_SINK@"])
           .output()
           .ok()?;
       // pactl exits non-zero when no sink is available; treat as absent.
       if !out.status.success() && out.stdout.is_empty() {
           return None;
       }
       let text = String::from_utf8_lossy(&out.stdout);
       let vol = parse_volume_pct(&text)?;
       let muted = parse_mute(&text);
       Some((vol, muted))
   }
   ```

4. Update `update()` to call `Self::read_volume_info()` (rename only — the
   logic is identical).

   Remove the `Command::new("pactl").args(["get-sink-volume", ...])` and
   `Command::new("pactl").args(["get-sink-mute", ...])` call sites entirely.
   Remove the `use std::process::Command` import if it was the only remaining
   use (it is still used; keep it).

Verify: `cargo build --workspace` exits 0.

---

#### Step 2 — Unit tests for parse helpers

**File:** `crates/frames_core/src/widgets/volume.rs`, `#[cfg(test)]` block

Add tests that exercise the parse helpers directly against representative fixture
text, without requiring `pactl` at all:

```rust
const SINK_INFO_FIXTURE: &str = "\
Sink #0
\tState: RUNNING
\tName: alsa_output.pci-0000_00_1f.3.analog-stereo
\tVolume: front-left: 45875 /  70% / -8.66 dB   \
         front-right: 45875 /  70% / -8.66 dB
\tBalance: 0.00
\tMute: no
";

const SINK_INFO_MUTED: &str = "\
Sink #0
\tVolume: front-left: 0 /   0% / -inf dB
\tMute: yes
";

const SINK_INFO_NO_VOLUME_LINE: &str = "\
Sink #0
\tMute: no
";

#[test]
fn parse_volume_pct_extracts_from_fixture() {
    assert_eq!(parse_volume_pct(SINK_INFO_FIXTURE), Some(70.0));
}

#[test]
fn parse_volume_pct_returns_zero_when_muted() {
    assert_eq!(parse_volume_pct(SINK_INFO_MUTED), Some(0.0));
}

#[test]
fn parse_volume_pct_none_when_volume_line_absent() {
    assert_eq!(parse_volume_pct(SINK_INFO_NO_VOLUME_LINE), None);
}

#[test]
fn parse_mute_false_on_unmuted_sink() {
    assert!(!parse_mute(SINK_INFO_FIXTURE));
}

#[test]
fn parse_mute_true_on_muted_sink() {
    assert!(parse_mute(SINK_INFO_MUTED));
}

#[test]
fn parse_mute_false_when_line_absent() {
    // Conservative default: treat absent Mute line as unmuted.
    assert!(!parse_mute(SINK_INFO_NO_VOLUME_LINE));
}
```

Also update the comment on the existing `volume_update_does_not_error` test to
clarify that it still covers the `pactl`-absent CI case:

```rust
#[test]
fn volume_update_does_not_error() {
    // pactl may be absent in CI; read_volume_info() returns None,
    // update() falls back to last_data / default, returns Ok(…).
    let mut w = VolumeWidget::new("vol");
    assert!(w.update().is_ok());
}
```

Verify: `cargo test -p frames_core --no-default-features` exits 0.

---

### Part 2 — `pactl subscribe` background thread

> **Prerequisite:** Step 2 must pass before beginning Part 2.
> Part 2 restructures `VolumeWidget` significantly; having the Part 1
> parse tests green provides a stable regression baseline.

#### Step 3 — Add `VolumeData` struct and `subscribe_loop`

**File:** `crates/frames_core/src/widgets/volume.rs`

Add at the top of the module, below the `use` block:

```rust
/// Volume and mute snapshot transmitted from the subscribe background thread.
#[derive(Debug, Clone, Copy)]
struct VolumeData {
    volume_pct: f32,
    muted:      bool,
}
```

Add the background thread driver below `read_volume_info()`:

```rust
/// Drive `pactl subscribe` and forward default-sink change events via `tx`.
///
/// Spawns `pactl subscribe --format=json` (falling back to plain text), reads
/// stdout line by line, and sends a [`VolumeData`] snapshot through `tx` when
/// a sink `change` event is detected. Returns (dropping `tx`) when `pactl`
/// exits, when `tx.send()` fails because the receiver was dropped, or when
/// the spawn itself fails.
///
/// This function is intended to be called from `std::thread::spawn`. It
/// never panics.
fn subscribe_loop(tx: std::sync::mpsc::Sender<VolumeData>) {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut child = match Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "pactl subscribe failed to start; \
                           volume widget will not receive live updates");
            return; // Drops tx → Receiver sees Disconnected on next try_recv
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        // SAFETY: stdout is always Some because Stdio::piped() was set above.
        None => unreachable!("pactl subscribe stdout not captured"),
    };

    let reader = BufReader::new(stdout);

    for line in reader.lines().map_while(Result::ok) {
        // pactl subscribe plain-text output: "Event 'change' on sink #N"
        if line.contains("change") && line.contains("sink") {
            if let Some((volume_pct, muted)) = read_volume_info() {
                let data = VolumeData { volume_pct, muted };
                if tx.send(data).is_err() {
                    // Receiver dropped; widget was torn down — exit cleanly.
                    break;
                }
            }
        }
    }
    // pactl exited or read error. Dropping `tx` signals Receiver.
    // The widget will continue returning its last cached value.
    tracing::debug!("pactl subscribe loop exited");
}
```

Verify: `cargo build -p frames_core` exits 0.

---

#### Step 4 — Restructure `VolumeWidget`

**File:** `crates/frames_core/src/widgets/volume.rs`

Replace the current struct definition:

```rust
// Current (Part 1):
pub struct VolumeWidget {
    name:      String,
    last_data: Option<WidgetData>,
}
```

with:

```rust
/// Provides audio output volume and mute state.
///
/// Spawns a background thread running `pactl subscribe` on construction.
/// `update()` drains an `mpsc` channel fed by that thread and returns the
/// latest data, or the last cached value when no events have arrived.
///
/// The background thread exits automatically when the widget is dropped
/// (the `Sender` is dropped, making the next `try_recv` return
/// `Disconnected`). Widget teardown does not join the thread — it exits at
/// the next `pactl subscribe` event or when `pactl` itself exits.
pub struct VolumeWidget {
    name:    String,
    cached:  WidgetData,
    rx:      std::sync::Mutex<std::sync::mpsc::Receiver<VolumeData>>,
    _thread: std::thread::JoinHandle<()>,
}
```

Verify: `cargo build -p frames_core` exits 0 (expected: constructor and update()
do not compile yet — compile errors are expected here and fixed in Steps 5–6).

---

#### Step 5 — Update `VolumeWidget::new()`

**File:** `crates/frames_core/src/widgets/volume.rs`

Replace the `new()` body:

```rust
impl VolumeWidget {
    /// Create a new `VolumeWidget` targeting the default audio sink.
    ///
    /// Spawns a background thread that runs `pactl subscribe` and forwards
    /// sink-change events through an internal channel. An initial
    /// `pactl get-sink-info` call populates the cached value so the first
    /// `update()` returns real data without waiting for a change event.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<VolumeData>();

        // Warm the cache before spawning so update() has real data immediately.
        let initial = read_volume_info()
            .map(|(volume_pct, muted)| WidgetData::Volume { volume_pct, muted })
            .unwrap_or(WidgetData::Volume { volume_pct: 0.0, muted: false });

        let thread_handle =
            std::thread::Builder::new()
                .name("frames-volume-subscribe".into())
                .spawn(move || subscribe_loop(tx))
                // Spawning a thread can only fail if the OS has hit its thread limit;
                // treat this the same as pactl being absent — the widget will return
                // the initial cached value permanently but will not crash.
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "could not spawn volume subscribe thread");
                    // We must return a JoinHandle. Use a no-op thread instead.
                    std::thread::spawn(|| {})
                });

        Self {
            name: name.into(),
            cached: initial,
            rx: std::sync::Mutex::new(rx),
            _thread: thread_handle,
        }
    }
}
```

Verify: `cargo build -p frames_core` exits 0.

---

#### Step 6 — Rewrite `Widget::update()`

**File:** `crates/frames_core/src/widgets/volume.rs`

Replace the `impl Widget for VolumeWidget` block:

```rust
impl Widget for VolumeWidget {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) -> Result<WidgetData, FramesError> {
        let rx = self.rx.lock().expect(
            "volume subscribe rx mutex poisoned; this indicates a bug in subscribe_loop"
        );

        // Drain all events that arrived since the last poll; keep only the latest.
        use std::sync::mpsc::TryRecvError;
        loop {
            match rx.try_recv() {
                Ok(data) => {
                    self.cached =
                        WidgetData::Volume { volume_pct: data.volume_pct, muted: data.muted };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Background thread exited (pactl absent, OS restart, or
                    // thread spawn failure). Return stale cached value per
                    // WIDGET_API §7.2 — this is not an error condition.
                    tracing::warn!(
                        widget = self.name,
                        "volume subscribe thread disconnected; \
                         returning last cached value"
                    );
                    break;
                }
            }
        }
        // Drop the lock before cloning cached (minimise lock hold time).
        drop(rx);
        Ok(self.cached.clone())
    }
}
```

Verify: `cargo build --workspace` exits 0.
Verify: `cargo clippy --workspace -- -D warnings` exits 0.

---

#### Step 7 — Update unit tests

**File:** `crates/frames_core/src/widgets/volume.rs`, `#[cfg(test)]` block

The existing three tests remain valid. Add one new test and update one:

**Add:**
```rust
#[test]
fn volume_widget_update_returns_ok_when_subscribe_absent() {
    // When pactl is absent, the subscribe thread exits immediately, the
    // channel disconnects, and update() must return Ok(…) with a safe default
    // rather than Err(…). This is the CI-safe baseline.
    let mut w = VolumeWidget::new("vol-absent");
    // Allow the thread time to attempt spawn and exit.
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert!(w.update().is_ok());
}
```

**Update comment on:** `volume_widget_satisfies_send_sync`:
```rust
#[test]
fn volume_widget_satisfies_send_sync() {
    // Verifies that the Mutex<Receiver<…>> + JoinHandle fields preserve the
    // Send + Sync bound required by the Widget trait (WIDGET_API §3).
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VolumeWidget>();
}
```

Verify: `cargo test -p frames_core --no-default-features` exits 0.
Verify: `cargo test --workspace` exits 0.

---

#### Step 8 — Update `WIDGET_API.md`

**File:** `standards/WIDGET_API.md`

**8a — Patch §7.2** (stale-data policy):

Append to the end of §7.2 section text:

> Returning the last cached `WidgetData` is also correct when no new data has
> arrived since the previous `update()` call (e.g., a channel-driven widget
> whose `mpsc` channel has no pending messages). An empty channel is not a
> failure condition. `Err` should only be returned for persistent, unrecoverable
> failures — not for "nothing changed since last tick."

**8b — Add §7.4** immediately after §7.3:

```markdown
### 7.4 Stateful Widgets with Background Threads

A widget may own a background thread and an `mpsc::Receiver` when event-driven
updates are preferable to polling (e.g., audio volume via `pactl subscribe`).

**Required pattern:**

```rust
pub struct ExampleWidget {
    cached:  WidgetData,
    rx:      std::sync::Mutex<std::sync::mpsc::Receiver<SomeData>>,
    _thread: std::thread::JoinHandle<()>,
}
```

**`Send + Sync` obligation:** `std::sync::mpsc::Receiver<T>` is `Send` but
`!Sync`. Wrap it in `std::sync::Mutex<Receiver<T>>` to satisfy the `Sync`
bound required by `Widget: Send + Sync`. `Mutex<T>: Sync where T: Send`.
`JoinHandle<T>` is `Send + Sync`.

**`update()` contract for channel-driven widgets:**
1. Acquire the lock on the `Receiver`: `self.rx.lock().expect("… poisoned")`.
2. Drain the channel in a loop using `try_recv()`.
3. On `TryRecvError::Empty` — break; return cached value. This is normal.
4. On `TryRecvError::Disconnected` — log `tracing::warn!`; break; return
   cached value. The thread has exited (external process absent, OS restart,
   etc.). Do not return `Err` — this is a degraded but recoverable state.
5. Drop the lock before cloning or returning the cached value.

**Thread naming:** Use `std::thread::Builder::new().name("frames-<purpose>")` so
threads appear with meaningful names in debuggers and OS process listings.

**Thread lifecycle:** The background thread exits when `tx.send()` fails because
the widget was dropped (receiver gone) or when its external subprocess exits.
Storing `_thread: JoinHandle` (not calling `join()` in `Drop`) is correct for
bar-lifetime widgets — the OS reclaims the thread when the process exits.
Implement `Drop` with `join()` only if the widget can outlive the bar or if the
thread holds OS resources that must be released before process exit.
```

**8c — Bump version and add changelog entry** at the top of §2:

Change:

```rust
pub const WIDGET_API_VERSION: &str = "1.7.0";
```

to:

```rust
pub const WIDGET_API_VERSION: &str = "1.7.1";
```

Add to the `## Changelog` section:

```markdown
### 1.7.1 (2026-03-18)
- Clarified §7.2 stale-data policy: returning cached data when an `mpsc`
  channel has no pending messages is valid and expected, not an error.
- Added §7.4 "Stateful Widgets with Background Threads" — documents the
  `Mutex<Receiver<T>>` pattern for `Send + Sync` compliance, `update()`
  channel-drain contract, thread naming conventions, and lifecycle policy.
- Patch bump (documentation only; no `WidgetData` or trait signature change).
```

Verify: `cargo build --workspace` exits 0 (WIDGET_API.md is a docs file — build
is unaffected, but run it to confirm no accidental Rust edits).

---

#### Step 9 — Update `DOCS/futures.md`

**File:** `DOCS/futures.md`

Mark the entry in "Technical Debt & Refactoring" as completed:

Locate:
```
- **(2026-03-17)** `VolumeWidget` spawns two `pactl` subprocesses per poll cycle …
```

Strike-through and add completion note (following the same style as existing
completed entries):

```markdown
- **(2026-03-17)** `VolumeWidget` spawns two `pactl` subprocesses per poll cycle —
  ~~one for volume and one for mute state. This works but is heavier than necessary.
  Future improvement: subscribe to `pactl subscribe` events via a background thread
  and push updates through a channel, eliminating all periodic subprocess spawning.~~
  **Completed — `pactl get-sink-info` replaces the two-call polling path (Part 1);
  `pactl subscribe` background thread + `Mutex<Receiver<VolumeData>>` channel
  eliminates per-poll spawning entirely (Part 2). VolumeWidget is now event-driven.**
```

Move the entry to the "Completed / Integrated" section with the range
`(2026-03-17 → 2026-03-18)`.

---

## Widget API Impact

| Category | Impact |
|----------|--------|
| `WidgetData` variants | None — `WidgetData::Volume { volume_pct, muted }` unchanged |
| `Widget` trait signature | None — `update()`, `name()` signatures unchanged |
| `Widget` trait documented contract | Extended in §7.2, §7.4 |
| `WIDGET_API_VERSION` bump | `1.7.0 → 1.7.1` (patch — docs only) |
| `WIDGET_API.md` update | Required in same commit as Step 8 |

The change is **non-breaking**. No downstream match arms need updating. No
version gate is required for renderers in `frames_bar`.

---

## Error Handling Plan

All changes are in `frames_core`. Per CODING_STANDARDS §3.2, typed errors apply.

| Scenario | Handling |
|----------|----------|
| `pactl get-sink-info` absent (CI, no audio daemon) | `read_volume_info()` returns `None`; `update()` returns stale `cached` via `Ok(…)` — identical to current fallback |
| `pactl subscribe` spawn fails (OS thread limit) | `subscribe_loop` logs warn, returns, drops `tx`; `update()` detects `Disconnected`, logs warn, returns stale `cached` |
| `pactl subscribe` exits mid-session (PipeWire restart) | Same as spawn failure — `Disconnected` path, stale cached return |
| `Mutex` poisoned | `.expect("…")` with invariant message — the only permitted `.expect()` case per CODING_STANDARDS §3.3 |
| Empty `mpsc` channel (no events since last tick) | `TryRecvError::Empty` → normal path; return cached; do **not** log |

No new `FramesError` variants are needed. No `.unwrap()` in production code paths.

---

## Test Plan

Per TESTING_GUIDE §3 and §5:

| Test | Type | File | Headless? |
|------|------|------|-----------|
| `parse_volume_pct_extracts_from_fixture` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `parse_volume_pct_returns_zero_when_muted` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `parse_volume_pct_none_when_volume_line_absent` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `parse_mute_false_on_unmuted_sink` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `parse_mute_true_on_muted_sink` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `parse_mute_false_when_line_absent` | Unit | `volume.rs` `#[cfg(test)]` | ✅ |
| `volume_widget_name_non_empty` | Unit | `volume.rs` `#[cfg(test)]` | ✅ (existing) |
| `volume_widget_satisfies_send_sync` | Unit | `volume.rs` `#[cfg(test)]` | ✅ (existing, updated comment) |
| `volume_update_does_not_error` | Unit | `volume.rs` `#[cfg(test)]` | ✅ (existing, updated comment) |
| `volume_widget_update_returns_ok_when_subscribe_absent` | Unit | `volume.rs` `#[cfg(test)]` | ✅ (new) |

All tests run with `cargo test -p frames_core --no-default-features`. No GTK init.
No `DISPLAY` required. No pactl required — tests use fixture strings or rely on the
widget's `None`-return fallback path.

The `20ms` sleep in `volume_widget_update_returns_ok_when_subscribe_absent` is
an intentional timing allow for thread spawn + pactl absence determination. This
is acceptable because `frames_core` never imports GTK and `std::thread::sleep` is
display-system-agnostic.

---

## Documentation Updates Required

Per RULE_OF_LAW §4.2:

| Code change | Standard to update | Step |
|-------------|-------------------|------|
| `VolumeWidget` now event-driven | `WIDGET_API.md §7.2`, new `§7.4` | Step 8 |
| `WIDGET_API_VERSION` bumped | `WIDGET_API.md` changelog | Step 8c |
| `volume.rs` module description updated | `ARCHITECTURE.md §4.1` description for `volume.rs` | Step 9 (inline update) |
| futures.md debt entry resolved | `DOCS/futures.md` | Step 9 |

`ARCHITECTURE.md §4.1` currently says `volume.rs — Audio output volume and mute
state via pactl`. Update to: `volume.rs — Audio output volume and mute state;
event-driven via pactl subscribe background thread`.

---

## futures.md Impact

**Closes:** `(2026-03-17) VolumeWidget spawns two pactl subprocesses per poll cycle`

**New debt created:** None. The `subscribe` thread exits on pactl restart and does not reconnect. This is a known limitation. File in `futures.md` only if a reconnect mechanism is discovered to be needed based on real usage — do not pre-file speculative debt.

---

## Risks & Open Questions

| Risk | Severity | Mitigation |
|------|----------|------------|
| `pactl subscribe` exits on PipeWire restart (suspend/resume on Fedora primary target) | Low | Widget returns last cached value with warn log; bar continues rendering last known volume. Volume display is stale until bar restart. Reconnect logic is explicitly punted per design note. |
| Default sink changes (user switches audio device in Cinnamon applet) | Low | `read_volume_info()` always queries `@DEFAULT_SINK@`; will read the new default on the next subscribe event that fires. Empirical verification needed on first real-device test. |
| `pactl get-sink-info` output format differences between PulseAudio and PipeWire | Low | `Volume:` and `Mute:` lines are stable across PulseAudio ≥ 9 and PipeWire + pipewire-pulse. Fixture tests hard-code expected format; will catch regressions at test time. |
| Thread spawn failure (OS thread limit) | Very low | Handled gracefully in Step 5 via `unwrap_or_else` that spawns a no-op thread. Widget degrades to static display. |
| `20ms` sleep in `volume_widget_update_returns_ok_when_subscribe_absent` flaky on overloaded CI | Low | The thread only needs to *attempt* to spawn pactl. If the thread hasn't started yet, `try_recv()` returns `Empty` (not `Disconnected`), which is also a valid path that returns `Ok(cached)`. The test passes either way. The sleep is a quality hint, not a correctness requirement. |

---

## Testing Checklist

- [ ] `cargo build --workspace` exits 0
- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo fmt --all -- --check` exits 0
- [ ] `cargo test -p frames_core --no-default-features` exits 0
- [ ] `cargo test --workspace` exits 0

---

## Completion Checklist

- [ ] Part 1: `parse_volume_pct` and `parse_mute` helpers extracted (Step 1)
- [ ] Part 1: `read_volume_info()` calls `get-sink-info` single subprocess (Step 1)
- [ ] Part 1: fix-up tests + fixture data (Step 2)
- [ ] Part 2: `VolumeData` struct and `subscribe_loop` added (Step 3)
- [ ] Part 2: `VolumeWidget` struct restructured with `cached`, `rx`, `_thread` (Step 4)
- [ ] Part 2: `new()` spawns thread, populates cache (Step 5)
- [ ] Part 2: `update()` drains channel, handles `Disconnected` (Step 6)
- [ ] Part 2: tests updated and new `_subscribe_absent` test added (Step 7)
- [ ] `WIDGET_API.md §7.2` extended + `§7.4` added + version `1.7.1` (Step 8)
- [ ] `ARCHITECTURE.md §4.1` `volume.rs` description updated (Step 9)
- [ ] `DOCS/futures.md` entry completed (Step 9)
