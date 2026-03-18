//! Brightness widget — screen backlight level data provider.
//!
//! Reads `brightness` and `max_brightness` from the first entry under
//! `/sys/class/backlight/`. Returns `brightness_pct: 0.0` on machines without a
//! backlight (desktop PCs without a variable backlight) — correct, not an error.

use std::path::{Path, PathBuf};

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// The sysfs root for backlight entries.
const BACKLIGHT_PATH: &str = "/sys/class/backlight";

/// Provides screen brightness by reading sysfs.
pub struct BrightnessWidget {
    name: String,
    sysfs_root: PathBuf,
    last_data: Option<WidgetData>,
}

impl BrightnessWidget {
    /// Create a new `BrightnessWidget` reading from the real sysfs path.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sysfs_root: PathBuf::from(BACKLIGHT_PATH),
            last_data: None,
        }
    }

    /// Create a `BrightnessWidget` reading from a custom sysfs root.
    ///
    /// Used in tests to provide a fake `/sys/class/backlight/` tree.
    #[must_use]
    pub fn new_with_sysfs_root(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            sysfs_root: root.into(),
            last_data: None,
        }
    }

    /// Read current brightness as a percentage from sysfs.
    ///
    /// Returns `None` if no backlight entry exists or the files cannot be read.
    fn read_brightness(&self) -> Option<f32> {
        let path = std::fs::read_dir(&self.sysfs_root).ok()?.find_map(Result::ok)?.path();
        let current = read_sysfs_u32(&path.join("brightness"))?;
        let max = read_sysfs_u32(&path.join("max_brightness"))?;
        if max == 0 {
            return None;
        }
        // clippy::cast_precision_loss: brightness values are small integers; precision loss is negligible
        #[allow(clippy::cast_precision_loss)]
        Some((current as f32) / (max as f32) * 100.0)
    }
}

/// Read the first line of a sysfs file as a `u32`.
fn read_sysfs_u32(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

impl Widget for BrightnessWidget {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let brightness_pct = self.read_brightness().unwrap_or({
            if let Some(WidgetData::Brightness {
                brightness_pct: last,
            }) = &self.last_data
            {
                *last
            } else {
                0.0
            }
        });
        let data = WidgetData::Brightness { brightness_pct };
        self.last_data = Some(data.clone());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn brightness_widget_name_non_empty() {
        let w = BrightnessWidget::new("bri");
        assert!(!w.name().is_empty());
    }

    #[test]
    fn brightness_widget_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BrightnessWidget>();
    }

    #[test]
    fn brightness_no_backlight_returns_zero() {
        let dir = TempDir::new().expect("tempdir");
        let mut w = BrightnessWidget::new_with_sysfs_root("bri", dir.path());
        let result = w.update().expect("update should not error");
        assert!(
            matches!(result, WidgetData::Brightness { brightness_pct } if brightness_pct < 0.01),
            "expected 0%, got {result:?}"
        );
    }

    #[test]
    fn brightness_reads_correct_percentage() {
        let dir = TempDir::new().expect("tempdir");
        let bl_dir = dir.path().join("intel_backlight");
        std::fs::create_dir(&bl_dir).expect("mkdir");
        std::fs::write(bl_dir.join("brightness"), "750\n").expect("write");
        std::fs::write(bl_dir.join("max_brightness"), "1000\n").expect("write");
        let mut w = BrightnessWidget::new_with_sysfs_root("bri", dir.path());
        let result = w.update().expect("update should not error");
        assert!(
            matches!(result, WidgetData::Brightness { brightness_pct } if (brightness_pct - 75.0).abs() < 0.1),
            "expected 75%, got {result:?}"
        );
    }
}
