//! App launcher renderer — opens a search-driven application dropdown.
//!
//! Renders as a single `gtk::Button` in the bar. Clicking it opens a standalone
//! `gtk::Window` (popup-menu type) positioned below the button, containing a
//! `gtk::SearchEntry` and a `gtk::ListBox` of installed applications filtered by
//! the current query. Pressing Enter or clicking a row launches the selected app
//! via `gio::AppInfo::launch`.
//!
//! A dedicated window is used instead of `gtk::Popover` because the bar is a
//! dock window only 30 px tall — GTK3 on X11 clips popovers to the parent
//! window bounds, preventing them from opening fully.
//!
//! # Architecture note
//!
//! This renderer does **not** implement the [`parapet_core::Widget`] trait or use
//! the [`parapet_core::Poller`]. The application list is loaded once at construction
//! via [`gio::AppInfo::all()`] and refreshed automatically whenever the system app
//! list changes via [`gio::AppInfoMonitor`]. Fuzzy filtering runs synchronously on
//! the GTK main thread — acceptable for lists of ~200–500 apps.
//!
//! This is the same architectural exception established for `WorkspacesWidget`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gdk::prelude::*;
use gio::prelude::AppInfoExt;
use gtk::prelude::*;

use parapet_core::LauncherConfig;

// ── Search corpus type ────────────────────────────────────────────────────────

/// Precomputed search data for one installed application.
///
/// Built once at widget construction (and on each live refresh) so that
/// corpus strings are not re-extracted on every keystroke.
struct AppSearchData {
    /// Primary display name (always populated).
    name: String,
    /// `GenericName=` field from the `.desktop` file; empty when absent.
    generic_name: String,
    /// `Keywords=` entries joined by a single space; empty when absent.
    keywords: String,
    /// `Comment=` / description field; empty when absent.
    description: String,
}

