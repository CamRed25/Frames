//! Built-in widget data providers.
//!
//! Each submodule implements the [`crate::widget::Widget`] trait for one widget
//! type. All modules are pure Rust with no GTK or display-system dependencies.

pub mod battery;
pub mod brightness;
pub mod clock;
pub mod cpu;
pub mod disk;
pub mod media;
pub mod memory;
pub mod network;
pub mod volume;
pub mod weather;
pub mod workspaces;
