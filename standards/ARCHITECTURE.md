# Frames — Architecture

> **Scope:** Crate structure, module organization, dependency graph, and system design.
> **Last Updated:** Mar 17, 2026

---

## 1. Design Philosophy

Frames is a ground-up Linux-native status bar for the Cinnamon desktop. There is no upstream codebase to inherit from. Every architectural decision is deliberate.

**Core principles:**

- **One language boundary** — Rust top to bottom except the GTK3 C library underneath `gtk-rs`. No Python layer, no shell scripts doing the heavy lifting.
- **Display isolation** — `frames_core` is a pure library with no dependency on GTK, GDK, X11, or any display system. All display logic lives in `frames_bar`.
- **Widget data, not widget widgets** — `frames_core` produces *data* (CPU %, clock string, battery level). `frames_bar` turns that data into GTK3 widgets. The boundary is strict.
- **Config-driven** — Every visual and behavioral property is expressed in TOML. Hard-coded values are a bug.
- **Polling, not pushing** — Widgets are updated on a timer. There is no IPC bus or event system to synchronize.

---

## 2. Workspace Structure

```
frames/
├── Cargo.toml              ← workspace root
├── crates/
│   ├── frames_core/        ← pure library: widget data, system info, config, errors
│   └── frames_bar/         ← GTK3 binary: bar window, widget renderers, X11 EWMH
├── standards/              ← All .md standards documents
├── DOCS/                   ← Governance and planning documents
│   ├── futures.md          ← Ideas, debt, and completed work log
│   ├── DECISIONS.md        ← Architectural decision records
│   ├── conflict.md         ← Standards gap and conflict notes
│   └── cleanup.md          ← Archived code removal log
└── doa/                    ← Archived code (never deleted)
```

**Planned future crates:** None currently. Additional crates are extracted only when a module inside `frames_core` has a proven, distinct dependency surface. Do not extract speculatively.

---

## 3. Dependency Graph

```
frames_bar
    ├── frames_core
    ├── gtk (~0.18, GTK3 bindings)
    ├── gdk (~0.18, transitive)
    ├── glib (~0.18, transitive)
    ├── anyhow
    └── tracing-subscriber

frames_core
    ├── sysinfo (~0.30, CPU/RAM/network stats)
    ├── chrono (~0.4, date/time)
    ├── serde + toml (config serialization)
    ├── thiserror (~1.0, error types)
    ├── anyhow (~1.0, internal propagation)
    ├── tracing (~0.1, logging)
    ├── ureq (~3.2, blocking HTTP for weather widget)
    ├── zbus (~5.1, D-Bus client for MPRIS media widget)
    └── notify (~6.1, config file hot-reload)
```

**Dependency direction rules:**
- `frames_bar` may depend on `frames_core`. Never the reverse.
- `frames_core` must not depend on `gtk`, `gdk`, `glib`, `x11`, or any crate that requires a display server.
- `frames_bar` is the only crate that may talk to GTK3 or X11.

---

## 4. frames_core — Module Structure

### 4.1 Top-Level Modules

```
frames_core/
├── src/
│   ├── lib.rs
│   ├── widget.rs       ← Widget trait + WidgetData enum (uniform widget interface)
│   ├── widgets/        ← Built-in widget data providers
│   │   ├── mod.rs
│   │   ├── clock.rs    ← Date/time string generation
│   │   ├── cpu.rs      ← CPU usage via sysinfo
│   │   ├── memory.rs   ← RAM/swap usage via sysinfo
│   │   ├── network.rs  ← Network rx/tx via sysinfo
│   │   ├── battery.rs  ← Battery level/status via /sys/class/power_supply/
│   │   ├── disk.rs     ← Filesystem used/total via sysinfo::Disks
│   │   ├── volume.rs   ← Audio output volume and mute state via pactl
│   │   ├── brightness.rs ← Screen backlight percentage via /sys/class/backlight/
│   │   ├── weather.rs  ← Current conditions from Open-Meteo API via ureq
│   │   ├── media.rs    ← MPRIS2 playback info via zbus (D-Bus session bus)
│   │   └── workspaces.rs ← Workspace count/name stubs (X11 query handled in frames_bar)
│   ├── poll.rs         ← Poller — interval-based widget update scheduling
│   ├── config.rs       ← FramesConfig, BarConfig, WidgetConfig, WidgetKind (+ 13 per-widget structs) — TOML config; ConfigWatcher — notify-backed file watcher for hot-reload
│   └── error.rs        ← FramesError — crate-level error types (thiserror)
```

