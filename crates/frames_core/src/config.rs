//! TOML configuration model for Frames.
//!
//! [`FramesConfig`] is the top-level structure loaded from
//! `~/.config/frames/config.toml` (or the path in `FRAMES_CONFIG`).
//! All fields have sane defaults so the config file section is optional.
//!
//! See `CONFIG_MODEL.md` for the full field documentation and example configs.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

// ── Top-level ────────────────────────────────────────────────────────────────

/// Top-level configuration, loaded from `~/.config/frames/config.toml`.
///
/// The config file is not created automatically if absent — the bar exits with
/// a clear error message pointing to the expected path.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FramesConfig {
    /// Bar window configuration (position, height, monitor, CSS).
    #[serde(default)]
    pub bar: BarConfig,

    /// Ordered list of widget definitions. Widgets render in config order
    /// within their respective section (left / center / right).
    #[serde(default)]
    pub widgets: Vec<WidgetConfig>,
}

impl FramesConfig {
    /// Load and validate the config from `path`.
    ///
    /// Returns [`ConfigError::NotFound`] if the file does not exist,
    /// [`ConfigError::Io`] on read failure, [`ConfigError::Parse`] on invalid
    /// TOML, and [`ConfigError::Validation`] if field values fail validation
    /// rules (see `CONFIG_MODEL §5`).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] on any of the above failure modes.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotFound {
                path: path.to_path_buf(),
            });
        }
        let source = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&source)?;
        config.validate()?;
        Ok(config)
    }

    /// Return the XDG default config path: `~/.config/frames/config.toml`.
    ///
    /// Uses the `HOME` environment variable. Falls back to `/root` if `HOME`
    /// is unset (unusual in a normal desktop session).
    #[must_use]
    pub fn default_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        PathBuf::from(home).join(".config").join("frames").join("config.toml")
    }

    /// Validate all fields according to `CONFIG_MODEL §5`.
    ///
    /// Also applies `~` / `$HOME` expansion to all path-type fields in-place,
    /// so downstream code always receives absolute paths.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Validation`] describing the first violated rule.
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        if self.bar.height == 0 {
            return Err(ConfigError::Validation {
                field: "bar.height".to_string(),
                reason: "must be greater than 0".to_string(),
            });
        }

        // Apply path expansion to bar-level path fields.
        if let Some(css) = self.bar.css.take() {
            self.bar.css = Some(expand_path(&css));
        }
        if let Some(theme) = self.bar.theme.take() {
            self.bar.theme = Some(expand_path(&theme));
        }

        for (i, w) in self.widgets.iter_mut().enumerate() {
            // Reject zero polling intervals.
            if w.interval == Some(0) {
                return Err(ConfigError::Validation {
                    field: format!("widgets[{i}].interval"),
                    reason: "must be > 0".to_string(),
                });
            }

            match &mut w.kind {
                WidgetKind::Cpu(cpu) => Self::validate_cpu_widget(cpu, i)?,
                WidgetKind::Battery(bat) => Self::validate_battery_widget(bat, i)?,
                WidgetKind::Weather(weather) => {
                    if let Some(lat) = weather.latitude {
                        if !(-90.0..=90.0).contains(&lat) {
                            return Err(ConfigError::Validation {
                                field: format!("widgets[{i}].latitude"),
                                reason: "must be in range -90.0 to 90.0".to_string(),
                            });
                        }
                    }
                    if let Some(lon) = weather.longitude {
                        if !(-180.0..=180.0).contains(&lon) {
                            return Err(ConfigError::Validation {
                                field: format!("widgets[{i}].longitude"),
                                reason: "must be in range -180.0 to 180.0".to_string(),
                            });
                        }
                    }
                }
                WidgetKind::Disk(disk) => {
                    // Apply path expansion to mount before the absolute-path check.
                    if let Some(mount) = disk.mount.take() {
                        disk.mount = Some(expand_path(&mount));
                    }
                    if let Some(ref mount) = disk.mount {
                        if !mount.starts_with('/') {
                            return Err(ConfigError::Validation {
                                field: format!("widgets[{i}].mount"),
                                reason: "must be an absolute path (starts with /)".to_string(),
                            });
                        }
                    }
                }
                // All other widget kinds have no extra validation rules.
                WidgetKind::Clock(_)
                | WidgetKind::Memory(_)
                | WidgetKind::Network(_)
                | WidgetKind::Volume(_)
                | WidgetKind::Brightness(_)
                | WidgetKind::Media(_)
                | WidgetKind::Workspaces(_)
                | WidgetKind::Launcher(_)
                | WidgetKind::Separator(_) => {}
            }
        }
        Ok(())
    }

    fn validate_cpu_widget(cpu: &CpuConfig, idx: usize) -> Result<(), ConfigError> {
        let warn = cpu.warn_threshold.unwrap_or(80.0);
        let crit = cpu.crit_threshold.unwrap_or(95.0);
        if warn >= crit {
            return Err(ConfigError::Validation {
                field: format!("widgets[{idx}].warn_threshold"),
                reason: format!(
                    "CPU warn_threshold ({warn}) must be less than crit_threshold ({crit})"
                ),
            });
        }
        for (field_name, value) in [
            ("warn_threshold", cpu.warn_threshold),
            ("crit_threshold", cpu.crit_threshold),
        ] {
            if let Some(v) = value {
                if !(0.0..=100.0).contains(&v) {
                    return Err(ConfigError::Validation {
                        field: format!("widgets[{idx}].{field_name}"),
                        reason: "must be in range 0.0 to 100.0".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_battery_widget(bat: &BatteryConfig, idx: usize) -> Result<(), ConfigError> {
        let warn = bat.warn_threshold.unwrap_or(20.0);
        let crit = bat.crit_threshold.unwrap_or(5.0);
        if crit >= warn {
            return Err(ConfigError::Validation {
                field: format!("widgets[{idx}].crit_threshold"),
                reason: format!(
                    "battery crit_threshold ({crit}) must be less than warn_threshold ({warn})"
                ),
            });
        }
        for (field_name, value) in [
            ("warn_threshold", bat.warn_threshold),
            ("crit_threshold", bat.crit_threshold),
        ] {
            if let Some(v) = value {
                if !(0.0..=100.0).contains(&v) {
                    return Err(ConfigError::Validation {
                        field: format!("widgets[{idx}].{field_name}"),
                        reason: "must be in range 0.0 to 100.0".to_string(),
                    });
                }
            }
        }
        Ok(())
    }
}

// ── Bar config ────────────────────────────────────────────────────────────────

/// Global bar window configuration.
///
/// All fields are optional in the TOML source; `Default` provides sensible
/// values so `[bar]` can be omitted entirely.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct BarConfig {
    /// Screen edge where the bar is anchored. Default: `Top`.
    #[serde(default)]
    pub position: BarPosition,

    /// Bar height in pixels. Must be > 0. Default: `30`.
    #[serde(default = "BarConfig::default_height")]
    pub height: u32,

    /// Which monitor to display on. Default: `Primary`.
    #[serde(default)]
    pub monitor: MonitorTarget,

    /// Path to a user CSS file. `~` is expanded by the loader. `None` uses
    /// the built-in default theme compiled into `frames_bar`.
    #[serde(default)]
    pub css: Option<String>,

    /// Named theme to load from `~/.config/frames/themes/<name>.css`.
    ///
    /// Takes precedence over `css` when both are set. `None` falls through to
    /// `css`; if that is also `None`, the built-in default theme is used.
    /// Use the `--theme` CLI flag to override this field at runtime.
    #[serde(default)]
    pub theme: Option<String>,

    /// Pixel gap between adjacent widgets. Default: `4`.
    #[serde(default = "BarConfig::default_widget_spacing")]
    pub widget_spacing: u32,
}

impl BarConfig {
    fn default_height() -> u32 {
        30
    }

    fn default_widget_spacing() -> u32 {
        4
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::default(),
            height: Self::default_height(),
            monitor: MonitorTarget::default(),
            css: None,
            theme: None,
            widget_spacing: Self::default_widget_spacing(),
        }
    }
}

/// Screen edge where the bar is anchored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    /// Anchor the bar to the top of the screen.
    #[default]
    Top,
    /// Anchor the bar to the bottom of the screen.
    Bottom,
}

/// Which monitor to display the bar on.
///
/// Serializes as `"primary"` (string) or an integer index. Custom serde
/// impl required because `#[serde(untagged)]` cannot round-trip unit variants
/// in TOML.
#[derive(Debug, Clone, Default)]
pub enum MonitorTarget {
    /// Display on the primary monitor as reported by GDK.
    #[default]
    Primary,
    /// Use a specific 0-based GDK monitor index.
    Index(usize),
}

impl schemars::JsonSchema for MonitorTarget {
    fn schema_name() -> String {
        "MonitorTarget".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::{InstanceType, SchemaObject, SingleOrVec, SubschemaValidation};
        SchemaObject {
            subschemas: Some(Box::new(SubschemaValidation {
                one_of: Some(vec![
                    // "primary" — the primary monitor
                    SchemaObject {
                        instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
                        enum_values: Some(vec![serde_json::Value::String("primary".to_string())]),
                        ..Default::default()
                    }
                    .into(),
                    // non-negative integer monitor index
                    SchemaObject {
                        instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Integer))),
                        ..Default::default()
                    }
                    .into(),
                ]),
                ..Default::default()
            })),
            ..Default::default()
        }
        .into()
    }
}

