//! CPU widget renderer — displays aggregate usage percentage.
//! Applies `.warning` and `.critical` CSS classes based on thresholds from
//! the widget config. Shows CPU temperature as a hover tooltip.
//!
//! `gtk::Label` has no GDK window of its own so mouse-enter/leave events
//! never reach it; tooltip tracking requires a windowed container. The label
//! is therefore wrapped in an `EventBox`, which has its own GDK window and
//! receives pointer events. The tooltip is set on the `EventBox`.

use gtk::prelude::*;

use parapet_core::{CpuConfig, WidgetData};

/// GTK3 renderer for the CPU usage widget.
pub struct CpuWidget {
    event_box: gtk::EventBox,
    label: gtk::Label,
    warn_threshold: f32,
    crit_threshold: f32,
}

impl CpuWidget {
    /// Create a new CPU renderer from the given CPU config.
    ///
    /// The label is wrapped in an `EventBox` so that hover-tooltip tracking
    /// works. CSS classes `.widget` and `.widget-cpu` are applied to the
    /// label. Threshold values drive `.warning` and `.critical` class toggling.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for renderer contract
    /// consistency.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(config: &CpuConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(Some("CPU"));
        label.set_widget_name("cpu");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-cpu");

        let event_box = gtk::EventBox::new();
        event_box.add(&label);

        Ok(Self {
            event_box,
            label,
            warn_threshold: config.warn_threshold.unwrap_or(70.0),
            crit_threshold: config.crit_threshold.unwrap_or(90.0),
        })
    }

    /// Return a reference to the root GTK widget (the `EventBox` container).
    pub fn widget(&self) -> &gtk::Widget {
        self.event_box.upcast_ref()
    }

    /// Apply new CPU data: update label text, threshold CSS classes, and the
    /// temperature tooltip on the `EventBox`.
    ///
    /// When `temp_celsius` is present, the hover tooltip reads `"72°C"`.
    /// When absent (VM or hardware without thermal sensors) the tooltip is
    /// cleared.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Cpu {
            usage_pct,
            temp_celsius,
            ..
        } = data
        {
            self.label.set_text(&format!("CPU {usage_pct:.0}%"));
            let ctx = self.label.style_context();
            if *usage_pct >= self.crit_threshold {
                ctx.add_class("critical");
                ctx.remove_class("warning");
            } else if *usage_pct >= self.warn_threshold {
                ctx.add_class("warning");
                ctx.remove_class("critical");
            } else {
                ctx.remove_class("warning");
                ctx.remove_class("critical");
            }
            // Tooltip lives on the EventBox, which has a GDK window and
            // therefore receives the mouse-enter events GTK needs to trigger
            // tooltip display.
            match temp_celsius {
                Some(t) => self.event_box.set_tooltip_text(Some(&format!("{t:.0}°C"))),
                None => self.event_box.set_tooltip_text(None),
            }
        }
    }
}
