//! `parapet_bar` — GTK3 status bar binary for the Cinnamon desktop.
//!
//! Startup sequence:
//! 1. Initialize tracing
//! 2. Check for early-exit subcommands (`--init-config`, `--dump-schema`), then
//!    parse `--theme <name>` CLI override
//! 3. Load `ParapetConfig` from TOML (env `PARAPET_CONFIG` or default path)
//! 4. Initialize GTK3
//! 5. Create the `Bar` window
//! 6. Resolve theme source; detect dark/light variant; load and apply CSS
//! 7. Build widget renderers from config; register core widgets with `Poller`
//! 8. Wire glib timer to poll widgets and update renderers
//! 9. Register SIGTERM handler for clean shutdown
//! 10. Start config hot-reload watcher; start CSS file hot-reload watcher
//! 11. Enter `gtk::main()`

mod bar;
mod css;
mod widgets;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Context;
use gdk::prelude::*;
use glib::ControlFlow;
use gtk::prelude::*;

use parapet_core::config::{BarSection, ConfigWatcher, WidgetConfig, WidgetKind};
use parapet_core::{ParapetConfig, Poller, WidgetData};

/// Resolve the active CSS file path from the current config and CLI theme override.
///
/// Returns `Some(path)` when a named theme or explicit `bar.css` path resolves
/// to a file, `None` when the built-in default theme is active. The returned
/// path is suitable for [`css::load_theme`] and [`parapet_core::config::ConfigWatcher::new`].
///
/// Must be called after `gtk::init()` when a theme name is involved, because
/// [`css::resolve_theme_variant`] reads GTK settings to select dark/light variants.
///
/// # Parameters
/// - `config` — current [`ParapetConfig`]; reads `bar.theme` and `bar.css`.
/// - `cli_theme` — optional `--theme` CLI override; takes precedence over `bar.theme`.
fn resolve_active_css_path(config: &ParapetConfig, cli_theme: Option<&str>) -> Option<PathBuf> {
    let effective_name = cli_theme.or(config.bar.theme.as_deref()).map(css::resolve_theme_variant);
    match effective_name.as_deref() {
        Some(name) => {
            let p = css::resolve_theme_path(name);
            if p.as_os_str().is_empty() {
                None
            } else {
                Some(p)
            }
        }
        None => config.bar.css.as_deref().map(PathBuf::from),
    }
}

