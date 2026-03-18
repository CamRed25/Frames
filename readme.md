# Frames

A Rust-native GTK3 status bar for the Cinnamon desktop on Linux.

Frames anchors a thin bar to the top or bottom of the screen and fills it with configurable widgets: clock, CPU, memory, network, battery, disk, volume, brightness, workspaces, a fuzzy application launcher, weather, and MPRIS2 media controls. It reserves screen space via `_NET_WM_STRUT_PARTIAL` so maximised windows don't overlap it.

**Status:** `v0.1.0-alpha` — active development on `master`.

---

## Features

- Workspace switcher with click-to-switch (sends `_NET_CURRENT_DESKTOP` via `wmctrl`)
- Application launcher popup with fuzzy search, keyboard navigation, and live app-list refresh via `gio::AppInfoMonitor`
- Audio volume via `pactl subscribe` (event-driven, no per-poll subprocess) with scroll-to-adjust
- Weather from the [Open-Meteo](https://open-meteo.com/) free API — no API key needed
- MPRIS2 media widget via D-Bus (`zbus`) — shows currently playing track and responds to `playerctl`
- Per-widget `on_click` / `on_scroll_up` / `on_scroll_down` shell command bindings
- Named CSS themes with automatic dark/light variant selection and hot-reload
- Full config hot-reload via `notify` — edit `config.toml`, changes apply instantly without restart

---

## Requirements

- Fedora / any Linux distribution with GTK3 (`>= 3.24`) and a Cinnamon (or EWMH-compliant) window manager
- `pactl` (PulseAudio or PipeWire) — required for the volume widget
- `wmctrl` — required for workspace click-to-switch
- Rust `>= 1.75` (MSRV)

---

## Build

```bash
cargo build --release
```

The binary is at `target/release/frames_bar`.

---

## Install

Copy the binary somewhere on `$PATH` and drop a config file at `~/.config/frames/config.toml`:

```bash
install -Dm755 target/release/frames_bar ~/.local/bin/frames_bar
mkdir -p ~/.config/frames
cp examples/config.toml ~/.config/frames/config.toml   # if provided
```

Then add `frames_bar &` to your Cinnamon startup commands (System Settings → Startup Applications).

---

## Configuration

Config file: `~/.config/frames/config.toml`  
Override with `FRAMES_CONFIG=/path/to/config.toml frames_bar`.

### Minimal example

```toml
[bar]
position = "top"
height = 28

[[widgets]]
type = "workspaces"
position = "left"

[[widgets]]
type = "clock"
position = "center"
format = "%a %d %b  %H:%M"

[[widgets]]
type = "cpu"
position = "right"

[[widgets]]
type = "memory"
position = "right"

[[widgets]]
type = "battery"
position = "right"
```

### [bar] fields

| Field | Default | Description |
|-------|---------|-------------|
| `position` | `"top"` | `"top"` or `"bottom"` |
| `height` | `30` | Bar height in pixels |
| `monitor` | `"primary"` | `"primary"` or a 0-based GDK monitor index |
| `theme` | — | Named theme from `~/.config/frames/themes/<name>.css` |
| `css` | — | Absolute path to a CSS file (overridden by `theme`) |
| `widget_spacing` | `4` | Pixel gap between widgets |

### Widget types

| `type` | Description | Default interval |
|--------|-------------|-----------------|
| `clock` | Date/time (`format`, `timezone`) | 1 s |
| `cpu` | CPU usage (`warn_threshold`, `crit_threshold`) | 2 s |
| `memory` | RAM/swap (`format`, `show_swap`) | 3 s |
| `network` | RX/TX speed (`interface`, `show_interface`) | 2 s |
| `battery` | Charge and status (`show_icon`, `warn_threshold`, `crit_threshold`) | 5 s |
| `disk` | Filesystem usage (`mount`, `format`) | 30 s |
| `volume` | PulseAudio/PipeWire volume (`show_icon`) | event-driven |
| `brightness` | Backlight (`show_icon`) | 5 s |
| `workspaces` | Clickable workspace buttons (`show_names`) | event-driven |
| `launcher` | Fuzzy app launcher popup | — |
| `weather` | Current conditions (`latitude`, `longitude`, `units`) | 30 min |
| `media` | MPRIS2 now playing | 2 s |
| `separator` | Visual divider (`format` for glyph, default `"\|"`) | — |

All widget entries accept common fields: `interval`, `label`, `on_click`, `on_scroll_up`, `on_scroll_down`, `extra_class`.

### Full volume widget example

```toml
[[widgets]]
type = "volume"
position = "right"
show_icon = true
on_click = "pavucontrol"
on_scroll_up = "pactl set-sink-volume @DEFAULT_SINK@ +5%"
on_scroll_down = "pactl set-sink-volume @DEFAULT_SINK@ -5%"
```

---

## Theming

Themes are CSS files located at `~/.config/frames/themes/<name>.css`. Select a theme with `theme = "mytheme"` in `[bar]`. Dark/light variants are picked up automatically if `mytheme-dark.css` / `mytheme-light.css` exist, following the system GTK preference.

Six named colour tokens are available in the default theme and should be used in custom themes:

```css
@define-color color-pill    #1e1e2e;
@define-color color-fg      #cdd6f4;
@define-color color-fg-dim  #6c7086;
@define-color color-accent  #89b4fa;
@define-color color-warning #f9e2af;
@define-color color-urgent  #f38ba8;
```

---

## Architecture

```
frames_core   — pure library: widget traits, system info polling, config
frames_bar    — GTK3 binary: bar window, widget renderers, X11 EWMH
```

`frames_core` has **no dependency on GTK, GDK, X11, or any display system**. It can be built and tested without a display server. All display logic lives in `frames_bar`.

See [`standards/ARCHITECTURE.md`](standards/ARCHITECTURE.md) for full design documentation.

---

## Development

```bash
# Build
cargo build --workspace

# Clippy (enforced, -D warnings)
cargo clippy --workspace -- -D warnings

# Tests (headless — no display required)
cargo test --workspace --no-default-features

# Full test suite
cargo test --workspace
```

Standards documents are in [`standards/`](standards/). Read [`standards/RULE_OF_LAW.md`](standards/RULE_OF_LAW.md) first.

---

## License

GPL-3.0-or-later
