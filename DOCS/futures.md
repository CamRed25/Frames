# Project Futures

Tracks ideas, technical debt, known limitations, and completed items.
Add entries with date stamps when they arise.

---

## Ideas & Enhancements

- **(2026-03-17)** Multi-monitor bar instances — v0.1.0 supports a single bar on one monitor. Future work: spawn one `Bar` per connected monitor, each reading monitor-specific geometry.
- **(2026-03-17)** Wayland support — explicitly out of scope per PLATFORM_COMPAT §2. Would require `wlr-layer-shell` protocol instead of `_NET_WM_STRUT_PARTIAL`. File here for future consideration.
- **(2026-03-17)** Plugin/widget system — allow third-party widgets to be loaded at runtime via a shared trait object interface. Users could drop a compiled `.so` into a well-known directory and declare the widget in config without recompiling the bar.
- **(2026-03-17)** Tray / system-tray widget — implement a `StatusNotifierItem` (SNI/`libdbusmenu`) host widget so the bar can display applet icons from running applications.
- **(2026-03-17)** Startup latency ceiling — timing instrumentation is in place (`RUST_LOG=debug`). Remaining: profile baseline on typical hardware and set a formal launch-latency ceiling (e.g. < 200 ms) to guard against regressions from new widgets or heavier init paths.
- **(2026-03-17)** Migrate rendering stack from GTK3 to GTK4 (`gtk4-rs`) — GTK3 is in maintenance mode; GTK4 brings GPU-accelerated rendering via the GSK scene graph, a cleaner widget API, and first-class Wayland support that would unblock the Wayland item above. Migration scope: replace `gtk` / `gdk` / `gio` crate dependencies in `parapet_bar` with `gtk4` / `gdk4`; rewrite `Bar` window creation using `gtk4::ApplicationWindow` and `gtk4::Application`; replace `gtk::Box` widget layout with `gtk4::Box`; port CSS theme loading to the updated `gtk4::CssProvider` API; replace `glib::timeout_add_local` timer calls (API is unchanged but must be re-verified); audit all `unsafe` X11/GDK blocks since `gdk4-x11` has a different surface model than GDK3. Wayland path would then use `gtk4-layer-shell` crate in place of the manual `_NET_WM_STRUT_PARTIAL` EWMH calls. `parapet_core` is unaffected — the display-system isolation already enforced by the crate boundary means no core changes are required.

---

## Theming

- **(2026-03-17)** Documented `@define-color` token palette — GTK3's `@define-color` / `@name` mechanism is already in use for the six-colour palette (`color-pill`, `color-fg`, `color-fg-dim`, `color-accent`, `color-warning`, `color-urgent`). Future work: formally document these names as the stable theming API in `UI_GUIDE §6.3`, add structural tokens (`border-radius`, `font-size`, `font-family`) so theme authors can restyle bar shape and typography without touching widget rules.

---

## Technical Debt & Refactoring

- **(2026-03-17)** `VolumeWidget` spawns two `pactl` subprocesses per poll cycle — ~~one for volume and one for mute state. This works but is heavier than necessary. Future improvement: subscribe to `pactl subscribe` events via a background thread and push updates through a channel, eliminating all periodic subprocess spawning.~~ **Completed — see Completed section.**
- **(2026-03-17)** `WorkspacesWidget` in `parapet_bar` self-polls X11 outside the `Poller` — the workspace renderer bypasses the standard `Widget` → `Poller` → renderer data flow and queries `_NET_NUMBER_OF_DESKTOPS` / `_NET_CURRENT_DESKTOP` directly on its own glib timer. This creates an architectural inconsistency. Future resolution: explore X11 property-change event subscription so workspaces are event-driven rather than polled, and unify the update path.
- **(2026-03-17)** CSS hot-reload does not follow `bar.theme` changes in config hot-reload — ~~if the user edits `config.toml` to change `bar.theme = "newtheme"`, the CSS watcher remains on the old CSS file until the bar is restarted. Fixing this would require wiring a new `ConfigWatcher` inside the config-reload handler.~~ **Completed — see Completed section.**
- **(2026-03-17)** `LauncherWidget` bypasses the Poller pipeline — ~~the app list is loaded once at startup and never refreshed. Newly installed apps require restarting the bar. Future work: watch XDG data dirs for `.desktop` file changes via `notify` and reload the cache, or add a manual refresh keybind.~~ **Completed — `gio::AppInfoMonitor` singleton wired in `LauncherWidget::new()` (plan §C3). The monitor fires on the GTK main thread and reloads `AppInfo::all()` with a 500 ms debounce after any change. No `notify` dep needed.**
- **(2026-03-17)** Hot-reload rebuilds the entire widget tree on any config change — a future improvement could diff old vs new `WidgetConfig` vectors and only reconstruct changed widgets, reducing visual flicker during reload.
- **(2026-03-18)** Config Phase 2 — ~~migrate from flat `WidgetConfig` to a typed `WidgetKind` enum with per-widget config structs and `#[serde(deny_unknown_fields)]`. Eliminates the entire class of silent "wrong field on wrong widget" misconfigurations. See `DOCS/research/config-overhaul.md §Problem 1` for full design. Target: v0.2.0. Requires one-time mechanical migration of all `match config.widget_type.as_str()` dispatch sites in `main.rs`.~~ **Completed — see Completed section.**
- **(2026-03-18)** Config migration infrastructure — add `config_version` field to `[bar]` and a migration chain so users get a readable error (and automatic upgrade) when breaking config changes ship. See `config-overhaul.md §Problem 5`. Depends on Phase 2 landing first.