impl Serialize for MonitorTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MonitorTarget::Primary => serializer.serialize_str("primary"),
            // clippy::cast_possible_truncation: monitor index fits in u64 on any real machine
            #[allow(clippy::cast_possible_truncation)]
            MonitorTarget::Index(i) => serializer.serialize_u64(*i as u64),
        }
    }
}

impl<'de> Deserialize<'de> for MonitorTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MonitorVisitor;

        impl serde::de::Visitor<'_> for MonitorVisitor {
            type Value = MonitorTarget;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, r#"\"primary\" or an integer monitor index"#)
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "primary" {
                    Ok(MonitorTarget::Primary)
                } else {
                    Err(E::custom(format!("unknown monitor target: {v}")))
                }
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                usize::try_from(v).map(MonitorTarget::Index).map_err(E::custom)
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                usize::try_from(v).map(MonitorTarget::Index).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(MonitorVisitor)
    }
}

/// Bar section (column) that a widget belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BarSection {
    /// Left-aligned section (packed with `expand=false`).
    Left,
    /// Centre section (expands to fill remaining space).
    Center,
    /// Right-aligned section (packed with `expand=false`).
    Right,
}

// ── Widget config ─────────────────────────────────────────────────────────────

/// Widget type discriminant and per-widget configuration.
///
/// Internally tagged with the `type` TOML key (e.g. `type = "clock"`). Each
/// variant wraps a struct holding only the fields valid for that widget type.
/// Unknown fields are rejected at parse time by `#[serde(deny_unknown_fields)]`
/// on each variant struct.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WidgetKind {
    /// Formatted clock / date display.
    Clock(ClockConfig),
    /// CPU usage percentage with threshold-based CSS classes.
    Cpu(CpuConfig),
    /// RAM (and optionally swap) usage.
    Memory(MemoryConfig),
    /// Network I/O rates for a named interface.
    Network(NetworkConfig),
    /// Battery charge percentage and status.
    Battery(BatteryConfig),
    /// Filesystem disk usage for a mount point.
    Disk(DiskConfig),
    /// System audio volume level.
    Volume(VolumeConfig),
    /// Screen brightness level.
    Brightness(BrightnessConfig),
    /// Current weather conditions from Open-Meteo.
    Weather(WeatherConfig),
    /// Media player transport controls.
    Media(MediaConfig),
    /// Cinnamon workspace switcher buttons.
    Workspaces(WorkspacesConfig),
    /// Application launcher popup.
    Launcher(LauncherConfig),
    /// Visual divider between widgets.
    Separator(SeparatorConfig),
}

