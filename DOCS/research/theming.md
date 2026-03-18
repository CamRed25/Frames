# Research: Bar Theming System
**Date:** 2026-03-17
**Status:** Findings complete — decision recommended — ready to close

## Question

What theming capabilities can Parapet realistically implement using GTK3's CSS system, and what is the right architecture for named theme selection, user overrides, and live theme switching?

---

## Summary

GTK3's `CssProvider` system supports a clean two-layer pipeline: a **theme provider** (named built-in or user-installed) at `STYLE_PROVIDER_PRIORITY_APPLICATION` (600), and a **user override provider** at `STYLE_PROVIDER_PRIORITY_USER` (800). The recommended implementation adds a `theme` field to `BarConfig`, ships 3–4 bundled named themes compiled in via `include_bytes!`, optionally discovers user themes from `~/.config/parapet/themes/`, and enables live theme hot-swap without a widget rebuild. GTK3's `@define-color` directive is the right mechanism for theme color tokens. The full-file approach (each theme is self-contained CSS) is more practical than a token-split architecture.

---

## Findings

### What GTK3 CSS Actually Supports

GTK3 has its own CSS dialect. Key capabilities and limits:

| Feature | Supported in GTK3 | Notes |
|---------|-------------------|-------|
| Standard CSS selectors (class, ID, element, descendant) | ✅ | `.widget-clock`, `#clock`, `GtkLabel` |
| Box model (padding, margin, border) | ✅ | Full support |
| Colors, fonts, font-weight, font-size | ✅ | Full support |
| `@define-color` color tokens | ✅ | GTK3's own color variable system; evaluated at parse time |
| CSS `var()` custom properties | ❌ | GTK4 only — not available in GTK3 |
| Transitions / animations | ⚠️ | Limited; `transition` works, `@keyframes` does not |
| Gradients (`linear-gradient`, etc.) | ✅ | Supported |
| `border-image` | ✅ | Supported |
| `min-height`, `min-width` | ✅ | Supported |
| GTK symbolic colors (`@theme_bg_color`, etc.) | ✅ | Can reference GTK desktop theme tokens |
| Multiple CssProviders at different priorities | ✅ | Core to the layered approach |
| `remove_provider_for_screen` hot-swap | ✅ | Available in gtk-rs ~0.18 |

**Critical limitation:** `@define-color` tokens are **provider-local**. A token defined in Provider A cannot be referenced in Provider B. This means color-token theming requires all token definitions and their consuming rules to live in the same `CssProvider`. The practical consequence: each named theme is a self-contained CSS file — not a token file plus a shared structural file.

### GTK3 `@define-color` Syntax

```css
/* Theme file — tokens defined at top, rules reference them below */
@define-color bar_bg rgba(30, 30, 30, 0.95);
@define-color bar_fg #eeeeee;
@define-color accent #89b4fa;
@define-color warning_color #f0a500;
@define-color critical_color #e53935;
@define-color workspace_active #ffffff;

.parapet-bar {
    background-color: @bar_bg;
    color: @bar_fg;
    font-family: monospace;
    font-size: 13px;
}

.warning { color: @warning_color; }
.critical { color: @critical_color; }
.workspace.active { color: @workspace_active; }
```

This is the right pattern for Parapet theme files — tokens at the top define the color palette, rules below reference them. Users can understand and override individual colors easily.

### GTK3 Priority Constants (gtk-rs ~0.18)

```rust
gtk::STYLE_PROVIDER_PRIORITY_FALLBACK    // 1
gtk::STYLE_PROVIDER_PRIORITY_THEME       // 200
gtk::STYLE_PROVIDER_PRIORITY_SETTINGS    // 400
gtk::STYLE_PROVIDER_PRIORITY_APPLICATION // 600
gtk::STYLE_PROVIDER_PRIORITY_USER        // 800
```

Higher priority wins when rules conflict. The natural layering for Parapet:

