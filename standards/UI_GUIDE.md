# Parapet — UI Guide

> **Scope:** GTK3 conventions, CSS theming, widget renderer rules, X11 EWMH window setup, and the boundary between widget data and widget rendering.
> **Last Updated:** Mar 17, 2026

---

## 1. Overview

Parapet uses GTK3 for its UI layer. The bar is a single GtkWindow anchored to the screen edge via X11 EWMH properties. Widgets are GTK3 widgets arranged in a horizontal GtkBox.

**UI layer responsibilities:**
- Consume `WidgetData` from `parapet_core` and render it as GTK3 widgets
- Handle X11 EWMH window type and strut setup
- Apply CSS theming
- Register glib timers that drive the polling loop

**UI layer is NOT responsible for:**
- System information collection (CPU %, RAM, network)
- Config parsing
- Business logic of any kind
- Any operation that blocks the GTK main thread

All data computation lives in `parapet_core`. Renderers in `parapet_bar` display data and handle GTK events only.

---

## 2. Technology Stack

| Library | Version | Purpose |
|---------|---------|---------|
| `gtk` crate | ~0.18 | GTK3 bindings |
| `gdk` crate | ~0.18 | GDK (display backend) |
| `glib` crate | ~0.18 | GLib event loop, timers, idle callbacks |

**Rule:** Use native GTK3 components before writing custom drawing code. `GtkLabel`, `GtkProgressBar`, `GtkBox`, and `GtkButton` cover most status bar widget needs.

---

## 3. Bar Window Setup

### 3.1 Window Creation

```rust
// bar.rs
let window = gtk::Window::new(gtk::WindowType::Popup);
window.set_title("parapet");
window.set_decorated(false);
window.set_resizable(false);
window.set_skip_taskbar_hint(true);
window.set_skip_pager_hint(true);
window.set_type_hint(gdk::WindowTypeHint::Dock);
```

`gtk::WindowType::Popup` combined with `gdk::WindowTypeHint::Dock` tells the window manager this is a panel, not a normal application window.

### 3.2 Positioning

Position the bar explicitly after the window is realized:

```rust
window.connect_realize(move |win| {
    let screen = win.screen().expect("window must have a screen");
    let monitor = screen.primary_monitor();
    let geom = screen.monitor_geometry(monitor);

    // Top of screen
    win.move_(geom.x(), geom.y());
    win.resize(geom.width(), config.bar.height);

    // Set EWMH strut
    set_strut(win, &config.bar, &geom);
});
```

Do not position before the window is realized — GDK does not have an X11 window handle yet.

### 3.3 X11 EWMH Strut (_NET_WM_STRUT_PARTIAL)

The strut tells the window manager to reserve space for the bar, preventing maximized windows from overlapping it:

```rust
fn set_strut(window: &gtk::Window, bar: &BarConfig, geom: &gdk::Rectangle) {
    use gdk::prelude::*;

    let gdk_window = window.window().expect("window must be realized");

    // _NET_WM_STRUT_PARTIAL: left, right, top, bottom,
    //   left_start_y, left_end_y, right_start_y, right_end_y,
    //   top_start_x, top_end_x, bottom_start_x, bottom_end_x
    let strut: [u32; 12] = match bar.position {
        BarPosition::Top => [
            0, 0, bar.height as u32, 0,
            0, 0, 0, 0,
            geom.x() as u32, (geom.x() + geom.width()) as u32, 0, 0,
        ],
        BarPosition::Bottom => [
            0, 0, 0, bar.height as u32,
            0, 0, 0, 0,
            0, 0, geom.x() as u32, (geom.x() + geom.width()) as u32,
        ],
    };

    gdk_window.set_utf8_property("_NET_WM_STRUT_PARTIAL", &strut_to_string(&strut));
    // Or use xlib directly via unsafe FFI if set_utf8_property is insufficient.
}
```

> See BAR_DESIGN.md §3 for the full strut property specification and multi-monitor behavior.

### 3.4 Window Type Hint

The window type must be set before the window is shown:

```rust
window.set_type_hint(gdk::WindowTypeHint::Dock);
```

This prevents Cinnamon from listing the bar in window switchers or applying tiling rules to it.

---

## 4. Layout

### 4.1 Bar Layout Structure

```
GtkWindow (Popup, Dock)
└── GtkBox (horizontal, full width)
    ├── GtkBox (left section, expand=false)
    │   └── [left widget renderers in config order]
    ├── GtkBox (center section, expand=true, fill=true)
    │   └── [center widget renderers]
    └── GtkBox (right section, expand=false)
        └── [right widget renderers in config order]
```

