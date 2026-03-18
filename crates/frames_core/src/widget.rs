//! Widget trait, `WidgetData` enum, and `BatteryStatus` — the core contract
//! between `frames_core` data providers and `frames_bar` renderers.
//!
//! Every data-producing component implements [`Widget`]. Renderers in
//! `frames_bar` consume [`WidgetData`] values returned by `Widget::update`.
//! No GTK or display-system types appear in this module.

use crate::error::FramesError;

/// Semantic version of the Widget API contract.
///
/// Bump this string in the same commit as any change to [`Widget`] or
/// [`WidgetData`]. See `WIDGET_API.md §2` for the versioning policy.
pub const WIDGET_API_VERSION: &str = "1.7.1";

/// Uniform interface for all widget data providers.
///
/// Implementors collect system information and return it as [`WidgetData`].
/// All implementations must be `Send + Sync` to allow future multi-threaded
/// polling via [`crate::poll::Poller`].
///
/// # Contract
///
/// - `name()` must return a stable, non-empty string across calls.
/// - `update()` must not block for longer than the widget's configured interval.
/// - `update()` should return `Err` only for genuinely unrecoverable failures;
///   transient failures should return stale or degraded data (see `WIDGET_API §7.2`).
pub trait Widget: Send + Sync {
    /// Human-readable name for this widget instance.
    ///
    /// Used in config references, log messages, and CSS widget names.
    /// Must be non-empty and stable (same value on every call).
    fn name(&self) -> &str;

    /// Refresh internal state and return the latest data snapshot.
    ///
    /// Called by the [`crate::poll::Poller`] on each polling interval.
    /// Implementations read system state and construct the appropriate
    /// [`WidgetData`] variant.
    ///
    /// # Errors
    ///
    /// Returns [`FramesError`] only for persistent, unrecoverable failures.
    /// Transient errors (e.g. a brief `/proc` read glitch) should return the
    /// last known good value or a safe zero/placeholder variant.
    fn update(&mut self) -> Result<WidgetData, FramesError>;
}

