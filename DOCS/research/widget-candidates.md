# Research: Next Widget Candidates
**Date:** 2026-03-18
**Status:** Findings complete — decision pending

## Question

What widgets should be prioritised next, based on what the most-used Linux status bars
ship and what users most commonly request?

## Summary

The clearest next four widgets are **CPU temperature**, **disk usage**, **active window
title**, and **custom/script** — all require zero new dependencies. Temperature, disk,
load, and uptime can all be read from `sysinfo` 0.30 (already a workspace dep). Active
window and keyboard layout are X11-only and must live entirely in `parapet_bar`. A
custom/script widget unlocks unlimited user-defined displays with no new crate work at
all.

---

## Findings

### Comparison matrix: what popular bars ship

| Widget | Polybar | Waybar | i3bar | Parapet (today) |
|--------|:-------:|:------:|:-----:|:--------------:|
| Clock | ✓ | ✓ | ✓ | ✓ |
| CPU usage | ✓ | ✓ | ✓ | ✓ |
| Memory | ✓ | ✓ | — | ✓ |
| Network | ✓ | ✓ | — | ✓ |
| Workspaces | ✓ | ✓ | ✓ | ✓ |
| Battery | ✓ | ✓ | — | ✓ |
| Volume | ✓ | ✓ | — | ✓ |
| Brightness | ✓ | ✓ | — | ✓ |
| Media (MPRIS) | — | ✓ | — | ✓ |
| Weather | — | — | — | ✓ |
| Launcher | — | — | — | ✓ |
| **CPU temperature** | ✓ | ✓ | — | ✗ |
| **Disk / filesystem** | ✓ | ✓ | — | ✗ |
| **Active window title** | ✓ | ✓ | — | ✗ |
| **Keyboard layout** | ✓ | ✓ | — | ✗ |
| **Custom / script** | ✓ | ✓ | ✓ | ✗ |
| **Load average** | — | ✓ | — | ✗ |
| **Uptime** | — | — | — | ✗ |
| Tray / SNI | ✓ | ✓ | ✓ | ✗ (futures.md) |
| Bluetooth | — | ✓ | — | ✗ |

Sources: Polybar wiki (github.com/polybar/polybar/wiki), Waybar wiki (github.com/Alexays/Waybar/wiki).

---

### Option A: CPU Temperature (`temperature`)

**Popularity:** Present in every major bar. Demand is near-universal among power users
and gamers.

**Data source:** `sysinfo::Components::new_with_refreshed_list()` + `.refresh()`.
Each `Component` exposes:
- `label() -> &str` — e.g. `"Package id 0"`, `"Core 0"`, `"Core 1"`, `"acpitz"`, etc.
- `temperature() -> f32` — current reading in °C
- `max() -> f32` — peak reading since boot
- `critical() -> Option<f32>` — throttle/shutdown threshold

The most useful display is the CPU *package* temperature: find the component whose
label contains `"Package"` or defaults to the highest `Core` reading. Laptop
users also want to know when they are near `critical`.

**Implementation tier:** `parapet_core` — no display deps at all.

**New deps:** None. `sysinfo` ~0.30 already in workspace.

**Config:**
```toml
[[widgets]]
type = "temperature"
position = "right"
interval = 3000
component = "Package id 0"   # optional; auto-detect CPU package if omitted
show_max   = false
```

**Render example:** `🌡 52°C` (normal) / `🌡 91°C` (warning) / `🌡 98°C` (urgent — near critical)

**CSS:** adds `.warning` when temp > 80°C, `.urgent` when within 10°C of critical.

**Risk:** On machines without hardware sensors (some VMs) `Components` returns an empty
list. Widget should render `"🌡 —"` in that case rather than erroring.

---

### Option B: Disk Usage (`disk`)

**Popularity:** In every major bar. Among the most-requested items on r/unixporn config
threads.

**Data source:** `sysinfo::Disks::new_with_refreshed_list()`. Each `Disk` exposes:
- `mount_point() -> &Path`
- `total_space() -> u64`
- `available_space() -> u64`
- `name() -> &OsStr` — device name (e.g. `/dev/nvme0n1p3`)

Used = total − available. Percent = used / total.

**Implementation tier:** `parapet_core`.

**New deps:** None.

**Config:**
```toml
[[widgets]]
type = "disk"
position = "right"
interval = 30000
mount = "/"          # default "/"
format = "percent"   # "percent" | "used_gb" | "free_gb"
```

**Render example:** `💾 45%` or `💾 120G / 512G`

**Risk:** `available_space` can return 0 on pseudo-filesystems (tmpfs, overlayfs).
Safe to skip any disk with `total_space == 0`.

---