```rust
let bar_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);

let left = gtk::Box::new(gtk::Orientation::Horizontal, 4);
let center = gtk::Box::new(gtk::Orientation::Horizontal, 4);
let right = gtk::Box::new(gtk::Orientation::Horizontal, 4);

bar_box.pack_start(&left, false, false, 0);
bar_box.set_center_widget(Some(&center));
bar_box.pack_end(&right, false, false, 0);
```

### 4.2 Widget Spacing

Inter-widget spacing is configured globally in `bar.widget_spacing` (pixels). Per-widget margins are set via CSS classes. Do not hardcode pixel values in Rust code — use CSS.

---

## 5. Widget Renderers

### 5.1 Renderer Contract

Each widget renderer in `parapet_bar/src/widgets/` must:

1. Implement a `new(config: &WidgetConfig) -> anyhow::Result<Self>` constructor
2. Expose a `widget(&self) -> &gtk::Widget` method returning the root GTK widget to embed in the bar
3. Implement `update(&self, data: &WidgetData)` to apply new data to the GTK widget

```rust
pub struct ClockWidget {
    label: gtk::Label,
}

impl ClockWidget {
    pub fn new(config: &WidgetConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(None);
        label.set_widget_name("clock");
        label.get_style_context().add_class("widget");
        label.get_style_context().add_class("widget-clock");
        Ok(Self { label })
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Clock { display } = data {
            self.label.set_text(display);
        }
    }
}
```

### 5.2 No Logic in Renderers

Widget renderers must not compute data. They receive `WidgetData` and apply it:

```rust
// Forbidden — logic in renderer
fn update(&self, data: &WidgetData) {
    if let WidgetData::Cpu { usage_pct, .. } = data {
        let color = if *usage_pct > 80.0 { "red" } else { "green" }; // <-- logic
        self.label.set_markup(&format!("<span color='{}'>{:.0}%</span>", color, usage_pct));
    }
}

// Correct — formatting only; color logic belongs in CSS or parapet_core
fn update(&self, data: &WidgetData) {
    if let WidgetData::Cpu { usage_pct, .. } = data {
        self.label.set_text(&format!("{:.0}%", usage_pct));
        // High CPU CSS class applied based on threshold, set by parapet_core output
        let ctx = self.label.get_style_context();
        if *usage_pct > 80.0 {
            ctx.add_class("warning");
        } else {
            ctx.remove_class("warning");
        }
    }
}
```

---

## 6. CSS Theming

### 6.1 Loading CSS

Theme loading is handled by `css.rs`. The `ThemeSource` enum describes where
the CSS should come from. Priority chain: `--theme` CLI flag > `bar.theme`
config field > `bar.css` raw path > built-in default.

```rust
// css.rs
pub enum ThemeSource<'a> {
    Named(&'a str),   // ~/.config/parapet/themes/<name>.css
    Path(&'a Path),   // raw path from bar.css config field
    Default,          // compiled-in built-in theme
}

pub fn load_theme(source: ThemeSource<'_>) -> gtk::CssProvider { ... }

pub fn apply_provider(provider: &gtk::CssProvider) {
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("default screen must exist"),
        provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn remove_provider(provider: &gtk::CssProvider) {
    gtk::StyleContext::remove_provider_for_screen(
        &gdk::Screen::default().expect("default screen must exist"),
        provider,
    );
}
```

The provider is stored in `Rc<RefCell<gtk::CssProvider>>` in `main.rs` so it
can be replaced during CSS hot-reload without provider stacking. Always call
`remove_provider` before `apply_provider` when replacing the active theme.

### 6.2 CSS Class Naming

All bar and widget elements have stable CSS class names. These are the CSS API surface — do not rename them without updating this standard.

| Element | CSS Class |
|---------|-----------|
| Bar root box | `.parapet-bar` |
| Left section | `.parapet-left` |
| Center section | `.parapet-center` |
| Right section | `.parapet-right` |
| Any widget container | `.widget` |
| Clock widget | `.widget-clock` |
| CPU widget | `.widget-cpu` |
| Memory widget | `.widget-memory` |
| Network widget | `.widget-network` |
| Battery widget | `.widget-battery` |
| Volume widget | `.widget-volume` |
| Brightness widget | `.widget-brightness` |
| Workspace button (inactive) | `.workspace` |
| Workspace button (active) | `.workspace.active` |
| Launcher button | `.widget-launcher` |
| Launcher popover | `.launcher-popover` |
| Launcher search entry | `.launcher-search` |
| Launcher result list | `.launcher-list` |
| Launcher list row | `.launcher-row` |
| High threshold state | `.warning` |
| Critical threshold state | `.critical` |

