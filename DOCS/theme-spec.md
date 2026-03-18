# Parapet Theme Specification

> **Scope:** Community-shareable theme pack format for `parapet_bar`.
> **Last Updated:** 2026-03-17
> **Requires:** Parapet ≥ 0.1.0

---

## 1. Overview

A Parapet theme is a CSS file (or small set of CSS files) placed in
`~/.config/parapet/themes/`. The active theme is selected via the `[bar]`
config field or the `--theme` CLI flag:

```toml
[bar]
theme = "gruvbox"
```

```bash
parapet_bar --theme gruvbox
```

The bar searches for `~/.config/parapet/themes/gruvbox.css` at startup. If the
file does not exist, the built-in default theme is used and a warning is logged.

---

## 2. Directory Layout

```
~/.config/parapet/themes/
├── gruvbox.css          ← base theme (required)
├── gruvbox-dark.css     ← dark variant (optional; auto-selected)
├── gruvbox-light.css    ← light variant (optional; auto-selected)
└── gruvbox.toml         ← metadata (optional; not read by parapet_bar at runtime)
```

### Base file

`<name>.css` — Required. Used when no colour-scheme variant file is found.

### Dark and light variants

`<name>-dark.css` and `<name>-light.css` — Optional. When GTK's
`gtk-application-prefer-dark-theme` setting is `true`, `parapet_bar` appends
`-dark` to the name and checks whether that file exists; if so, it loads the
dark variant instead. When the preference is `false`, `-light` is tried.

If the variant file does not exist, the base `<name>.css` is used.

### Metadata file (optional)

`<name>.toml` — Metadata for tooling and human readers. Not read by
`parapet_bar` itself in v0.1.x.

```toml
name = "gruvbox"
author = "your-name"
description = "Gruvbox-inspired dark theme for Parapet."
frames_min_version = "0.1.0"
```

---

## 3. Naming Rules

Theme names must match `[a-z0-9_-]+`:

- Lowercase ASCII letters, digits, underscores, and hyphens only.
- No path separators (`/`), dots (`.`), or non-ASCII characters.
- `parapet_bar` rejects names containing `/` or `..` with a warning and falls
  back to the built-in default (path traversal guard).

Valid: `dark`, `gruvbox`, `solarized-light`, `my_theme_01`
Invalid: `../evil`, `Gruvbox`, `my theme`, `gruvbox.dark`

---

## 4. CSS Variable Contract

The built-in default theme defines a six-variable palette in a `*` selector.
Theme authors should override all six to ensure full visual coverage:

| Variable | Role | Default value |
|----------|------|---------------|
| `color-pill` | Pill island background | `rgba(15, 12, 10, 0.55)` |
| `color-fg` | Primary text | `rgba(255, 255, 255, 0.9)` |
| `color-fg-dim` | Dimmed / secondary text | `rgba(255, 255, 255, 0.4)` |
| `color-accent` | Active/highlighted text | `#ffffff` |
| `color-warning` | Warning state (`.warning`) | `#f0a500` |
| `color-urgent` | Critical/urgent state (`.critical`) | `#e53935` |

The bar background is always transparent — the wallpaper shows between pills.
Override `color-pill` to change the pill fill colour and opacity.

Minimal theme example:

```css
@define-color color-pill    rgba(24, 20, 36, 0.65);
@define-color color-fg      rgba(255, 255, 255, 0.88);
@define-color color-fg-dim  rgba(255, 255, 255, 0.35);
@define-color color-accent  #c4a8ff;
@define-color color-warning #f0a500;
@define-color color-urgent  #e53935;
```

Variables that are left undefined fall back to the GTK default (`initial`),
which may produce unreadable results. Always define all six.

> **GTK3 requirement:** CSS custom properties (`var()`) require GTK ≥ 3.20
> (Fedora 25+, Ubuntu 18.04+). Earlier versions silently ignore unknown CSS
> rules, leaving the bar unstyled.

---

## 5. Stable CSS Class Names

These class names are the CSS API surface — do not rename them without updating
`UI_GUIDE §6.2`.

| Element | CSS class |
|---------|-----------|
| Bar root `GtkBox` | `.parapet-bar` |
| Left section | `.parapet-left` |
| Centre section | `.parapet-center` |
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
| Warning threshold state | `.warning` |
| Critical threshold state | `.critical` |

### Per-widget instance targeting (`extra_class`)

Any `[[widgets]]` entry may carry an `extra_class` field that `parapet_bar`
applies to the widget's outermost GTK container:

```toml
[[widgets]]
type = "clock"
position = "center"
extra_class = "my-clock"
```

Then in the theme CSS:

```css
.my-clock {
    font-size: 15px;
    color: var(--color-accent);
}
```

This lets theme authors target individual widget instances without forking
widget source code.

---

## 6. Hot-Reload

When a theme file is loaded from disk (named theme or raw `bar.css` path),
`parapet_bar` watches the file for changes. Editing and saving the CSS file
causes the bar to reapply the theme within ~500 ms — no bar restart needed.

**Known limitation:** If the config file's `bar.theme` field is changed to a
different theme name, the bar continues to watch the *old* CSS file until
restarted. To switch themes dynamically, restart the bar or use the
`--theme` flag.

---

## 7. Sharing Themes

To share a theme, distribute:
- `<name>.css` (required)
- `<name>-dark.css` and/or `<name>-light.css` (recommended)
- `<name>.toml` (recommended — helps users know what version they need)

Users install by placing the files in `~/.config/parapet/themes/` and setting
`theme = "<name>"` in their `config.toml`.
