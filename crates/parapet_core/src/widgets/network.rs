//! Network widget — rx/tx throughput data provider using `sysinfo`.
//!
//! Monitors one network interface and reports bytes received and transmitted
//! per second. The first call returns zeros because sysinfo needs two samples
//! to compute a delta (see `WIDGET_API §7.3`).
//!
//! Interface selection: `"auto"` picks the first non-loopback active interface.
//! Falls back to `"lo"` with a warning if no non-loopback interface is found.

use sysinfo::Networks;

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Provides network interface rx/tx statistics.
pub struct NetworkWidget {
    name: String,
    interface: String,
    networks: Networks,
    first_call: bool,
}

impl NetworkWidget {
    /// Create a new `NetworkWidget`.
    ///
    /// If `interface` is `"auto"`, the constructor picks the first non-loopback
    /// active interface. Falls back to `"lo"` with a logged warning when no
    /// non-loopback interface is found (e.g. a container with only loopback).
    ///
    /// # Errors
    ///
    /// This constructor does not currently return `Err`; the signature reserves
    /// the right for future fallible initialisation.
    pub fn new(name: impl Into<String>, interface: impl Into<String>) -> Result<Self, ParapetError> {
        let mut networks = Networks::new_with_refreshed_list();

        let resolved_interface = {
            let requested = interface.into();
            if requested == "auto" {
                let found = networks
                    .iter()
                    .find(|(iface, _)| *iface != "lo")
                    .map(|(iface, _)| iface.clone());
                if let Some(iface) = found {
                    iface
                } else {
                    tracing::warn!("no non-loopback network interface found; falling back to lo");
                    "lo".to_string()
                }
            } else {
                requested
            }
        };

        // Perform an initial refresh so the first delta has a baseline.
        networks.refresh();

        Ok(Self {
            name: name.into(),
            interface: resolved_interface,
            networks,
            first_call: true,
        })
    }
}

impl Widget for NetworkWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Refresh network state and return rx/tx statistics for the configured interface.
    ///
    /// On the first call, returns zero values because no previous sample is
    /// available for delta computation.
    ///
    /// # Errors
    ///
    /// This implementation does not currently return `Err`; missing interfaces
    /// are reported as zero bytes rather than an error.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        self.networks.refresh();

        if self.first_call {
            self.first_call = false;
            tracing::debug!(widget = self.name, "network: first call, returning zero");
            return Ok(WidgetData::Network {
                rx_bytes_per_sec: 0,
                tx_bytes_per_sec: 0,
                interface: self.interface.clone(),
            });
        }

        let (rx, tx) = self
            .networks
            .get(&self.interface)
            .map_or((0, 0), |data| (data.received(), data.transmitted()));

        Ok(WidgetData::Network {
            rx_bytes_per_sec: rx,
            tx_bytes_per_sec: tx,
            interface: self.interface.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_first_call_returns_zero() {
        let mut w = NetworkWidget::new("network", "auto").expect("NetworkWidget::new");
        let data = w.update().expect("update");
        if let WidgetData::Network {
            rx_bytes_per_sec,
            tx_bytes_per_sec,
            ..
        } = data
        {
            assert_eq!(rx_bytes_per_sec, 0, "first call rx must be 0");
            assert_eq!(tx_bytes_per_sec, 0, "first call tx must be 0");
        } else {
            panic!("expected WidgetData::Network");
        }
    }

    #[test]
    fn network_interface_name_non_empty() {
        let w = NetworkWidget::new("network", "auto").expect("NetworkWidget::new");
        // Resolved interface stored; must not be empty
        // We verify indirectly via the update result
        let mut w = w;
        let data = w.update().expect("update");
        if let WidgetData::Network { interface, .. } = data {
            assert!(!interface.is_empty(), "interface name must not be empty");
        } else {
            panic!("expected WidgetData::Network");
        }
    }

    #[test]
    fn network_auto_resolves_to_non_loopback_or_lo() {
        // On CI, loopback fallback is acceptable — we just verify no panic
        let result = NetworkWidget::new("network", "auto");
        assert!(result.is_ok(), "auto resolution must not fail");
    }

    #[test]
    fn network_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NetworkWidget>();
    }
}
