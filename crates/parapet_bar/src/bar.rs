//! Bar window — GTK3 popup window with EWMH dock properties.
//!
//! Creates and manages the main bar window. Responsible for:
//! - GTK3 window setup (`Popup` type, `Dock` hint, no decorations)
//! - Three-section horizontal layout (left / center / right)
//! - X11 EWMH strut properties to reserve screen space after realize
//! - Correct positioning on the chosen monitor
//!
//! The strut and positioning are applied inside `connect_realize` — never
//! before — because the GDK window handle (`gdk::Window`) does not exist
//! until the GTK window is realized.

use gdk::prelude::*;
use gtk::prelude::*;

use parapet_core::config::{BarConfig, BarPosition, BarSection};

/// The main bar window.
///
/// Owns the GTK window and the three layout sections. Add widget renderers
/// via [`Bar::add_widget`] and call [`Bar::show`] when ready.
pub struct Bar {
    window: gtk::Window,
    left: gtk::Box,
    center: gtk::Box,
    right: gtk::Box,
}

impl Bar {
    /// Create a new bar window from the given [`BarConfig`].
    ///
    /// Configures the GTK window (Popup type, Dock hint, no title bar) and
    /// creates the three-section interior layout. The `connect_realize`
    /// callback is registered to position the window and apply the EWMH strut
    /// once the underlying X11 window handle is available.
    ///
    /// The window is NOT shown yet — call [`Bar::show`] when all widgets have
    /// been added.
    pub fn new(config: &BarConfig) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("parapet");
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_type_hint(gdk::WindowTypeHint::Dock);
        window.set_app_paintable(true);

        // Enable RGBA compositing so .parapet-bar can be fully transparent and
        // pill sections float over the desktop wallpaper. Falls back silently;
        // on non-compositing WMs the pill background is still opaque.
        if let Some(visual) = gdk::Display::default().and_then(|d| d.default_screen().rgba_visual())
        {
            window.set_visual(Some(&visual));
        } else {
            tracing::warn!("RGBA visual unavailable; bar background will be opaque");
        }

        // Three-section bar interior
        let bar_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        // clippy::cast_possible_wrap: widget_spacing is 0–255 in practice; fits i32
        #[allow(clippy::cast_possible_wrap)]
        let spacing = config.widget_spacing as i32;
        let left = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
        let center = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
        let right = gtk::Box::new(gtk::Orientation::Horizontal, spacing);

        // CSS classes per UI_GUIDE §6.2
        bar_box.style_context().add_class("parapet-bar");
        left.style_context().add_class("parapet-left");
        center.style_context().add_class("parapet-center");
        right.style_context().add_class("parapet-right");

        bar_box.pack_start(&left, false, false, 0);
        bar_box.set_center_widget(Some(&center));
        bar_box.pack_end(&right, false, false, 0);

        window.add(&bar_box);

        // Get monitor geometry now (display is available after gtk::init()) so we can
        // set_default_size and move_ before show_all, ensuring the window is the right
        // size on first paint. Popup/override-redirect windows are not managed by the WM
        // so move_() before realize sets the stored position correctly.
        let display = gdk::Display::default().expect("no GDK display after gtk::init");
        let monitor = display
            .primary_monitor()
            .or_else(|| display.monitor(0))
            .expect("no monitor available on display");
        let geom = monitor.geometry();
        let position = config.position.clone();
        let height = config.height;

        // clippy::cast_possible_wrap: bar height and monitor coords are well under 2^31
        #[allow(clippy::cast_possible_wrap)]
        let height_i32 = height as i32;
        #[allow(clippy::cast_possible_wrap)]
        let bar_y = match position {
            BarPosition::Top => geom.y(),
            BarPosition::Bottom => geom.y() + geom.height() - height_i32,
        };

        // set_default_size + move_ before show_all guarantee full-width first paint.
        window.set_default_size(geom.width(), height_i32);
        window.move_(geom.x(), bar_y);

        // connect_realize is still needed to apply the X11 EWMH strut, which requires
        // the underlying gdk::Window handle.
        window.connect_realize(move |win| {
            if let Some(gdk_win) = win.window() {
                apply_strut(&gdk_win, &position, height, &geom);
            }
        });

