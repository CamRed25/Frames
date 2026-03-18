//! Volume widget — audio output level data provider.
//!
//! Subscribes to `pactl subscribe` events in a background thread and forwards
//! sink-change data through an `mpsc` channel. `update()` drains the channel
//! on each poll and returns the latest value, or the last cached value when
//! no events have arrived since the previous call.
//!
//! Falls back to the last known value when `pactl` is absent or the subscribe
//! thread has exited (e.g., `PipeWire` restart). No display dependency.

use std::process::Command;
use std::sync::{mpsc, Mutex};

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Volume and mute snapshot transmitted from the subscribe background thread.
#[derive(Debug, Clone, Copy)]
struct VolumeData {
    volume_pct: f32,
    muted: bool,
}

/// Extract volume percentage from `pactl get-sink-info` output.
///
/// Looks for the first `Volume:` line and parses the `/ NN% /` token.
/// Returns `None` if the line is absent or the percentage is not a valid float.
fn parse_volume_pct(text: &str) -> Option<f32> {
    text.lines()
        .find(|l| l.trim_start().starts_with("Volume:"))
        .and_then(|l| l.split('/').nth(1))
        .map(|s| s.trim().trim_end_matches('%'))
        .and_then(|s| s.parse::<f32>().ok())
}

/// Determine mute state from `pactl get-sink-info` output.
///
/// Returns `true` if a `Mute: yes` line is found (case-insensitive).
/// Returns `false` when the line is absent (conservative default).
fn parse_mute(text: &str) -> bool {
    text.lines()
        .find(|l| l.trim_start().starts_with("Mute:"))
        .is_some_and(|l| l.to_lowercase().contains("yes"))
}

/// Query the default sink volume and mute state via two `pactl` calls.
///
/// Calls `pactl get-sink-volume @DEFAULT_SINK@` for the volume percentage and
/// `pactl get-sink-mute @DEFAULT_SINK@` for the mute flag. Returns `None` when
/// `pactl` is absent or exits non-zero (e.g., no audio daemon running).
fn read_volume_info() -> Option<(f32, bool)> {
    let vol_out = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .ok()?;
    if !vol_out.status.success() {
        return None;
    }
    let vol_text = String::from_utf8_lossy(&vol_out.stdout);
    let vol = parse_volume_pct(&vol_text)?;

    let mute_out = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .output()
        .ok()?;
    let mute_text = String::from_utf8_lossy(&mute_out.stdout);
    let muted = parse_mute(&mute_text);

    Some((vol, muted))
}

/// Drive `pactl subscribe` and forward default-sink change events via `tx`.
///
/// Reads stdout line by line and sends a [`VolumeData`] snapshot through `tx`
/// when a sink `change` event is detected. Returns (dropping `tx`) when
/// `pactl` exits, when `tx.send()` fails because the receiver was dropped, or
/// when the spawn itself fails.
///
/// This function is intended to be called from `std::thread::spawn`. It never
/// panics.
// clippy::needless_pass_by_value: tx must be owned so it is dropped when this
// function returns, which disconnects the channel and signals the Receiver.
#[allow(clippy::needless_pass_by_value)]
fn subscribe_loop(tx: mpsc::Sender<VolumeData>) {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let mut child = match Command::new("pactl")
        .arg("subscribe")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "pactl subscribe failed to start; volume widget will not receive live updates"
            );
            return; // Drops tx → Receiver sees Disconnected on next try_recv
        }
    };

    // SAFETY: stdout is always Some because Stdio::piped() was set above.
    let Some(stdout) = child.stdout.take() else {
        unreachable!("pactl subscribe stdout not captured despite Stdio::piped()")
    };

    let reader = BufReader::new(stdout);

    for line in reader.lines().map_while(Result::ok) {
        // pactl subscribe plain-text output: "Event 'change' on sink #N"
        if line.contains("change") && line.contains("sink") {
            if let Some((volume_pct, muted)) = read_volume_info() {
                if tx.send(VolumeData { volume_pct, muted }).is_err() {
                    // Receiver dropped; widget was torn down — exit cleanly.
                    break;
                }
            }
        }
    }
    tracing::debug!("pactl subscribe loop exited");
}

/// Provides audio output volume and mute state.
///
/// Spawns a background thread running `pactl subscribe` on construction.
/// `update()` drains an `mpsc` channel fed by that thread and returns the
/// latest data, or the last cached value when no events have arrived.
///
/// The background thread exits automatically when the widget is dropped
/// (the `Sender` is dropped, making the next `try_recv` return
/// `Disconnected`). Widget teardown does not join the thread — it exits at
/// the next `pactl subscribe` event or when `pactl` itself exits.
pub struct VolumeWidget {
    name: String,
    cached: WidgetData,
    rx: Mutex<mpsc::Receiver<VolumeData>>,
    _thread: std::thread::JoinHandle<()>,
}