#[allow(clippy::too_many_lines)] // GTK startup sequence is inherently sequential; extracting into subfunctions would obscure the ordered startup steps
fn main() -> anyhow::Result<()> {
    let t0 = Instant::now();

    // ── 1. Tracing ──────────────────────────────────────────────────────────
    tracing_subscriber::fmt::init();

    // ── 2. CLI args ──────────────────────────────────────────────────────────
    // Collect once; used for early-exit subcommands and --theme parsing.
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Early-exit subcommands — run before config load and before GTK init.
    if raw_args.contains(&"--init-config".to_string()) {
        init_config().context("--init-config failed")?;
        return Ok(());
    }
    if raw_args.contains(&"--dump-schema".to_string()) {
        dump_schema();
        return Ok(());
    }

    // `--theme <name>` overrides bar.theme and bar.css in config.
    let cli_theme: Option<String> = {
        let mut iter = raw_args.iter();
        let mut found = None;
        while let Some(arg) = iter.next() {
            if arg == "--theme" {
                found = iter.next().cloned();
                break;
            }
        }
        found
    };

    // ── 2. Config ───────────────────────────────────────────────────────────
    let config_path = std::env::var("PARAPET_CONFIG")
        .map_or_else(|_| ParapetConfig::default_path(), std::path::PathBuf::from);

    let config = match ParapetConfig::load(&config_path) {
        Ok(c) => c, // validate() (including path expansion) is called inside load()
        Err(parapet_core::ParapetConfigError::NotFound { path }) => {
            anyhow::bail!(
                "config file not found: {}\n\
                 Create it or set PARAPET_CONFIG to point to your config.",
                path.display()
            );
        }
        Err(e) => return Err(e).context("failed to load config"),
    };
    tracing::debug!("config loaded ({:.1}ms)", t0.elapsed().as_secs_f64() * 1000.0);

    // ── 5. GTK init ─────────────────────────────────────────────────────────
    gtk::init().context("failed to initialize GTK")?;
    tracing::debug!("GTK initialised ({:.1}ms)", t0.elapsed().as_secs_f64() * 1000.0);

    // ── 6. Bar window ───────────────────────────────────────────────────────
    let bar_rc = Rc::new(bar::Bar::new(&config.bar));
    tracing::debug!("bar window created ({:.1}ms)", t0.elapsed().as_secs_f64() * 1000.0);

    // ── 7. CSS theme — resolve source, detect dark/light variant, load ──────
    //
    // Priority: --theme CLI > bar.theme config > bar.css raw path > built-in.
    // After GTK init so that resolve_theme_variant can read GtkSettings.
    let initial_css_path = resolve_active_css_path(&config, cli_theme.as_deref());
    let theme_source = match &initial_css_path {
        Some(p) => css::ThemeSource::Path(p.as_path()),
        None => css::ThemeSource::Default,
    };
    let provider: Rc<RefCell<gtk::CssProvider>> =
        Rc::new(RefCell::new(css::load_theme(theme_source)));
    css::apply_provider(&provider.borrow());

    // ── 8. Widgets ──────────────────────────────────────────────────────────
    let poller: Rc<RefCell<Poller>> = Rc::new(RefCell::new(Poller::new()));
    let renderers: Rc<RefCell<Vec<Rc<dyn RendererDispatch>>>> = Rc::new(RefCell::new(Vec::new()));
    let self_poll_ids: Rc<RefCell<Vec<glib::SourceId>>> = Rc::new(RefCell::new(Vec::new()));

    build_all_widgets(
        &bar_rc,
        &mut poller.borrow_mut(),
        &mut renderers.borrow_mut(),
        &mut self_poll_ids.borrow_mut(),
        &config,
    );
    tracing::debug!("widgets built ({:.1}ms)", t0.elapsed().as_secs_f64() * 1000.0);

    // ── 9. Polling timer ────────────────────────────────────────────────────
    // Tick every 100 ms; each widget fires only when its own interval elapses.
    {
        let poller = Rc::clone(&poller);
        let renderers = Rc::clone(&renderers);
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let results = poller.borrow_mut().poll(Instant::now());
            for (name, data) in results {
                for renderer in renderers.borrow().iter() {
                    renderer.dispatch(&name, &data);
                }
            }
            ControlFlow::Continue
        });
    }

    // ── 10. SIGTERM handler ─────────────────────────────────────────────────
    let (sig_tx, sig_rx) = mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = sig_tx.send(());
    })
    .context("failed to install signal handler")?;

    glib::timeout_add_local(Duration::from_millis(100), move || {
        if sig_rx.try_recv().is_ok() {
            tracing::info!("received shutdown signal; exiting");
            gtk::main_quit();
            return ControlFlow::Break;
        }
        ControlFlow::Continue
    });

    // ── 11. Config hot-reload watcher + CSS file hot-reload watcher ─────────
    let config_watcher = match ConfigWatcher::new(&config_path) {
        Ok(w) => {
            tracing::info!(path = %config_path.display(), "config watcher started");
            Some(w)
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not start config watcher; hot-reload disabled");
            None
        }
    };

    let active_css_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(initial_css_path));
    // Optional watcher on the active CSS file (only when a file-based theme is used).
    let css_watcher: Rc<RefCell<Option<ConfigWatcher>>> =
        Rc::new(RefCell::new(active_css_path.borrow().as_deref().and_then(|p| {
            match ConfigWatcher::new(p) {
                Ok(w) => {
                    tracing::info!(path = %p.display(), "CSS theme watcher started");
                    Some(w)
                }
                Err(e) => {
                    tracing::warn!(error = %e, path = %p.display(),
                    "could not start CSS theme watcher; CSS hot-reload disabled");
                    None
                }
            }
        })));

    if config_watcher.is_some() || css_watcher.borrow().is_some() {
        let bar = Rc::clone(&bar_rc);
        let poller = Rc::clone(&poller);
        let renderers = Rc::clone(&renderers);
        let self_poll_ids = Rc::clone(&self_poll_ids);
        let config_path = config_path.clone();
        let provider = Rc::clone(&provider);
        let active_css_path_rc = Rc::clone(&active_css_path);
        let css_watcher_rc = Rc::clone(&css_watcher);
        let mut last_reload = Instant::now();

        glib::timeout_add_local(Duration::from_millis(500), move || {
            // Config change — rebuild widget tree.
            if let Some(ref watcher) = config_watcher {
                if watcher.has_changed() && last_reload.elapsed() >= Duration::from_millis(500) {
                    match ParapetConfig::load(&config_path) {
                        Ok(new_config) => {
                            // Cancel widget-owned self-poll timers before clearing the bar.
                            {
                                let mut ids = self_poll_ids.borrow_mut();
                                for id in ids.drain(..) {
                                    id.remove();
                                }
                            }

                            bar.clear_widgets();

                            let mut new_poller = Poller::new();
                            let mut new_renderers = Vec::new();
                            build_all_widgets(
                                &bar,
                                &mut new_poller,
                                &mut new_renderers,
                                &mut self_poll_ids.borrow_mut(),
                                &new_config,
                            );
                            *poller.borrow_mut() = new_poller;
                            *renderers.borrow_mut() = new_renderers;

                            // Swap CSS provider and watcher if the theme path changed.
                            let new_css =
                                resolve_active_css_path(&new_config, cli_theme.as_deref());
                            if new_css != *active_css_path_rc.borrow() {
                                let new_provider = match &new_css {
                                    Some(p) => css::load_theme(css::ThemeSource::Path(p.as_path())),
                                    None => css::load_theme(css::ThemeSource::Default),
                                };
                                css::remove_provider(&provider.borrow());
                                css::apply_provider(&new_provider);
                                *provider.borrow_mut() = new_provider;
                                *css_watcher_rc.borrow_mut() =
                                    new_css.as_deref().and_then(|p| ConfigWatcher::new(p).ok());
                                *active_css_path_rc.borrow_mut() = new_css;
                                tracing::info!("CSS theme swapped on config reload");
                            }

                            bar.show();
                            last_reload = Instant::now();
                            tracing::info!("config reloaded successfully");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "config reload failed; keeping current config");
                        }
                    }
                }
            }

            // CSS file change — reapply theme without restarting.
            {
                let css_w_borrow = css_watcher_rc.borrow();
                if let Some(ref css_w) = *css_w_borrow {
                    if css_w.has_changed() {
                        if let Some(ref css_path) = *active_css_path_rc.borrow() {
                            let new_provider =
                                css::load_theme(css::ThemeSource::Path(css_path.as_path()));
                            css::remove_provider(&provider.borrow());
                            css::apply_provider(&new_provider);
                            *provider.borrow_mut() = new_provider;
                            tracing::info!(path = %css_path.display(), "CSS theme hot-reloaded");
                        }
                    }
                }
            }

            ControlFlow::Continue
        });
    }

    // ── 12. Bar show + main loop ─────────────────────────────────────────────
    bar_rc.show();
    tracing::debug!(
        "bar ready, entering main loop ({:.1}ms total)",
        t0.elapsed().as_secs_f64() * 1000.0
    );
    gtk::main();
    Ok(())
}