### Option C: Active Window Title (`active_window`)

**Popularity:** Present in virtually every real-world Polybar/Waybar config screenshot.
Extremely high demand. It is the single most common "why doesn't Parapet show the focused
window?" request category across bar communities.

**Data source:** X11 EWMH properties on the root window and the active client window:
1. Read `_NET_ACTIVE_WINDOW` from root → `Window XID` (u32)
2. Read `_NET_WM_NAME` (UTF-8, XA_STRING) from that window → title string

`gdk::property_get()` already handles both reads per ADR-004. No new crate needed.

**Implementation tier:** `parapet_bar` only — X11 calls cannot enter `parapet_core`.

This means the active window widget **cannot use the standard `Poller` pipeline**. It
needs either:
- **A**: A dedicated glib timer in the bar (same pattern as the existing `WorkspacesWidget`
  self-poll, which is already noted as an inconsistency in `futures.md`)
- **B**: A new `WidgetData::Text(String)` variant in `parapet_core` so a stub core widget
  exists for the data flow but reads are still taken over by the bar renderer via X11

Option A is simpler and keeps the inconsistency contained. Option B is cleaner long-term
but inflates the `WidgetData` enum with a generic catch-all. **Recommendation: Option A
for now**, with the inconsistency documented in `futures.md` as a companion to the
existing workspaces note. The addition of `WidgetData::Text` can be a separate decision.

**Config:**
```toml
[[widgets]]
type = "active_window"
position = "center"
interval = 200        # ms; fast enough to feel reactive
max_length = 60       # truncate to N chars
```

**Render example:** `Firefox — GitHub` / `Helix — main.rs` / (empty when no window)

**Risk:** `_NET_WM_NAME` may return an empty string or be absent; fall back to
`WM_NAME` (Latin-1). On rapid window switches the poll may lag a frame — acceptable.

---

### Option D: Custom / Script (`script`)

**Popularity:** Polybar's `script` module and Waybar's `custom` module are among the
most-used. They are the escape hatch that lets users add anything the bar doesn't
natively support: VPN status, package updates, todo count, uptime, etc.

**Data source:** `std::process::Command`, capturing stdout. One-shot or continuous
(`tail -f`-style is out of scope; one-shot poll only).

**Implementation tier:** `parapet_core`. The core widget spawns a child process and
stores the trimmed stdout string in `WidgetData::Script { text: String }`. On non-zero
exit or empty output the widget optionally hides (renders blank) or shows the last
successful value.

**New deps:** None. `std::process::Command` is in stdlib.

**Security note:** The command is specified in user-controlled `config.toml`, not
injected from an external source. This is the same trust model as `on_click`. No
sanitisation needed beyond what the shell already provides. Parapet should **not** run
the script through `sh -c` with a root-inherited environment — using `Command::new`
with explicit args and inheriting the user environment is correct.

**Config:**
```toml
[[widgets]]
type = "script"
position = "right"
interval = 10000
command = "cat /sys/class/thermal/thermal_zone0/temp"
# prefix = "🌡 "      # optional static prefix
# on_empty = "hide"   # hide | last | placeholder
```

**Render example:** `🌡 52000` → user formats via the command itself

**Risk:** Commands that hang block the polling goroutine. Must apply a timeout — wrap
the `Command` with a 5-second wall-clock limit (use `std::process::Command` +
`child.wait_timeout`). If the `wait-timeout` crate is not available, use a thread with
a `std::sync::mpsc` channel and a deadline. Actually simplest: `Command::output()` in a
thread with timeout via `crossbeam-channel` — but that adds a dep. Simplest safe
approach: document the 5s timeout and implement it with a spawned OS thread +
`child.wait()` + `thread.join()` with a timeout duration.

---

### Option E: Keyboard Layout (`keyboard`)

**Popularity:** High among multilingual users. Present in most Polybar configs that
target non-English setups.

**Data source:** Spawn `setxkbmap -query` (same subprocess approach as `volume` uses
`pactl`). Parse the `layout:` and optionally `variant:` lines.

Alternative: Use GDK's `gdk::Keymap::get_direction()` — but this does not expose the
layout name string, only text direction. Using `setxkbmap` is the standard approach.

**Implementation tier:** `parapet_bar` only (spawns an X11-related subprocess). Or
`parapet_core` (subprocess is headless-safe though keymaps are display-adjacent).
Verdict: put the polling in `parapet_core` since `setxkbmap -query` has no display
dependency — it reads from the X server but is a plain subprocess from Rust's
perspective. The `WidgetData` variant can carry the layout string.

**New deps:** None.

**Config:**
```toml
[[widgets]]
type = "keyboard"
position = "right"
interval = 1000
show_variant = false
```