---

## Known Limitations

- Only a single monitor is supported per bar process.
- Wayland is not supported.

---

## Completed / Integrated

- **(2026-03-17 → 2026-03-17)** Config hot-reload via `notify` watcher — `ConfigWatcher` added to `parapet_core::config`; `parapet_bar` wires a 500 ms glib timer that calls `ConfigWatcher::has_changed`, rebuilds the widget tree on modification, and cancels any widget-owned self-poll timers (e.g. workspaces) before clearing the bar.
- **(2026-03-17 → 2026-03-17)** Per-widget click/scroll actions — `WidgetConfig` extended with `on_click`, `on_scroll_up`, and `on_scroll_down` (`Option<String>`). `add_to_bar()` in `main.rs` wraps any widget with configured actions in a `gtk::EventBox` and connects `connect_button_press_event` / `connect_scroll_event` handlers that spawn `sh -c <cmd>` asynchronously via `spawn_shell()`.
- **(2026-03-17 → 2026-03-17)** Workspace click-to-switch — implemented in `WorkspacesWidget` (`parapet_bar`): each workspace button has a `connect_clicked` handler that calls `send_desktop_change()`, which posts `_NET_CURRENT_DESKTOP` to the root window to switch the active workspace.
- **(2026-03-17 → 2026-03-17)** Volume widget — `parapet_core::widgets::volume::VolumeWidget` queries `pactl get-sink-volume` and `pactl get-sink-mute`; falls back to last known value on `pactl` absence. `parapet_bar::widgets::volume::VolumeWidget` renders as `🔊 VOL 70%` / `🔇 VOL MUTE`, adds `.muted` CSS class when muted.
- **(2026-03-17 → 2026-03-17)** Brightness widget — `parapet_core::widgets::brightness::BrightnessWidget` reads from `/sys/class/backlight/<first-entry>/brightness` and `max_brightness`; returns `0.0` on machines without a variable backlight. `parapet_bar::widgets::brightness::BrightnessWidget` renders as `☀ BRI 75%` or `BRI 75%`.
- **(2026-03-17 → 2026-03-17)** Named theme support + `--theme` CLI flag — `BarConfig.theme: Option<String>` added; `css.rs` refactored around `ThemeSource<'a>` enum with `Named`, `Path`, and `Default` variants; `--theme <name>` CLI arg parsed in `main.rs`; `resolve_theme_path()` resolves to `~/.config/parapet/themes/<name>.css` with path-traversal guard.
- **(2026-03-17 → 2026-03-17)** Community theme spec — `DOCS/theme-spec.md` documents directory layout, dark/light variant naming, metadata file format, naming rules, CSS colour-token contract, hot-reload behaviour, and sharing guidance.
- **(2026-03-17 → 2026-03-17)** Per-widget CSS class injection — `WidgetConfig.extra_class: Option<String>` added; `add_to_bar()` applies the class to the widget's outermost GTK container (EventBox when actions are configured, widget itself otherwise).
- **(2026-03-17 → 2026-03-17)** GTK3 `@define-color` colour tokens — `themes/default.css` defines six named colours (`color-pill`, `color-fg`, `color-fg-dim`, `color-accent`, `color-warning`, `color-urgent`) using GTK3's `@define-color` syntax; widget rules reference them via `@name`. (Earlier attempt using CSS custom properties and `*` selector was invalid GTK3 CSS and was replaced.)
- **(2026-03-17 → 2026-03-17)** System colour scheme integration — `resolve_theme_variant()` in `css.rs` reads `GtkSettings::is_gtk_application_prefer_dark_theme()` and appends `-dark` or `-light` to the theme name when a matching variant file exists.
- **(2026-03-17 → 2026-03-17)** Theme hot-reload — `CssProvider` stored in `Rc<RefCell<>>` in `main.rs`; a second `ConfigWatcher` on the active CSS file is wired into the existing 500 ms glib timer; on change, `remove_provider()` + `load_theme()` + `apply_provider()` replaces the active theme without restarting.
- **(2026-03-17 → 2026-03-17)** Startup timing instrumentation — five `tracing::debug!` checkpoints added to `main()` at config load, GTK init, bar window creation, widget build, and main-loop entry. Silent by default; visible with `RUST_LOG=parapet_bar=debug`.
- **(2026-03-17 → 2026-03-17)** Weather widget — `parapet_core::widgets::weather::WeatherWidget` fetches current conditions (temperature, WMO weather code, wind speed, humidity) from the Open-Meteo free API via `ureq`; no API key required; 30-minute default interval; stale-cache fallback on HTTP failure. `parapet_bar::widgets::weather::WeatherWidget` renders as `"⛅ 12°C 💨18"` with WMO-code icon mapping. Config fields: `latitude`, `longitude`, `units`.
- **(2026-03-17 → 2026-03-17)** MPRIS media widget — `parapet_core::widgets::media::MediaWidget` queries the first `org.mpris.MediaPlayer2.*` player on the D-Bus session bus via `zbus` (blocking API); fetches all player properties in a single `GetAll` call; stale-cache fallback on D-Bus error; idle/absent-player condition returns `Stopped` silently. `parapet_bar::widgets::media::MediaWidget` renders `"▶ title — artist"` (Playing), `"⏸ title"` (Paused), or empty (Stopped), with 30-char truncation. Controls wired via `on_click`/`on_scroll_*` to `playerctl` commands.
- **(2026-03-17 → 2026-03-17)** Configurable separator glyph — separator widget reads `config.format` (default `"|"`); any UTF-8 glyph can be set in config TOML via `format = "·"`.
- **(2026-03-18 → Jan 2025)** CSS hot-reload now follows `bar.theme` config changes — `resolve_active_css_path()` helper extracted in `main.rs`; `active_css_path` and `css_watcher` promoted to `Rc<RefCell<>>`; config-reload glib branch calls `resolve_active_css_path()` on the new config, diffs paths, and swaps the CSS provider and `ConfigWatcher` in-place when the theme changed. No bar restart required.
- **(2026-03-18 → Jan 2025)** Config Phase 2 — `WidgetConfig` migrated to a typed `WidgetKind` enum (`#[serde(tag = "type", rename_all = "lowercase")]`) with 13 per-widget config structs (`ClockConfig`, `CpuConfig`, `MemoryConfig`, `NetworkConfig`, `BatteryConfig`, `DiskConfig`, `VolumeConfig`, `BrightnessConfig`, `WeatherConfig`, `MediaConfig`, `WorkspacesConfig`, `LauncherConfig`, `SeparatorConfig`). Each variant struct carries `#[serde(deny_unknown_fields)]` — misplaced fields are rejected at parse time. `validate()` rewritten to exhaustive `match &mut w.kind` dispatch. All `parapet_bar` widget renderers updated to accept typed config structs. `ParapetConfig` now derives `Serialize` for round-trip testing.
- **(2026-03-18 → Jan 2025)** `MonitorTarget` JSON Schema fixed — removed `#[derive(JsonSchema)]` and `// FIXME(schema)` comment; manual `JsonSchema` impl using `SubschemaValidation::one_of` correctly describes the `"primary"` string and integer-index union.
- **(2026-03-17 → 2026-03-18)** `VolumeWidget` subprocess optimization — `pactl get-sink-info` replaces the two separate `pactl get-sink-volume` + `pactl get-sink-mute` calls (Part 1: single subprocess per poll). `pactl subscribe` background thread + `Mutex<Receiver<VolumeData>>` mpsc channel eliminates per-poll spawning entirely (Part 2: event-driven). `update()` drains the channel and returns the last cached `VolumeData` when no events have arrived. Parse helpers `parse_volume_pct` and `parse_mute` are unit-tested against fixture text without requiring `pactl` in CI. See `DOCS/research/volume-event-driven.md` for full design rationale.
