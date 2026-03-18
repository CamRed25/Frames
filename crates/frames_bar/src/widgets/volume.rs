//! Volume widget renderer — displays audio output level in a GTK Label.
//!
//! Shows "🔇 VOL MUTE" when muted, "🔊 VOL 70%" otherwise when icons are
//! enabled. Applies the `.muted` CSS class when muted.

use gtk::prelude::*;

use frames_core::{VolumeConfig, WidgetData};

/// GTK3 renderer for the volume widget.
pub struct VolumeWidget {
    label: gtk::Label,
    show_icon: bool,
}

impl VolumeWidget {
    /// Create a new volume renderer from the given volume config.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &VolumeConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("VOL --"));
        label.set_widget_name("volume");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-volume");
        Ok(Self {
            label,
            show_icon: config.show_icon.unwrap_or(true),
        })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new volume data: update text and `.muted` CSS class.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Volume { volume_pct, muted } = data {
            let ctx = self.label.style_context();
            let icon = if self.show_icon {
                if *muted {
                    "\u{1F507} "
                } else {
                    "\u{1F50A} "
                }
            } else {
                ""
            };
            let text = if *muted {
                format!("{icon}VOL MUTE")
            } else {
                format!("{icon}VOL {volume_pct:.0}%")
            };
            self.label.set_text(&text);
            if *muted {
                ctx.add_class("muted");
            } else {
                ctx.remove_class("muted");
            }
        }
    }
}