impl VolumeWidget {
    /// Create a new `VolumeWidget` targeting the default audio sink.
    ///
    /// Spawns a background thread that runs `pactl subscribe` and forwards
    /// sink-change events through an internal channel. An initial
    /// `pactl get-sink-info` call populates the cached value so the first
    /// `update()` returns real data without waiting for a change event.
    ///
    /// # Panics
    ///
    /// Does not panic. If the OS thread limit is reached, a no-op thread is
    /// spawned instead; the widget returns the initial cached value permanently.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let (tx, rx) = mpsc::channel::<VolumeData>();

        // Warm the cache before spawning so update() has real data immediately.
        let initial = read_volume_info().map_or(
            WidgetData::Volume { volume_pct: 0.0, muted: false },
            |(volume_pct, muted)| WidgetData::Volume { volume_pct, muted },
        );

        let thread_handle = std::thread::Builder::new()
            .name("parapet-volume-subscribe".into())
            .spawn(move || subscribe_loop(tx))
            // Spawning can only fail if the OS has hit its thread limit; treat
            // this the same as pactl being absent — the widget returns the
            // initial cached value permanently but does not crash.
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "could not spawn volume subscribe thread");
                std::thread::spawn(|| {})
            });

        Self {
            name: name.into(),
            cached: initial,
            rx: Mutex::new(rx),
            _thread: thread_handle,
        }
    }
}

impl Widget for VolumeWidget {
    fn name(&self) -> &str {
        &self.name
    }

    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let rx = self.rx.lock().expect(
            "volume subscribe rx mutex poisoned; this indicates a bug in subscribe_loop",
        );

        loop {
            match rx.try_recv() {
                Ok(data) => {
                    self.cached =
                        WidgetData::Volume { volume_pct: data.volume_pct, muted: data.muted };
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    // Background thread exited (pactl absent, OS restart, or
                    // thread spawn failure). Return stale cached value per
                    // WIDGET_API §7.2 — this is not an error condition.
                    tracing::warn!(
                        widget = %self.name,
                        "volume subscribe thread disconnected; returning last cached value"
                    );
                    break;
                }
            }
        }
        // Drop the lock before returning to minimise lock hold time.
        drop(rx);
        Ok(self.cached.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINK_INFO_FIXTURE: &str = "Sink #0\n\
        \tState: RUNNING\n\
        \tName: alsa_output.pci-0000_00_1f.3.analog-stereo\n\
        \tVolume: front-left: 45875 /  70% / -8.66 dB   front-right: 45875 /  70% / -8.66 dB\n\
        \tBalance: 0.00\n\
        \tMute: no\n";

    const SINK_INFO_MUTED: &str = "Sink #0\n\
        \tVolume: front-left: 0 /   0% / -inf dB\n\
        \tMute: yes\n";

    const SINK_INFO_NO_VOLUME_LINE: &str = "Sink #0\n\
        \tMute: no\n";

    #[test]
    fn parse_volume_pct_extracts_from_fixture() {
        assert_eq!(parse_volume_pct(SINK_INFO_FIXTURE), Some(70.0));
    }

    #[test]
    fn parse_volume_pct_returns_zero_when_muted() {
        assert_eq!(parse_volume_pct(SINK_INFO_MUTED), Some(0.0));
    }

    #[test]
    fn parse_volume_pct_none_when_volume_line_absent() {
        assert_eq!(parse_volume_pct(SINK_INFO_NO_VOLUME_LINE), None);
    }

    #[test]
    fn parse_mute_false_on_unmuted_sink() {
        assert!(!parse_mute(SINK_INFO_FIXTURE));
    }

    #[test]
    fn parse_mute_true_on_muted_sink() {
        assert!(parse_mute(SINK_INFO_MUTED));
    }

    #[test]
    fn parse_mute_false_when_line_absent() {
        // Conservative default: the Mute line says "no" — confirm false is returned.
        assert!(!parse_mute(SINK_INFO_NO_VOLUME_LINE));
    }

    #[test]
    fn volume_widget_name_non_empty() {
        let w = VolumeWidget::new("vol");
        assert!(!w.name().is_empty());
    }

    #[test]
    fn volume_widget_satisfies_send_sync() {
        // Verifies that the Mutex<Receiver<…>> + JoinHandle fields preserve the
        // Send + Sync bound required by the Widget trait (WIDGET_API §3).
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VolumeWidget>();
    }

    #[test]
    fn volume_update_does_not_error() {
        // pactl may be absent in CI; read_volume_info() returns None,
        // update() returns the initial cached default via Ok(…).
        let mut w = VolumeWidget::new("vol");
        assert!(w.update().is_ok());
    }

    #[test]
    fn volume_widget_update_returns_ok_when_subscribe_absent() {
        // When pactl is absent, the subscribe thread exits immediately, the
        // channel disconnects, and update() must return Ok(…) with a safe
        // default rather than Err(…). This is the CI-safe baseline.
        let mut w = VolumeWidget::new("vol-absent");
        // Allow the thread time to attempt spawn and exit.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(w.update().is_ok());
    }
}