/// Configuration for a single widget entry in `[[widgets]]`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct WidgetConfig {
    /// Bar section to place this widget in.
    pub position: BarSection,

    /// Polling interval in milliseconds. `None` uses the per-widget default
    /// defined in `CONFIG_MODEL §4.2`.
    #[serde(default)]
    pub interval: Option<u64>,

    /// Static text label displayed before the widget value.
    #[serde(default)]
    pub label: Option<String>,

    /// Shell command to execute when the widget is left-clicked.
    ///
    /// Spawned via `sh -c <command>`. Example: `"pavucontrol"`.
    #[serde(default)]
    pub on_click: Option<String>,

    /// Shell command to execute on scroll-wheel up.
    ///
    /// Spawned via `sh -c <command>`. Example: `"pactl set-sink-volume @DEFAULT_SINK@ +5%"`.
    #[serde(default)]
    pub on_scroll_up: Option<String>,

    /// Shell command to execute on scroll-wheel down.
    ///
    /// Spawned via `sh -c <command>`. Example: `"pactl set-sink-volume @DEFAULT_SINK@ -5%"`.
    #[serde(default)]
    pub on_scroll_down: Option<String>,

    /// Extra CSS class name applied to this widget's root GTK container.
    ///
    /// Allows theme authors to target individual widget instances in CSS
    /// without modifying widget source code. For example,
    /// `extra_class = "my-clock"` can be styled with
    /// `.my-clock { color: #ff0; }` in the user theme.
    #[serde(default)]
    pub extra_class: Option<String>,

    /// Widget type and its type-specific configuration.
    ///
    /// The `type` key in TOML acts as the discriminant (e.g. `type = "clock"`).
    /// Unknown fields for the selected widget type are rejected at parse time.
    #[serde(flatten)]
    pub kind: WidgetKind,
}