// ── Widget renderer dispatch ─────────────────────────────────────────────────

/// Object-safe dispatch trait so renderers of different types can live in a
/// `Vec<Rc<dyn RendererDispatch>>`.
trait RendererDispatch {
    /// Dispatch new widget data to this renderer if the widget name matches.
    fn dispatch(&self, name: &str, data: &WidgetData);
}
struct ClockRenderer {
    name: String,
    renderer: widgets::clock::ClockWidget,
}

impl RendererDispatch for ClockRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct CpuRenderer {
    name: String,
    renderer: widgets::cpu::CpuWidget,
}

impl RendererDispatch for CpuRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct MemoryRenderer {
    name: String,
    renderer: widgets::memory::MemoryWidget,
}

impl RendererDispatch for MemoryRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct NetworkRenderer {
    name: String,
    renderer: widgets::network::NetworkWidget,
}

impl RendererDispatch for NetworkRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct BatteryRenderer {
    name: String,
    renderer: widgets::battery::BatteryWidget,
}

impl RendererDispatch for BatteryRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct VolumeRenderer {
    name: String,
    renderer: widgets::volume::VolumeWidget,
}

impl RendererDispatch for VolumeRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct BrightnessRenderer {
    name: String,
    renderer: widgets::brightness::BrightnessWidget,
}

