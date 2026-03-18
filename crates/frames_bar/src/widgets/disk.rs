//! Disk widget renderer — displays used/total filesystem space for the primary
//! mount point. Shows a hover-tooltip listing all real mounted filesystems.
//!
//! `gtk::Label` has no GDK window so pointer events never reach it; the label
//! is wrapped in an `EventBox` (same pattern as the CPU temperature tooltip)
//! so GTK's tooltip machinery receives the mouse-enter events it needs.
//!
//! Bar label format is controlled by `config.format`:
//! - `"percent"` (default) — `"DISK 5%"`
//! - `"used"` — `"DISK 45.2/931.5 GiB"`
//! - `"free"` — `"DISK free 886.3 GiB"`
//!
//! Hover tooltip shows one line per filesystem:
//! ```text
//! /        870.5 / 931.5 GiB  (93%)
//! /boot    0.5 / 1.0 GiB      (50%)
//! ```

use gtk::prelude::*;

use frames_core::{DiskConfig, WidgetData};

/// GTK3 renderer for the disk usage widget.
pub struct DiskWidget {
    event_box: gtk::EventBox,
    label: gtk::Label,
    format: String,
}

impl DiskWidget {
    /// Create a new disk renderer from the given disk config.
    ///
    /// The label is wrapped in an `EventBox` so that hover-tooltip events are
    /// received. CSS classes `.widget` and `.widget-disk` are applied to the
    /// label. The `format` field controls the bar label style.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &DiskConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("DISK"));
        label.set_widget_name("disk");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-disk");

        let event_box = gtk::EventBox::new();
        event_box.add(&label);

        Ok(Self {
            event_box,
            label,
            format: config.format.clone().unwrap_or_else(|| "percent".to_string()),
        })
    }

    /// Return a reference to the root GTK widget (the `EventBox` container).
    pub fn widget(&self) -> &gtk::Widget {
        self.event_box.upcast_ref()
    }

    /// Apply new disk data: update bar label and build the hover-tooltip
    /// listing all mounted filesystems.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Disk {
            used_bytes,
            total_bytes,
            all_disks,
            ..
        } = data
        {
            // ── Bar label ────────────────────────────────────────────────────
            let text = match self.format.as_str() {
                "percent" => {
                    if *total_bytes == 0 {
                        "DISK --%".to_string()
                    } else {
                        // clippy::cast_precision_loss: byte counts vs f64 precision is acceptable here
                        #[allow(clippy::cast_precision_loss)]
                        let pct = (*used_bytes as f64 / *total_bytes as f64) * 100.0;
                        format!("DISK {pct:.0}%")
                    }
                }
                "free" => {
                    let free = total_bytes.saturating_sub(*used_bytes);
                    format!("DISK free {}", format_bytes(free))
                }
                _ => format!("DISK {}/{}", format_bytes(*used_bytes), format_bytes(*total_bytes)),
            };
            self.label.set_text(&text);

            // ── Hover tooltip — one line per real filesystem ─────────────────
            if all_disks.is_empty() {
                self.event_box.set_tooltip_text(None);
            } else {
                let tooltip = all_disks
                    .iter()
                    .map(|e| {
                        // clippy::cast_precision_loss: byte counts vs f64 acceptable
                        // clippy::cast_possible_truncation: pct is always 0–100, fits in u8
                        // clippy::cast_sign_loss: ratio * 100 is always non-negative
                        #[allow(
                            clippy::cast_precision_loss,
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss
                        )]
                        let pct = if e.total_bytes > 0 {
                            (e.used_bytes as f64 / e.total_bytes as f64 * 100.0) as u8
                        } else {
                            0
                        };
                        format!(
                            "{:<12} {}  /  {}  ({}%)",
                            e.mount,
                            format_bytes(e.used_bytes),
                            format_bytes(e.total_bytes),
                            pct,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.event_box.set_tooltip_text(Some(&tooltip));
            }
        }
    }
}

/// Format bytes as a human-readable size string (`MiB`, `GiB`, or `TiB`).
fn format_bytes(bytes: u64) -> String {
    const TIB: f64 = 1_099_511_627_776.0;
    const GIB: f64 = 1_073_741_824.0;
    const MIB: f64 = 1_048_576.0;
    // clippy::cast_precision_loss: disk sizes well within f64 precision range
    #[allow(clippy::cast_precision_loss)]
    let f = bytes as f64;
    if f >= TIB {
        format!("{:.1} TiB", f / TIB)
    } else if f >= GIB {
        format!("{:.1} GiB", f / GIB)
    } else {
        format!("{:.0} MiB", f / MIB)
    }
}