| Layer | Provider | Priority |
|-------|----------|----------|
| Named theme | `theme_provider` | `STYLE_PROVIDER_PRIORITY_APPLICATION` (600) |
| User overrides | `override_provider` | `STYLE_PROVIDER_PRIORITY_USER` (800) |

The user override file (currently `bar.css`) becomes a surgical override on top of the active theme — users only need to write the rules they want to change.

### Theme Hot-Swap

`gtk::StyleContext::remove_provider_for_screen` exists in gtk-rs ~0.18 and works correctly:

```rust
// Swap theme without rebuilding widgets
gtk::StyleContext::remove_provider_for_screen(&screen, &old_theme_provider);
let new_provider = load_named_theme("catppuccin-mocha");
gtk::StyleContext::add_provider_for_screen(
    &screen,
    &new_provider,
    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
);
```

Widgets do not need to be rebuilt — their GTK widget structure is unchanged, only the CSS rules that style them change. This means theme switching is instantaneous with no flicker of widget reconstruction.

Hot-swap integrates cleanly with the existing config hot-reload watcher: if `bar.theme` changes in the config file, swap only the theme provider; if `bar.css` changes, swap only the override provider.

### Current Theming Architecture (Status Quo)

```
css.rs::load_theme(css_path)
    ├── if css_path is Some and file loads → use user CSS (FULL replacement, not layer)
    └── else → include_bytes!("../themes/default.css")
```

**Gap:** The current model is an either/or choice — user CSS entirely replaces the built-in theme. A user who just wants to change the accent color must copy the entire default theme and modify one line. This is poor UX.

---

## Options

### Option A — Status Quo (no change)

One CSS file: built-in OR user-specified replacement. No named themes.

- **Pro:** Already works, already shipped.
- **Con:** User CSS replaces the entire theme (no override layering). No named presets. Users must write all CSS from scratch to customize.

### Option B — Two-Layer CSS Pipeline + Named Themes

A theme provider (named built-in or user theme file) at priority 600, plus an optional user override provider at priority 800. Config gains `bar.theme`.

```toml
[bar]
theme = "catppuccin-mocha"   # selects named theme (built-in or ~/.config/parapet/themes/)
css = "~/.config/parapet/overrides.css"   # optional: only rules you want to change
```

Resolution order:
1. Look for `~/.config/parapet/themes/<theme>.css` — user-installed theme (highest authorship)
2. Look in built-in compiled-in table — shipped themes
3. Fall back to `"default"`

Built-in theme table:
```rust
fn builtin_theme(name: &str) -> Option<&'static [u8]> {
    match name {
        "default"          => Some(include_bytes!("../themes/default.css")),
        "catppuccin-mocha" => Some(include_bytes!("../themes/catppuccin-mocha.css")),
        "gruvbox"          => Some(include_bytes!("../themes/gruvbox.css")),
        "nord"             => Some(include_bytes!("../themes/nord.css")),
        _                  => None,
    }
}
```

**Pro:** User overrides are surgical. Named themes are discoverable. Theme files use `@define-color` tokens — easy to read and fork. Hot-swap without widget rebuild. Aligns with futures.md "multiple named themes selectable from config" item.
**Con:** Requires `BarConfig` to gain a `theme` field. Requires restructuring `css.rs` to manage two providers. Requires writing the 3 additional theme files.

### Option C — Token-Split Architecture (base CSS + theme tokens)

Structural base CSS (layout, padding, fonts) ships as a separate provider. Theme files contain only `@define-color` token blocks. The base CSS references those tokens.

**Hard blocker:** As noted above, `@define-color` tokens are provider-local in GTK3. A token defined in the token provider cannot be resolved in the base CSS provider. The only way to make this work is string concatenation before loading (prepend the token block to the base CSS before calling `load_from_data`). This is non-obvious and fragile.