impl RendererDispatch for BrightnessRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct WeatherRenderer {
    name: String,
    renderer: widgets::weather::WeatherWidget,
}

impl RendererDispatch for WeatherRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct MediaRenderer {
    name: String,
    renderer: widgets::media::MediaWidget,
}

impl RendererDispatch for MediaRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

struct DiskRenderer {
    name: String,
    renderer: widgets::disk::DiskWidget,
}

impl RendererDispatch for DiskRenderer {
    fn dispatch(&self, name: &str, data: &WidgetData) {
        if name == self.name {
            self.renderer.update(data);
        }
    }
}

// ── Widget factory ───────────────────────────────────────────────────────────

/// Return type for [`build_widget`]: `(renderer, self_poll_source_id)`.
///
/// `renderer` is `Some` for polled widgets. `source_id` is `Some` for
/// self-polling widgets (e.g. workspaces) whose glib timer must be
/// cancelled before a hot-reload.
type BuildWidgetResult = anyhow::Result<(Option<Rc<dyn RendererDispatch>>, Option<glib::SourceId>)>;

/// Build all widgets from `config`, register polled widgets with `poller`,
/// accumulate renderers into `renderers`, and store any widget-owned
/// `glib::SourceId`s (for cancellation on reload) into `self_poll_ids`.
///
/// Widget construction failures are logged as `WARN` and skipped; a single
/// broken widget config does not block the whole bar.
fn build_all_widgets(
    bar: &bar::Bar,
    poller: &mut Poller,
    renderers: &mut Vec<Rc<dyn RendererDispatch>>,
    self_poll_ids: &mut Vec<glib::SourceId>,
    config: &ParapetConfig,
) {
    for widget_config in &config.widgets {
        match build_widget(bar, poller, widget_config) {
            Ok((maybe_renderer, maybe_id)) => {
                if let Some(renderer) = maybe_renderer {
                    renderers.push(renderer);
                }
                if let Some(id) = maybe_id {
                    self_poll_ids.push(id);
                }
            }
            Err(e) => {
                tracing::warn!(
                    widget = widget_kind_name(&widget_config.kind),
                    error = %e,
                    "failed to create widget; skipping"
                );
            }
        }
    }
}

/// Return the string name of a [`WidgetKind`] variant, matching the TOML `type` key.
fn widget_kind_name(kind: &WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Clock(_) => "clock",
        WidgetKind::Cpu(_) => "cpu",
        WidgetKind::Memory(_) => "memory",
        WidgetKind::Network(_) => "network",
        WidgetKind::Battery(_) => "battery",
        WidgetKind::Disk(_) => "disk",
        WidgetKind::Volume(_) => "volume",
        WidgetKind::Brightness(_) => "brightness",
        WidgetKind::Weather(_) => "weather",
        WidgetKind::Media(_) => "media",
        WidgetKind::Workspaces(_) => "workspaces",
        WidgetKind::Launcher(_) => "launcher",
        WidgetKind::Separator(_) => "separator",
    }
}

