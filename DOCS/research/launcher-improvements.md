# Research: Apps Launcher Improvements
**Date:** 2026-03-18
**Status:** Findings complete — decision pending

## Question

What are the highest-value improvements to `LauncherWidget` in `parapet_bar`, and how should each be implemented using the existing dependency set?

## Summary

Seven distinct improvements to the launcher are available — all but one require zero new dependencies. The highest-priority items are keyboard navigation (pure GTK signal wiring, no deps, high UX impact), `gio::AppInfoMonitor` for live app-list refresh (supersedes the `notify`+thread approach tracked in `futures.md`, stays on main thread, requires no new dep), and richer search via `DesktopAppInfo` downcast (adds `keywords`, `generic_name` to the fuzzy corpus using the existing `gio` dep). Match-highlight rendering and configurable label/dimensions are straightforward follow-ons.

## Current State

File: `crates/parapet_bar/src/widgets/launcher.rs`

- App list loaded **once** at `new()` — never refreshed. Tracked debt in `futures.md`.
- Fuzzy search scored against `app.name()` **only** via `SkimMatcherV2`.
- `fuzzy_indices()` is available on the existing `fuzzy-matcher` dep but is **unused** — match positions could drive highlight rendering.
- No keyboard focus transfer: arrow keys do not move from `SearchEntry` into `ListBox`.
- Button label hardcoded to `"Apps"`.
- Popup dimensions hardcoded to `set_default_size(280, -1)` / `set_min_content_height(200)`.
- `LauncherConfig` has a single field: `max_results: Option<u32>`.

---

## Findings

### Option 1 — Keyboard navigation (Arrow keys: SearchEntry → ListBox)

**Problem:** Pressing Down from the `SearchEntry` does not move focus to the first `ListBoxRow`. Users must reach for the mouse.

**Mechanism:**
- `search_entry.connect_key_press_event` — intercept `gdk::keys::constants::Down` (and `Up`)
- On `Down`: call `list_box.child_focus(gtk::DirectionType::Down)` → returns `glib::Propagation::Stop`
- On `Up` from first row: return focus to `SearchEntry` by calling `search.grab_focus()`
- Enter key must launch the currently selected row (not just the first in the list)

**Dependencies:** None — pure GTK signal wiring.
**Crates affected:** `parapet_bar` only.

---

### Option 2 — Live app-list refresh via `gio::AppInfoMonitor`

**Problem:** `futures.md` tracks that newly-installed apps require restarting the bar.

**Prior approach (futures.md):** Use `notify ~6.1` to watch XDG data dirs in a background thread, send `()` via `glib::MainContext::channel()`, reload on main thread.

**Better approach — confirmed from gio 0.18.4 docs:**

```
gio::AppInfoMonitor::get() -> AppInfoMonitor
AppInfoMonitor::connect_changed(f: Fn(&Self) + 'static) -> SignalHandlerId
```

`AppInfoMonitor` is a GIO singleton that emits `changed` whenever the system app list changes (app installed, removed, or `.desktop` updated). It integrates directly with the GLib main loop — no background thread, no channel, no `notify` dep.

**Implementation sketch:**

```rust
// In new() or when building the popup:
let monitor = gio::AppInfoMonitor::get();
let apps_ref = apps.clone();   // Rc<RefCell<Vec<gio::AppInfo>>>
monitor.connect_changed(move |_| {
    *apps_ref.borrow_mut() = AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .collect();
});
// Store monitor to keep it alive: LauncherWidget { ..., _app_monitor: monitor }
```

**Constraints:** `AppInfoMonitor` is `!Send + !Sync` — it must be created and used on the GTK main thread only. All existing launcher code already runs on the main thread, so there is no constraint issue.

**Dependencies:** None new — `gio::AppInfoMonitor` is part of `gio ~0.18`, already in the dep tree via `gtk`.
**Crates affected:** `parapet_bar` only.
**Supersedes:** The `futures.md` `notify`-based approach. Recommend closing out that futures entry when this is implemented.

> **Important:** The `notify` dep in the workspace (`Cargo.toml`) is used by `parapet_core` for config hot-reload and is not needed for this. Do not add `notify` to `parapet_bar/Cargo.toml`.

---

### Option 3 — Richer search corpus via `DesktopAppInfo` downcast

**Problem:** Search is scored against `app.name()` only. Users searching by category, generic name ("web browser", "text editor"), or keyword ("ide", "email") get no results.

**API confirmed — gio 0.18.4 `gio::DesktopAppInfo`:**

| Method | Data source | Example |
|--------|-------------|---------|
| `generic_name() -> Option<GString>` | `GenericName=` | "Web Browser" |
| `keywords() -> Vec<GString>` | `Keywords=` | ["ide", "editor"] |
| `categories() -> Option<GString>` | `Categories=` | "Network;WebBrowser;" |
| `description() -> Option<GString>` | `Comment=` | inherited from `AppInfo` |