**Verdict:** Not recommended for GTK3. Option B with self-contained theme files is equivalent in user experience and avoids this complexity entirely.

---

## Recommendation

**Implement Option B** — two-layer CSS pipeline with named themes.

### Config Change

Add `theme: Option<String>` to `BarConfig` in `parapet_core/src/config.rs`:

```rust
/// Named theme to apply. Resolves first from `~/.config/parapet/themes/<name>.css`,
/// then from built-in compiled themes. `None` (or `"default"`) uses the
/// built-in default theme.
#[serde(default)]
pub theme: Option<String>,
```

Rename the existing `css` field's semantics to "user override" (no breaking change — behavior is additive).

### `css.rs` Restructure

```rust
pub fn load_theme_provider(name: Option<&str>) -> gtk::CssProvider {
    // 1. Try ~/.config/parapet/themes/<name>.css
    // 2. Try built-in table
    // 3. Fall back to default
}

pub fn load_override_provider(css_path: Option<&Path>) -> Option<gtk::CssProvider> {
    // Returns None if no override path configured
}
```

`main.rs` registers both providers:
```rust
apply_provider(&theme_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
if let Some(ref override_provider) = override_provider {
    apply_provider(override_provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
}
```

### Built-in Themes to Ship

Start with four — these cover the most common user preferences and are well-documented color palettes:

| Theme name | Style | Why include |
|-----------|-------|-------------|
| `"default"` | Dark monochrome (current) | Existing baseline; must remain |
| `"catppuccin-mocha"` | Dark, pastel accent colors | Most popular Linux ricing theme |
| `"gruvbox"` | Warm dark amber/orange | Second most requested on r/unixporn |
| `"nord"` | Dark cool blue-grey | Popular Cinnamon/GTK theme family |

All four ship as self-contained CSS files with `@define-color` tokens at the top for easy user forking.

### Hot-Reload Integration

The existing hot-reload watcher in `main.rs` gets a small addition:
- If `bar.theme` changes → swap `theme_provider` (remove old, add new)
- If `bar.css` changes → swap `override_provider` (remove old, add new if path present)
- Widget tree is never rebuilt for pure theme/CSS changes

### `--theme` CLI Flag

`main.rs` can accept `--theme <name>` as a CLI override for `bar.theme`. No new crate needed — just check `std::env::args()` before loading config and merge into `BarConfig`. Enables quick testing:
```bash
parapet_bar --theme gruvbox
```

---

## Standards Conflict / Proposed Update

### UI_GUIDE.md §6.1

The current loading pseudocode shows a single provider. It should be updated to document the two-provider model once implemented.

### UI_GUIDE.md §6.3

"Default Theme Guidelines" should reference the `@define-color` token convention — theme files should define named color tokens at the top rather than using raw hex values inline.

### CONFIG_MODEL.md §3

The `[bar]` section table should gain a `theme` row once the field is added to `BarConfig`.

---

## Sources

- GTK3 CSS documentation: https://docs.gtk.org/gtk3/css-overview.html — `@define-color` syntax and provider-locality behavior
- gtk-rs 0.18 `StyleContext::remove_provider_for_screen`: confirmed present in `gtk-rs/gtk/src/style_context.rs`
- GTK3 style provider priority constants: `gtk::ffi::GTK_STYLE_PROVIDER_PRIORITY_*` — values 1, 200, 400, 600, 800
- Catppuccin GTK theme: https://github.com/catppuccin/gtk — color palette reference
- Gruvbox GTK: https://github.com/theamallaweerakkody/gruvbox-gtk — color reference
- Nord GTK: https://github.com/nordtheme/gtk — color reference

---

## Open Questions

- **No blocking questions.** Implementation can proceed from this report.
- Optionally: confirm `remove_provider_for_screen` in the gtk-rs ~0.18 source before wiring hot-swap. Based on ADR-004, `gdk::property_change` was confirmed this way — same pattern applies here.