// ── Per-widget config structs ─────────────────────────────────────────────────

/// Clock widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClockConfig {
    /// `chrono::format` pattern string (e.g. `"%H:%M:%S"`).
    #[serde(default)]
    pub format: Option<String>,

    /// `"local"` or an IANA timezone name (e.g. `"America/New_York"`).
    #[serde(default)]
    pub timezone: Option<String>,
}

/// CPU widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CpuConfig {
    /// Percentage above which the `.warning` CSS class is applied. Default: 80.0.
    #[serde(default)]
    pub warn_threshold: Option<f32>,

    /// Percentage above which the `.critical` CSS class is applied. Default: 95.0.
    #[serde(default)]
    pub crit_threshold: Option<f32>,
}

/// Memory widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    /// Display mode — `"used"`, `"free"`, or `"percent"`.
    #[serde(default)]
    pub format: Option<String>,

    /// Whether to include swap usage in the display.
    #[serde(default)]
    pub show_swap: Option<bool>,
}

/// Network widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfig {
    /// Interface name (e.g. `"eth0"`) or `"auto"`.
    #[serde(default)]
    pub interface: Option<String>,

    /// Whether to prefix the display with the interface name.
    #[serde(default)]
    pub show_interface: Option<bool>,
}

/// Battery widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatteryConfig {
    /// Percentage below which the `.warning` CSS class is applied. Default: 20.0.
    #[serde(default)]
    pub warn_threshold: Option<f32>,

    /// Percentage below which the `.critical` CSS class is applied. Default: 5.0.
    #[serde(default)]
    pub crit_threshold: Option<f32>,

    /// Whether to show a Unicode battery icon.
    #[serde(default)]
    pub show_icon: Option<bool>,
}

/// Disk widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiskConfig {
    /// Filesystem mount point to monitor (e.g. `"/"` or `"/home"`). Default: `"/"`.
    #[serde(default)]
    pub mount: Option<String>,

    /// Bar label format: `"used"` (default), `"percent"`, or `"free"`.
    #[serde(default)]
    pub format: Option<String>,
}

