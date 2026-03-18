# Research: App Launcher Widget
**Date:** 2025-01-30
**Status:** Findings complete — decision pending

## Question

What is the best approach to add a Windows-style app launcher widget to the center of the Parapet status bar? Specifically: how should installed applications be enumerated, how should the search list be fuzzy-filtered as the user types, and what GTK3 widgets should compose the popup UI?

## Summary

Use `gio::AppInfo::all()` to enumerate installed apps (zero new dependencies — `gio` is already a transitive dependency via `gtk ~0.18`), add `fuzzy-matcher = "0.3"` (MIT, 14.9 M downloads) for interactive filtering, and build the popup using `gtk::Popover` + `gtk::SearchEntry` + `gtk::ListBox`. The launcher is a `parapet_bar`-only UI interaction widget — it does not produce `WidgetData` and bypasses the `Poller` pipeline entirely, the same pattern already used by `WorkspacesWidget`.

---

## Findings

### App Enumeration

#### Option A — `gio::AppInfo` (recommended, zero new deps)

`gio` is already a **transitive dependency** pulled in by `gtk ~0.18` → `glib` → `gio`. No `Cargo.toml` change is required for the app catalog itself.

Key API surface (gtk-rs stable 0.15 / 0.18 parity confirmed):

```rust
use gio::prelude::AppInfoExt;

// List all installed apps (includes NoDisplay=true; caller must filter)
let apps: Vec<gio::AppInfo> = gio::AppInfo::all();

// Filter to visible apps before caching
let visible: Vec<gio::AppInfo> = apps
    .into_iter()
    .filter(|a| a.should_show())   // respects NoDisplay, OnlyShowIn, NotShowIn
    .collect();

// Display name and launch
let name: glib::GString = app.name();
app.launch(&[], gio::AppLaunchContext::NONE)?;
```

`AppInfo` is `Clone` but `!Send + !Sync` (GObject). All access must occur on the GTK main thread — fine for a bar widget.

`AppInfo::all()` does **not** include apps with `Hidden=true` in their `.desktop` file.

**Pros:** No new dep, freedesktop-standard, handles `%f`/`%u`/`%F`/`%U` Exec substitutions automatically, respects D-Bus activation.

**Cons:** Returns a heterogeneous `Vec<AppInfo>` — name + icon only; no categories. Requires GTK initialisation (calls must happen after `gtk::init()`).

#### Option B — `freedesktop-desktop-entry v0.8.1`

A Rust crate that parses `.desktop` files directly from the XDG data dirs.

```toml
freedesktop-desktop-entry = "0.8.1"
```

- **License:** MPL-2.0 (copyleft for *modified* source files — compatible but stricter than MIT)
- **Maintained by:** pop-os / mmstick, actively maintained
- **API:** `Iter::new(default_paths()).entries(Some(&locales))` yields typed `Entry` structs with `.name()`, `.exec()`, `.icon()`, `.categories()`, `.no_display()`
- **Pros:** Works without GTK, exposes full `.desktop` metadata including categories
- **Cons:** Requires manual Exec string parsing for launch (handles substitution markers less gracefully than GIO), adds a new MPL-2.0 dependency

**Decision:** Use Option A (`gio::AppInfo`). Zero new dependencies, correct Exec handling, and fully sufficient for a name-based fuzzy search UI.

---

### Fuzzy Matching

#### Option A — `fuzzy-matcher v0.3.7` (recommended)

```toml
fuzzy-matcher = "0.3"
```

- **License:** MIT
- **Downloads:** 14.9 M
- **Algorithm:** SkimMatcherV2 — same scorer used in fzf/skim
- **Last release:** 2022 (stable, no breaking changes needed)

```rust
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

let matcher = SkimMatcherV2::default();

// Score a candidate; None = no match
if let Some(score) = matcher.fuzzy_match(app_name, query) { ... }

// Score + match positions for highlight rendering
if let Some((score, indices)) = matcher.fuzzy_indices(app_name, query) { ... }
```

**Pros:** MIT, battle-tested, excellent documentation, minimal API surface.