**Downcast pattern** (existing `AppInfo` → `DesktopAppInfo`):

```rust
use glib::Cast;
let dinfo: Option<gio::DesktopAppInfo> = app_info.clone()
    .dynamic_cast::<gio::DesktopAppInfo>()
    .ok();
```

**Suggested scoring weights** (input to `SkimMatcherV2::fuzzy_match`):

```
Name:         weight 3   (already used)
GenericName:  weight 2   (new)
Keywords:     weight 2   (new — scored individually, take max)
Description:  weight 1   (new)
```

Take the maximum score across all fields; use the field that produced the highest score for highlight position extraction (Option 4).

**Constraints:** `DesktopAppInfo` is `!Send + !Sync` — must remain on GTK main thread. Already satisfied by the launcher's architecture.

**Dependencies:** None new — `DesktopAppInfo` is in the `gio ~0.18` dep already used.
**Crates affected:** `parapet_bar` only.

---

### Option 4 — Match highlight rendering

**Problem:** Matched characters are not highlighted in search results. Users can't tell why a result matched.

**Mechanism:** `fuzzy-matcher ~0.3` — already a workspace dep, used by `parapet_bar`. The `fuzzy_indices()` method returns `(score, Vec<usize>)` where the `Vec<usize>` is the positions of matched characters.

```rust
// Instead of fuzzy_match(), use:
let (score, indices) = matcher.fuzzy_indices(candidate, query)?;
```

**Rendering:** Build a `gtk::Label` with `set_markup()` using `<b>` tags around matched character positions.

```rust
fn build_highlighted_label(text: &str, indices: &[usize]) -> gtk::Label {
    let mut markup = String::new();
    for (i, ch) in text.chars().enumerate() {
        if indices.binary_search(&i).is_ok() {
            write!(markup, "<b>{}</b>", glib::markup_escape_text(&ch.to_string())).ok();
        } else {
            markup.push_str(&glib::markup_escape_text(&ch.to_string()));
        }
    }
    let label = gtk::Label::new(None);
    label.set_markup(&markup);
    label
}
```

Note: `glib::markup_escape_text` must be used to prevent XSS-style injection through app names containing `<`, `>`, `&`.

**Dependencies:** None new — uses `fuzzy_indices()` already available in `fuzzy-matcher`.
**Crates affected:** `parapet_bar` only.

---

### Option 5 — App pinning / favorites

**Problem:** No way to make frequently-used apps always visible at the top of the unfiltered list.

**Mechanism:** Add `pinned: Vec<String>` to `LauncherConfig` — a list of desktop ID stems (e.g. `["firefox", "code", "alacritty"]`). Match via:

```rust
fn is_pinned(app: &gio::AppInfo, pinned: &[String]) -> bool {
    let id = app.id().unwrap_or_default();
    let stem = id.strip_suffix(".desktop").unwrap_or(&id);
    pinned.iter().any(|p| p == stem)
}
```

Pinned apps are prepended to the list in config order; remaining apps follow alphabetically. When a search query is active, the pinned ordering is kept but only matching apps are shown.

**Dependencies:** None new.
**Crates affected:** `parapet_bar`, `parapet_core` (`LauncherConfig`).

---

### Option 6 — Configurable button label and popup dimensions

**Problem:** Button label is hardcoded `"Apps"`. Popup width/height hardcoded to `280px` / `200px` min.

**Mechanism:** Extend `LauncherConfig`:

```toml
# ~/.config/parapet/config.toml
[launcher]
button_label = "󰀻"          # nerd font icon, or any string
popup_width = 320
popup_min_height = 240
max_results = 12
```

New fields in `LauncherConfig`:

```rust
pub button_label: Option<String>,    // default: "Apps"
pub popup_width: Option<i32>,        // default: 280
pub popup_min_height: Option<i32>,   // default: 200
```

**Dependencies:** None.
**Crates affected:** `parapet_bar`, `parapet_core` (`LauncherConfig`).

---

### Option 7 — `DesktopAppInfo::search()` as alternative or hybrid search backend

**API confirmed — gio 0.18.4:**

```rust
gio::DesktopAppInfo::search(search_string: &str) -> Vec<Vec<GString>>
```

Returns groups of app IDs scored by GLib's own internal search algorithm, which indexes across `Name`, `GenericName`, `Keywords`, and `Comment` from the `.desktop` file cache. Results are ordered by relevance (first group = highest match).

**Tradeoffs vs. current `SkimMatcherV2`:**