/// Volume widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VolumeConfig {
    /// Show a Unicode speaker icon (🔊/🔇) before the level. Default: `true`.
    #[serde(default)]
    pub show_icon: Option<bool>,
}

/// Brightness widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrightnessConfig {
    /// Show a Unicode sun icon (☀) before the percentage. Default: `true`.
    #[serde(default)]
    pub show_icon: Option<bool>,
}

/// Weather widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherConfig {
    /// WGS-84 latitude in decimal degrees (negative = south).
    #[serde(default)]
    pub latitude: Option<f64>,

    /// WGS-84 longitude in decimal degrees (negative = west).
    #[serde(default)]
    pub longitude: Option<f64>,

    /// Temperature unit — `"celsius"` (default) or `"fahrenheit"`.
    #[serde(default)]
    pub units: Option<String>,
}

/// Media widget configuration.
///
/// No custom fields; click/scroll actions are configured on the outer
/// [`WidgetConfig`] via `on_click`, `on_scroll_up`, `on_scroll_down`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaConfig {}

/// Workspaces widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspacesConfig {
    /// Whether to display workspace names or just numbers.
    #[serde(default)]
    pub show_names: Option<bool>,
}

/// Launcher widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LauncherConfig {
    /// Maximum number of search results shown in the popup list. Default: 10.
    #[serde(default)]
    pub max_results: Option<u32>,
    /// Label shown on the bar button. Default: `"Apps"`.
    #[serde(default)]
    pub button_label: Option<String>,
    /// Width of the popup window in pixels. Default: 280.
    #[serde(default)]
    pub popup_width: Option<i32>,
    /// Minimum height of the popup scrolled list in pixels. Default: 200.
    #[serde(default)]
    pub popup_min_height: Option<i32>,
    /// Desktop ID stems of pinned apps shown at the top regardless of query.
    /// Each entry is a bare stem without the `.desktop` suffix, e.g. `"firefox"`.
    /// Default: empty.
    #[serde(default)]
    pub pinned: Vec<String>,
}

/// Separator widget configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeparatorConfig {
    /// Glyph text rendered as the divider. Default: `"|"`.
    #[serde(default)]
    pub format: Option<String>,
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Expand a leading `~` or `$HOME` prefix to the user's home directory.
///
/// Handles two forms:
/// - `~/…` (leading tilde-slash)
/// - `$HOME/…` (leading `$HOME` followed by `/` or end-of-string)
///
/// All other `$VAR` sequences are left unexpanded to avoid unintentionally
/// expanding mount paths that may contain `$` characters.
///
/// Returns the input unchanged if no home-directory prefix is found or if
/// `HOME` is not set in the environment.
fn expand_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if path.starts_with("~/") {
        format!("{}{}", home, &path[1..])
    } else if path == "~" {
        home
    } else if path.starts_with("$HOME/") {
        format!("{}{}", home, &path[5..])
    } else if path == "$HOME" {
        home
    } else {
        path.to_string()
    }
}

/// Generate and return the full JSON Schema for [`FramesConfig`] as a JSON string.
///
/// The schema is produced by `schemars::schema_for!(FramesConfig)` and
/// serialised to a pretty-printed JSON string.
///
/// Intended for use by the `frames_bar --dump-schema` subcommand. Not called
/// during normal bar operation.
///
/// # Panics
///
/// Panics if `schemars` internal serialisation fails. This is a compile-time
/// derived schema — failure indicates a bug in the derive macros, not a runtime
/// condition.
#[must_use]
pub fn config_schema_json() -> String {
    let schema = schemars::schema_for!(FramesConfig);
    serde_json::to_string_pretty(&schema).expect("schemars schema is always serialisable")
}

// ── Config watcher ────────────────────────────────────────────────────────────