In addition, any widget with `extra_class` set in `WidgetConfig` will have
that class applied to its outermost GTK container. These user-defined classes
are not part of the stable API — they are at the theme author's discretion.

### 6.3 Default Theme Guidelines

The built-in default theme (`themes/default.css`) provides sensible defaults
using CSS custom properties (variables). Requires GTK ≥ 3.20.

**CSS colour variable palette** — defined in a `*` selector so they cascade
to all child elements:

| Variable | Role | Default |
|----------|------|---------|
| `color-pill` | Pill island background | `rgba(15, 12, 10, 0.55)` |
| `color-fg` | Primary text | `rgba(255, 255, 255, 0.9)` |
| `color-fg-dim` | Dimmed / secondary text | `rgba(255, 255, 255, 0.4)` |
| `color-accent` | Active/highlighted text | `#ffffff` |
| `color-warning` | Warning state (`.warning`) | `#f0a500` |
| `color-urgent` | Critical/urgent state (`.critical`) | `#e53935` |

The bar window background is always transparent — wallpaper shows between pills.
Theme authors override only these variables to produce a complete theme:

```css
/* my-theme.css */
@define-color color-pill    rgba(24, 20, 36, 0.65);
@define-color color-fg      rgba(255, 255, 255, 0.88);
@define-color color-fg-dim  rgba(255, 255, 255, 0.35);
@define-color color-accent  #c4a8ff;
@define-color color-warning #f0a500;
@define-color color-urgent  #ea6962;
```

Do not hardcode pixel dimensions or colors in Rust code. All visual properties belong in CSS.

### 6.4 Theme Naming and Directory Layout

See `DOCS/theme-spec.md` for the full community theme specification, including
the directory layout (`~/.config/parapet/themes/`), dark/light variant naming
(`<name>-dark.css`, `<name>-light.css`), metadata format, naming rules, CSS
variable contract, and hot-reload behaviour.

---

## 7. Multi-Monitor Support

### 7.1 Monitor Detection

```rust
fn get_target_monitor(screen: &gdk::Screen, config: &BarConfig) -> i32 {
    match config.monitor {
        MonitorTarget::Primary => screen.primary_monitor(),
        MonitorTarget::Index(i) => i.min(screen.n_monitors() - 1),
    }
}
```

The bar renders on one monitor. Multi-bar setups (one bar per monitor) require running multiple `parapet_bar` instances with different configs.

### 7.2 Monitor Change Handling

Connect to the `monitors-changed` signal to handle hot-plug events:

```rust
screen.connect_monitors_changed(move |screen| {
    // Re-query monitor geometry and update strut
    let geom = screen.monitor_geometry(get_target_monitor(screen, &config));
    window.move_(geom.x(), geom.y());
    window.resize(geom.width(), config.bar.height);
    set_strut(&window, &config.bar, &geom);
});
```

---

## 8. Accessibility

### 8.1 Minimum Requirements

- Every interactive element (workspace buttons) has a tooltip and accessible label
- Keyboard focus should not be trapped in the bar — it is a panel, not an application window
- Text sizes must be readable at default system DPI

### 8.2 Widget Names for Accessibility

Set `set_widget_name()` on each widget root — this becomes the accessible name and aids in CSS targeting:

```rust
label.set_widget_name("clock");           // accessible + CSS ID
button.set_widget_name("workspace-1");    // accessible + CSS ID
```

---

## 9. No Business Logic in Renderers

This rule is stated once and applies everywhere:

> **Renderers display data. Renderers handle GTK events. Renderers do not compute or transform data.**

Data transformation (formatting, thresholds, unit conversion) belongs in `parapet_core` widget implementations or in the renderer's `update()` method as simple display formatting only.

---

## 10. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Bar window module structure | [ARCHITECTURE.md §5](ARCHITECTURE.md) |
| Display isolation rule | [ARCHITECTURE.md §1](ARCHITECTURE.md) |
| GTK threading rules | [CODING_STANDARDS.md §5](CODING_STANDARDS.md) |
| X11 EWMH strut details | [BAR_DESIGN.md §3](BAR_DESIGN.md) |
| Platform requirements | [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) |
