# Parapet — Config Model

> **Scope:** TOML configuration file structure, field definitions, defaults, validation rules, and config file location.
> **Last Updated:** Jan 11, 2025

---

## 1. Overview

Parapet is configured entirely via a single TOML file. There is no database, no registry, and no binary config format. The config file is human-editable and version-controllable.

**Config file location:** `~/.config/parapet/config.toml`

Override with the `PARAPET_CONFIG` environment variable:
```bash
PARAPET_CONFIG=/path/to/config.toml parapet_bar
```

The config file is watched for changes at runtime. When the file is modified, affected widgets are reloaded without restarting the bar.

**Hot-reload:** The config file is watched via `notify`. When the file is modified, `parapet_bar` reloads the config and rebuilds the widget tree without restarting the process. A 500 ms debounce prevents thrashing on rapid saves.

---

## 2. Top-Level Structure

```toml
# ~/.config/parapet/config.toml

[bar]
position = "top"        # "top" | "bottom"
height = 30             # pixels
monitor = "primary"     # "primary" | integer index
css = "~/.config/parapet/parapet.css"   # optional path to custom CSS

[[widgets]]
type = "workspaces"
position = "left"

[[widgets]]
type = "clock"
position = "center"
format = "%H:%M"

[[widgets]]
type = "cpu"
position = "right"
interval = 2000

[[widgets]]
type = "memory"
position = "right"

[[widgets]]
type = "network"
position = "right"
interface = "auto"

[[widgets]]
type = "battery"
position = "right"
```

---

## 3. [bar] Section

Global bar configuration. All fields have defaults — `[bar]` is optional.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `position` | `"top"` \| `"bottom"` | `"top"` | Screen edge where the bar is anchored |
| `height` | integer (pixels) | `30` | Bar height in pixels |
| `monitor` | `"primary"` \| integer | `"primary"` | Monitor to display on. Integer is 0-based GDK monitor index |
| `css` | string (path) | `None` | Path to user CSS file. `~` is expanded. If absent, built-in default theme is used |
| `theme` | string | `None` | Named theme to load from `~/.config/parapet/themes/<name>.css`. Takes precedence over `css`. See `DOCS/theme-spec.md` |
| `widget_spacing` | integer (pixels) | `4` | Pixel gap between adjacent widgets |

```toml
[bar]
position = "top"
height = 28
monitor = "primary"
css = "~/.config/parapet/parapet.css"
widget_spacing = 6
```

---

## 4. [[widgets]] Entries

Each `[[widgets]]` entry defines one widget. Widgets are rendered in config order within their section (left/center/right).

### 4.1 Common Fields

All widget entries share these fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | ✅ | Widget type identifier (see §4.2) |
| `position` | `"left"` \| `"center"` \| `"right"` | ✅ | Which bar section to place this widget in |
| `interval` | integer (ms) | no | Polling interval in milliseconds. Default varies by widget type |
| `label` | string | no | Static prefix text displayed before the widget value |
| `on_click` | string | no | Shell command spawned via `sh -c` on left mouse button press |
| `on_scroll_up` | string | no | Shell command spawned via `sh -c` on scroll-wheel up event |
| `on_scroll_down` | string | no | Shell command spawned via `sh -c` on scroll-wheel down event |
| `extra_class` | string | no | Extra CSS class applied to the widget's root GTK container. Allows per-instance theme targeting without modifying widget code |

### 4.2 Widget Types

Each `type` value maps to a variant of the `WidgetKind` enum in `parapet_core`. The `type` key is an internal serde tag — each variant wraps a dedicated struct that holds **only** the fields valid for that widget. Fields from another widget type (e.g. `latitude` on a `clock` widget) are **rejected at parse time** by `#[serde(deny_unknown_fields)]` on each variant struct. This means config errors surface immediately with a clear TOML parse error rather than at runtime.

| `type` value | Rust struct | Description | Default interval |
|-------------|-------------|-------------|------------------|
| `"clock"` | `ClockConfig` | Date and time display | 1000 ms |
| `"cpu"` | `CpuConfig` | CPU usage percentage | 2000 ms |
| `"memory"` | `MemoryConfig` | RAM and swap usage | 3000 ms |
| `"network"` | `NetworkConfig` | Network rx/tx speed | 2000 ms |
| `"battery"` | `BatteryConfig` | Battery charge and status | 5000 ms |
| `"disk"` | `DiskConfig` | Filesystem usage for a mount point | 30 000 ms |
| `"workspaces"` | `WorkspacesConfig` | Clickable workspace switcher | Event-driven (no poll) |
| `"launcher"` | `LauncherConfig` | Application launcher popup | Event-driven (no poll) |
| `"volume"` | `VolumeConfig` | PulseAudio/PipeWire output volume and mute state | 2000 ms |
| `"brightness"` | `BrightnessConfig` | Screen backlight brightness | 5000 ms |
| `"weather"` | `WeatherConfig` | Current weather conditions from Open-Meteo API | 1 800 000 ms (30 min) |
| `"media"` | `MediaConfig` | Currently playing track via MPRIS2 D-Bus | 2000 ms |
| `"separator"` | `SeparatorConfig` | Visual divider between widgets | — |

