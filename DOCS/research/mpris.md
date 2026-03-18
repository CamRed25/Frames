# Research: MPRIS Media Widget
**Date:** 2026-03-17
**Status:** Decision recommended — ready to close

## Question

What D-Bus client crate should be used to implement an MPRIS2 media widget in `parapet_core`, and how should the blocking/async mismatch between D-Bus I/O and the synchronous `Widget::update()` contract be resolved?

## Summary

Use `zbus ~5.14` with the default `blocking-api` feature flag. The `zbus::blocking` module provides a synchronous `Connection` and typed proxy API that integrates directly with the `Widget::update()` → `WidgetData` contract. Avoid the `mpris` crate (last release March 2022, 126K total downloads, wraps the C `dbus` library). `zbus` is pure Rust, MIT-licensed, actively maintained, and has 43M downloads.

---

## Findings

### Option A: `mpris v2.0.1` (not recommended)

- **License:** Apache-2.0 (compatible with GPL-3.0-or-later)
- **Downloads:** 126K total (very low — fewer than many niche crates)
- **Last release:** March 2022 (~3 years ago, only 10 versions ever published)
- **Maintenance signal:** Repository activity is minimal; no releases since 2022
- **C FFI dependency:** Wraps the `dbus` crate (`^0.9.6`), which itself wraps `libdbus-1` via C FFI
- **System library required:** `libdbus-1-dev` (`dbus-devel` on Fedora) — would need to be added to `BUILD_GUIDE §1.2`
- **API surface:**
  ```rust
  let finder = PlayerFinder::new()?;  // creates a dbus connection
  let player = finder.find_active()?; // returns first active MPRIS player
  let meta = player.get_metadata()?;  // title, artist, album, etc.
  let status = player.get_playback_status()?;
  player.play_pause()?;
  player.next()?;
  ```
- **Concern:** 3 years without a release is a significant maintenance risk for a crate handling a D-Bus IPC layer. If `libdbus` or the MPRIS2 spec introduces a breaking change, the crate is unlikely to be updated. The `dbus` crate it wraps itself carries C FFI `unsafe` blocks that bypass Rust's memory safety guarantees.

### Option B: `zbus ~5.14` with `blocking-api` (recommended)

- **License:** MIT
- **Downloads:** 43.2 M (340× more than `mpris`)
- **Last release:** 23 days ago — actively maintained
- **Pure Rust:** No C FFI, no system library required; no `BUILD_GUIDE §1.2` change needed
- **Stable status:** Self-described as "Stable"
- **MSRV:** Compatible with workspace minimum (1.75)
- **Blocking API:** Provided by default via the `blocking-api` feature (enabled since v1.0, can be disabled since v5.0); accessible at `zbus::blocking::{Connection, Proxy, ...}`
- **Threading:** `zbus::blocking::Connection` internally spawns **one background thread** per connection to drive the async executor. This is acceptable for a long-lived bar widget (one thread for the lifetime of the process).
- **`#[proxy]` macro:** Generates a typed proxy struct from an interface trait declaration:
  ```rust
  use zbus::proxy;

  #[proxy(
      interface = "org.mpris.MediaPlayer2.Player",
      default_path = "/org/mpris/MediaPlayer2",
  )]
  trait MediaPlayer2Player {
      fn play_pause(&self) -> zbus::Result<()>;
      fn next(&self) -> zbus::Result<()>;
      fn previous(&self) -> zbus::Result<()>;
      #[zbus(property)]
      fn playback_status(&self) -> zbus::Result<String>;
      #[zbus(property)]
      fn metadata(&self) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;
      #[zbus(property)]
      fn can_go_next(&self) -> zbus::Result<bool>;
      #[zbus(property)]
      fn can_go_previous(&self) -> zbus::Result<bool>;
  }
  ```
  The blocking version `MediaPlayer2PlayerProxyBlocking` is generated automatically alongside the async variant.
- **RUSTSEC:** No advisories for `zbus` found
- **Display isolation:** D-Bus is a session IPC mechanism — no display server dependency. `zbus` is safe to use in `parapet_core` (no GTK, GDK, or X11 imports required).

```toml
# parapet_core/Cargo.toml
[dependencies]
zbus = { version = "~5.14", features = ["blocking-api"] }
```

> Note: `blocking-api` is on by default in `~5.14`; the explicit feature declaration documents intent and prevents accidental removal.

**Finding an active MPRIS player:**

MPRIS players register well-known names on the session bus matching `org.mpris.MediaPlayer2.*`. To find the first active player:

```rust
let conn = zbus::blocking::Connection::session()?;
let names: Vec<String> = conn.call_method(
    Some("org.freedesktop.DBus"),
    "/org/freedesktop/DBus",
    Some("org.freedesktop.DBus"),
    "ListNames",
    &(),
)?.body().deserialize()?;

let player_name = names.iter()
    .find(|n| n.starts_with("org.mpris.MediaPlayer2."));
```

If no player is active, `update()` should return `WidgetData::Media` with `status: PlaybackStatus::Stopped` and empty strings rather than an error (player absence is normal, not exceptional).