**Render example:** `⌨ us` / `⌨ de` / `⌨ ru`

**Risk:** `setxkbmap` may not be installed. Widget should return a graceful
`WidgetData::Keyboard { layout: String }` with empty string if the command is absent,
matching how `brightness` handles a missing backlight device.

---

### Option F: Load Average / Uptime (minor)

Both are trivially available from `sysinfo`:
- `System::load_average()` → `LoadAvg { one, five, fifteen }` — already in sysinfo 0.30
- `System::uptime()` → `u64` seconds — already in sysinfo 0.30

Neither requires a dedicated data collector struct. They could be fields in a
combined `SystemInfo` widget or rendered as standalone `script`-style calculations.
**Low implementation cost but also low user demand** compared to temperature and disk.
Consider adding them as config options on an *existing* widget rather than standalone
widgets:
- Load average as a display mode on the existing `cpu` widget (`format = "load"`)
- Uptime as a `script` one-liner: `command = "uptime -p | sed 's/up //'"` once the
  script widget exists

---

### Option G: Bluetooth

**Data source:** BlueZ D-Bus (`org.bluez` on *system* bus via `zbus`).  
`zbus` is already in the workspace dep graph.  
Would show connected device name + battery level (if reported).

**Demand:** Medium. High for laptop users, irrelevant for desktop headphone users who
prefer wired. Not present in Polybar natively (community module only).

**Risk:** BlueZ power states (`org.bluez.Adapter1.Powered`) and device enumeration
(`org.bluez.Device1.Connected`, `.Alias`, `.Battery.Percentage` via
`org.bluez.Battery1`) are straightforward but require careful handling of absent adapters
(Bluetooth disabled or hardware absent). `org.bluez.Battery1` is only available on
BlueZ 5.48+ and is not universally present.

**Verdict:** Viable but lower priority than A–D.

---

## Recommendation

**Priority 1 (ship next, zero new deps):**

| # | Widget | Layer | Key API |
|---|--------|-------|---------|
| 1 | `temperature` | `parapet_core` | `sysinfo::Components` |
| 2 | `disk` | `parapet_core` | `sysinfo::Disks` |
| 3 | `active_window` | `parapet_bar` only | `gdk::property_get` EWMH |
| 4 | `script` | `parapet_core` | `std::process::Command` |

These four cover the biggest gap between Parapet and the Polybar/Waybar feature matrix.
They require no new crates, no new unsafe blocks, and fit cleanly into the existing
widget architecture.

**Priority 2 (next batch, minimal deps):**

| # | Widget | Layer | Note |
|---|--------|-------|------|
| 5 | `keyboard` | `parapet_core` | Subprocess `setxkbmap`, X11-adjacent |
| 6 | `bluetooth` | `parapet_core` | `zbus` already present |

**Defer:**
- Load average and uptime: implement as `script` one-liners until demand is shown
  for dedicated widgets
- Tray/SNI: already tracked in `futures.md`, large scope, separate research needed

---

## Standards Conflict / Proposed Update

No conflict. All candidates fit within ARCHITECTURE §3 (sysinfo/zbus already listed),
WIDGET_API §4 (new variants would be added at minor version), and PLATFORM_COMPAT §2
(X11 requirement is not extended). `active_window` deepens the existing workspaces
inconsistency noted in `futures.md`; the proposed mitigation (`WidgetData::Text` or
accepted bar-only implementation) should be recorded in `futures.md` alongside Option C
if it is implemented.

---

## Sources

- Polybar wiki module list: https://github.com/polybar/polybar/wiki — full module inventory
- Waybar wiki module list: https://github.com/Alexays/Waybar/wiki/Module:-Idle-Inhibitor — sidebar list
- sysinfo 0.30.13 docs: https://docs.rs/sysinfo/0.30.13/sysinfo/index.html — confirmed Components, Disks, LoadAvg, uptime availability

---

## Open Questions

1. **`WidgetData::Text` vs bar-only for `active_window`**: Should a generic text variant
   be added to the `WidgetData` enum to unify script, active_window, and future bar-only
   string widgets through the standard Poller pipeline? Or keep bar-only widgets as an
   accepted pattern (current workspaces precedent)?

2. **Temperature auto-detect heuristic**: On machines with many sensors (Intel + NVIDIA),
   which component should be the default? Suggested: first component whose label matches
   `"Package id 0"` case-insensitively, then first matching `"CPU"`, then first
   non-fan/non-acpi component.

3. **Script widget timeout**: `std::process::Command::output()` has no built-in timeout.
   Acceptable approach (no new dep): spawn the child, use a background thread + channel
   with a `Duration::from_secs(5)` receive timeout, kill child on timeout.
