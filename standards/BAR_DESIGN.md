# Parapet — Bar Design

> **Scope:** Bar window design, X11 EWMH property setup, widget layout system, multi-monitor behavior, and CSS architecture.
> **Last Updated:** Mar 17, 2026

---

## 1. Overview

The Parapet bar is a single GTK3 window positioned at the top or bottom of the screen. It reserves screen space via X11 EWMH strut properties so that other windows maximize correctly around it.

**Design goals:**
- Zero overlap with maximized windows
- Single-process, no daemon
- Pixel-precise positioning on all monitors
- CSS-driven appearance — no hardcoded colors or sizes in Rust code
- Correct teardown on SIGTERM — no orphaned strut properties

---

## 2. Window Properties

### 2.1 GTK Window Configuration

The bar window is created with these properties set before it is shown:

| Property | Value | Reason |
|----------|-------|--------|
| `WindowType` | `Popup` | Bypasses normal WM decoration and placement |
| `TypeHint` | `Dock` | X11: `_NET_WM_WINDOW_TYPE_DOCK` |
| `decorated` | `false` | No title bar or borders |
| `resizable` | `false` | Fixed to screen width |
| `skip_taskbar_hint` | `true` | Not in window switcher |
| `skip_pager_hint` | `true` | Not in pager |
| `app_paintable` | `true` (if using RGBA) | Allow background transparency |

### 2.2 X11 Window Properties Set After Realize

These properties require the GDK window to be realized (have an actual X11 window ID):

| Property | EWMH Atom | Value |
|----------|-----------|-------|
| Window type | `_NET_WM_WINDOW_TYPE` | `_NET_WM_WINDOW_TYPE_DOCK` |
| Strut partial | `_NET_WM_STRUT_PARTIAL` | 12 cardinal values (see §3) |
| State | `_NET_WM_STATE` | `_NET_WM_STATE_STICKY`, `_NET_WM_STATE_ABOVE` |

---

## 3. Strut Property (_NET_WM_STRUT_PARTIAL)

The strut tells compliant window managers to reserve space for the bar. Without it, maximized windows overlap the bar.

### 3.1 Strut Value Layout

`_NET_WM_STRUT_PARTIAL` is a 12-element array of 32-bit cardinals:

```
[0]  left          — reserved pixels on left edge
[1]  right         — reserved pixels on right edge
[2]  top           — reserved pixels on top edge
[3]  bottom        — reserved pixels on bottom edge
[4]  left_start_y  — y coordinate where left strut starts
[5]  left_end_y    — y coordinate where left strut ends
[6]  right_start_y — y coordinate where right strut starts
[7]  right_end_y   — y coordinate where right strut ends
[8]  top_start_x   — x coordinate where top strut starts
[9]  top_end_x     — x coordinate where top strut ends
[10] bottom_start_x — x coordinate where bottom strut starts
[11] bottom_end_x   — x coordinate where bottom strut ends
```

### 3.2 Values for Top Bar

For a bar at the top of monitor with geometry `(x, y, width, height)` and bar height `H`:

```
left=0, right=0, top=H, bottom=0,
left_start_y=0, left_end_y=0,
right_start_y=0, right_end_y=0,
top_start_x=x, top_end_x=x+width,
bottom_start_x=0, bottom_end_x=0
```

### 3.3 Values for Bottom Bar

For a bar at the bottom with screen total height `S`:

```
left=0, right=0, top=0, bottom=H,
left_start_y=0, left_end_y=0,
right_start_y=0, right_end_y=0,
top_start_x=0, top_end_x=0,
bottom_start_x=x, bottom_end_x=x+width
```

### 3.4 Multi-Monitor Struts

On multi-monitor setups, each monitor's strut uses that monitor's x-offset in `top_start_x`/`top_end_x`. This scopes the strut to that monitor's column and avoids reserving space on adjacent monitors.

### 3.5 Setting the Strut

The strut must be set after `realize` and after any monitor geometry change:

```rust
fn apply_strut(gdk_window: &gdk::Window, bar: &BarConfig, geom: &gdk::Rectangle) {
    let strut = build_strut_array(bar.position, bar.height, geom);
    let display = gdk_window.display();

    // Use XChangeProperty via unsafe FFI or gdk_x11 property helpers
    // The exact call depends on what gtk-rs exposes for this platform
    set_cardinal_property(gdk_window, "_NET_WM_STRUT_PARTIAL", &strut);
    set_cardinal_property(gdk_window, "_NET_WM_STRUT", &strut[..4]); // legacy compat
}
```