---

## Recommendation

Use **`zbus ~5.14` with `blocking-api`**.

**Implementation model for `parapet_core::widgets::media`:**

```rust
pub struct MediaWidget {
    conn: Option<zbus::blocking::Connection>,
    last: MediaData,
}

impl Widget for MediaWidget {
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let conn = match self.conn.get_or_insert_with(|| {
            zbus::blocking::Connection::session().ok()
        }) {
            Some(c) => c,
            None => return Ok(WidgetData::Media(self.last.clone())),
        };
        // find first MPRIS player and read metadata...
        // store result in self.last for fallback on next error
        Ok(WidgetData::Media(self.last.clone()))
    }
}
```

Lazy connection initialisation (`conn: Option<Connection>`) avoids a startup failure if D-Bus is briefly unavailable and recovers gracefully if the session bus restarts.

**`WidgetData` variant to add** (minor bump → WIDGET_API 1.3.0 or 1.4.0, coordinate with weather widget):

```rust
/// Currently playing media from an MPRIS2-compatible player.
Media {
    /// Track title, empty string if unknown or no player active.
    title: String,
    /// Artist name, empty string if unknown.
    artist: String,
    /// Current playback state.
    status: PlaybackStatus,
    /// Whether the active player supports skipping to the next track.
    can_next: bool,
    /// Whether the active player supports going back to the previous track.
    can_prev: bool,
},
```

Add `PlaybackStatus` as a `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PlaybackStatus { Playing, Paused, Stopped }` in `parapet_core::widgets::media`.

**`WidgetConfig` fields to add:**

```toml
[[widgets]]
type = "media"
position = "center"
interval = 2000          # 2 seconds — responsive but not spammy
max_title_len = 30       # optional: truncate long titles
```

**Widget name for CSS:** `.widget-media`

**GTK renderer display** (suggestion for `parapet_bar`):
- Playing: `⏸ Artist — Title` (truncated to `max_title_len`)
- Paused: `▶ Artist — Title`
- Stopped / no player: widget hidden or shows `—`
- Click actions (`on_click`) can map to `play_pause`, `next`, `previous` via existing shell command mechanism; alternatively, a dedicated click handler in `parapet_bar::widgets::media` could call the D-Bus method directly without spawning a shell

**Architecture exception to note:** The `parapet_bar` media widget renderer will need a second reference to a `zbus::blocking::Connection` if it wants to issue `PlayPause`/`Next`/`Previous` commands on click. The connection stored in `parapet_core::MediaWidget` is not accessible from `parapet_bar`. Two options:
- **A (simpler):** Click actions use `playerctl play-pause` / `playerctl next` shell commands via the existing `on_click` / `on_scroll_up` mechanism — no `parapet_bar` D-Bus access needed
- **B (integrated):** `parapet_bar::widgets::media` opens its own `zbus::blocking::Connection` for sending commands; data still comes from `parapet_core` via `WidgetData::Media`

Option A (shell commands) is recommended for v0.1 — it requires zero new code in `parapet_bar` and leverages the already-built click-action system.

**Standards reference:** ARCHITECTURE §3 (no display deps in `parapet_core` — D-Bus passes this check); BUILD_GUIDE §1.2 (no new system libraries with `zbus`); WIDGET_API §5 (5-step checklist applies).

---

## Standards Conflict / Proposed Update

`BUILD_GUIDE §1.2` table does not need a new row (zbus is pure Rust). If `mpris` had been chosen instead, `dbus-devel` / `libdbus-1-dev` would have been required. The `zbus` choice was made **specifically** to avoid adding a C library dependency.

---

## Sources

- [crates.io/crates/zbus](https://crates.io/crates/zbus): v5.14.0 — MIT, 43.2M downloads, blocking-api feature, pure Rust
- [crates.io/crates/mpris/2.0.1/dependencies](https://crates.io/crates/mpris/2.0.1/dependencies): confirmed `dbus ^0.9.6` C FFI dependency
- [rustsec.org/advisories](https://rustsec.org/advisories): checked — no advisories for `zbus`, `mpris`, or `dbus`
- MPRIS2 specification: `org.mpris.MediaPlayer2.Player` interface, `/org/mpris/MediaPlayer2` path

---

## Open Questions

1. **Player selection strategy** — `find_active()` equivalent picks the first MPRIS bus name alphabetically. Should Parapet prefer the most recently foregrounded player? This would require listening to `NameOwnerChanged` signals on the session bus — more complexity. For v0.1, first-found ordering is acceptable.
2. **`playerctl` availability** — Option A (shell commands) for click actions depends on `playerctl` being installed. Should `parapet_bar` fall back silently if it is absent, or show a console warning? The existing `spawn_shell()` helper already ignores the child's exit code, so absence of `playerctl` is already handled gracefully.
3. **Cover art** — MPRIS `Metadata` includes `mpris:artUrl` (a `file://` or `http://` URI). Rendering cover art in a status bar is non-trivial and scope-appropriate for a future enhancement, not v0.1.