### 4.3 Clock Widget Fields

```toml
[[widgets]]
type = "clock"
position = "center"
format = "%H:%M"          # strftime-compatible format string
timezone = "local"        # "local" or IANA timezone name e.g. "America/New_York"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | string | `"%H:%M"` | `chrono::format` pattern string |
| `timezone` | string | `"local"` | `"local"` or IANA timezone name |

### 4.4 CPU Widget Fields

```toml
[[widgets]]
type = "cpu"
position = "right"
interval = 2000
show_per_core = false     # show per-core breakdown (future)
warn_threshold = 80.0     # add .warning CSS class above this %
crit_threshold = 95.0     # add .critical CSS class above this %
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `warn_threshold` | float (0–100) | `80.0` | CPU% above which the `.warning` CSS class is applied |
| `crit_threshold` | float (0–100) | `95.0` | CPU% above which the `.critical` CSS class is applied |

### 4.5 Memory Widget Fields

```toml
[[widgets]]
type = "memory"
position = "right"
interval = 3000
format = "used"           # "used" | "free" | "percent"
show_swap = false
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `format` | `"used"` \| `"free"` \| `"percent"` | `"used"` | How to display memory: used bytes, free bytes, or percentage |
| `show_swap` | bool | `false` | Include swap usage in display |

### 4.6 Network Widget Fields

```toml
[[widgets]]
type = "network"
position = "right"
interval = 2000
interface = "auto"        # "auto" (first non-loopback) | interface name e.g. "eth0"
show_interface = false    # prefix display with interface name
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interface` | string | `"auto"` | Network interface to monitor. `"auto"` picks the first non-loopback active interface |
| `show_interface` | bool | `false` | Whether to show the interface name in the widget display |

### 4.7 Battery Widget Fields

```toml
[[widgets]]
type = "battery"
position = "right"
interval = 5000
show_icon = true
warn_threshold = 20.0
crit_threshold = 5.0
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `show_icon` | bool | `true` | Show a Unicode battery icon |
| `warn_threshold` | float (0–100) | `20.0` | Charge% below which the `.warning` CSS class is applied |
| `crit_threshold` | float (0–100) | `5.0` | Charge% below which the `.critical` CSS class is applied |

### 4.8 Workspaces Widget Fields

```toml
[[widgets]]
type = "workspaces"
position = "left"
show_names = true         # show workspace name if set; otherwise show number
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `show_names` | bool | `true` | Display workspace names (from `_NET_DESKTOP_NAMES`) or just numbers |

### 4.9 Volume Widget Fields

```toml
[[widgets]]
type = "volume"
position = "right"
interval = 2000
show_icon = true
on_click = "pavucontrol"
on_scroll_up = "pactl set-sink-volume @DEFAULT_SINK@ +5%"
on_scroll_down = "pactl set-sink-volume @DEFAULT_SINK@ -5%"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `show_icon` | bool | `true` | Show a Unicode speaker icon (🔊/🔇). Set `false` to suppress the icon |
| `on_click` | string | — | Shell command run on left-click (inherited from §4.1) |
| `on_scroll_up` | string | — | Shell command run on scroll up (inherited from §4.1) |
| `on_scroll_down` | string | — | Shell command run on scroll down (inherited from §4.1) |

Data source: `pactl get-sink-volume @DEFAULT_SINK@` and `pactl get-sink-mute @DEFAULT_SINK@`. Falls back to last known value if `pactl` is unavailable.

### 4.10 Brightness Widget Fields

```toml
[[widgets]]
type = "brightness"
position = "right"
interval = 5000
show_icon = true
on_scroll_up = "brightnessctl set +10%"
on_scroll_down = "brightnessctl set 10%-"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `show_icon` | bool | `true` | Show a Unicode sun icon (☀). Set `false` to suppress the icon |
| `on_scroll_up` | string | — | Shell command run on scroll up (inherited from §4.1) |
| `on_scroll_down` | string | — | Shell command run on scroll down (inherited from §4.1) |

Data source: `/sys/class/backlight/<first-entry>/brightness` and `/sys/class/backlight/<first-entry>/max_brightness`. Returns `brightness_pct: 0.0` on machines without a variable backlight (e.g. desktop PCs).

### 4.11 Weather Widget Fields

```toml
[[widgets]]
type = "weather"
position = "right"
interval = 1800000        # 30 minutes; Open-Meteo free tier allows 10 000 calls/day
latitude = 51.5           # WGS-84 latitude
longitude = -0.1          # WGS-84 longitude
units = "celsius"         # "celsius" | "fahrenheit"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `latitude` | float | `0.0` | WGS-84 geographic latitude for the forecast location |
| `longitude` | float | `0.0` | WGS-84 geographic longitude for the forecast location |
| `units` | `"celsius"` \| `"fahrenheit"` | `"celsius"` | Temperature unit used for display and API request |