| | `DesktopAppInfo::search()` | `SkimMatcherV2` + `fuzzy_indices()` |
|---|---|---|
| Corpus | GLib-managed (all `.desktop` fields) | Manual corpus build (Options 3+4) |
| Match type | Full-text / substring | Fuzzy (character-order) |
| Position data for highlights | Not available | Available via `fuzzy_indices()` |
| Deps | None new | None new |
| Performance | System library call | Pure Rust, similar cost |

**Hybrid option:** Use `DesktopAppInfo::search()` for ranking only, then apply `fuzzy_indices()` on the winning field for highlight positions. This would:
1. Call `DesktopAppInfo::search(query)` to get ranked groups of app IDs
2. Construct `DesktopAppInfo::new(id)` for each result to retrieve display data
3. Call `fuzzy_indices()` on the winning field for rendering

**Recommendation:** This is worth a prototyping experiment rather than a firm recommendation at this stage. The manual corpus approach (Option 3) is less surprising and gives full control over scoring. If GLib's search proves to match user expectations better in practice, adopt the hybrid approach.

**Dependencies:** None new.
**Crates affected:** `parapet_bar` only.

---

## Recommendation

Implement in the following sequence (value/complexity ratio, highest first):

| Priority | Item | Deps | Scope |
|----------|------|------|-------|
| 1 | Keyboard navigation (Option 1) | None | `parapet_bar` |
| 2 | Configurable label/dimensions (Option 6) | None | `parapet_bar`, `parapet_core` |
| 3 | Live app-list refresh via `AppInfoMonitor` (Option 2) | None | `parapet_bar` |
| 4 | Match highlight rendering (Option 4) | None | `parapet_bar` |
| 5 | Richer search corpus (Option 3) | None | `parapet_bar` |
| 6 | App pinning/favorites (Option 5) | None | `parapet_bar`, `parapet_core` |
| 7 | `DesktopAppInfo::search()` hybrid backend (Option 7) | None | `parapet_bar` (prototype first) |

None of these require a new workspace dependency. Items 1–3 have the highest user-visible impact per implementation hour. Items 3 and 4 are natural pairs to implement together since both require touching `rebuild_list()` and the row construction code.

**On the `futures.md` debt:** The `gio::AppInfoMonitor` approach (Option 2) is strictly better than the `notify` background-thread approach tracked in `futures.md`. When Option 2 is implemented, remove the futures.md entry.

**On `nucleo`:** Re-evaluated here. Confirmed MPL-2.0, which disqualifies it under the same reasoning as ADR-006. Not reconsidered further.

## Standards Conflict / Proposed Update

No conflicts with existing standards. `WIDGET_API.md` and `BAR_DESIGN.md` do not need updating for these changes — all remain in `parapet_bar`, the `Widget` trait contract is unchanged, and `LauncherConfig` additions are additive/non-breaking.

One suggestion for `DOCS/futures.md`: the existing entry about watching XDG data dirs via `notify` should be updated to reference `gio::AppInfoMonitor` as the correct approach, or closed out when Option 2 is implemented.

## Sources

- `https://docs.rs/gio/0.18.4/gio/struct.DesktopAppInfo.html` — confirmed full method surface: `categories()`, `generic_name()`, `keywords()`, `filename()`, `list_actions()`, `search()`, `!Send + !Sync`, `IsA<AppInfo>` subtying
- `https://docs.rs/gio/0.18.4/gio/struct.AppInfoMonitor.html` — confirmed `get()` singleton, `connect_changed()` signal, `!Send + !Sync`
- `https://docs.rs/fuzzy-matcher/0.3.7/fuzzy_matcher/` — confirmed `fuzzy_indices()` returning `(i64, Vec<usize>)` available on `SkimMatcherV2`
- `https://crates.io/crates/nucleo` — v0.5.0, MPL-2.0 license, disqualified
- `DOCS/DECISIONS.md` ADR-005 (use `gio::AppInfo`), ADR-006 (use `fuzzy-matcher`, rejects `nucleo`)
- `DOCS/futures.md` — active debt item: app list not refreshed after startup
- `crates/parapet_bar/src/widgets/launcher.rs` — full implementation reviewed

## Open Questions

1. **Search experience:** Should fuzzy matching (Option 3/4) and `DesktopAppInfo::search()` (Option 7) both be prototyped to compare result quality before committing to one approach? A quick bench with a few common queries (`"fire"`, `"text edit"`, `"term"`) would settle this.

2. **`AppInfoMonitor` debounce:** GLib may emit `changed` multiple times during a package installation (one event per `.desktop` file written). Worth adding a short debounce (e.g. 500ms `glib::timeout_add_local` one-shot) before reloading the app list to avoid redundant `AppInfo::all()` calls.

3. **Pinning UX (Option 5):** Should pinned apps remain visible when a search query is active, or only in the empty-query state? User preference question — no research finding to resolve it.