### 4.2 widget.rs — Widget Trait and Data

The `Widget` trait is the uniform interface every widget data provider implements:

```rust
pub trait Widget: Send + Sync {
    /// Human-readable name for this widget (used in config and logs).
    fn name(&self) -> &str;

    /// Refresh internal state and return the latest data snapshot.
    fn update(&mut self) -> Result<WidgetData, FramesError>;
}
```

`WidgetData` is a non-exhaustive enum carrying the widget's current value:

```rust
#[non_exhaustive]
pub enum WidgetData {
    Clock { display: String },
    Cpu { usage_pct: f32, per_core: Vec<f32> },
    Memory { used_bytes: u64, total_bytes: u64, swap_used: u64, swap_total: u64 },
    Network { rx_bytes_per_sec: u64, tx_bytes_per_sec: u64, interface: String },
    Battery { charge_pct: f32, status: BatteryStatus },
    Disk { mount: String, used_bytes: u64, total_bytes: u64 },
    Workspaces { count: usize, active: usize, names: Vec<String> },
    Volume { volume_pct: f32, muted: bool },
    Brightness { brightness_pct: f32 },
    Weather { temperature: f32, weather_code: u16, wind_speed: f32, humidity: u8, unit: TempUnit },
    Media { title: String, artist: String, status: PlaybackStatus, can_go_next: bool, can_go_previous: bool },
}
```

`WidgetData` variants are `#[non_exhaustive]` so callers must handle unknown variants without breaking. See WIDGET_API.md for the full contract.

### 4.3 poll.rs — Polling Infrastructure

`Poller` drives widget updates. It is a pure Rust struct — no GTK, no glib timers. The actual timer registration (`glib::timeout_add_local`) lives in `frames_bar`, which calls `Poller::poll()` on the glib main thread.

```
Poller
├── per-widget interval configuration
├── last-updated timestamp tracking
└── poll() — updates all widgets whose interval has elapsed, returns changed WidgetData
```

### 4.4 config.rs — TOML Configuration

Config is loaded from `~/.config/frames/config.toml`. The structure:

```
FramesConfig
├── bar: BarConfig           ← position, height, monitor, CSS path
└── widgets: Vec<WidgetConfig>  ← ordered list of widget definitions
    └── kind: WidgetKind     ← internal serde tag; one of 13 typed variants
        ├── Clock(ClockConfig)
        ├── Cpu(CpuConfig)
        ├── ...
        └── Separator(SeparatorConfig)
```

`WidgetConfig` holds the common fields (`position`, `interval`, `label`, `on_click`, etc.) shared by all widgets, plus a `kind: WidgetKind` field that carries the widget-specific fields. `WidgetKind` is an internally-tagged serde enum (`#[serde(tag = "type")]`): the `type` key in TOML selects the variant and each variant struct has `#[serde(deny_unknown_fields)]` so misplaced fields are rejected at parse time rather than silently ignored.

The `WidgetKind` enum drives dispatch in `frames_bar::main::build_widget()` — the match exhaustiveness guarantees every widget type has a renderer.

See CONFIG_MODEL.md for full field documentation.

### 4.5 error.rs — Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum FramesError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("sysinfo error: {0}")]
    SysInfo(String),

    #[error("battery read error: {0}")]
    Battery(#[from] std::io::Error),

    #[error("http error: {0}")]
    Http(String),

    #[error("dbus error: {0}")]
    DBus(String),

    #[error("widget not found: {name}")]
    WidgetNotFound { name: String },
}
```

---

## 5. frames_bar — Module Structure

```
frames_bar/
├── src/
│   ├── main.rs         ← CLI entry point: GTK init, config load, Bar construction, gtk::main()
│   ├── bar.rs          ← Bar struct: GtkWindow + X11 EWMH setup + multi-monitor support
│   ├── widgets/        ← Per-widget GTK3 renderers (consume WidgetData, produce GtkWidgets)
│   │   ├── mod.rs
│   │   ├── clock.rs    ← GtkLabel, updated on each tick
│   │   ├── cpu.rs      ← GtkLabel or GtkProgressBar
│   │   ├── memory.rs   ← GtkLabel or GtkProgressBar
│   │   ├── network.rs  ← GtkLabel with rx/tx display
│   │   ├── battery.rs  ← GtkLabel with icon
│   │   ├── disk.rs     ← GtkLabel renderer; shows used/total or percent display
│   │   ├── volume.rs   ← GtkLabel renderer; shows 🔊/🔇 icon, volume %, .muted CSS class
│   │   ├── brightness.rs ← GtkLabel renderer; shows ☀ icon and brightness %
│   │   ├── weather.rs  ← GtkLabel renderer; shows WMO icon, temperature, wind speed
│   │   ├── media.rs    ← GtkLabel renderer; shows playback icon, title, and artist
│   │   └── workspaces.rs ← GtkBox of workspace buttons (queries X11 EWMH _NET_NUMBER_OF_DESKTOPS)
│   │   └── launcher.rs ← GtkButton + GtkPopover app search (bypasses Poller; self-contained GTK signals)
│   ├── css.rs          ← CssProvider loading: user theme + built-in fallback
│   └── config.rs       ← Config hot-reload via notify watcher
```

**UI rules:**
- No business logic in GTK renderers — all data computation lives in `frames_core`.
- Every renderer receives `&WidgetData` and updates its GTK widget accordingly.
- GTK main thread must never block — all `frames_core` polling is non-blocking.

> **Full GTK3 conventions, CSS rules, and X11 EWMH setup:** [UI_GUIDE.md](UI_GUIDE.md)

---

## 6. Data Flow

### 6.1 Startup Sequence

```
main()
    │
    ▼
