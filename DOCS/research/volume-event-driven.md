# Research: VolumeWidget Double-pactl Poll
**Date:** 2026-03-18
**Status:** Findings complete — decision recommended — ready to close

## Question

How should `VolumeWidget` be changed to avoid spawning two separate `pactl`
subprocesses per poll cycle? What is the right long-term architecture for
zero-poll, event-driven volume updates?

---

## Summary

**Immediate fix:** Replace two separate `pactl get-sink-volume` + `pactl
get-sink-mute` calls with a single `pactl get-sink-info @DEFAULT_SINK@` call
that returns both values in one multi-line block. This halves subprocess
overhead with zero architecture change. **Long-term:** Spawn `pactl subscribe`
once in a background thread, push updates through `std::sync::mpsc`, and have
`Widget::update()` drain the channel — making the poll interval irrelevant for
most refreshes.

---

## Current State

`crates/frames_core/src/widgets/volume.rs` — `read_volume()`:

```rust
// call 1 — volume percentage
let vol_out = Command::new("pactl")
    .args(["get-sink-volume", "@DEFAULT_SINK@"])
    .output()?;
// parse "/  70% /"

// call 2 — mute state
let mute_out = Command::new("pactl")
    .args(["get-sink-mute", "@DEFAULT_SINK@"])
    .output()?;
// parse "Mute: yes"
```

Each poll cycle: **2 × fork/exec + 2 × IPC round-trips to PulseAudio/PipeWire**.

---

## Findings

### Option A — Single `pactl get-sink-info` call (recommended for now)

`pactl get-sink-info @DEFAULT_SINK@` returns a multi-line block that contains
both volume and mute state:

```
Sink #0
        State: RUNNING
        Name: alsa_output.pci-0000_00_1f.3.analog-stereo
        ...
        Volume: front-left: 45875 /  70% / -8.66 dB, ...
        ...
        Mute: yes
        ...
```

Parse the same fields from a single output block:

```rust
fn read_volume() -> Result<VolumeData, FramesError> {
    let output = Command::new("pactl")
        .args(["get-sink-info", "@DEFAULT_SINK@"])
        .output()
        .map_err(|e| FramesError::Io(e))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let volume_pct = parse_volume_pct(&text)?;
    let muted = parse_mute(&text)?;
    Ok(VolumeData { volume_pct, muted })
}

fn parse_volume_pct(text: &str) -> Result<u8, FramesError> {
    // "Volume: front-left: 45875 /  70% / …"
    text.lines()
        .find(|l| l.trim_start().starts_with("Volume:"))
        .and_then(|l| {
            l.split('/').nth(1)
             .map(|s| s.trim().trim_end_matches('%'))
        })
        .and_then(|s| s.parse::<u8>().ok())
        .ok_or_else(|| FramesError::Parse("volume percentage".into()))
}

fn parse_mute(text: &str) -> Result<bool, FramesError> {
    // "Mute: yes"
    text.lines()
        .find(|l| l.trim_start().starts_with("Mute:"))
        .map(|l| l.contains("yes"))
        .ok_or_else(|| FramesError::Parse("mute state".into()))
}
```

**Pros:**
- One subprocess per cycle. Same polling model, no arch change.
- `get-sink-info` is stable; available in both PulseAudio and PipeWire's PA
  compatibility layer.
- The output format is slightly more verbose but both fields are reliably present.

**Cons:**
- Still polls. Still spawns a subprocess every N ms.
- Parsing is slightly more involved (multi-line rather than single-line output).
- `get-sink-info` is slightly more expensive than `get-sink-volume` alone, but
  the difference is sub-millisecond — the fork/exec dominates.

**Verdict: Ship this now.** It is strictly better than the current code with
minimal risk and zero dependency changes.

---

### Option B — `pactl subscribe` background thread + mpsc channel (event-driven)

`pactl subscribe` streams one line per event to stdout and runs indefinitely:

```
Event 'change' on sink #0
Event 'change' on sink #0
Event 'new' on sink-input #12
```

Architecture with this model:

```
[background thread]                    [glib main thread]
pactl subscribe stdout →               glib::timeout_add_local(100ms)
  line-by-line reader →                  rx.try_recv()
    filter "change on sink" →               → update widget label
      run get-sink-info once →
        mpsc::Sender::send(data)
```

Implementation sketch for `VolumeWidget`:

```rust
pub struct VolumeWidget {
    rx: std::sync::mpsc::Receiver<VolumeData>,
    cached: VolumeData,
    _thread_handle: std::thread::JoinHandle<()>,
}

impl VolumeWidget {
    pub fn new() -> Result<Self, FramesError> {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            subscribe_loop(tx);
        });
        Ok(Self { rx, cached: VolumeData::default(), _thread_handle: handle })
    }
}

fn subscribe_loop(tx: std::sync::mpsc::Sender<VolumeData>) {
    let mut child = Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .spawn()
        .expect("pactl subscribe failed to start");
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    for line in reader.lines().flatten() {
        if line.contains("sink") && line.contains("change") {
            if let Ok(data) = read_volume_info() {
                if tx.send(data).is_err() {
                    break; // receiver dropped; exit thread
                }
            }
        }
    }
    // pactl exited — log and exit thread; reconnect logic would restart
}
```

