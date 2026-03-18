//! Brightness widget renderer — displays screen backlight level in a GTK Label.
//!
//! Shows "BRI 75%" or "☀ 75%" with icons enabled. Displays "BRI --" on
//! machines without a variable backlight (e.g. desktops).

use gtk::prelude::*;

use parapet_core::{BrightnessConfig, WidgetData};

/// GTK3 renderer for the brightness widget.
pub struct BrightnessWidget {
    label: gtk::Label,
    show_icon: bool,
}

impl BrightnessWidget {
    /// Create a new brightness renderer from the given brightness config.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &BrightnessConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("BRI --"));
        label.set_widget_name("brightness");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-brightness");
        Ok(Self {
            label,
            show_icon: config.show_icon.unwrap_or(true),
        })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new brightness data to the label.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Brightness { brightness_pct } = data {
            let icon = if self.show_icon { "\u{2600} " } else { "" };
            self.label.set_text(&format!("{icon}BRI {brightness_pct:.0}%"));
        }
    }
}