**Cons:** Not the fastest implementation available. Adequate for ≤ 1 000 app names.

#### Option B — `nucleo-matcher v0.3.1`

- **License:** MPL-2.0
- Used in helix-editor; ~6× faster than fuzzy-matcher on large input
- **Cons:** MPL-2.0, more complex API (requires a `nucleo::Config` and `Utf32Str` conversion), overkill for a list of 200–500 apps

**Decision:** Use `fuzzy-matcher = "0.3"`. MIT license aligns with project conventions; the performance of SkimMatcherV2 is more than sufficient for a typical app list.

---

### GTK3 UI Architecture

#### Trigger Button

A `gtk::Button` placed in the **center** `gtk::Box` section of the bar:

```rust
let btn = gtk::Button::with_label("  Search");
btn.style_context().add_class("launcher-button");
```

Alternatively a search/grid icon from the icon theme (`gtk::Image::from_icon_name("system-search-symbolic", IconSize::SmallToolbar)`).

#### Popup — `gtk::Popover`

`gtk::Popover` is the correct GTK3 primitive for a contextual overlay anchored to a widget. It:

- Attaches to the trigger button with `Popover::new(Some(&btn))`
- Drops downward by default (`PositionType::Bottom`)
- Grabs keyboard focus when shown
- Dismisses automatically on Esc or a click outside its bounds
- Does **not** create a new `gtk::Window` — it piggybacks on the existing bar window

```rust
let popover = gtk::Popover::new(Some(&btn));
popover.set_position(gtk::PositionType::Bottom);
btn.connect_clicked(clone!(@weak popover => move |_| {
    popover.popup();
}));
```

#### Search Box — `gtk::SearchEntry`

`gtk::SearchEntry` is a specialised `gtk::Entry` subclass:

- Includes a built-in magnifier icon and a clear (×) button
- Fires `connect_search_changed` on every keystroke (debounced compared to `connect_changed`)
- Fires `connect_activate` (Enter key) to launch the top result

```rust
let entry = gtk::SearchEntry::new();
entry.connect_search_changed(clone!(@weak list_box, @strong apps => move |e| {
    let query = e.text();
    rebuild_list(&list_box, &apps, query.as_str());
}));
entry.connect_activate(clone!(@weak list_box => move |_| {
    if let Some(row) = list_box.selected_row() {
        launch_row(&row);
    }
}));
```

#### Results List — `gtk::ListBox`

- One `gtk::ListBoxRow` per matching app
- Each row: `gtk::Box` (horizontal) → `gtk::Image` + `gtk::Label`
- Rebuild the `ListBox` contents on every `search_changed` event (clear rows, re-score, re-insert sorted by score descending)
- Wrap in a `gtk::ScrolledWindow` with a max-height CSS constraint

#### Full Widget Tree

```
gtk::Button (.launcher-button)       ← bar center section
  └── gtk::Popover
        └── gtk::Box (vertical, spacing=4)
              ├── gtk::SearchEntry
              └── gtk::ScrolledWindow (max-height: 400px)
                    └── gtk::ListBox
                          ├── gtk::ListBoxRow (App 1 icon + name)
                          ├── gtk::ListBoxRow (App 2 icon + name)
                          └── ...
```

---

### Architecture Fit

The launcher is a **`parapet_bar`-only UI interaction widget**. Key points:

| Concern | Decision |
|---------|----------|
| `parapet_core` changes needed? | **None.** No new `WidgetData` variant, no new `Widget` impl. |
| Poller integration? | **None.** Like `WorkspacesWidget`, the launcher bypasses the `Widget → Poller → renderer` pipeline. |
| When to load app list? | Lazily on first `Popover::popup()` call, then cache in the widget struct. |
| Launch mechanism | `AppInfoExt::launch(&[], gio::AppLaunchContext::NONE)` — GLib-managed, no `std::process::Command`. |
| Config key | `type = "launcher"` in `[[bar.widgets]]`; no extra fields required at launch. |
| New file | `crates/parapet_bar/src/widgets/launcher.rs` |
| Factory case | `"launcher" => build_launcher_widget(...)` in `main.rs` |

