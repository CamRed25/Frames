//! Battery widget renderer — displays charge percentage and status.
//!
//! When `config.show_icon` is `true`, prepends a Unicode battery icon.
//! Applies `.warning` and `.critical` CSS classes based on thresholds.

use gtk::prelude::*;

use frames_core::{BatteryConfig, BatteryStatus, WidgetData};

/// GTK3 renderer for the battery widget.
pub struct BatteryWidget {
    label: gtk::Label,
    show_icon: bool,
    warn_threshold: f32,
    crit_threshold: f32,
}

impl BatteryWidget {
    /// Create a new battery renderer from the given battery config.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &BatteryConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("BAT"));
        label.set_widget_name("battery");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-battery");
        Ok(Self {
            label,
            show_icon: config.show_icon.unwrap_or(false),
            warn_threshold: config.warn_threshold.unwrap_or(30.0),
            crit_threshold: config.crit_threshold.unwrap_or(15.0),
        })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new battery data: format charge percentage and update CSS classes.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Battery { charge_pct, status } = data {
            let charge_str =
                charge_pct.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "--".to_string());
            let icon = if self.show_icon {
                match status {
                    BatteryStatus::Charging => "⚡",
                    BatteryStatus::Discharging => "🔋",
                    BatteryStatus::Full => "🔌",
                    BatteryStatus::Unknown => "❓",
                }
            } else {
                ""
            };
            let text = if icon.is_empty() {
                format!("BAT {charge_str}")
            } else {
                format!("{icon} {charge_str}")
            };
            self.label.set_text(&text);

            if let Some(pct) = charge_pct {
                let ctx = self.label.style_context();
                if *pct <= self.crit_threshold {
                    ctx.add_class("critical");
                    ctx.remove_class("warning");
                } else if *pct <= self.warn_threshold {
                    ctx.add_class("warning");
                    ctx.remove_class("critical");
                } else {
                    ctx.remove_class("warning");
                    ctx.remove_class("critical");
                }
            }
        }
    }
}
