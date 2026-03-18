//! MPRIS2 media player data provider widget.
//!
//! Connects to the `org.mpris.MediaPlayer2` D-Bus interface to read the
//! currently playing track and playback status. Uses `zbus` with the blocking
//! API so the `Widget::update()` contract (synchronous, returns immediately) is
//! satisfied without spawning additional threads beyond the one that `zbus`
//! itself creates per connection.
//!
//! If no player is present on the session bus, or if D-Bus is unavailable (e.g.
//! in a headless CI environment), `update()` returns `WidgetData::Media` with
//! `status: Stopped` and empty strings — player absence is a normal condition,
//! not an error.
//!
//! See `DOCS/research/mpris.md` for crate selection rationale and design notes.

use std::collections::HashMap;

use zbus::blocking::Connection;
use zbus::zvariant::OwnedValue;

use crate::error::FramesError;
use crate::widget::{PlaybackStatus, Widget, WidgetData};

/// The D-Bus well-known name prefix for MPRIS2 players.
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
/// The canonical MPRIS2 object path.
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
/// The MPRIS2 Player interface name.
const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

// ── MediaWidget ───────────────────────────────────────────────────────────────

/// Data provider for the MPRIS2 media widget.
///
/// Queries the active MPRIS2 player on each `update()` call. The D-Bus
/// connection is established lazily on the first call and reused thereafter.
/// If the connection fails, the widget returns `Stopped` gracefully.
pub struct MediaWidget {
    name: String,
    /// Session bus connection. `None` until the first successful connect.
    conn: Option<Connection>,
    /// Last successfully fetched data returned on D-Bus errors.
    last: Option<WidgetData>,
}

impl MediaWidget {
    /// Create a new `MediaWidget`.
    ///
    /// # Parameters
    /// - `name` — stable widget name for logging and Poller registration.
    ///
    /// # Returns
    ///
    /// A `MediaWidget` whose D-Bus connection is deferred until the first
    /// `update()` call.
    ///
    /// # Side effects
    ///
    /// None. All I/O is deferred to `update()`.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            conn: None,
            last: None,
        }
    }

    /// Return the stopped/empty `WidgetData::Media` variant.
    fn stopped() -> WidgetData {
        WidgetData::Media {
            title: String::new(),
            artist: String::new(),
            status: PlaybackStatus::Stopped,
            can_go_next: false,
            can_go_previous: false,
        }
    }

    /// Map a `PlaybackStatus` property string to [`PlaybackStatus`].
    ///
    /// Unknown strings are treated as `Stopped`.
    fn parse_playback_status(s: &str) -> PlaybackStatus {
        match s {
            "Playing" => PlaybackStatus::Playing,
            "Paused" => PlaybackStatus::Paused,
            _ => PlaybackStatus::Stopped,
        }
    }

    /// Find the first MPRIS bus name active on `conn`, if any.
    fn find_player(conn: &Connection) -> Option<String> {
        let names: Vec<String> = conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "ListNames",
                &(),
            )
            .ok()
            .and_then(|msg| msg.body().deserialize().ok())
            .unwrap_or_default();

        names.into_iter().find(|n| n.starts_with(MPRIS_PREFIX))
    }

    /// Fetch all MPRIS2 Player interface properties in a single `GetAll` call.
    ///
    /// # Parameters
    /// - `conn` — active session bus connection.
    /// - `dest` — the well-known bus name of the player (e.g.
    ///   `"org.mpris.MediaPlayer2.spotify"`).
    ///
    /// # Returns
    ///
    /// A `HashMap<String, OwnedValue>` with all property names mapped to their
    /// D-Bus variant values, or `None` if the call fails.
    fn get_all_props(conn: &Connection, dest: &str) -> Option<HashMap<String, OwnedValue>> {
        conn.call_method(
            Some(dest),
            MPRIS_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &(MPRIS_PLAYER_IFACE,),
        )
        .ok()
        .and_then(|msg| msg.body().deserialize::<HashMap<String, OwnedValue>>().ok())
    }

    /// Extract a `String` from the property map by key.
    ///
    /// Returns an empty string if the key is absent or the value is not a
    /// D-Bus string.
    fn prop_string(props: &HashMap<String, OwnedValue>, key: &str) -> String {
        props
            .get(key)
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Extract a `bool` from the property map by key.
    ///
    /// Returns `false` if the key is absent or the value is not a D-Bus
    /// boolean.
    fn prop_bool(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
        props.get(key).and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false)
    }

    /// Extract `(title, artist)` from the `Metadata` property.
    ///
    /// `xesam:artist` is an `as` (array of strings); multiple artists are
    /// joined with `", "`. Returns empty strings for absent or unparseable
    /// metadata.
    fn extract_metadata(props: &HashMap<String, OwnedValue>) -> (String, String) {
        // Metadata is a{sv} — convert the OwnedValue wrapper to a nested map.
        let metadata: Option<HashMap<String, OwnedValue>> = props
            .get("Metadata")
            .and_then(|v| HashMap::<String, OwnedValue>::try_from(v.clone()).ok());

        let title = metadata
            .as_ref()
            .and_then(|m| m.get("xesam:title"))
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();

        let artist = metadata
            .as_ref()
            .and_then(|m| m.get("xesam:artist"))
            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
            .map(|parts| parts.join(", "))
            .unwrap_or_default();

        (title, artist)
    }
}

