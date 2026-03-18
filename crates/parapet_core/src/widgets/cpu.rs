//! CPU usage widget — data provider using `sysinfo`.
//!
//! Returns aggregate CPU usage, per-core percentages, and the package
//! temperature on each update tick. The first call returns zero usage values
//! because `sysinfo` requires two samples separated by a real time interval to
//! compute a delta (see `WIDGET_API §7.3`). Temperature is read from
//! `sysinfo::Components`; `temp_celsius` is `None` when no temperature sensor
//! is available (VM guests, hardware without `coretemp`/`k10temp` loaded).

use sysinfo::{Components, CpuRefreshKind, RefreshKind, System};

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Provides CPU usage statistics via `sysinfo`.
pub struct CpuWidget {
    name: String,
    system: System,
    components: Components,
    first_call: bool,
}

impl CpuWidget {
    /// Create a new `CpuWidget`.
    ///
    /// Initialises the internal `sysinfo::System` and `sysinfo::Components`.
    /// The first subsequent call to `update()` returns zero usage values; the
    /// second and later calls return real usage percentages.
    ///
    /// # Errors
    ///
    /// Returns [`ParapetError::SysInfo`] if the system object cannot be
    /// initialised (in practice this does not fail on supported platforms).
    pub fn new(name: impl Into<String>) -> Result<Self, ParapetError> {
        let system =
            System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::everything()));
        let components = Components::new_with_refreshed_list();
        Ok(Self {
            name: name.into(),
            system,
            components,
            first_call: true,
        })
    }

    fn per_core(&self) -> Vec<f32> {
        self.system.cpus().iter().map(sysinfo::Cpu::cpu_usage).collect()
    }

    /// Read the CPU package temperature from `sysinfo::Components`.
    ///
    /// Priority order:
    /// 1. A component whose label contains `"package"` (Intel `"CPU Package"`,
    ///    AMD `"Package id 0"`).
    /// 2. The first component whose label contains `"core"` or `"cpu"`.
    /// 3. The first component with a positive temperature reading, regardless
    ///    of label — catches generic hwmon drivers such as `k10temp` whose
    ///    sysfs nodes lack a `temp1_label` file and are reported by sysinfo
    ///    under the driver name alone.
    ///
    /// Returns `None` only when no hwmon components are available at all
    /// (VM guests, hardware with no kernel thermal modules loaded).
    fn read_temp(&self) -> Option<f32> {
        // Priority 1 — explicit package sensor.
        for c in &self.components {
            if c.label().to_lowercase().contains("package") {
                return Some(c.temperature());
            }
        }
        // Priority 2 — first per-core or CPU-named sensor.
        for c in &self.components {
            let l = c.label().to_lowercase();
            if l.contains("core") || l.contains("cpu") {
                return Some(c.temperature());
            }
        }
        // Priority 3 — first sensor with any positive reading (e.g. k10temp).
        self.components
            .iter()
            .find(|c| c.temperature() > 0.0)
            .map(sysinfo::Component::temperature)
    }
}

impl Widget for CpuWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Refresh CPU state and return usage statistics.
    ///
    /// On the first call, returns `usage_pct: 0.0` and an empty `per_core`
    /// list because there is no previous sample to diff against.
    ///
    /// # Errors
    ///
    /// This implementation does not return `Err` under normal conditions.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        self.system.refresh_cpu_specifics(CpuRefreshKind::everything());
        self.components.refresh();
        if self.first_call {
            self.first_call = false;
            tracing::debug!(widget = self.name, "cpu: first call, returning zero");
            return Ok(WidgetData::Cpu {
                usage_pct: 0.0,
                per_core: vec![],
                temp_celsius: self.read_temp(),
            });
        }
        let usage_pct = self.system.global_cpu_info().cpu_usage();
        let per_core = self.per_core();
        let temp_celsius = self.read_temp();
        Ok(WidgetData::Cpu {
            usage_pct,
            per_core,
            temp_celsius,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_widget_name_non_empty() {
        let w = CpuWidget::new("cpu").expect("CpuWidget::new");
        assert!(!w.name().is_empty());
    }

    #[test]
    fn cpu_first_call_returns_zero() {
        let mut w = CpuWidget::new("cpu").expect("CpuWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Cpu {
            usage_pct,
            per_core,
            ..
        } = data
        {
            assert_eq!(usage_pct, 0.0, "first call must return 0.0");
            assert!(per_core.is_empty(), "first call per_core must be empty");
        } else {
            panic!("expected WidgetData::Cpu");
        }
    }

    #[test]
    fn cpu_second_call_returns_valid_range() {
        let mut w = CpuWidget::new("cpu").expect("CpuWidget::new");
        let _ = w.update(); // first call — zero
        let data = w.update().expect("second update");
        if let WidgetData::Cpu { usage_pct, .. } = data {
            assert!((0.0..=100.0).contains(&usage_pct), "usage_pct out of range: {usage_pct}");
        } else {
            panic!("expected WidgetData::Cpu");
        }
    }

    #[test]
    fn cpu_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CpuWidget>();
    }
}
