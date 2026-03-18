# Frames — Widget API

> **Scope:** Widget trait contract, `WidgetData` enum specification, widget lifecycle, and rules for adding new widget types.
> **Last Updated:** Mar 17, 2026

---

## 1. Overview

The `Widget` trait is the uniform interface between `frames_core` (data collection) and `frames_bar` (GTK3 rendering). Every data-providing component in Frames implements `Widget`.

The contract is strict:
- `frames_core` owns data collection and exposes `WidgetData`
- `frames_bar` owns rendering and consumes `WidgetData`
- No GTK types cross into `frames_core`; no system-info types cross into `frames_bar`

---

## 2. Widget API Version

```rust
// frames_core/src/widget.rs
pub const WIDGET_API_VERSION: &str = "1.7.1";
```

**Versioning policy:**

| Change Type | Version Bump | Example |
|-------------|-------------|---------|
| New `WidgetData` variant added | Minor bump | `1.0.0` → `1.1.0` |
| New field added to existing variant | Minor bump | `1.1.0` → `1.2.0` |
| Variant renamed or removed | Major bump | `1.2.0` → `2.0.0` |
| `Widget` trait method added | Major bump | `2.0.0` → `3.0.0` |
| `Widget` trait method signature changed | Major bump | |
| Bug fix with no API surface change | Patch bump | `1.0.0` → `1.0.1` |

**Rule:** Any change to the `Widget` trait or `WidgetData` enum requires a `WIDGET_API.md` update in the same commit.

---

## 3. The Widget Trait

```rust
/// Uniform interface for widget data providers.
///
/// Implementors collect system information and return it as [`WidgetData`].
/// All implementations must be `Send + Sync` to allow future multi-threaded polling.
///
/// # Contract
///
/// - `name()` must return a stable, non-empty string across calls.
/// - `update()` must not block for more than the widget's configured interval.
/// - `update()` should return `Err` only for genuinely unrecoverable failures;
///   transient failures (e.g., brief `/proc` read error) should return stale data
///   or a degraded `WidgetData` value.
pub trait Widget: Send + Sync {
    /// Human-readable name for this widget instance.
    ///
    /// Used in config references, log messages, and CSS widget names.
    /// Must be non-empty. Should be unique within a bar config.
    fn name(&self) -> &str;

    /// Refresh internal state and return the latest data snapshot.
    ///
    /// Called by the [`Poller`] on each polling interval. The implementation
    /// is responsible for reading system state and constructing the appropriate
    /// [`WidgetData`] variant.
    ///
    /// # Errors
    ///
    /// Returns [`FramesError`] if the data source is unavailable or unreadable.
    fn update(&mut self) -> Result<WidgetData, FramesError>;
}
```

---

## 4. The WidgetData Enum

`WidgetData` carries the widget's current value. It is `#[non_exhaustive]` — callers must handle unknown variants gracefully.

```rust
/// Data produced by a [`Widget`] on each update cycle.
///
/// This enum is `#[non_exhaustive]`. Match arms must include a `_ => {}` fallback
/// to remain forwards-compatible when new variants are added.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WidgetData {
    /// Date and time string ready for display.
    Clock {
        /// Formatted string per the widget's `format` config field.
        display: String,
    },

    /// CPU usage statistics.
    Cpu {
        /// Aggregate CPU usage across all cores, as a percentage 0.0–100.0.
        usage_pct: f32,
        /// Per-core usage percentages, in logical core order.
        per_core: Vec<f32>,        /// CPU package temperature in °C, or `None` when no sensor is available.
        temp_celsius: Option<f32>,    },

    /// Memory usage statistics.
    Memory {
        /// RAM in use, in bytes.
        used_bytes: u64,
        /// Total RAM available, in bytes.
        total_bytes: u64,
        /// Swap in use, in bytes.
        swap_used: u64,
        /// Total swap available, in bytes.
        swap_total: u64,
    },

    /// Network interface statistics.
    Network {
        /// Received bytes per second since last update.
        rx_bytes_per_sec: u64,
        /// Transmitted bytes per second since last update.
        tx_bytes_per_sec: u64,
        /// Name of the monitored interface (e.g. `"eth0"`, `"wlan0"`).
        interface: String,
    },

    /// Battery charge and status.
    Battery {
        /// Charge percentage, 0.0–100.0. `None` if no battery present.
        charge_pct: Option<f32>,
        /// Current battery status.
        status: BatteryStatus,
    },

    /// Filesystem disk usage for a single mount point.
    Disk {
        /// Mount point path monitored by this widget (e.g. `"/"` or `"/home"`).
        mount: String,
        /// Bytes currently used on the filesystem.
        used_bytes: u64,
        /// Total bytes on the filesystem.
        total_bytes: u64,
        /// All real (non-virtual) mounted filesystems detected on the host.
        /// Used to render a hover tooltip listing every disk.
        all_disks: Vec<DiskEntry>,
    },

    /// Workspace list and active workspace index.
    Workspaces {
        /// Total number of workspaces.
        count: usize,
        /// 0-based index of the currently active workspace.
        active: usize,
        /// Workspace names from `_NET_DESKTOP_NAMES`. Empty strings if names not set.
        names: Vec<String>,
    },

    /// Audio output volume level and mute state.
    Volume {
        /// Current output volume as a percentage (0.0–100.0).
        volume_pct: f32,
        /// Whether the output is currently muted.
        muted: bool,
    },

    /// Screen backlight brightness level.
    Brightness {
        /// Current brightness as a percentage (0.0–100.0).
        /// Zero on machines without a variable backlight.
        brightness_pct: f32,
    },

    /// Current weather conditions from the Open-Meteo API.
    Weather {
        /// Air temperature at 2 m in the widget's configured unit (°C or °F).
        temperature: f32,
        /// WMO weather interpretation code (0 = clear sky, 61 = rain, etc.).
        weather_code: u16,
        /// Wind speed at 10 m in km/h.
        wind_speed: f32,
        /// Relative humidity at 2 m, as a percentage (0–100).
        humidity: u8,
        /// The temperature unit in use (`Celsius` or `Fahrenheit`).
        unit: TempUnit,
    },

    /// Currently playing media track from an MPRIS2 player.
    Media {
        /// Track title from `xesam:title`. Empty string if no player is active.
        title: String,
        /// Track artist from `xesam:artist`. Empty string if unavailable.
        artist: String,
        /// Current playback status.
        status: PlaybackStatus,
        /// True if the player reports it can advance to the next track.
        can_go_next: bool,
        /// True if the player reports it can go back to the previous track.
        can_go_previous: bool,
    },
}

