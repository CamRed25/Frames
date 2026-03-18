//! Network widget renderer — displays receive/transmit throughput.
//!
//! Shows the interface name (if `config.show_interface` is `true`) plus
//! human-readable rx ↓ / tx ↑ rates in B/s, KB/s, or MB/s.

use gtk::prelude::*;

use parapet_core::{NetworkConfig, WidgetData};

/// GTK3 renderer for the network widget.
pub struct NetworkWidget {
    label: gtk::Label,
    show_interface: bool,
}

impl NetworkWidget {
    /// Create a new network renderer from the given network config.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &NetworkConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("NET"));
        label.set_widget_name("network");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-network");
        Ok(Self {
            label,
            show_interface: config.show_interface.unwrap_or(false),
        })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new network data: format rx/tx rates with human-readable units.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Network {
            rx_bytes_per_sec,
            tx_bytes_per_sec,
            interface,
        } = data
        {
            let rx = human_rate(*rx_bytes_per_sec);
            let tx = human_rate(*tx_bytes_per_sec);
            let text = if self.show_interface {
                format!("{interface} ↓{rx} ↑{tx}")
            } else {
                format!("↓{rx} ↑{tx}")
            };
            self.label.set_text(&text);
        }
    }
}

/// Format bytes/sec as a compact human-readable string.
fn human_rate(bytes_per_sec: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    // clippy::cast_precision_loss: acceptable for display-only byte rates
    #[allow(clippy::cast_precision_loss)]
    let f = bytes_per_sec as f64;
    if f >= MB {
        format!("{:.1}M", f / MB)
    } else if f >= KB {
        format!("{:.0}K", f / KB)
    } else {
        format!("{f:.0}B")
    }
}