/// Build [`AppSearchData`] from an `AppInfo`.
///
/// Attempts a [`gio::DesktopAppInfo`] downcast to access the extended fields.
/// Falls back to empty strings for any field that is unavailable (e.g. Flatpak
/// proxy entries that do not expose a full `.desktop` file).
fn build_search_data(app: &gio::AppInfo) -> AppSearchData {
    let dinfo = app.clone().dynamic_cast::<gio::DesktopAppInfo>().ok();
    AppSearchData {
        name: app.name().to_string(),
        generic_name: dinfo
            .as_ref()
            .and_then(gio::DesktopAppInfo::generic_name)
            .map(|s| s.to_string())
            .unwrap_or_default(),
        keywords: dinfo
            .as_ref()
            .map(|d| {
                d.keywords()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default(),
        description: app.description().map(|s| s.to_string()).unwrap_or_default(),
    }
}

/// Score one app against a query using multiple weighted corpus fields.
///
/// Weights: `name` ×3, `generic_name` ×2, `keywords` ×2, `description` ×1.
/// Returns the best `(score, match_indices_in_name)` across all fields, or
/// `None` if no field matches. Match indices are always from the `name` field
/// scoring pass so they can be used for highlight rendering on the display label.
fn score_app(
    matcher: &SkimMatcherV2,
    data: &AppSearchData,
    query: &str,
) -> Option<(i64, Vec<usize>)> {
    // Score the name field to get both a score and the usable indices.
    let name_result = matcher
        .fuzzy_indices(&data.name, query)
        .map(|(s, idx)| (s * 3, idx));

    // Score the other fields for the weight bonus only; indices are discarded.
    let generic_score = matcher
        .fuzzy_match(&data.generic_name, query)
        .map_or(i64::MIN, |s| s * 2);
    let keywords_score = matcher
        .fuzzy_match(&data.keywords, query)
        .map_or(i64::MIN, |s| s * 2);
    let desc_score = matcher.fuzzy_match(&data.description, query).unwrap_or(i64::MIN);

    // Best score across all fields.
    let best_alt = [generic_score, keywords_score, desc_score]
        .iter()
        .copied()
        .max()
        .unwrap_or(i64::MIN);

    match name_result {
        Some((name_score, idx)) => {
            // Use name indices regardless; pick the higher score.
            let final_score = name_score.max(best_alt);
            Some((final_score, idx))
        }
        None if best_alt > i64::MIN => {
            // An alternative field matched but name did not — show with empty indices.
            Some((best_alt, vec![]))
        }
        None => None,
    }
}

// ── Main widget type ──────────────────────────────────────────────────────────

/// GTK3 renderer for the app launcher widget.
///
/// Renders as a single `gtk::Button` in the bar. Clicking it opens a standalone
/// dropdown `gtk::Window` containing a `gtk::SearchEntry` and a `gtk::ListBox` of
/// installed applications filtered by the current query. Selecting an entry
/// launches the application via [`gio::AppInfo::launch`].
///
/// The app list is refreshed automatically whenever the system app registry
/// changes via [`gio::AppInfoMonitor`].
///
/// A standalone window is used instead of `gtk::Popover` because the bar is a
/// dock window 30 px tall on X11 — GTK3 clips popovers to their parent window
/// bounds, preventing the popup from opening fully.
pub struct LauncherWidget {
    button: gtk::Button,
    /// Kept alive so the `changed` signal fires; never read after construction.
    _app_monitor: gio::AppInfoMonitor,
}

impl LauncherWidget {
    /// Create a new launcher renderer.
    ///
    /// Loads the installed application list via `gio::AppInfo::all()`, filters to
    /// visible apps (`should_show()`), builds the extended search corpus, and caches
    /// everything in `Rc<RefCell<_>>` for signal-handler sharing.
    ///
    /// Wires an [`gio::AppInfoMonitor`] `changed` handler that reloads the app list
    /// with a 500 ms debounce, keeping the popup current after software installs or
    /// uninstalls without restarting the bar.
    ///
    /// `config` controls the button label, popup dimensions, result cap, and pinned
    /// apps. All fields default gracefully when `None` / empty.
    ///
    /// # Errors
    ///
    /// Returns `anyhow::Error` if GTK widget construction fails (should not happen
    /// after `gtk::init()` succeeds, but the contract is consistent with other
    /// renderers).
    // clippy::unnecessary_wraps: consistent renderer contract — future display init may fail
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &LauncherConfig) -> anyhow::Result<Self> {
        let max_results: usize = config.max_results.unwrap_or(10) as usize;
        let popup_width = config.popup_width.unwrap_or(280);
        let popup_min_height = config.popup_min_height.unwrap_or(200);
        let label_text = config.button_label.as_deref().unwrap_or("Apps").to_owned();
        let pinned: Rc<Vec<String>> = Rc::new(config.pinned.clone());

        // Load installed apps once; build search corpus in parallel.
        let (initial_apps, initial_corpus) = load_apps();
        if initial_apps.is_empty() {
            tracing::warn!("launcher: no installed applications found via gio::AppInfo::all()");
        }
        let apps: Rc<RefCell<Vec<gio::AppInfo>>> = Rc::new(RefCell::new(initial_apps));
        let corpus: Rc<RefCell<Vec<AppSearchData>>> = Rc::new(RefCell::new(initial_corpus));

        // ── Button ────────────────────────────────────────────────────────
        let button = gtk::Button::with_label(&label_text);
        button.set_relief(gtk::ReliefStyle::None);
        button.set_widget_name("launcher");
        button.style_context().add_class("widget");
        button.style_context().add_class("widget-launcher");

        // Build the dropdown window and wire all non-monitor signals.
        wire_dropdown(
            &button,
            &apps,
            &corpus,
            &pinned,
            max_results,
            popup_width,
            popup_min_height,
        );

        // ── AppInfoMonitor: live app-list refresh with 500 ms debounce ────
        // GIO may fire multiple `changed` events during a single package
        // install (one per .desktop file). We cancel any pending reload before
        // scheduling a new one.
        let monitor = gio::AppInfoMonitor::get();
        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        {
            let apps_rc = Rc::clone(&apps);
            let corpus_rc = Rc::clone(&corpus);
            let pending_rc = Rc::clone(&pending);
            monitor.connect_changed(move |_| {
                if let Some(id) = pending_rc.borrow_mut().take() {
                    id.remove();
                }
                let apps_inner = Rc::clone(&apps_rc);
                let corpus_inner = Rc::clone(&corpus_rc);
                let new_id =
                    glib::timeout_add_local_once(Duration::from_millis(500), move || {
                        let (new_apps, new_corpus) = load_apps();
                        *apps_inner.borrow_mut() = new_apps;
                        *corpus_inner.borrow_mut() = new_corpus;
                        tracing::debug!("launcher: app list refreshed via AppInfoMonitor");
                    });
                *pending_rc.borrow_mut() = Some(new_id);
            });
        }

        Ok(Self { button, _app_monitor: monitor })
    }

    /// Return a reference to the root GTK widget (the `gtk::Button`) for bar placement.
    pub fn widget(&self) -> &gtk::Widget {
        self.button.upcast_ref()
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Build the dropdown window, wire all non-monitor signals, and connect the button.
///
/// Extracts the dropdown construction and all GTK signal wiring from `new()` so
/// that `new()` stays within the clippy function-line limit. Keyboard navigation
/// and list signals are further delegated to [`wire_window_keyboard`] and
/// [`wire_list_signals`].
#[allow(clippy::too_many_arguments)] // all args are required; no sensible grouping
fn wire_dropdown(
    button: &gtk::Button,
    apps: &Rc<RefCell<Vec<gio::AppInfo>>>,
    corpus: &Rc<RefCell<Vec<AppSearchData>>>,
    pinned: &Rc<Vec<String>>,
    max_results: usize,
    popup_width: i32,
    popup_min_height: i32,
) {
    // ── Dropdown window ───────────────────────────────────────────────────
    // Standalone window, not gtk::Popover. On X11/GTK3 a Popover renders
    // within the parent surface; a 30 px dock bar would clip it to 30 px.
    let dropdown = gtk::Window::new(gtk::WindowType::Toplevel);
    dropdown.set_decorated(false);
    dropdown.set_resizable(false);
    dropdown.set_skip_taskbar_hint(true);
    dropdown.set_skip_pager_hint(true);
    dropdown.set_type_hint(gdk::WindowTypeHint::PopupMenu);
    dropdown.set_default_size(popup_width, -1);
    dropdown.style_context().add_class("launcher-popover");

    // ── Inner layout ──────────────────────────────────────────────────────
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let search = gtk::SearchEntry::new();
    search.style_context().add_class("launcher-search");

    let scrolled = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scrolled.set_min_content_height(popup_min_height);
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

    let list = gtk::ListBox::new();
    list.style_context().add_class("launcher-list");
    list.set_activate_on_single_click(false);

    scrolled.add(&list);
    vbox.pack_start(&search, false, false, 0);
    vbox.pack_start(&scrolled, true, true, 0);
    dropdown.add(&vbox);

    wire_window_keyboard(&dropdown, &search, &list);
    wire_list_signals(&search, &list, &dropdown, apps, corpus, pinned, max_results);

    // ── Signal: button clicked → toggle dropdown ──────────────────────────
    let apps = Rc::clone(apps);
    let corpus = Rc::clone(corpus);
    let pinned = Rc::clone(pinned);
    button.connect_clicked(move |btn| {
        if dropdown.is_visible() {
            dropdown.hide();
            return;
        }
        rebuild_list(&list, &apps.borrow(), &corpus.borrow(), "", max_results, &pinned);
        search.set_text("");
        if let Some(gdk_win) = btn.window() {
            let (wx, wy, _) = gdk_win.origin();
            let alloc = btn.allocation();
            dropdown.move_(wx + alloc.x(), wy + alloc.y() + alloc.height());
        }
        dropdown.show_all();
        search.grab_focus();
    });
}

/// Wire Escape/delete-event on the dropdown window plus Down/Up keyboard
/// navigation between `search` and `list`.
fn wire_window_keyboard(
    dropdown: &gtk::Window,
    search: &gtk::SearchEntry,
    list: &gtk::ListBox,
) {
    dropdown.connect_delete_event(|win, _| {
        win.hide();
        glib::Propagation::Stop
    });

    dropdown.connect_key_press_event(|win, event| {
        if event.keyval() == gdk::keys::constants::Escape {
            win.hide();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    // Down arrow on the search entry → move focus into the list.
    {
        let list = list.clone();
        search.connect_key_press_event(move |_, event| {
            if event.keyval() == gdk::keys::constants::Down {
                list.child_focus(gtk::DirectionType::Down);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }

    // Up arrow on the first list row → return focus to the search entry.
    {
        let search = search.clone();
        list.connect_key_press_event(move |lst, event| {
            if event.keyval() == gdk::keys::constants::Up {
                let selected = lst.selected_row();
                let is_first = selected
                    .as_ref()
                    .and_then(|r| lst.row_at_index(0).map(|first| r == &first))
                    .unwrap_or(true);
                if is_first {
                    search.grab_focus();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
    }
}

/// Wire search-changed, search-activate, and row-activated signals.
#[allow(clippy::too_many_arguments)] // all args are required; no sensible grouping
fn wire_list_signals(
    search: &gtk::SearchEntry,
    list: &gtk::ListBox,
    dropdown: &gtk::Window,
    apps: &Rc<RefCell<Vec<gio::AppInfo>>>,
    corpus: &Rc<RefCell<Vec<AppSearchData>>>,
    pinned: &Rc<Vec<String>>,
    max_results: usize,
) {
    // Search text changed → filter list.
    {
        let list = list.clone();
        let apps = Rc::clone(apps);
        let corpus = Rc::clone(corpus);
        let pinned = Rc::clone(pinned);
        search.connect_changed(move |entry| {
            let query = entry.text().to_string();
            rebuild_list(&list, &apps.borrow(), &corpus.borrow(), &query, max_results, &pinned);
        });
    }

    // Enter on search → launch selected row (or first if none selected).
    {
        let list = list.clone();
        let dropdown = dropdown.clone();
        search.connect_activate(move |_| {
            let target = list.selected_row().or_else(|| list.row_at_index(0));
            if let Some(row) = target {
                if let Some(app) = get_row_app(&row) {
                    launch_app(&app);
                    dropdown.hide();
                }
            }
        });
    }

    // Row activated (click or Enter while row focused) → launch.
    {
        let dropdown = dropdown.clone();
        list.connect_row_activated(move |_, row| {
            if let Some(app) = get_row_app(row) {
                launch_app(&app);
                dropdown.hide();
            }
        });
    }
}

/// Load all visible installed applications and build their search corpus.
///
/// Calls `gio::AppInfo::all()`, filters to entries where `should_show()` is true,
/// and builds an [`AppSearchData`] for each entry.
///
/// Returns `(apps, corpus)` as parallel vecs of equal length with matching indices.
fn load_apps() -> (Vec<gio::AppInfo>, Vec<AppSearchData>) {
    let apps: Vec<gio::AppInfo> =
        gio::AppInfo::all().into_iter().filter(AppInfoExt::should_show).collect();
    let corpus: Vec<AppSearchData> = apps.iter().map(build_search_data).collect();
    (apps, corpus)
}

/// Rebuild the `gtk::ListBox` contents from `apps` filtered and scored by `query`.
///
/// Removes all existing rows, scores and sorts via multi-field fuzzy matching
/// (see [`score_app`]), then inserts up to `max_results` new rows.
///
/// When `query` is empty, pinned apps (by desktop-ID stem) are shown first in
/// config order, followed by the remainder in their natural order.
///
/// Each row stores its `AppInfo` via `GObject` `set_data` so signal handlers can
/// retrieve and launch it.
fn rebuild_list(
    list: &gtk::ListBox,
    apps: &[gio::AppInfo],
    corpus: &[AppSearchData],
    query: &str,
    max_results: usize,
    pinned: &[String],
) {
    // Remove all existing rows.
    for child in list.children() {
        list.remove(&child);
    }

    let matcher = SkimMatcherV2::default();

    if query.is_empty() {
        // Pinned apps first (config order), then the rest.
        let (pinned_apps, rest_apps) = partition_apps(apps, pinned);
        for app in pinned_apps.into_iter().chain(rest_apps).take(max_results) {
            add_row(list, app, &[]);
        }
    } else {
        // Score and filter.
        let mut scored: Vec<(i64, &gio::AppInfo, Vec<usize>)> = apps
            .iter()
            .zip(corpus.iter())
            .filter_map(|(app, data)| {
                score_app(&matcher, data, query).map(|(score, idx)| (score, app, idx))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));

        for (_, app, indices) in scored.into_iter().take(max_results) {
            add_row(list, app, &indices);
        }
    }

    list.show_all();
}

/// Append one app row to `list` with optional match highlight indices.
///
/// The row carries the `AppInfo` in `GObject` qdata under the key `"app-info"` so
/// activation signal handlers can retrieve and launch it.
fn add_row(list: &gtk::ListBox, app: &gio::AppInfo, indices: &[usize]) {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row_box.style_context().add_class("launcher-row");

    // Icon (16×16 SmallToolbar size; omitted silently if absent).
    if let Some(icon) = app.icon() {
        let img = gtk::Image::from_gicon(&icon, gtk::IconSize::SmallToolbar);
        row_box.pack_start(&img, false, false, 0);
    }

    let label = highlighted_label(app.name().as_ref(), indices);
    label.set_halign(gtk::Align::Start);
    row_box.pack_start(&label, true, true, 0);

    let row = gtk::ListBoxRow::new();
    row.add(&row_box);

    // SAFETY: The AppInfo clone is owned exclusively by this ListBoxRow via
    // GObject qdata. It is freed by GObject when the row is finalized, which
    // occurs before the Rc<RefCell<Vec<AppInfo>>> owning the original is dropped.
    unsafe {
        row.set_data("app-info", app.clone());
    }

    list.add(&row);
}

/// Build a GTK label with matched character positions rendered in bold.
///
/// `text` is the display string (the app name). `indices` are the 0-based
/// char-position offsets returned by [`SkimMatcherV2::fuzzy_indices`]. Returns a
/// plain `gtk::Label` when `indices` is empty.
///
/// # Security note
///
/// Every character is individually escaped through [`glib::markup_escape_text`]
/// before being inserted into the Pango markup string. App names may contain
/// `<`, `>`, or `&` — omitting the escape is a markup injection vector.
fn highlighted_label(text: &str, indices: &[usize]) -> gtk::Label {
    if indices.is_empty() {
        return gtk::Label::new(Some(text));
    }

    let mut markup = String::with_capacity(text.len() + indices.len() * 7);
    for (i, ch) in text.chars().enumerate() {
        let escaped = glib::markup_escape_text(&ch.to_string());
        if indices.binary_search(&i).is_ok() {
            markup.push_str("<b>");
            markup.push_str(&escaped);
            markup.push_str("</b>");
        } else {
            markup.push_str(&escaped);
        }
    }

    let label = gtk::Label::new(None);
    label.set_markup(&markup);
    label
}

/// Retrieve the cached `AppInfo` from a `ListBoxRow`.
///
/// Returns `None` if no app data is present (defensive; should not occur for
/// rows constructed by [`add_row`]).
fn get_row_app(row: &gtk::ListBoxRow) -> Option<gio::AppInfo> {
    // SAFETY: The AppInfo stored by `add_row` matches the type requested here.
    // The row is alive during signal dispatch, so the pointer is valid.
    unsafe { row.data::<gio::AppInfo>("app-info").map(|ptr| ptr.as_ref().clone()) }
}

/// Launch an application via GIO, logging a warning on failure.
///
/// Non-fatal — a failed launch does not crash the bar.
fn launch_app(app: &gio::AppInfo) {
    if let Err(e) = app.launch(&[], None::<&gio::AppLaunchContext>) {
        tracing::warn!(app = %app.name(), error = %e, "failed to launch application");
    }
}

/// Partition `apps` into (pinned, rest) according to `pinned` desktop-ID stems.
///
/// Pinned apps are returned in the order they appear in `pinned`. Apps that do
/// not match any pinned stem are returned in their original order.
fn partition_apps<'a>(
    apps: &'a [gio::AppInfo],
    pinned: &[String],
) -> (Vec<&'a gio::AppInfo>, Vec<&'a gio::AppInfo>) {
    let mut pinned_out: Vec<&gio::AppInfo> = Vec::with_capacity(pinned.len());
    for stem in pinned {
            if let Some(a) = apps.iter().find(|a| desktop_id_stem(a) == *stem) {
            pinned_out.push(a);
        }
    }
    let rest: Vec<&gio::AppInfo> = apps
        .iter()
        .filter(|a| !pinned.iter().any(|s| desktop_id_stem(a) == *s))
        .collect();
    (pinned_out, rest)
}

/// Extract the bare desktop-ID stem from an `AppInfo`.
///
/// Returns the `id()` string with any trailing `.desktop` suffix stripped.
/// Returns an empty string if the app has no ID.
fn desktop_id_stem(app: &gio::AppInfo) -> String {
    app.id()
        .map(|id| {
            let s = id.to_string();
            s.strip_suffix(".desktop").unwrap_or(&s).to_owned()
        })
        .unwrap_or_default()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    /// Verify the `.desktop` suffix stripping logic (no GTK required).
    #[test]
    fn desktop_id_stem_logic() {
        let raw = "org.mozilla.firefox.desktop";
        let stem = raw.strip_suffix(".desktop").unwrap_or(raw);
        assert_eq!(stem, "org.mozilla.firefox");

        let no_suffix = "firefox-nightly";
        assert_eq!(
            no_suffix.strip_suffix(".desktop").unwrap_or(no_suffix),
            "firefox-nightly"
        );
    }

    /// Verify `highlighted_label` produces a label whose visible text equals the
    /// input, even when markup indices are supplied.
    ///
    /// Skipped automatically in headless environments (no `$DISPLAY`).
    #[test]
    fn highlighted_label_markup() {
        if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
            return; // skip in headless CI
        }
        gtk::init().expect("gtk::init failed");
        // Index 0 → 'F' should be wrapped in <b>…</b>.
        let label = super::highlighted_label("Firefox", &[0]);
        // The label carries markup; its visible text is still "Firefox".
        use gtk::prelude::LabelExt;
        assert_eq!(label.text().as_str(), "Firefox");
    }
}