/// File watcher for the Frames config file.
///
/// Spawns a `notify` background watcher on construction. On any file-system
/// event for the watched path, an internal channel is signalled. Poll
/// [`ConfigWatcher::has_changed`] from the GTK main thread to check without
/// blocking.
///
/// Drop the `ConfigWatcher` to stop watching. The background watcher thread
/// is joined on drop automatically by the `notify` crate.
///
/// # Errors
///
/// [`ConfigWatcher::new`] returns [`ConfigError::Io`] if the `notify` watcher
/// cannot be initialised or the path cannot be registered.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<()>,
}

impl ConfigWatcher {
    /// Start watching `path` for modifications.
    ///
    /// Returns a `ConfigWatcher` that will signal on any `Modify`, `Create`,
    /// or `Remove` event for the given path. Does not require the file to
    /// exist at construction time.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] if the underlying watcher cannot be created
    /// or the path cannot be registered.
    pub fn new(path: &Path) -> Result<Self, ConfigError> {
        let (tx, rx) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
                Ok(event) => {
                    use notify::EventKind;
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    ) {
                        // Ignore send errors — receiver may have been dropped during shutdown.
                        let _ = tx.send(());
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "notify watcher error");
                }
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Returns `true` if at least one file-system event has arrived since the
    /// last call that returned `true`.
    ///
    /// Drains all pending events so the next call starts clean.
    /// Never blocks.
    #[must_use]
    pub fn has_changed(&self) -> bool {
        let mut changed = false;
        // Drain all pending events — only the fact of change matters, not the count.
        while self.rx.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_config_default_height_is_thirty() {
        assert_eq!(BarConfig::default().height, 30);
    }

    #[test]
    fn bar_config_default_position_is_top() {
        assert_eq!(BarConfig::default().position, BarPosition::Top);
    }

    #[test]
    fn bar_config_default_widget_spacing_is_four() {
        assert_eq!(BarConfig::default().widget_spacing, 4);
    }

    #[test]
    fn config_parses_valid_toml() {
        let toml = r#"
[bar]
position = "top"
height = 28

[[widgets]]
type = "clock"
position = "center"
format = "%H:%M"
"#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.bar.height, 28);
        assert_eq!(config.bar.position, BarPosition::Top);
        assert_eq!(config.widgets.len(), 1);
        assert!(matches!(config.widgets[0].kind, WidgetKind::Clock(_)));
        if let WidgetKind::Clock(ref clock) = config.widgets[0].kind {
            assert_eq!(clock.format.as_deref(), Some("%H:%M"));
        }
    }

    #[test]
    fn config_uses_defaults_when_bar_section_absent() {
        let toml = "[[widgets]]\ntype = \"clock\"\nposition = \"center\"\n";
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.bar.height, BarConfig::default().height);
        assert_eq!(config.bar.position, BarPosition::Top);
    }