Data source: `https://api.open-meteo.com/v1/forecast` (no API key required). Fetches `temperature_2m`, `weather_code`, `wind_speed_10m`, and `relative_humidity_2m` from the `current` block. On HTTP failure the last successfully fetched value is displayed rather than clearing the widget. No credentials are stored or transmitted.

Display format: `"{icon} {temperature:.0}°{C|F} 💨{wind_speed:.0}"` e.g. `"⛅ 12°C 💨18"`.

### 4.12 Media Widget Fields

```toml
[[widgets]]
type = "media"
position = "center"
interval = 2000
on_click = "playerctl play-pause"
on_scroll_up = "playerctl next"
on_scroll_down = "playerctl previous"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `on_click` | string | — | Shell command run on left-click. Use `playerctl play-pause` to toggle playback |
| `on_scroll_up` | string | — | Shell command run on scroll up. Use `playerctl next` to skip forward |
| `on_scroll_down` | string | — | Shell command run on scroll down. Use `playerctl previous` to go back |

Data source: `org.mpris.MediaPlayer2.Player` interface on the D-Bus session bus. The widget connects lazily on the first poll cycle and reuses the connection. If no MPRIS2 player is active the widget displays nothing (empty label). Play/Pause, Next, and Previous controls are intentionally **not** built as GTK buttons inside the renderer — they are wired via the standard `on_click` / `on_scroll_up` / `on_scroll_down` fields to `playerctl` shell commands, keeping the renderer purely display-oriented.

### 4.13 Disk Widget Fields

```toml
[[widgets]]
type = "disk"
position = "right"
interval = 30000       # 30 seconds; disk stats rarely change rapidly
mount = "/"            # filesystem mount point to monitor
format = "used"        # "used" | "percent" | "free"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mount` | string | `"/"` | Filesystem mount point to report usage for |
| `format` | `"used"` \| `"percent"` \| `"free"` | `"used"` | Display format: used/total sizes, percentage used, or free space |

Data source: `sysinfo::Disks`. Calls `refresh_list()` on each poll tick. When the configured mount point is not found in the disk list, the widget displays zero values and logs a `WARN` rather than failing.

Display formats:
- `"used"` (default): `"DISK 45.2/931.5 GiB"`
- `"percent"`: `"DISK 5%"`
- `"free"`: `"DISK free 886.3 GiB"`

---

## 5. Config Validation Rules

Config loading uses a two-stage gate:

**Stage 1 — Parse-time (serde):**
- `type` must be one of the 13 known widget types (see §4.2 table). An unknown or missing `type` key causes an immediate `toml::de::Error`.
- `position` must be `"left"`, `"center"`, or `"right"`. Serde rejects other values.
- Widget-specific fields are validated structurally: each `WidgetKind` variant wraps a dedicated struct annotated with `#[serde(deny_unknown_fields)]`. A field that belongs to a different widget type (e.g. `latitude` on a `clock` widget) is **rejected immediately** with a clear parse error, before `validate()` ever runs.

**Stage 2 — `ParapetConfig::validate()` (semantic, called after parse):**

1. `bar.height` must be > 0
2. `interval`, when present, must be > 0 (a value of 0 causes a tight polling busy-loop)
3. For `WidgetKind::Cpu` and `WidgetKind::Battery`: threshold fields must be in range 0.0–100.0; for CPU `warn < crit`; for battery `crit < warn`
4. For `WidgetKind::Weather`, `latitude` (when present) must be in −90.0–+90.0; `longitude` (when present) must be in −180.0–+180.0
5. For `WidgetKind::Disk`, `mount` (when present) must be an absolute path starting with `/` after `~` / `$HOME` expansion

`validate()` also performs `~` / `$HOME` path expansion **in-place** on `bar.css`, `bar.theme`, and disk widget `mount` fields so downstream code always receives expanded absolute paths.

Validation errors are reported via `ParapetConfigError::Validation { field, reason }` and cause the bar to exit with a user-readable message.

---

## 6. Complete Example Config

```toml
[bar]
position = "top"
height = 28
monitor = "primary"
css = "~/.config/parapet/parapet.css"
widget_spacing = 4

[[widgets]]
type = "workspaces"
position = "left"
show_names = true

[[widgets]]
type = "cpu"
position = "right"
interval = 2000
warn_threshold = 80.0
crit_threshold = 95.0

[[widgets]]
type = "memory"
position = "right"
interval = 3000
format = "percent"

[[widgets]]
type = "network"
position = "right"
interval = 2000
interface = "auto"

[[widgets]]
type = "battery"
position = "right"
interval = 10000

[[widgets]]
type = "clock"
position = "center"
format = "%a %b %d  %H:%M"
```

---

## 7. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Config module in parapet_core | [ARCHITECTURE.md §4.4](ARCHITECTURE.md) |
| Widget data types | [WIDGET_API.md](WIDGET_API.md) |
| Config round-trip tests | [TESTING_GUIDE.md §6](TESTING_GUIDE.md) |
