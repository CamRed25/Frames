//! Memory widget — RAM and swap usage data provider using `sysinfo`.
//!
//! Returns total and used bytes for both RAM and swap on each update tick.

use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Provides RAM and swap usage statistics via `sysinfo`.
pub struct MemoryWidget {
    name: String,
    system: System,
}

impl MemoryWidget {
    /// Create a new `MemoryWidget`.
    ///
    /// Initialises the internal `sysinfo::System` with memory refresh enabled.
    ///
    /// # Errors
    ///
    /// Returns [`ParapetError::SysInfo`] if `total_bytes` would be zero on the
    /// first read, indicating the system cannot provide memory information.
    pub fn new(name: impl Into<String>) -> Result<Self, ParapetError> {
        let system = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        Ok(Self {
            name: name.into(),
            system,
        })
    }
}

impl Widget for MemoryWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Refresh memory state and return current usage statistics.
    ///
    /// # Errors
    ///
    /// Returns [`ParapetError::SysInfo`] if `total_memory()` reports zero bytes,
    /// which indicates that sysinfo cannot read memory information on this host.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        self.system.refresh_memory();
        let total_bytes = self.system.total_memory();
        if total_bytes == 0 {
            return Err(ParapetError::SysInfo("total memory is zero".to_string()));
        }
        Ok(WidgetData::Memory {
            used_bytes: self.system.used_memory(),
            total_bytes,
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_update_returns_valid_totals() {
        let mut w = MemoryWidget::new("memory").expect("MemoryWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Memory {
            total_bytes,
            used_bytes,
            ..
        } = data
        {
            assert!(total_bytes > 0, "total_bytes must be > 0");
            assert!(used_bytes <= total_bytes, "used cannot exceed total");
        } else {
            panic!("expected WidgetData::Memory");
        }
    }

    #[test]
    fn memory_swap_fields_non_negative() {
        let mut w = MemoryWidget::new("memory").expect("MemoryWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Memory {
            swap_used,
            swap_total,
            ..
        } = data
        {
            assert!(swap_used <= swap_total, "swap_used must not exceed swap_total");
        } else {
            panic!("expected WidgetData::Memory");
        }
    }

    #[test]
    fn memory_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MemoryWidget>();
    }
}
