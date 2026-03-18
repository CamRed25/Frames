//! Memory widget renderer — displays used/total or percentage RAM usage.
//!
//! Format is controlled by `config.format`:
//! - `"used"` (default) — `"MEM 3.2/15.6 GiB"`
//! - `"percent"` — `"MEM 21%"`
//! - `"free"` — `"MEM free 12.4 GiB"`

use gtk::prelude::*;

use frames_core::{MemoryConfig, WidgetData};

/// GTK3 renderer for the memory widget.
pub struct MemoryWidget {
    label: gtk::Label,
    format: String,
}

impl MemoryWidget {
    /// Create a new memory renderer from the given memory config.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &MemoryConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("MEM"));
        label.set_widget_name("memory");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-memory");
        Ok(Self {
            label,
            format: config.format.clone().unwrap_or_else(|| "used".to_string()),
        })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new memory data; format according to `config.format`.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Memory {
            used_bytes,
            total_bytes,
            ..
        } = data
        {
            let text = match self.format.as_str() {
                "percent" => {
                    if *total_bytes == 0 {
                        "MEM --%".to_string()
                    } else {
                        // clippy::cast_precision_loss: byte counts vs f64 precision is acceptable here
                        #[allow(clippy::cast_precision_loss)]
                        let pct = (*used_bytes as f64 / *total_bytes as f64) * 100.0;
                        format!("MEM {pct:.0}%")
                    }
                }
                "free" => {
                    let free = total_bytes.saturating_sub(*used_bytes);
                    format!("MEM free {}", gib(free))
                }
                _ => format!("MEM {}/{}", gib(*used_bytes), gib(*total_bytes)),
            };
            self.label.set_text(&text);
        }
    }
}

/// Format bytes as `X.X GiB` or `XXX MiB`.
fn gib(bytes: u64) -> String {
    const GIB: f64 = 1_073_741_824.0;
    const MIB: f64 = 1_048_576.0;
    // clippy::cast_precision_loss: byte counts ≤ ~16 EiB; acceptable f64 precision
    #[allow(clippy::cast_precision_loss)]
    let f = bytes as f64;
    if f >= GIB {
        format!("{:.1} GiB", f / GIB)
    } else {
        format!("{:.0} MiB", f / MIB)
    }
}