Both `_NET_WM_STRUT_PARTIAL` and the older `_NET_WM_STRUT` should be set for maximum WM compatibility.

---

## 4. Positioning

### 4.1 Startup Positioning

Position and size the window in the `realize` signal handler, not before:

```
window.connect_realize(|win| {
    let screen = win.screen();
    let monitor_idx = config.monitor.resolve(&screen);
    let geom = screen.monitor_geometry(monitor_idx);

    win.move_(geom.x(), match bar.position {
        Top => geom.y(),
        Bottom => geom.y() + geom.height() - bar.height,
    });
    win.resize(geom.width(), bar.height);
    apply_strut(win.window(), &bar, &geom);
});
```

### 4.2 Multi-Monitor Geometry

Use `gdk::Screen::monitor_geometry(monitor_idx)` to get pixel coordinates. Do not use `screen.width()`/`screen.height()` for multi-monitor setups — those return the full combined display size.

---

## 5. Widget Layout System

### 5.1 Three-Section Model

The bar interior is divided into three horizontal sections:

```
┌────────────────────────────────────────────────────────────┐
│  [LEFT widgets]  │        [CENTER widgets]        │  [RIGHT widgets]  │
└────────────────────────────────────────────────────────────┘
```

Left and right sections are packed with `expand=false`. The center section expands to fill remaining space.

### 5.2 Widget Ordering

Widgets appear in the order they are defined in `config.toml` within their section. The first `[[widgets]]` entry with `position = "left"` is leftmost, etc.

### 5.3 Widget GTK Hierarchy

Each widget renderer produces a root widget (usually `GtkLabel` or `GtkBox`) wrapped in an event box for CSS targeting:

```
GtkEventBox (.widget .widget-{type})
└── GtkLabel or GtkBox (renderer-specific)
```

The event box allows padding and background via CSS and captures click events for workspace buttons.

---

## 6. CSS Architecture

### 6.1 Theme Loading Order

1. Built-in default CSS (`themes/default.css`, compiled in via `include_bytes!`)
2. User CSS file from `bar.css` config field (overrides built-in)

Both are loaded into the same `GtkCssProvider`. User CSS takes precedence via `STYLE_PROVIDER_PRIORITY_APPLICATION`.

### 6.2 CSS Targeting Strategy

CSS classes are the public API for theming. Targets in priority order:

```css
/* Target all widgets */
.widget { }

/* Target specific widget type */
.widget-clock { }
.widget-cpu { }

/* Target widget state */
.widget-cpu.warning { }
.widget-cpu.critical { }

/* Target bar sections */
.parapet-bar .parapet-left { }
.parapet-bar .parapet-right { }
```

Widget IDs (set via `set_widget_name()`) can also be used for unique per-instance targeting:
```css
#clock { font-weight: bold; }
```

---

## 7. Transparency

If a compositor is running, the bar can have a transparent or semi-transparent background:

```rust
// Enable RGBA visual on the window
if let Some(screen) = window.screen() {
    if let Some(visual) = screen.rgba_visual() {
        window.set_visual(Some(&visual));
    }
}
window.set_app_paintable(true);
```

Without this, the background is opaque regardless of CSS `rgba()` values.

---

## 8. Shutdown and Cleanup

When the bar exits (SIGTERM or window close), GTK cleans up the X11 window automatically. The strut property is removed when the window is destroyed — no manual cleanup required.

Signal handling:

```rust
// In main.rs — handle SIGTERM gracefully
let (tx, rx) = std::sync::mpsc::channel();
ctrlc::set_handler(move || { let _ = tx.send(()); }).ok();

// Poll for shutdown signal on each glib tick
glib::timeout_add_local(Duration::from_millis(100), move || {
    if rx.try_recv().is_ok() {
        gtk::main_quit();
        return glib::ControlFlow::Break;
    }
    glib::ControlFlow::Continue
});
```

---

## 9. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Bar module structure | [ARCHITECTURE.md §5](ARCHITECTURE.md) |
| GTK3 widget renderer rules | [UI_GUIDE.md](UI_GUIDE.md) |
| Platform EWMH requirements | [PLATFORM_COMPAT.md §3](PLATFORM_COMPAT.md) |
| Config for bar position | [CONFIG_MODEL.md §3](CONFIG_MODEL.md) |
