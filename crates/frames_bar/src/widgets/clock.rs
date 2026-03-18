//! Clock widget renderer — displays a formatted time string in a GTK Label.
//!
//! Consumes [`frames_core::WidgetData::Clock`] and updates the label text.
//! All formatting is done in `frames_core::widgets::clock::ClockWidget`; this
//! renderer only applies the resulting `display` string to the GTK widget.

use gtk::prelude::*;

use frames_core::{ClockConfig, WidgetData};

/// GTK3 renderer for the clock widget.
pub struct ClockWidget {
    label: gtk::Label,
}

impl ClockWidget {
    /// Create a new clock renderer from the given clock config.
    ///
    /// Creates a `GtkLabel`, names it `"clock"`, and adds the standard CSS
    /// classes `.widget` and `.widget-clock` for theming.
    ///
    /// # Errors
    ///
    /// Currently infallible; returns `Ok` always. Signature uses `anyhow::Result`
    /// so the renderer contract is consistent with other widget constructors
    /// that may fail (e.g. when acquiring X11 handles).
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(_config: &ClockConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(None);
        label.set_widget_name("clock");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-clock");
        Ok(Self { label })
    }

    /// Return a reference to the root GTK widget for embedding in the bar.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new widget data to the label.
    ///
    /// Matches [`WidgetData::Clock`] and calls `label.set_text(display)`.
    /// The `_ => {}` fallback is required because [`WidgetData`] is
    /// `#[non_exhaustive]`.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Clock { display } = data {
            self.label.set_text(display);
        }
        // exhaustiveness fallback: other variants are intentionally ignored
    }
}