/// Data produced by a [`Widget`] on each update cycle.
///
/// This enum is `#[non_exhaustive]`. All `match` arms over this type **must**
/// include a `_ => {}` fallback to remain forward-compatible when new variants
/// are added. Failure to do so is a compile error after any minor API bump.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WidgetData {
    /// Date and time string ready for display.
    Clock {
        /// Formatted string produced by the widget's `format` config field.
        display: String,
    },

    /// CPU usage statistics.
    Cpu {
        /// Aggregate CPU usage across all cores, as a percentage (0.0–100.0).
        usage_pct: f32,
        /// Per-core usage percentages in logical core order.
        per_core: Vec<f32>,
        /// CPU package temperature in degrees Celsius, or `None` if the
        /// platform does not expose temperature sensors (e.g. inside a VM or
        /// on hardware without `coretemp`/`k10temp` kernel modules loaded).
        temp_celsius: Option<f32>,
    },

    /// RAM and swap usage statistics.
    Memory {
        /// RAM currently in use, in bytes.
        used_bytes: u64,
        /// Total RAM available, in bytes.
        total_bytes: u64,
        /// Swap currently in use, in bytes.
        swap_used: u64,
        /// Total swap space available, in bytes.
        swap_total: u64,
    },

    /// Network interface statistics.
    Network {
        /// Received bytes per second since last update.
        rx_bytes_per_sec: u64,
        /// Transmitted bytes per second since last update.
        tx_bytes_per_sec: u64,
        /// Name of the monitored interface (e.g. `"eth0"`, `"wlan0"`).
        interface: String,
    },

    /// Battery charge level and status.
    Battery {
        /// Charge percentage (0.0–100.0), or `None` if no battery is present
        /// (e.g. on a desktop machine without a battery).
        charge_pct: Option<f32>,
        /// Current battery charging status.
        status: BatteryStatus,
    },

    /// Filesystem disk usage for a single mount point.
    Disk {
        /// Mount point path monitored by this widget (e.g. `"/"` or `"/home"`).
        mount: String,
        /// Bytes currently used on the filesystem.
        used_bytes: u64,
        /// Total bytes on the filesystem.
        total_bytes: u64,
        /// Usage summary for every real mounted filesystem, used by the
        /// renderer to populate the hover-tooltip dropdown.
        all_disks: Vec<DiskEntry>,
    },

    /// Workspace list and active workspace index.
    Workspaces {
        /// Total number of workspaces.
        count: usize,
        /// 0-based index of the currently active workspace.
        active: usize,
        /// Workspace names from `_NET_DESKTOP_NAMES`. Empty strings when names
        /// are not set by the window manager.
        names: Vec<String>,
    },

    /// Audio output volume level and mute state.
    Volume {
        /// Current output volume as a percentage (0.0–100.0).
        volume_pct: f32,
        /// Whether the output is currently muted.
        muted: bool,
    },

    /// Screen backlight brightness level.
    Brightness {
        /// Current brightness as a percentage (0.0–100.0).
        /// Zero on machines without a variable backlight.
        brightness_pct: f32,
    },

    /// Current weather conditions from the Open-Meteo API.
    Weather {
        /// Air temperature at 2 m in the widget's configured unit (°C or °F).
        temperature: f32,
        /// WMO weather interpretation code (0 = clear sky, 61 = rain, etc.).
        weather_code: u16,
        /// Wind speed at 10 m in km/h.
        wind_speed: f32,
        /// Relative humidity at 2 m, as a percentage (0–100).
        humidity: u8,
        /// The temperature unit in use (`Celsius` or `Fahrenheit`).
        unit: TempUnit,
    },

    /// Currently playing media track from an MPRIS2 player.
    Media {
        /// Track title from `xesam:title`. Empty string if no player is active.
        title: String,
        /// Track artist from `xesam:artist`. Empty string if unavailable.
        artist: String,
        /// Current playback status.
        status: PlaybackStatus,
        /// True if the player reports it can advance to the next track.
        can_go_next: bool,
        /// True if the player reports it can go back to the previous track.
        can_go_previous: bool,
    },
}

/// One row in the disk summary shown in the hover-tooltip dropdown.
///
/// Produced by [`DiskWidget`](crate::widgets::disk::DiskWidget) and carried
/// inside [`WidgetData::Disk::all_disks`]. Each entry corresponds to one
/// real mounted filesystem reported by `sysinfo::Disks`.
#[derive(Debug, Clone)]
pub struct DiskEntry {
    /// Mount point path (e.g. `"/"`, `"/home"`, `"/boot"`).
    pub mount: String,
    /// Bytes currently in use.
    pub used_bytes: u64,
    /// Total bytes on the filesystem.
    pub total_bytes: u64,
}

/// Temperature display unit for the weather widget.
///
/// Determines which temperature unit is requested from the Open-Meteo API and
/// which unit symbol the renderer displays. Deserialised from the `units`
/// widget config field as `"celsius"` (default) or `"fahrenheit"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TempUnit {
    /// Degrees Celsius (°C).
    Celsius,
    /// Degrees Fahrenheit (°F).
    Fahrenheit,
}

/// MPRIS2 playback status, from `org.mpris.MediaPlayer2.Player.PlaybackStatus`.
///
/// `Stopped` is also returned when no player is present on the session bus —
/// player absence is a normal condition, not an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    /// A track is actively playing.
    Playing,
    /// Playback is paused.
    Paused,
    /// The player is stopped or no player is present.
    Stopped,
}