/// Temperature display unit for the weather widget.
///
/// Deserialises from `"celsius"` and `"fahrenheit"` config strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

/// MPRIS2 playback status.
///
/// `Stopped` is also returned when no player is present on the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Battery charging status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    Unknown,
}
```

---

## 5. Adding a New Widget Type

When adding a new built-in widget to `frames_core`:

1. **Add the `WidgetData` variant** in `widget.rs` with all fields documented
2. **Implement the `Widget` trait** in `widgets/<name>.rs`
3. **Add the widget type** to `FramesConfig` deserialization in `config.rs`
4. **Add a renderer** in `frames_bar/src/widgets/<name>.rs` that consumes the new variant
5. **Update CONFIG_MODEL.md** with the new `type` string and its config fields
6. **Update WIDGET_API.md** (this file) — add the variant to §4, bump the version in §2
7. **Update ARCHITECTURE.md §4.1** — add the new module to the module table
8. **Write tests** — widget trait contract test + config round-trip test

Do not add a new widget type without completing all steps above. Partial implementations cause confusing failures when users reference the new type in their config.

---

## 6. Widget Lifecycle

```
Config loaded
    │
    ▼
Widget instance created via factory (match config.widget_type { ... })
    │
    ▼
Renderer created from widget config (frames_bar)
    │
    ▼
Renderer widget() added to bar layout
    │
    ▼
Poller::register(widget, interval_ms)
    │
    ▼
[glib timer fires every interval_ms]
    │
    ▼
widget.update() called — returns WidgetData
    │
    ▼
renderer.update(&widget_data) called — updates GtkLabel/etc.
    │
    ▼
