//! Disk usage widget — filesystem space data provider using `sysinfo`.
//!
//! Reports used and total bytes for the configured mount point, plus a
//! full summary of every real mounted filesystem. The summary is carried in
//! `WidgetData::Disk::all_disks` and consumed by the `frames_bar` renderer to
//! populate the hover-tooltip dropdown. Virtual or zero-size mounts (tmpfs,
//! devtmpfs, etc.) are excluded from the summary.
//!
//! When the configured mount point is not found (e.g. a typo), the widget
//! returns zero values for the primary entry and logs a `WARN` rather than
//! returning an error.

use sysinfo::Disks;

use crate::error::FramesError;
use crate::widget::{DiskEntry, Widget, WidgetData};

/// Provides disk usage statistics for one primary mount point via `sysinfo`.
pub struct DiskWidget {
    name: String,
    /// Configured mount point (e.g. `"/"` or `"/home"`).
    mount: String,
    disks: Disks,
}

impl DiskWidget {
    /// Create a new `DiskWidget`.
    ///
    /// `mount` is the filesystem mount point to show in the bar label. The
    /// constructor performs an initial disk enumeration so the first call to
    /// `update()` returns real data immediately.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `Result` for consistency with other
    /// widget constructors.
    pub fn new(name: impl Into<String>, mount: impl Into<String>) -> Result<Self, FramesError> {
        Ok(Self {
            name: name.into(),
            mount: mount.into(),
            disks: Disks::new_with_refreshed_list(),
        })
    }

    /// Build the `all_disks` summary from the current disk list.
    ///
    /// Excludes filesystems whose total space is zero (virtual/pseudo mounts
    /// such as `tmpfs`, `devtmpfs`, `sysfs`, etc.).
    fn all_disks(&self) -> Vec<DiskEntry> {
        self.disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| {
                let total = d.total_space();
                DiskEntry {
                    mount: d.mount_point().to_string_lossy().into_owned(),
                    used_bytes: total.saturating_sub(d.available_space()),
                    total_bytes: total,
                }
            })
            .collect()
    }
}

impl Widget for DiskWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Refresh the disk list and return usage statistics.
    ///
    /// `used_bytes`/`total_bytes` reflect the configured `mount` point.
    /// `all_disks` contains one entry per real mounted filesystem and is used
    /// by the renderer to build the hover-tooltip summary.
    ///
    /// Returns zero values for the primary entry if the mount point is not
    /// found rather than returning an error.
    ///
    /// # Errors
    ///
    /// This implementation does not currently return `Err`.
    fn update(&mut self) -> Result<WidgetData, FramesError> {
        self.disks.refresh_list();

        let all_disks = self.all_disks();

        let primary = self
            .disks
            .iter()
            .find(|d| d.mount_point().to_str() == Some(self.mount.as_str()));

        let (used_bytes, total_bytes) = if let Some(disk) = primary {
            let total = disk.total_space();
            (total.saturating_sub(disk.available_space()), total)
        } else {
            tracing::warn!(
                widget = self.name,
                mount = self.mount,
                "disk: mount point not found in disk list; returning zeros"
            );
            (0, 0)
        };

        Ok(WidgetData::Disk {
            mount: self.mount.clone(),
            used_bytes,
            total_bytes,
            all_disks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_update_returns_disk_variant() {
        let mut w = DiskWidget::new("disk", "/").expect("DiskWidget::new");
        let data = w.update().expect("update");
        assert!(matches!(data, WidgetData::Disk { .. }), "expected WidgetData::Disk");
    }

    #[test]
    fn disk_root_has_nonzero_total() {
        let mut w = DiskWidget::new("disk", "/").expect("DiskWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Disk {
            total_bytes,
            used_bytes,
            mount,
            ..
        } = data
        {
            assert_eq!(mount, "/");
            assert!(total_bytes > 0, "root filesystem total must be > 0");
            assert!(used_bytes <= total_bytes, "used cannot exceed total");
        } else {
            panic!("expected WidgetData::Disk");
        }
    }

    #[test]
    fn disk_all_disks_non_empty_for_root() {
        let mut w = DiskWidget::new("disk", "/").expect("DiskWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Disk { all_disks, .. } = data {
            assert!(!all_disks.is_empty(), "all_disks should contain at least one entry");
            // Every entry must have non-zero total (virtual mounts are excluded).
            for entry in &all_disks {
                assert!(entry.total_bytes > 0, "entry {} has zero total", entry.mount);
            }
        } else {
            panic!("expected WidgetData::Disk");
        }
    }

    #[test]
    fn disk_missing_mount_returns_zeros() {
        let mut w =
            DiskWidget::new("disk", "/nonexistent-mount-point-xyz").expect("DiskWidget::new");
        let data = w.update().expect("update should not error for missing mount");
        if let WidgetData::Disk {
            used_bytes,
            total_bytes,
            ..
        } = data
        {
            assert_eq!(used_bytes, 0);
            assert_eq!(total_bytes, 0);
        } else {
            panic!("expected WidgetData::Disk");
        }
    }

    #[test]
    fn disk_name_is_stable() {
        let w = DiskWidget::new("my-disk", "/").expect("DiskWidget::new");
        assert_eq!(w.name(), "my-disk");
    }
}
