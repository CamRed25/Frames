//! Media widget renderer — displays the currently playing track title, artist,
//! and playback status icon in a GTK label.
//!
//! The renderer is display-only. Playback controls (play/pause, next, previous)
//! are wired via the `on_click` / `on_scroll_up` / `on_scroll_down` config
//! fields to `playerctl` shell commands, keeping this module purely visual.

use gtk::prelude::*;

use parapet_core::widget::PlaybackStatus;
use parapet_core::{MediaConfig, WidgetData};

/// Maximum display length (in Unicode scalar values) for the track title and
/// artist fields. Strings longer than this threshold are truncated with `…`.
const MAX_TITLE_CHARS: usize = 30;
const MAX_ARTIST_CHARS: usize = 30;

/// GTK3 renderer for the MPRIS2 media widget.
pub struct MediaWidget {
    label: gtk::Label,
}

impl MediaWidget {
    /// Create a new media renderer from the given widget config.
    ///
    /// CSS classes `.widget` and `.widget-media` are applied to the label.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for a consistent renderer
    /// contract with constructors that can fail.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(_config: &MediaConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(None);
        label.set_widget_name("media");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-media");
        Ok(Self { label })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new media data: update label text based on playback status.
    ///
    /// - `Stopped` — empty label (hides the widget's rendered space).
    /// - `Paused`  — `"⏸ {title}"` with title truncated to `MAX_TITLE_CHARS`.
    /// - `Playing` — `"▶ {title}  —  {artist}"` with each field truncated.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Media {
            title,
            artist,
            status,
            ..
        } = data
        {
            let text = match status {
                PlaybackStatus::Stopped => String::new(),
                PlaybackStatus::Paused => {
                    format!("⏸ {}", truncate(title, MAX_TITLE_CHARS))
                }
                PlaybackStatus::Playing => {
                    format!(
                        "▶ {}  —  {}",
                        truncate(title, MAX_TITLE_CHARS),
                        truncate(artist, MAX_ARTIST_CHARS)
                    )
                }
            };
            self.label.set_text(&text);
        }
    }
}

/// Truncate `s` to at most `max_chars` Unicode scalar values.
///
/// If truncation is required, the last character is replaced with `…` so
/// the total remains at most `max_chars` visual positions.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        // There are more characters beyond max_chars.
        let mut truncated: String = collected.chars().take(max_chars - 1).collect();
        truncated.push('…');
        truncated
    } else {
        collected
    }
}