        Self {
            window,
            left,
            center,
            right,
        }
    }

    /// Add a GTK widget to the given bar section.
    ///
    /// Widgets within a section appear in the order they are added.
    /// `section` maps to left/center/right per [`BarSection`].
    pub fn add_widget(&self, widget: &gtk::Widget, section: &BarSection) {
        match section {
            BarSection::Left => self.left.pack_start(widget, false, false, 0),
            BarSection::Center => self.center.pack_start(widget, false, false, 0),
            BarSection::Right => self.right.pack_start(widget, false, false, 0),
        }
    }

    /// Remove all widget children from the left, centre, and right sections.
    ///
    /// Called before rebuilding the widget tree on a hot-reload cycle. Safe to
    /// call while the bar window is visible — GTK removes the children
    /// immediately and repaints on the next expose event.
    pub fn clear_widgets(&self) {
        for child in self.left.children() {
            self.left.remove(&child);
        }
        for child in self.center.children() {
            self.center.remove(&child);
        }
        for child in self.right.children() {
            self.right.remove(&child);
        }
    }

    /// Show the bar window (delegates to `window.show_all()`).
    pub fn show(&self) {
        self.window.show_all();
    }
}

/// Apply `_NET_WM_STRUT_PARTIAL` and legacy `_NET_WM_STRUT` on the GDK window.
///
/// Must be called only after the bar window is realized (has an X11 handle).
/// See `BAR_DESIGN` §3 for the full 12-element strut array layout.
///
/// # Parameters
///
/// * `gdk_window` — the realized `gdk::Window`
/// * `position` — bar position (top or bottom)
/// * `height` — bar height in pixels
/// * `geom` — monitor geometry rectangle
fn apply_strut(
    gdk_window: &gdk::Window,
    position: &BarPosition,
    height: u32,
    geom: &gdk::Rectangle,
) {
    // clippy::cast_sign_loss: monitor x/width are always non-negative pixel offsets
    #[allow(clippy::cast_sign_loss)]
    let x = geom.x() as u32;
    #[allow(clippy::cast_sign_loss)]
    let w = geom.width() as u32;

    // _NET_WM_STRUT_PARTIAL: 12 CARDINAL values — BAR_DESIGN §3.2 / §3.3
    let strut: [u32; 12] = match position {
        BarPosition::Top => [
            0,
            0,
            height,
            0, // left, right, top, bottom
            0,
            0, // left_start_y, left_end_y
            0,
            0, // right_start_y, right_end_y
            x,
            x + w, // top_start_x, top_end_x
            0,
            0, // bottom_start_x, bottom_end_x
        ],
        BarPosition::Bottom => [
            0,
            0,
            0,
            height, // left, right, top, bottom
            0,
            0, // left_start_y, left_end_y
            0,
            0, // right_start_y, right_end_y
            0,
            0, // top_start_x, top_end_x
            x,
            x + w, // bottom_start_x, bottom_end_x
        ],
    };

    let strut_wide: Vec<libc::c_ulong> = strut.iter().map(|&v| libc::c_ulong::from(v)).collect();
    let strut4: Vec<libc::c_ulong> = strut[..4].iter().map(|&v| libc::c_ulong::from(v)).collect();

    let cardinal = gdk::Atom::intern("CARDINAL");
    let p_strut = gdk::Atom::intern("_NET_WM_STRUT_PARTIAL");
    let p_legacy = gdk::Atom::intern("_NET_WM_STRUT");

    gdk::property_change(
        gdk_window,
        &p_strut,
        &cardinal,
        32,
        gdk::PropMode::Replace,
        gdk::ChangeData::ULongs(&strut_wide),
    );
    gdk::property_change(
        gdk_window,
        &p_legacy,
        &cardinal,
        32,
        gdk::PropMode::Replace,
        gdk::ChangeData::ULongs(&strut4),
    );

    // Flush so the X server receives the property before MapWindow arrives.
    gdk_window.display().flush();

    tracing::debug!(
        position = ?position,
        height,
        monitor_x = x,
        monitor_w = w,
        "strut applied"
    );
}