impl Widget for MediaWidget {
    /// Return the widget's name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Query the active MPRIS2 player and return current media state as
    /// [`WidgetData::Media`].
    ///
    /// Lazily establishes the D-Bus connection on the first call. If D-Bus is
    /// unavailable (e.g. in a headless CI environment) or no player is active,
    /// returns `WidgetData::Media` with `status: Stopped` — never returns `Err`
    /// for player absence.
    ///
    /// # Errors
    ///
    /// Returns [`FramesError::DBus`] only when a D-Bus error occurs *and* no
    /// cached data is available. Transient errors return the last cached value.
    fn update(&mut self) -> Result<WidgetData, FramesError> {
        // Lazily connect on first call.
        if self.conn.is_none() {
            match Connection::session() {
                Ok(conn) => {
                    self.conn = Some(conn);
                }
                Err(e) => {
                    tracing::warn!(
                        widget = self.name,
                        error = %e,
                        "D-Bus session unavailable; media widget returning Stopped"
                    );
                    return Ok(Self::stopped());
                }
            }
        }

        let conn = self.conn.as_ref().expect("conn established above");

        // Find the first active MPRIS player on the bus.
        let Some(player_name) = Self::find_player(conn) else {
            // No player active — normal condition.
            let data = Self::stopped();
            self.last = Some(data.clone());
            return Ok(data);
        };

        // Fetch all player properties in one D-Bus round-trip.
        let Some(props) = Self::get_all_props(conn, &player_name) else {
            tracing::warn!(
                widget = self.name,
                player = player_name,
                "GetAll failed; returning cached media data"
            );
            return Ok(self.last.clone().unwrap_or_else(Self::stopped));
        };

        let status_str = Self::prop_string(&props, "PlaybackStatus");
        let status = Self::parse_playback_status(&status_str);
        let can_go_next = Self::prop_bool(&props, "CanGoNext");
        let can_go_previous = Self::prop_bool(&props, "CanGoPrevious");
        let (title, artist) = Self::extract_metadata(&props);

        let data = WidgetData::Media {
            title,
            artist,
            status,
            can_go_next,
            can_go_previous,
        };
        self.last = Some(data.clone());
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_widget_name_returns_name() {
        let w = MediaWidget::new("my-media");
        assert_eq!(w.name(), "my-media");
    }

    #[test]
    fn media_widget_name_non_empty() {
        let w = MediaWidget::new("media");
        assert!(!w.name().is_empty());
    }

    #[test]
    fn playback_status_from_str_playing() {
        assert_eq!(MediaWidget::parse_playback_status("Playing"), PlaybackStatus::Playing);
    }

    #[test]
    fn playback_status_from_str_paused() {
        assert_eq!(MediaWidget::parse_playback_status("Paused"), PlaybackStatus::Paused);
    }

    #[test]
    fn playback_status_from_str_stopped() {
        assert_eq!(MediaWidget::parse_playback_status("Stopped"), PlaybackStatus::Stopped);
    }

    #[test]
    fn playback_status_from_str_unknown_maps_to_stopped() {
        assert_eq!(MediaWidget::parse_playback_status("Buffering"), PlaybackStatus::Stopped);
        assert_eq!(MediaWidget::parse_playback_status(""), PlaybackStatus::Stopped);
    }

    #[test]
    fn media_widget_stopped_helper_returns_correct_variant() {
        let data = MediaWidget::stopped();
        match data {
            WidgetData::Media {
                status,
                title,
                artist,
                ..
            } => {
                assert_eq!(status, PlaybackStatus::Stopped);
                assert!(title.is_empty());
                assert!(artist.is_empty());
            }
            _ => panic!("stopped() must return WidgetData::Media"),
        }
    }

    #[test]
    fn prop_string_returns_empty_for_missing_key() {
        let props: HashMap<String, OwnedValue> = HashMap::new();
        assert_eq!(MediaWidget::prop_string(&props, "PlaybackStatus"), "");
    }

    #[test]
    fn prop_bool_returns_false_for_missing_key() {
        let props: HashMap<String, OwnedValue> = HashMap::new();
        assert!(!MediaWidget::prop_bool(&props, "CanGoNext"));
    }
}