    #[test]
    fn config_validation_rejects_zero_height() {
        let toml = "[bar]\nheight = 0\nposition = \"top\"\n";
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let result = config.validate();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("bar.height"), "error should mention field; got: {msg}");
    }

    #[test]
    fn config_validation_rejects_inverted_cpu_thresholds() {
        let toml = r#"
[[widgets]]
type = "cpu"
position = "right"
warn_threshold = 95.0
crit_threshold = 80.0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let result = config.validate();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("warn_threshold"),
            "error should mention threshold field; got: {msg}"
        );
    }

    #[test]
    fn config_validation_accepts_valid_thresholds() {
        let toml = r#"
[bar]
height = 30
position = "top"

[[widgets]]
type = "cpu"
position = "right"
warn_threshold = 80.0
crit_threshold = 95.0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn launcher_max_results_field_round_trips() {
        let toml = r#"
            [[widgets]]
            type = "launcher"
            position = "center"
            max_results = 15
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        if let WidgetKind::Launcher(ref launcher) = config.widgets[0].kind {
            assert_eq!(launcher.max_results, Some(15));
        } else {
            panic!("expected WidgetKind::Launcher");
        }
    }

    #[test]
    fn config_watcher_new_on_existing_file_succeeds() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[bar]\n").expect("write");
        let watcher = ConfigWatcher::new(&path);
        assert!(watcher.is_ok(), "watcher creation should succeed");
    }

    #[test]
    fn config_watcher_has_changed_returns_true_after_write() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[bar]\n").expect("initial write");
        let watcher = ConfigWatcher::new(&path).expect("watcher");

        // Give the watcher thread time to register before the write.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&path, "[bar]\nheight = 28\n").expect("modify");
        std::thread::sleep(std::time::Duration::from_millis(100));

        assert!(watcher.has_changed(), "should detect file modification");
    }

    #[test]
    fn widget_config_parses_on_click_and_scroll_actions() {
        let toml = r#"
            [[widgets]]
            type = "volume"
            position = "right"
            on_click = "pavucontrol"
            on_scroll_up = "pactl set-sink-volume @DEFAULT_SINK@ +5%"
            on_scroll_down = "pactl set-sink-volume @DEFAULT_SINK@ -5%"
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let w = &config.widgets[0];
        assert_eq!(w.on_click.as_deref(), Some("pavucontrol"));
        assert_eq!(w.on_scroll_up.as_deref(), Some("pactl set-sink-volume @DEFAULT_SINK@ +5%"));
        assert_eq!(w.on_scroll_down.as_deref(), Some("pactl set-sink-volume @DEFAULT_SINK@ -5%"));
    }

    #[test]
    fn widget_config_on_click_defaults_to_none() {
        let toml = r#"
            [[widgets]]
            type = "clock"
            position = "center"
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let w = &config.widgets[0];
        assert!(w.on_click.is_none());
        assert!(w.on_scroll_up.is_none());
        assert!(w.on_scroll_down.is_none());
    }

    #[test]
    fn extra_class_round_trip() {
        let toml = r#"
            [[widgets]]
            type = "clock"
            position = "center"
            extra_class = "my-clock"
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.widgets[0].extra_class.as_deref(), Some("my-clock"));
    }

    #[test]
    fn extra_class_absent_is_none() {
        let toml = r#"
            [[widgets]]
            type = "clock"
            position = "center"
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert!(config.widgets[0].extra_class.is_none());
    }

    #[test]
    fn bar_theme_field_round_trip() {
        let toml = r#"
            [bar]
            theme = "dark"
        "#;
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.bar.theme.as_deref(), Some("dark"));
    }

    #[test]
    fn bar_theme_field_absent_is_none() {
        let toml = "[[widgets]]\ntype = \"clock\"\nposition = \"center\"\n";
        let config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert!(config.bar.theme.is_none());
    }

    // ── expand_path tests ─────────────────────────────────────────────────────

    #[test]
    fn expand_path_tilde_slash() {
        let home = std::env::var("HOME").unwrap_or_default();
        let result = expand_path("~/foo/bar");
        assert_eq!(result, format!("{home}/foo/bar"));
    }

    #[test]
    fn expand_path_dollar_home_slash() {
        let home = std::env::var("HOME").unwrap_or_default();
        let result = expand_path("$HOME/foo/bar");
        assert_eq!(result, format!("{home}/foo/bar"));
    }

    #[test]
    fn expand_path_bare_tilde() {
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(expand_path("~"), home);
    }

    #[test]
    fn expand_path_unchanged_absolute() {
        assert_eq!(expand_path("/absolute/path"), "/absolute/path");
    }

    #[test]
    fn expand_path_dollar_other_var_unchanged() {
        assert_eq!(expand_path("$XDG_DATA/foo"), "$XDG_DATA/foo");
    }

    // ── validate() new-rule tests ─────────────────────────────────────────────

    #[test]
    fn validate_rejects_zero_interval() {
        let toml = r#"
[[widgets]]
type = "clock"
position = "center"
interval = 0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("interval"), "expected interval in error; got: {err}");
    }

    #[test]
    fn validate_rejects_latitude_out_of_range() {
        let toml = r#"
[[widgets]]
type = "weather"
position = "right"
latitude = 91.0
longitude = 0.0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("latitude"), "expected latitude in error; got: {err}");
    }

    #[test]
    fn validate_rejects_longitude_out_of_range() {
        let toml = r#"
[[widgets]]
type = "weather"
position = "right"
latitude = 0.0
longitude = 181.0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("longitude"), "expected longitude in error; got: {err}");
    }

    #[test]
    fn validate_rejects_relative_mount_path() {
        let toml = r#"
[[widgets]]
type = "disk"
position = "right"
mount = "relative/path"
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("mount"), "expected mount in error; got: {err}");
    }

    #[test]
    fn validate_rejects_threshold_out_of_range() {
        let toml = r#"
[[widgets]]
type = "cpu"
position = "right"
warn_threshold = 150.0
crit_threshold = 200.0
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("threshold"), "expected threshold in error; got: {err}");
    }

    #[test]
    fn validate_accepts_valid_weather_config() {
        let toml = r#"
[bar]
height = 30

[[widgets]]
type = "weather"
position = "right"
latitude = 51.5
longitude = -0.1
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_accepts_missing_latitude() {
        // An absent latitude on a weather widget is not a validation error.
        let toml = r#"
[bar]
height = 30

[[widgets]]
type = "weather"
position = "right"
"#;
        let mut config: FramesConfig = toml::from_str(toml).expect("valid toml");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_schema_json_is_valid_json() {
        let schema = config_schema_json();
        assert!(!schema.is_empty());
        let _parsed: serde_json::Value =
            serde_json::from_str(&schema).expect("schema should be valid JSON");
    }

    // ── WidgetKind round-trip and rejection tests (B8) ─────────────────────────

    #[test]
    fn widget_kind_round_trips_clock_via_toml() {
        let toml = "[[widgets]]\ntype = \"clock\"\nposition = \"center\"\nformat = \"%H:%M\"\n";
        let config: FramesConfig = toml::from_str(toml).expect("parse");
        let serialized = toml::to_string(&config).expect("serialize");
        let config2: FramesConfig = toml::from_str(&serialized).expect("re-parse");
        if let (WidgetKind::Clock(c1), WidgetKind::Clock(c2)) =
            (&config.widgets[0].kind, &config2.widgets[0].kind)
        {
            assert_eq!(c1.format, c2.format);
        } else {
            panic!("expected Clock variant after round-trip");
        }
    }

    #[test]
    fn widget_kind_round_trips_cpu_via_toml() {
        let toml = "[[widgets]]\ntype = \"cpu\"\nposition = \"right\"\nwarn_threshold = 75.0\ncrit_threshold = 90.0\n";
        let config: FramesConfig = toml::from_str(toml).expect("parse");
        let serialized = toml::to_string(&config).expect("serialize");
        let config2: FramesConfig = toml::from_str(&serialized).expect("re-parse");
        if let (WidgetKind::Cpu(c1), WidgetKind::Cpu(c2)) =
            (&config.widgets[0].kind, &config2.widgets[0].kind)
        {
            assert_eq!(c1.warn_threshold, c2.warn_threshold);
            assert_eq!(c1.crit_threshold, c2.crit_threshold);
        } else {
            panic!("expected Cpu variant after round-trip");
        }
    }

    #[test]
    fn widget_kind_rejects_unknown_field_on_clock() {
        // `latitude` is a weather field; `deny_unknown_fields` must reject it on clock.
        let toml = "[[widgets]]\ntype = \"clock\"\nposition = \"center\"\nlatitude = 42.0\n";
        let result = toml::from_str::<FramesConfig>(toml);
        assert!(result.is_err(), "unknown field should be rejected by deny_unknown_fields");
    }

    #[test]
    fn widget_kind_rejects_unknown_field_on_cpu() {
        // `latitude` is a weather field; `deny_unknown_fields` must reject it on cpu.
        let toml = "[[widgets]]\ntype = \"cpu\"\nposition = \"right\"\nlatitude = 42.0\n";
        let result = toml::from_str::<FramesConfig>(toml);
        assert!(result.is_err(), "unknown field should be rejected by deny_unknown_fields");
    }
}