/// Build one widget from config, register with the poller, add to the bar.
///
/// Returns a pair `(renderer, source_id)`. `renderer` is `Some` for polled
/// widgets. `source_id` is `Some` for self-polling widgets (e.g. workspaces)
/// and holds the glib timer [`glib::SourceId`] for later cancellation on
/// hot-reload. Returns `Err` if widget construction fails.
///
/// # Errors
///
/// Returns an error if the renderer or core widget construction fails.
#[allow(clippy::too_many_lines)] // match arms are the canonical widget registry; refactoring into subfunctions would obscure the pattern
fn build_widget(bar: &bar::Bar, poller: &mut Poller, config: &WidgetConfig) -> BuildWidgetResult {
    let section = config.position.clone();
    let name = config
        .label
        .clone()
        .unwrap_or_else(|| widget_kind_name(&config.kind).to_string());

    match &config.kind {
        WidgetKind::Clock(clock) => {
            let interval = config.interval.unwrap_or(1000);
            let format = clock.format.clone().unwrap_or_else(|| "%H:%M:%S".to_string());

            let core_widget = parapet_core::widgets::clock::ClockWidget::new(&name, &format);
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::clock::ClockWidget::new(clock)
                .context("clock renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(ClockRenderer { name, renderer })), None))
        }
        WidgetKind::Cpu(cpu) => {
            let interval = config.interval.unwrap_or(2000);

            let core_widget = parapet_core::widgets::cpu::CpuWidget::new(&name)?;
            poller.register(Box::new(core_widget), interval);

            let renderer =
                widgets::cpu::CpuWidget::new(cpu).context("cpu renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(CpuRenderer { name, renderer })), None))
        }
        WidgetKind::Memory(memory) => {
            let interval = config.interval.unwrap_or(5000);

            let core_widget = parapet_core::widgets::memory::MemoryWidget::new(&name)?;
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::memory::MemoryWidget::new(memory)
                .context("memory renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(MemoryRenderer { name, renderer })), None))
        }
        WidgetKind::Network(network) => {
            let interval = config.interval.unwrap_or(2000);

            let interface = network.interface.clone().unwrap_or_else(|| "eth0".to_string());
            let core_widget = parapet_core::widgets::network::NetworkWidget::new(&name, &interface)?;
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::network::NetworkWidget::new(network)
                .context("network renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(NetworkRenderer { name, renderer })), None))
        }
        WidgetKind::Battery(battery) => {
            let interval = config.interval.unwrap_or(30_000);

            let core_widget = parapet_core::widgets::battery::BatteryWidget::new(&name);
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::battery::BatteryWidget::new(battery)
                .context("battery renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(BatteryRenderer { name, renderer })), None))
        }
        WidgetKind::Volume(volume) => {
            let interval = config.interval.unwrap_or(2000);

            let core_widget = parapet_core::widgets::volume::VolumeWidget::new(&name);
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::volume::VolumeWidget::new(volume)
                .context("volume renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(VolumeRenderer { name, renderer })), None))
        }
        WidgetKind::Brightness(brightness) => {
            let interval = config.interval.unwrap_or(5000);

            let core_widget = parapet_core::widgets::brightness::BrightnessWidget::new(&name);
            poller.register(Box::new(core_widget), interval);

            let renderer = widgets::brightness::BrightnessWidget::new(brightness)
                .context("brightness renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);

            Ok((Some(Rc::new(BrightnessRenderer { name, renderer })), None))
        }
        WidgetKind::Workspaces(_) => {
            // WorkspacesWidget is self-polling; it manages its own glib timer.
            let renderer = Rc::new(
                widgets::workspaces::WorkspacesWidget::new()
                    .context("workspaces renderer construction failed")?,
            );
            add_to_bar(bar, renderer.widget(), config, &section);

            // Initial fill.
            renderer.refresh();

            // Self-polling timer: refresh workspace buttons every 500 ms.
            // The SourceId is returned so it can be cancelled on hot-reload.
            let renderer_clone = Rc::clone(&renderer);
            let interval_ms = config.interval.unwrap_or(500);
            let source_id =
                glib::timeout_add_local(Duration::from_millis(interval_ms), move || {
                    renderer_clone.refresh();
                    ControlFlow::Continue
                });

            // Workspaces does not participate in the Poller dispatch loop.
            Ok((None, Some(source_id)))
        }
        WidgetKind::Launcher(launcher) => {
            // LauncherWidget is self-contained; it manages its own GTK signals.
            let renderer = Rc::new(
                widgets::launcher::LauncherWidget::new(launcher)
                    .context("launcher renderer construction failed")?,
            );
            add_to_bar(bar, renderer.widget(), config, &section);
            // Launcher does not participate in the Poller dispatch loop.
            Ok((None, None))
        }
        WidgetKind::Separator(sep) => {
            // Thin visual divider between widgets. Renders a configurable glyph
            // (default "|") styled by .widget-separator in the theme. The glyph
            // is set via the `format` field in SeparatorConfig. Does not poll or dispatch.
            let glyph = sep.format.as_deref().unwrap_or("|");
            let label = gtk::Label::new(Some(glyph));
            label.set_widget_name("separator");
            label.style_context().add_class("widget-separator");
            if let Some(cls) = &config.extra_class {
                label.style_context().add_class(cls.as_str());
            }
            bar.add_widget(label.upcast_ref(), &section);
            Ok((None, None))
        }
        WidgetKind::Weather(weather) => {
            let interval = config.interval.unwrap_or(1_800_000);
            let latitude = weather.latitude.unwrap_or(0.0);
            let longitude = weather.longitude.unwrap_or(0.0);
            let unit = match weather.units.as_deref().unwrap_or("celsius") {
                "fahrenheit" => parapet_core::widget::TempUnit::Fahrenheit,
                _ => parapet_core::widget::TempUnit::Celsius,
            };
            let core_widget =
                parapet_core::widgets::weather::WeatherWidget::new(&name, latitude, longitude, unit);
            poller.register(Box::new(core_widget), interval);
            let renderer = widgets::weather::WeatherWidget::new(weather)
                .context("weather renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);
            Ok((Some(Rc::new(WeatherRenderer { name, renderer })), None))
        }
        WidgetKind::Media(media) => {
            let interval = config.interval.unwrap_or(2000);
            let core_widget = parapet_core::widgets::media::MediaWidget::new(&name);
            poller.register(Box::new(core_widget), interval);
            let renderer = widgets::media::MediaWidget::new(media)
                .context("media renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);
            Ok((Some(Rc::new(MediaRenderer { name, renderer })), None))
        }
        WidgetKind::Disk(disk) => {
            let interval = config.interval.unwrap_or(30_000);
            let mount = disk.mount.clone().unwrap_or_else(|| "/".to_string());
            let core_widget = parapet_core::widgets::disk::DiskWidget::new(&name, &mount)?;
            poller.register(Box::new(core_widget), interval);
            let renderer = widgets::disk::DiskWidget::new(disk)
                .context("disk renderer construction failed")?;
            add_to_bar(bar, renderer.widget(), config, &section);
            Ok((Some(Rc::new(DiskRenderer { name, renderer })), None))
        }
    }
}

/// Add a widget to the bar, wrapping in an `EventBox` when click or scroll
/// actions are configured in `config`.
///
/// When any of `on_click`, `on_scroll_up`, or `on_scroll_down` is set, the
/// raw widget is placed inside an `EventBox` with appropriate signal handlers
/// before being added to the bar section. Otherwise the widget is added
/// directly.
///
/// If `config.extra_class` is set, the CSS class is applied to whatever
/// container is added to the section box (the `EventBox` when wrapping, the
/// widget itself when not).
fn add_to_bar(bar: &bar::Bar, widget: &gtk::Widget, config: &WidgetConfig, section: &BarSection) {
    let has_actions = config.on_click.is_some()
        || config.on_scroll_up.is_some()
        || config.on_scroll_down.is_some();

    if !has_actions {
        if let Some(cls) = &config.extra_class {
            widget.style_context().add_class(cls.as_str());
        }
        bar.add_widget(widget, section);
        return;
    }

    let event_box = gtk::EventBox::new();
    event_box.add(widget);

    if let Some(cls) = &config.extra_class {
        event_box.style_context().add_class(cls.as_str());
    }

    if let Some(cmd) = config.on_click.clone() {
        event_box.connect_button_press_event(move |_, event| {
            if event.button() == 1 {
                spawn_shell(&cmd);
            }
            glib::Propagation::Proceed
        });
    }

    if config.on_scroll_up.is_some() || config.on_scroll_down.is_some() {
        event_box.add_events(gdk::EventMask::SCROLL_MASK);
        let up_cmd = config.on_scroll_up.clone();
        let down_cmd = config.on_scroll_down.clone();
        event_box.connect_scroll_event(move |_, event| {
            match event.direction() {
                gdk::ScrollDirection::Up => {
                    if let Some(cmd) = &up_cmd {
                        spawn_shell(cmd);
                    }
                }
                gdk::ScrollDirection::Down => {
                    if let Some(cmd) = &down_cmd {
                        spawn_shell(cmd);
                    }
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
    }

    bar.add_widget(event_box.upcast_ref(), section);
}

/// Spawn a shell command asynchronously via `sh -c`.
///
/// Failures (e.g. `sh` not on `PATH`) are logged as `WARN` and ignored so a
/// misconfigured action does not crash or hang the bar.
fn spawn_shell(cmd: &str) {
    if let Err(e) = std::process::Command::new("sh").args(["-c", cmd]).spawn() {
        tracing::warn!(command = cmd, error = %e, "widget action command failed to spawn");
    }
}

// ── Starter config ────────────────────────────────────────────────────────────

/// Well-commented starter config written by `--init-config`.
///
/// Matches the complete example from `CONFIG_MODEL.md §6`.
const STARTER_CONFIG: &str = r#"# Parapet status bar configuration
# Generated by `parapet_bar --init-config`
#
# Full field reference: https://github.com/CamRed25/Parapet/blob/master/standards/CONFIG_MODEL.md
# Generate JSON Schema for editor autocomplete: parapet_bar --dump-schema

[bar]
position = "top"       # "top" | "bottom"
height = 28            # bar height in pixels
monitor = "primary"    # "primary" | integer (0-based GDK monitor index)
# css = "~/.config/parapet/parapet.css"   # path to a custom CSS file (~ is expanded)
widget_spacing = 4     # pixel gap between adjacent widgets

# ── Left section ──────────────────────────────────────────────────────────────

[[widgets]]
type = "workspaces"
position = "left"
show_names = true

# ── Centre section ────────────────────────────────────────────────────────────

[[widgets]]
type = "clock"
position = "center"
format = "%a %b %d  %H:%M"

# ── Right section ─────────────────────────────────────────────────────────────

[[widgets]]
type = "cpu"
position = "right"
interval = 2000
warn_threshold = 80.0
crit_threshold = 95.0

[[widgets]]
type = "memory"
position = "right"
interval = 3000
format = "percent"

[[widgets]]
type = "network"
position = "right"
interval = 2000
interface = "auto"

[[widgets]]
type = "battery"
position = "right"
interval = 10000
"#;

// ── CLI subcommand helpers ────────────────────────────────────────────────────

/// Write the starter config to the XDG config path.
///
/// Writes [`STARTER_CONFIG`] to `~/.config/parapet/config.toml` (or the path
/// in `PARAPET_CONFIG`). Returns an error if the file already exists to prevent
/// accidental overwrite. Creates parent directories if absent.
///
/// # Errors
///
/// Returns an error if the file already exists, if the directory cannot be
/// created, or if the write fails.
fn init_config() -> anyhow::Result<()> {
    let target = std::env::var("PARAPET_CONFIG").map_or_else(
        |_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            std::path::PathBuf::from(home)
                .join(".config")
                .join("parapet")
                .join("config.toml")
        },
        std::path::PathBuf::from,
    );

    if target.exists() {
        anyhow::bail!(
            "config already exists at {}; refusing to overwrite.\n\
             Delete or rename the file first if you want a fresh starter config.",
            target.display()
        );
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create config directory {}", parent.display()))?;
    }

    std::fs::write(&target, STARTER_CONFIG)
        .with_context(|| format!("could not write config to {}", target.display()))?;

    println!("Wrote config to {}", target.display());
    Ok(())
}

/// Print the JSON Schema for [`ParapetConfig`] to stdout.
///
/// Calls [`parapet_core::config_schema_json`] and prints the result. No GTK
/// initialisation is required. Intended for use with editor tooling (VS Code +
/// taplo / Even Better TOML extension).
fn dump_schema() {
    println!("{}", parapet_core::config_schema_json());
}
