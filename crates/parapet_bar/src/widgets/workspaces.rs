//! Workspace switcher renderer — queries X11 EWMH directly.
//!
//! Reads `_NET_NUMBER_OF_DESKTOPS`, `_NET_CURRENT_DESKTOP`, and
//! `_NET_DESKTOP_NAMES` from the root window. Renders as a row of buttons;
//! clicking a button sends `_NET_CURRENT_DESKTOP` via a client message.
//!
//! # Architecture note
//!
//! This renderer does **not** use a `parapet_core` `WorkspacesWidget` or the
//! `Poller`. It fetches fresh X11 data on every `refresh()` call and
//! schedules its own `glib::timeout_add_local` timer. This is intentional —
//! workspace changes are low-frequency EWMH events that do not benefit from
//! the generic polling abstraction.

use std::process::Command;

use gdk::prelude::*;
use gtk::prelude::*;

/// GTK3 renderer for the workspace switcher widget.
pub struct WorkspacesWidget {
    container: gtk::Box,
}

impl WorkspacesWidget {
    /// Create a new workspace renderer.
    ///
    /// Create a new workspace renderer.
    // clippy::unnecessary_wraps: consistent renderer contract — future display init may fail
    #[allow(clippy::unnecessary_wraps)]
    pub fn new() -> anyhow::Result<Self> {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        container.set_widget_name("workspaces");
        container.style_context().add_class("widget");
        container.style_context().add_class("widget-workspaces");
        Ok(Self { container })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }

    /// Refresh workspace buttons using X11 EWMH data.
    ///
    /// Clears existing buttons and rebuilds from current desktop state.
    pub fn refresh(&self) {
        // Clear all existing children.
        for child in self.container.children() {
            self.container.remove(&child);
        }

        let (count, current, names) = read_workspaces();

        for idx in 0..count {
            // Use the raw EWMH name only if it is a short custom name; strip the
            // common Cinnamon default "Workspace N" down to just "N", and fall
            // back to the 1-based index when no name is set.
            let raw = names.get(idx).map_or("", String::as_str);
            let label =
                if raw.eq_ignore_ascii_case(&format!("workspace {}", idx + 1)) || raw.is_empty() {
                    (idx + 1).to_string()
                } else {
                    raw.to_string()
                };
            let btn = gtk::Button::with_label(&label);
            btn.set_relief(gtk::ReliefStyle::None);
            btn.style_context().add_class("workspace");
            if idx == current {
                btn.style_context().add_class("active");
            }
            // Send _NET_CURRENT_DESKTOP client message on click.
            let target = idx;
            btn.connect_clicked(move |b| {
                if let Some(win) = b.window() {
                    send_desktop_change(&win, target);
                }
            });
            self.container.add(&btn);
        }
        self.container.show_all();
    }
}

/// Read workspace state from the root X11 window via EWMH atoms.
///
/// Returns `(desktop_count, current_desktop, names)`. Defaults to 1 desktop
/// index 0 when EWMH properties are unavailable.
fn read_workspaces() -> (usize, usize, Vec<String>) {
    let root = gdk::Window::default_root_window();
    let count = get_cardinal(&root, "_NET_NUMBER_OF_DESKTOPS").unwrap_or(1);
    let current = get_cardinal(&root, "_NET_CURRENT_DESKTOP").unwrap_or(0);
    let names = get_utf8_list(&root, "_NET_DESKTOP_NAMES");
    (count, current, names)
}

/// Read a `CARDINAL` (u32) property from the given window.
fn get_cardinal(win: &gdk::Window, atom_name: &str) -> Option<usize> {
    let atom = gdk::Atom::intern(atom_name);
    let cardinal_type = gdk::Atom::intern("CARDINAL");
    // offset and length are in multiples of 32-bit words; 0 offset, 1 word.
    let (_, _, data) = gdk::property_get(win, &atom, &cardinal_type, 0, 4, 0)?;
    // Cardinal data arrives as a Vec<u8>; interpret first 4 bytes as native-endian u32.
    if data.len() < 4 {
        return None;
    }
    let bytes: [u8; 4] = data[..4].try_into().ok()?;
    #[allow(clippy::cast_possible_truncation)] // desktop count is always small
    Some(u32::from_ne_bytes(bytes) as usize)
}

/// Read a `UTF8_STRING` list property (NUL-separated) from the given window.
fn get_utf8_list(win: &gdk::Window, atom_name: &str) -> Vec<String> {
    let atom = gdk::Atom::intern(atom_name);
    let utf8_type = gdk::Atom::intern("UTF8_STRING");
    let Some((_, _, data)) = gdk::property_get(win, &atom, &utf8_type, 0, 65536, 0) else {
        return Vec::new();
    };
    // Desktop names are NUL-separated.
    let raw = String::from_utf8_lossy(&data);
    raw.split('\0')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Send a `_NET_CURRENT_DESKTOP` client message to switch to workspace `idx`.
///
/// Uses `wmctrl -s <idx>` which sends the correct EWMH `_NET_CURRENT_DESKTOP`
/// `ClientMessage` event. Setting the property directly via `XChangeProperty` is
/// ignored by Cinnamon — only the `ClientMessage` path triggers a workspace switch.
fn send_desktop_change(_win: &gdk::Window, idx: usize) {
    if let Err(e) = Command::new("wmctrl")
        .args(["-s", &idx.to_string()])
        .spawn()
    {
        tracing::warn!(error = %e, desktop = idx, "wmctrl failed to send desktop change");
    }
}
