# Architectural Decision Records

Records closed architectural decisions for Parapet. Once closed, a decision is
not re-litigated. Open the entry and document a superseding decision if
circumstances change.

---

## ADR-001: GTK3 over GTK4

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Use GTK3 (`gtk-rs ~0.18`) as the UI toolkit.
- **Rationale:** Cinnamon is a GTK3 desktop. Using GTK4 would require cross-toolkit process isolation and introduce significant complexity for no user-visible gain. GTK 3.22 is the floor (PLATFORM_COMPAT §4); all supported Cinnamon installations provide a compatible GTK3.
- **Consequences:** GTK4 APIs are not available. `gtk::deprecated` warnings for GTK3 functions must be suppressed with explanatory `#[allow]` comments if encountered.

---

## ADR-002: X11-only; Wayland out of scope

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Parapet requires X11. Wayland is not supported.
- **Rationale:** The bar relies on `_NET_WM_STRUT_PARTIAL` and `_NET_WM_WINDOW_TYPE_DOCK` (EWMH X11 properties) to reserve screen space and prevent window overlap. These properties do not exist in the Wayland protocol. The equivalent (`wlr-layer-shell`) is compositor-specific and would require a separate implementation surface. Primary target is Cinnamon on Fedora — an X11 environment (PLATFORM_COMPAT §2).
- **Consequences:** `DISPLAY` must be set when launching Parapet. Wayland sessions are unsupported.

---

## ADR-003: `sysinfo` as the system data crate

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Use the `sysinfo` crate (~0.30) for CPU, memory, and network statistics.
- **Rationale:** Provides CPU usage, per-core breakdown, total/used RAM, swap, and network interface rx/tx in a single crate with a consistent cross-platform API. /proc parsing is handled internally. Alternatives (procfs, psutil-like manual reads) would require maintaining per-distro parsing logic. `sysinfo`'s refresh model aligns with Parapet' polling architecture.
- **Consequences:** CPU and network widgets require two refresh calls separated by a real time interval to compute a delta; first call returns zero values (WIDGET_API §7.3). Minimum effective poll interval is ~250ms.

---

## ADR-004: `gdk::property_change` / `gdk::property_get` for EWMH — no gdk-x11 crate

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Use `gdk::property_change()` and `gdk::property_get()` (from the `gdk` crate itself, via `gdk/src/auto/functions.rs`) for all X11 EWMH property reads and writes. Do not add a `gdk-x11` or `x11` crate dependency.
- **Rationale:** `gtk-rs ~0.18` exposes `gdk::property_change()` and `gdk::property_get()` directly — no extra crate is required. The `gdk-x11` crate in the gtk-rs 0.18 ecosystem is a thin wrapper with the same underlying FFI. Adding it would be redundant. Using `gdk::property_change` with `gdk::ChangeData::ULongs(&[c_ulong])` and `gdk::PropMode::Replace` is sufficient for setting `_NET_WM_STRUT_PARTIAL`, `_NET_WM_WINDOW_TYPE_DOCK`, and `_NET_CURRENT_DESKTOP`.
- **Consequences:** Monitor geometry must be obtained via `display.primary_monitor().geometry()` (not `Screen::monitor_geometry()`, which does not exist in gdk 0.18). Root window access uses `gdk::Window::default_root_window()` (infallible) rather than `Screen::root_window()` (returns `Option`).

---

## ADR-005: `gio::AppInfo` for installed app enumeration

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Use `gio::AppInfo::all()` for enumerating installed applications in the launcher widget.
- **Rationale:** `gio` is already in the dependency graph as a transitive dependency of `gtk ~0.18`. No new crate is required. `gio::AppInfo` correctly handles `NoDisplay`, `OnlyShowIn`, and `Hidden` desktop entry fields via `should_show()`. The `launch()` method handles Exec field substitution correctly per the freedesktop.org Desktop Entry specification.
- **Rejected:** `freedesktop-desktop-entry` (MPL-2.0, redundant new dep with no advantages).
- **Consequences:** The app list is loaded once at startup. Newly installed apps require restarting the bar. See `DOCS/futures.md` for the planned XDG data dir watcher improvement.

---

## ADR-006: `fuzzy-matcher v0.3` for launcher search

- **Date:** 2026-03-17
- **Status:** Closed
- **Decision:** Use `fuzzy-matcher = "~0.3"` (`SkimMatcherV2`) for fuzzy-filtering the launcher app list.
- **Rationale:** MIT license; 14.9M downloads; same scoring algorithm as fzf/skim; stable API since 2022. `fuzzy_match()` returns a score that sorts correctly for relevance ranking. The API is a single trait (`FuzzyMatcher`) with no async or unsafe surface.
- **Rejected:** `nucleo-matcher` (MPL-2.0 license); `skim` (terminal TUI library, not embeddable in GTK).
- **Consequences:** `fuzzy-matcher` had its last release in 2022. It is considered stable and sufficient. MIT license permits forking if maintenance is ever required.
