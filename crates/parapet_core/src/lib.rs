//! `parapet_core` — pure library crate for the Parapet status bar.
//!
//! Provides widget data providers, system information polling, configuration
//! parsing, and the core error types. Contains no GTK, GDK, X11, or display
//! system dependencies. All modules in this crate are safe to use headlessly
//! without a running display server.

pub mod error;
pub use error::{ParapetConfigError, ParapetError};

pub mod widget;
pub use widget::{BatteryStatus, DiskEntry, Widget, WidgetData, WIDGET_API_VERSION};

pub mod config;
pub use config::{
    config_schema_json, BarConfig, BarPosition, BarSection, BatteryConfig, BrightnessConfig,
    ClockConfig, ConfigWatcher, CpuConfig, DiskConfig, ParapetConfig, LauncherConfig, MediaConfig,
    MemoryConfig, MonitorTarget, NetworkConfig, SeparatorConfig, VolumeConfig, WeatherConfig,
    WidgetConfig, WidgetKind, WorkspacesConfig,
};

pub mod widgets;

pub mod poll;
pub use poll::Poller;
