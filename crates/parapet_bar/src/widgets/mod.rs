//! Widget renderers for `parapet_bar`.
//!
//! Each sub-module contains a GTK3 renderer for one widget type. Renderers
//! consume [`parapet_core::WidgetData`] produced by the polling loop and apply
//! it to their GTK widgets. No data computation takes place here — only
//! display formatting and CSS class updates.

pub mod battery;
pub mod brightness;
pub mod clock;
pub mod cpu;
pub mod disk;
pub mod launcher;
pub mod media;
pub mod memory;
pub mod network;
pub mod volume;
pub mod weather;
pub mod workspaces;