**Architectural exception note (same as workspaces):** launcher bypasses the `Widget → Poller` pipeline. Must be documented in `DOCS/futures.md` as technical debt, and the exception noted in the implementation.

---

### Scope of Change (if approved)

| File | Change |
|------|--------|
| `crates/parapet_bar/Cargo.toml` | Add `fuzzy-matcher = "0.3"` |
| `crates/parapet_bar/src/widgets/launcher.rs` | New file — button, popover, search entry, list box, launch handler |
| `crates/parapet_bar/src/widgets/mod.rs` | `pub mod launcher;` |
| `crates/parapet_bar/src/main.rs` | Factory arm `"launcher"` |
| `crates/parapet_bar/src/themes/default.css` | `.launcher-button`, `.launcher-popover`, `.launcher-row` styles |
| `DOCS/futures.md` | Note: launcher bypasses Poller (same debt as workspaces) |

`parapet_core` — **zero changes**.

---

## Recommendation

Implement the launcher as follows:

1. **App catalog:** `gio::AppInfo::all()` filtered by `should_show()` — no new dependency
2. **Fuzzy matching:** `fuzzy-matcher = "0.3"` (MIT) — one new dependency
3. **UI:** `gtk::Button` → `gtk::Popover` → `gtk::SearchEntry` + `gtk::ListBox`
4. **Architecture:** `parapet_bar`-only, no `parapet_core` changes, same bypass pattern as `WorkspacesWidget`

This is the minimum-viable path. It fits cleanly into the existing codebase, introduces only one new dependency (MIT-licensed), and produces a correct freedesktop-standard launch experience on Cinnamon / X11.

**One dependency approval required:** `fuzzy-matcher = "0.3"`  
**One standards-plan deviation required:** launcher widget is not in the original 34-step `DOCS/plan.md` — must be added as a new step or a follow-on plan before implementation begins.

---

## Standards Conflict / Proposed Update

None. The existing standards already accommodate `parapet_bar`-only UI interaction widgets (WIDGET_API.md §3, WorkspacesWidget precedent). No standard changes are needed.

---

## Sources

- [https://gtk-rs.org/gtk-rs-core/stable/0.15/docs/gio/struct.AppInfo.html](https://gtk-rs.org/gtk-rs-core/stable/0.15/docs/gio/struct.AppInfo.html): Confirmed `AppInfo::all()`, `AppInfoExt::should_show()`, `AppInfoExt::launch()` API in gtk-rs 0.15
- [https://crates.io/crates/fuzzy-matcher](https://crates.io/crates/fuzzy-matcher): fuzzy-matcher v0.3.7, MIT, 14.9 M downloads, SkimMatcherV2 API
- [https://crates.io/crates/nucleo-matcher](https://crates.io/crates/nucleo-matcher): nucleo-matcher v0.3.1, MPL-2.0, helix-editor — evaluated and rejected (overkill + license)
- [https://crates.io/crates/freedesktop-desktop-entry](https://crates.io/crates/freedesktop-desktop-entry): v0.8.1, MPL-2.0, pop-os maintained — evaluated as Option B for app catalog
- GTK3 Popover / SearchEntry / ListBox: confirmed in gtk-rs ~0.18 safe bindings, no unsafe FFI required

---

## Open Questions

1. **Icon rendering:** `AppInfoExt::icon()` returns `Option<gio::Icon>`. Rendering via `gtk::Image::set_from_gicon()` needs a size hint. What pixel size fits the bar height? (Probably 24 px — needs platform test.)
2. **Popover positioning relative to strut:** the bar uses `_NET_WM_STRUT_PARTIAL` — does `gtk::Popover` drop below the bar into desktop space, or can it extend above? Needs visual test on Cinnamon.
3. **Keyboard shortcut:** Should a global keybinding (Super key) open the launcher, or is a click-only trigger acceptable for v0.1?
4. **Plan amendment:** This feature is outside the original 34-step plan. Confirm with developer before implementation begins.