Load FramesConfig from ~/.config/frames/config.toml
    │
    ▼
gtk::init() — initialize GTK3
    │
    ▼
Bar::new(config) — create GtkWindow, set _NET_WM_WINDOW_TYPE_DOCK
    │
    ▼
Set _NET_WM_STRUT_PARTIAL — reserve screen space
    │
    ▼
Instantiate widget renderers from config.widgets list
    │
    ▼
Instantiate Poller with configured intervals
    │
    ▼
Register glib::timeout_add_local — drives Poller::poll() on each tick
    │
    ▼
Load CSS theme via CssProvider
    │
    ▼
gtk::main() — event loop
```

### 6.2 Widget Update Cycle

```
glib timer fires
    │
    ▼
Poller::poll() — calls Widget::update() for each due widget
    │
    ▼
Returns Vec<(widget_name, WidgetData)> for changed widgets
    │
    ▼
frames_bar: for each changed widget, call renderer.update(data)
    │
    ▼
Renderer updates GtkLabel/GtkProgressBar text/value
    │
    ▼
GTK3 queues expose event — screen redraws
```

---

## 7. Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| Fedora 40+ | Primary | Development platform, Cinnamon available in repos |
| Linux Mint 21+ | Primary target | Cinnamon's home distro |
| Ubuntu 24.04 | Supported | Cinnamon installable |
| Arch Linux | Supported | Rolling, current GTK3 |
| Other GTK3/X11 desktops | Best effort | Any EWMH-compliant WM should work |

> **Full distro matrix and X11 requirements:** [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md)

---

## 8. Key Design Decisions

### 8.1 GTK3, Not GTK4

Cinnamon uses GTK3. A GTK4 bar would require running in a separate process and communicating across toolkit boundaries. GTK3 is the natural choice — same toolkit, same CSS system, same theming as Cinnamon itself.

### 8.2 X11 Only (for now)

Cinnamon does not support Wayland as of this writing. Frames targets X11 exclusively. Wayland support (via layer-shell) is tracked in `DOCS/futures.md` as a future consideration.

### 8.3 No Daemon / IPC

Frames is a single process. There is no daemon, no IPC socket, and no reload protocol. Config hot-reload is handled by watching the file with `notify` and restarting the relevant widgets. A full restart is acceptable for major config changes.

### 8.4 sysinfo for System Stats

`sysinfo` provides cross-platform CPU, RAM, and network stats in pure Rust. No shell out to `top`, `vmstat`, or `/proc` parsing by hand. The crate handles caching and refresh intervals internally.

---

## 9. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Rust coding conventions | [CODING_STANDARDS.md](CODING_STANDARDS.md) |
| Widget trait and data contract | [WIDGET_API.md](WIDGET_API.md) |
| Bar window and X11 design | [BAR_DESIGN.md](BAR_DESIGN.md) |
| TOML config structure | [CONFIG_MODEL.md](CONFIG_MODEL.md) |
| Build prerequisites and steps | [BUILD_GUIDE.md](BUILD_GUIDE.md) |
| Test suite structure | [TESTING_GUIDE.md](TESTING_GUIDE.md) |
| Distro and X11 support | [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) |
| GTK3 and UI conventions | [UI_GUIDE.md](UI_GUIDE.md) |