/// Battery charging status, parsed from `/sys/class/power_supply/<name>/status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatteryStatus {
    /// The battery is actively charging.
    Charging,
    /// The battery is discharging (running on battery power).
    Discharging,
    /// The battery is fully charged.
    Full,
    /// The status string was not recognised or is absent.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal widget used to verify the [`Widget`] trait contract in tests.
    struct MinimalWidget;

    impl Widget for MinimalWidget {
        fn name(&self) -> &str {
            "minimal"
        }

        fn update(&mut self) -> Result<WidgetData, FramesError> {
            Ok(WidgetData::Clock {
                display: "00:00".to_string(),
            })
        }
    }

    #[test]
    fn widget_api_version_non_empty() {
        assert!(!WIDGET_API_VERSION.is_empty());
    }

    #[test]
    fn battery_status_eq() {
        assert_eq!(BatteryStatus::Charging, BatteryStatus::Charging);
        assert_ne!(BatteryStatus::Charging, BatteryStatus::Discharging);
        assert_ne!(BatteryStatus::Full, BatteryStatus::Unknown);
    }

    #[test]
    fn widget_data_clock_clone() {
        let original = WidgetData::Clock {
            display: "12:34".to_string(),
        };
        let cloned = original.clone();
        if let (WidgetData::Clock { display: a }, WidgetData::Clock { display: b }) =
            (original, cloned)
        {
            assert_eq!(a, b);
        } else {
            panic!("clone changed variant");
        }
    }

    #[test]
    fn minimal_widget_name_non_empty() {
        let w = MinimalWidget;
        assert!(!w.name().is_empty());
    }

    #[test]
    fn minimal_widget_update_returns_ok() {
        let mut w = MinimalWidget;
        assert!(w.update().is_ok());
    }

    /// Compile-time proof that MinimalWidget is Send + Sync.
    #[test]
    fn minimal_widget_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MinimalWidget>();
    }

    #[test]
    fn temp_unit_serde_roundtrip() {
        // Deserialise TempUnit via a wrapper struct, as it appears in config.
        #[derive(serde::Deserialize)]
        struct Wrapper {
            unit: TempUnit,
        }
        let celsius: Wrapper = toml::from_str("unit = \"celsius\"").expect("celsius");
        let fahrenheit: Wrapper = toml::from_str("unit = \"fahrenheit\"").expect("fahrenheit");
        assert_eq!(celsius.unit, TempUnit::Celsius);
        assert_eq!(fahrenheit.unit, TempUnit::Fahrenheit);
    }

    #[test]
    fn temp_unit_eq() {
        assert_eq!(TempUnit::Celsius, TempUnit::Celsius);
        assert_ne!(TempUnit::Celsius, TempUnit::Fahrenheit);
    }

    #[test]
    fn playback_status_eq() {
        assert_eq!(PlaybackStatus::Playing, PlaybackStatus::Playing);
        assert_ne!(PlaybackStatus::Playing, PlaybackStatus::Paused);
        assert_ne!(PlaybackStatus::Paused, PlaybackStatus::Stopped);
    }

    #[test]
    fn playback_status_clone() {
        let s = PlaybackStatus::Paused;
        let c = s;
        assert_eq!(s, c);
    }

    #[test]
    fn widget_data_weather_clone() {
        let original = WidgetData::Weather {
            temperature: 12.3,
            weather_code: 61,
            wind_speed: 18.5,
            humidity: 74,
            unit: TempUnit::Celsius,
        };
        let cloned = original.clone();
        match (original, cloned) {
            (
                WidgetData::Weather {
                    temperature: t1,
                    weather_code: w1,
                    ..
                },
                WidgetData::Weather {
                    temperature: t2,
                    weather_code: w2,
                    ..
                },
            ) => {
                assert!((t1 - t2).abs() < f32::EPSILON);
                assert_eq!(w1, w2);
            }
            _ => panic!("clone changed variant"),
        }
    }

    #[test]
    fn widget_data_media_clone() {
        let original = WidgetData::Media {
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            status: PlaybackStatus::Playing,
            can_go_next: true,
            can_go_previous: false,
        };
        let cloned = original.clone();
        match (original, cloned) {
            (
                WidgetData::Media {
                    title: t1,
                    status: s1,
                    ..
                },
                WidgetData::Media {
                    title: t2,
                    status: s2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(s1, s2);
            }
            _ => panic!("clone changed variant"),
        }
    }
}