`Widget::update()` in this model becomes:

```rust
fn update(&mut self) -> Result<WidgetData, FramesError> {
    // Drain channel; use latest if multiple events arrived since last tick.
    while let Ok(data) = self.rx.try_recv() {
        self.cached = data;
    }
    Ok(WidgetData::Volume(self.cached.clone()))
}
```

**Pros:**
- Volume label updates within the 100ms glib tick of the event, not the poll
  interval. Feels instant.
- Zero subprocess spawning after initial `pactl subscribe` start.
- Works on both PulseAudio and PipeWire (both support `pactl subscribe`).

**Cons:**
- `VolumeWidget` is no longer a simple stateless struct — it owns a thread handle
  and a channel receiver.
- Thread lifecycle: if `pactl` exits (PipeWire restart, suspend/resume), the
  thread exits silently. Need reconnect logic or a watchdog.
- The `Widget` trait contract (`update()` returns a `Result`) does not currently
  model "no new data since last tick" — `update()` would return stale data.
  Acceptable behaviour (same value is a no-op in the renderer) but worth noting.
- `_thread_handle` is intentionally not joined — the widget drop scenario during
  bar teardown could block. Use a detached thread or a stop signal instead.
- `frames_core` restriction: `subscribe_loop` is pure process I/O + no GTK
  imports — safe to keep in `frames_core`.

**Verdict: The right long-term answer.** Implement immediately after Option A
ships and is confirmed stable. Plan as a follow-on task.

---

### Option C — PulseAudio D-Bus (`org.PulseAudio.Core1`) + zbus

PulseAudio exposes a `org.PulseAudio.Core1` D-Bus interface. Sinks emit
`VolumeUpdated` and `MuteUpdated` signals. `zbus` (already in deps, blocking
API available) could subscribe to these signals.

**Problems:**
- PipeWire's PA compatibility layer (`pipewire-pulse`) does **not** reliably
  expose `org.PulseAudio.Core1` over D-Bus. The session bus address used by the
  native PA D-Bus server (`PULSE_DBUS_SERVER`) is an environment variable set
  by `pulseaudio` daemon — not available under `pipewire-pulse`.
- PipeWire's own `pw-pulse` only implements the legacy PA protocol (TCP socket),
  not the D-Bus interface.
- Requires dynamic lookup of the D-Bus socket, PA extension negotiation via
  `org.PulseAudio.Core1.GetServerVersion`, and extension enabling.

**Verdict: Fragile.** Works on pure PulseAudio setups; broken on Fedora 36+
(PipeWire default). Rejected as primary path. May be revisited if D-Bus
support is added to WirePlumber in a future PipeWire release.

---

### Option D — PipeWire native library (`libpipewire-0.3`)

Would require a new `-sys` crate dependency with C library linking (`libpipewire
-dev`). Violates the project's philosophy of using only `pactl` for audio
interaction (no C library deps for audio). Out of scope.

---

## Recommendation

**Ship Option A immediately** (single `pactl get-sink-info` call). It is a
self-contained change to `crates/frames_core/src/widgets/volume.rs`.

**Plan Option B as a follow-on task.** It requires:
- Making `VolumeWidget` stateful (thread + channel)
- Deciding thread lifecycle policy (reconnect vs. restart)
- Updating the `Widget` contract documentation to clarify "stale return is valid"
- Adding a `WidgetData::Volume` variant field `fresh: bool` is **not** needed —
  the renderer already handles identical successive values as no-ops

Both options live entirely in `frames_core` and require no display code.

---

## Standards Conflict / Proposed Update

`WIDGET_API.md §3.2` (Widget::update contract) does not discuss stateful widgets
that own background threads. When Option B is implemented, add a note:

> **Stateful widgets**: A widget may own a background thread and a `Receiver<T>`.
> `update()` MUST return `Ok` with the cached (possibly stale) value if no new
> data has arrived. Returning `Err` should be reserved for unrecoverable state
> (thread dead, channel closed). Stale returns are valid and expected.

---

## Sources

- `crates/frames_core/src/widgets/volume.rs`: current two-call implementation
- `crates/frames_bar/src/widgets/volume.rs`: renderer and `VolumeConfig`
- `pactl(1)` man page: `get-sink-info`, `subscribe` subcommand output formats
- PipeWire D-Bus compatibility notes: PipeWire issue tracker (no stable PA Core1
  D-Bus support under `pipewire-pulse`)
- `https://docs.rs/zbus/5.1.0/zbus/index.html`: blocking API confirmed available
- Workspace `Cargo.toml`: `zbus` already present with `blocking-api` feature

## Open Questions

1. **ThreadHandle drop policy for Option B:** Should the background thread be
   detached (`thread::spawn` without storing handle) or should `VolumeWidget`
   implement `Drop` to send a stop signal? Detached is simpler for now; a stop
   signal requires an `Arc<AtomicBool>` shared with the thread.

2. **`get-sink-info` on headless/test builds:** `pactl` is not available in CI.
   The existing `#[cfg(not(test))]` guard or mock substitution strategy in
   `volume.rs` needs to be confirmed before shipping Option A.

3. **Multi-sink support:** `get-sink-info` and `subscribe` both concern the
   default sink. If multi-sink support is ever needed, this architecture would
   need revision.