[repeat until bar exits]
```

---

## 7. Widget Implementation Rules

### 7.1 Polling Responsibility

Widgets are responsible for managing their own internal state between `update()` calls. The `sysinfo` crate requires a `refresh_*()` call before sampling — the widget implementation must call this before reading values.

### 7.2 Stale Data on Error

If `update()` encounters a transient error (e.g., brief `/proc` read failure), it should return the last known good `WidgetData` rather than an error. Only return `Err` for persistent, unrecoverable failures.

Returning the last cached `WidgetData` is also correct when no new data has arrived since the previous `update()` call (e.g., a channel-driven widget whose `mpsc` channel has no pending messages). An empty channel is not a failure condition. `Err` should only be returned for persistent, unrecoverable failures — not for "nothing changed since last tick."

### 7.3 First-Call Behavior

On the first call to `update()`, some widgets (CPU, network) have no previous sample to compare against. These widgets should return a zero/placeholder value on first call rather than an error:

```rust
// cpu.rs
pub fn update(&mut self) -> Result<WidgetData, FramesError> {
    self.system.refresh_cpu();
    let usage = if self.first_call {
        self.first_call = false;
        0.0  // First sample has no delta to compare against
    } else {
        self.system.global_cpu_info().cpu_usage()
    };
    Ok(WidgetData::Cpu { usage_pct: usage, per_core: self.per_core() })
}
```

### 7.4 Stateful Widgets with Background Threads

A widget may own a background thread and an `mpsc::Receiver` when event-driven updates are preferable to polling (e.g., audio volume via `pactl subscribe`).

**Required pattern:**

```rust
pub struct ExampleWidget {
    cached:  WidgetData,
    rx:      std::sync::Mutex<std::sync::mpsc::Receiver<SomeData>>,
    _thread: std::thread::JoinHandle<()>,
}
```

**`Send + Sync` obligation:** `std::sync::mpsc::Receiver<T>` is `Send` but `!Sync`. Wrap it in `std::sync::Mutex<Receiver<T>>` to satisfy the `Sync` bound required by `Widget: Send + Sync`. `Mutex<T>: Sync where T: Send`. `JoinHandle<T>` is `Send + Sync`.

**`update()` contract for channel-driven widgets:**
1. Acquire the lock on the `Receiver`: `self.rx.lock().expect("… poisoned")`.
2. Drain the channel in a loop using `try_recv()`.
3. On `TryRecvError::Empty` — break; return cached value. This is normal.
4. On `TryRecvError::Disconnected` — log `tracing::warn!`; break; return cached value. The thread has exited (external process absent, OS restart, etc.). Do not return `Err` — this is a degraded but recoverable state.
5. Drop the lock before cloning or returning the cached value.

**Thread naming:** Use `std::thread::Builder::new().name("frames-<purpose>")` so threads appear with meaningful names in debuggers and OS process listings.

**Thread lifecycle:** The background thread exits when `tx.send()` fails because the widget was dropped (receiver gone) or when its external subprocess exits. Storing `_thread: JoinHandle` (not calling `join()` in `Drop`) is correct for bar-lifetime widgets — the OS reclaims the thread when the process exits. Implement `Drop` with `join()` only if the widget can outlive the bar or if the thread holds OS resources that must be released before process exit.

---

## 8. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Widget module structure | [ARCHITECTURE.md §4.1–§4.2](ARCHITECTURE.md) |
| Widget config fields | [CONFIG_MODEL.md §4](CONFIG_MODEL.md) |
| Widget renderer rules | [UI_GUIDE.md §5](UI_GUIDE.md) |
| Widget trait contract tests | [TESTING_GUIDE.md §2.3](TESTING_GUIDE.md) |

---

## Changelog

### 1.7.1 (2026-03-18)
- Clarified §7.2 stale-data policy: returning cached data when an `mpsc` channel has no pending messages is valid and expected, not an error.
- Added §7.4 "Stateful Widgets with Background Threads" — documents the `Mutex<Receiver<T>>` pattern for `Send + Sync` compliance, `update()` channel-drain contract, thread naming conventions, and lifecycle policy.
- Patch bump (documentation only; no `WidgetData` or trait signature change).

### 1.7.0 (2026-03-18)
- Added `all_disks: Vec<DiskEntry>` to `WidgetData::Disk` — full list of real mounted filesystems for hover tooltip
- Added `DiskEntry { mount: String, used_bytes: u64, total_bytes: u64 }` public struct (re-exported from `frames_core`)
- Bar renderer wraps disk label in `gtk::EventBox` and sets multi-line tooltip from `all_disks`
- Non-breaking field addition (`#[non_exhaustive]`)

### 1.6.0 (2026-03-18)
- Added `temp_celsius: Option<f32>` to `WidgetData::Cpu` — CPU package temperature from `sysinfo::Components`
- Rendered as a GTK hover tooltip on the CPU label; `None` on platforms without temperature sensors
- Non-breaking field addition (`#[non_exhaustive]`)

### 1.5.0 (2026-03-18)
- Added `WidgetData::Disk { mount, used_bytes, total_bytes }` — filesystem disk usage widget support
- Non-breaking addition (`#[non_exhaustive]`)

### 1.4.0 (2026-03-17)
- Added `WidgetData::Weather { temperature, weather_code, wind_speed, humidity, unit }` — Open-Meteo weather widget support
- Added `WidgetData::Media { title, artist, status, can_go_next, can_go_previous }` — MPRIS2 media widget support
- Added `TempUnit` enum (`Celsius` / `Fahrenheit`) — temperature unit selector for weather widget
- Added `PlaybackStatus` enum (`Playing` / `Paused` / `Stopped`) — MPRIS2 playback state
- Both new `WidgetData` variants are non-breaking (`#[non_exhaustive]`)

### 1.2.0 (2026-03-17)
- Added `WidgetData::Volume { volume_pct: f32, muted: bool }` — audio widget support
- Added `WidgetData::Brightness { brightness_pct: f32 }` — backlight widget support
- Both variants are non-breaking (`#[non_exhaustive]`)
