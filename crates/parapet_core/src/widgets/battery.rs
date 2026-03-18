//! Battery widget — charge level and status data provider.
//!
//! Reads `/sys/class/power_supply/` directly via `std::fs`. Returns
//! `charge_pct: None` on desktop machines without a battery — this is normal
//! and correct, not an error (see `WIDGET_API §7.2`).
//!
//! Status strings are mapped: `"Charging"` → `Charging`, `"Discharging"` →
//! `Discharging`, `"Full"` / `"Not charging"` → `Full`, anything else →
//! `Unknown`.

use std::path::{Path, PathBuf};

use crate::error::ParapetError;
use crate::widget::{BatteryStatus, Widget, WidgetData};

/// The sysfs root for power supply entries.
const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";

/// Provides battery charge level and status by reading sysfs.
pub struct BatteryWidget {
    name: String,
    /// Override the sysfs root for testing (injected via `new_with_sysfs_root`).
    sysfs_root: PathBuf,
    /// Last known good data, returned on transient read errors.
    last_data: Option<WidgetData>,
}

impl BatteryWidget {
    /// Create a new `BatteryWidget` reading from the real sysfs path.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sysfs_root: PathBuf::from(POWER_SUPPLY_PATH),
            last_data: None,
        }
    }

    /// Create a `BatteryWidget` reading from a custom sysfs root path.
    ///
    /// Used in tests to provide a fake `/sys/class/power_supply/` tree.
    #[must_use]
    pub fn new_with_sysfs_root(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            sysfs_root: root.into(),
            last_data: None,
        }
    }

    /// Read the battery data from `sysfs_root`, returning `None` if no battery
    /// entry exists, or `Some(WidgetData::Battery {...})` if one is found.
    fn read_battery(&self) -> Result<WidgetData, ParapetError> {
        let entries = match std::fs::read_dir(&self.sysfs_root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // sysfs path absent entirely — treat as no battery (desktop)
                return Ok(no_battery());
            }
            Err(e) => return Err(ParapetError::Battery(e)),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !is_battery_supply(&path) {
                continue;
            }
            let charge_pct = read_capacity(&path);
            let status = read_status(&path);
            return Ok(WidgetData::Battery { charge_pct, status });
        }

        Ok(no_battery())
    }
}

impl Widget for BatteryWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Read battery state from sysfs and return charge level and status.
    ///
    /// On machines without a battery, returns `charge_pct: None` and
    /// `status: BatteryStatus::Full`.  Transient read errors return the last
    /// known good data (or the no-battery default). Only persistent,
    /// unrecoverable errors propagate as `Err`.
    ///
    /// # Errors
    ///
    /// Returns [`ParapetError::Battery`] if `read_dir` on the sysfs root fails
    /// for a reason other than the path being absent.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        match self.read_battery() {
            Ok(data) => {
                self.last_data = Some(data.clone());
                Ok(data)
            }
            Err(e) => {
                tracing::warn!(widget = self.name, error = %e, "battery read error; using last data");
                Ok(self.last_data.clone().unwrap_or_else(no_battery))
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn no_battery() -> WidgetData {
    WidgetData::Battery {
        charge_pct: None,
        status: BatteryStatus::Full,
    }
}

/// Returns `true` if the supply entry at `path` is a battery (not AC adapter).
fn is_battery_supply(path: &Path) -> bool {
    let type_path = path.join("type");
    std::fs::read_to_string(&type_path)
        .map(|s| s.trim() == "Battery")
        .unwrap_or(false)
}

/// Read the capacity percentage from `path/capacity`. Returns `None` on error.
fn read_capacity(path: &Path) -> Option<f32> {
    let raw = std::fs::read_to_string(path.join("capacity")).ok()?;
    // clippy::cast_precision_loss: capacity is 0–100, no precision loss
    #[allow(clippy::cast_precision_loss)]
    Some(raw.trim().parse::<u32>().ok()? as f32)
}

/// Read and parse the battery status from `path/status`.
fn read_status(path: &Path) -> BatteryStatus {
    let Ok(raw) = std::fs::read_to_string(path.join("status")) else {
        return BatteryStatus::Unknown;
    };
    match raw.trim() {
        "Charging" => BatteryStatus::Charging,
        "Discharging" => BatteryStatus::Discharging,
        "Full" | "Not charging" => BatteryStatus::Full,
        _ => BatteryStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn make_battery_entry(root: &Path, name: &str, capacity: u32, status: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("type"), "Battery\n").unwrap();
        fs::write(dir.join("capacity"), format!("{capacity}\n")).unwrap();
        fs::write(dir.join("status"), format!("{status}\n")).unwrap();
    }

    #[test]
    fn battery_reads_charging_state() {
        let dir = TempDir::new().unwrap();
        make_battery_entry(dir.path(), "BAT0", 75, "Charging");

        let mut w = BatteryWidget::new_with_sysfs_root("battery", dir.path());
        let data = w.update().expect("update");

        if let WidgetData::Battery { charge_pct, status } = data {
            assert_eq!(charge_pct, Some(75.0));
            assert_eq!(status, BatteryStatus::Charging);
        } else {
            panic!("expected WidgetData::Battery");
        }
    }

    #[test]
    fn battery_reads_discharging_state() {
        let dir = TempDir::new().unwrap();
        make_battery_entry(dir.path(), "BAT0", 42, "Discharging");

        let mut w = BatteryWidget::new_with_sysfs_root("battery", dir.path());
        let data = w.update().expect("update");

        if let WidgetData::Battery { charge_pct, status } = data {
            assert_eq!(charge_pct, Some(42.0));
            assert_eq!(status, BatteryStatus::Discharging);
        } else {
            panic!("expected WidgetData::Battery");
        }
    }

    #[test]
    fn battery_no_battery_returns_none_charge() {
        let dir = TempDir::new().unwrap();
        // Add only an AC adapter entry (type = "Mains"), no battery
        let ac = dir.path().join("AC0");
        fs::create_dir_all(&ac).unwrap();
        fs::write(ac.join("type"), "Mains\n").unwrap();

        let mut w = BatteryWidget::new_with_sysfs_root("battery", dir.path());
        let data = w.update().expect("update");

        if let WidgetData::Battery { charge_pct, .. } = data {
            assert_eq!(charge_pct, None, "no battery must yield charge_pct: None");
        } else {
            panic!("expected WidgetData::Battery");
        }
    }

    #[test]
    fn battery_unknown_status_maps_to_unknown_variant() {
        let dir = TempDir::new().unwrap();
        make_battery_entry(dir.path(), "BAT0", 50, "Alien power source");

        let mut w = BatteryWidget::new_with_sysfs_root("battery", dir.path());
        let data = w.update().expect("update");

        if let WidgetData::Battery { status, .. } = data {
            assert_eq!(status, BatteryStatus::Unknown);
        } else {
            panic!("expected WidgetData::Battery");
        }
    }

    #[test]
    fn battery_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BatteryWidget>();
    }
}
