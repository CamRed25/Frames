//! Interval-based widget update scheduler.
//!
//! [`Poller`] drives all widget data refreshes. It is a pure-Rust scheduler —
//! no GTK, no glib timers, no threads. `parapet_bar` calls [`Poller::poll`]
//! from a `glib::timeout_add_local` callback on a single-millisecond tick,
//! passing the current [`std::time::Instant`].
//!
//! Widgets whose interval has elapsed since their last successful update are
//! refreshed. Errors from [`crate::widget::Widget::update`] are logged via
//! `tracing::warn!` and skipped — they never propagate to the caller.

use std::time::{Duration, Instant};

use crate::widget::{Widget, WidgetData};

/// Internal bookkeeping for a single registered widget.
struct RegisteredWidget {
    widget: Box<dyn Widget>,
    interval_ms: u64,
    last_polled: Option<Instant>,
}

/// Interval-based widget update scheduler.
///
/// Register widgets with [`Poller::register`], then call [`Poller::poll`] on
/// each timer tick. Results are returned as `(widget_name, data)` pairs for
/// every widget whose interval has elapsed.
#[derive(Default)]
pub struct Poller {
    widgets: Vec<RegisteredWidget>,
}

impl Poller {
    /// Create an empty `Poller` with no registered widgets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a widget with a polling interval in milliseconds.
    ///
    /// The widget will be polled on the first call to [`Poller::poll`]
    /// regardless of the elapsed time, and thereafter every `interval_ms`
    /// milliseconds.
    ///
    /// # Parameters
    ///
    /// * `widget` — boxed [`Widget`] implementation to drive
    /// * `interval_ms` — minimum milliseconds between successive updates
    pub fn register(&mut self, widget: Box<dyn Widget>, interval_ms: u64) {
        self.widgets.push(RegisteredWidget {
            widget,
            interval_ms,
            last_polled: None,
        });
    }

    /// Poll all widgets whose interval has elapsed since their last update.
    ///
    /// Returns `(widget_name, data)` for each widget that produced new data.
    /// Widgets that return an error are logged via [`tracing::warn!`] and
    /// excluded from the result — errors never propagate to the caller.
    ///
    /// # Parameters
    ///
    /// * `now` — the current [`Instant`]; pass `Instant::now()` from the
    ///   caller so tests can supply deterministic fake timestamps.
    pub fn poll(&mut self, now: Instant) -> Vec<(String, WidgetData)> {
        let mut results = Vec::new();
        for reg in &mut self.widgets {
            let should_poll = reg.last_polled.map_or(true, |last| {
                now.duration_since(last) >= Duration::from_millis(reg.interval_ms)
            });

            if !should_poll {
                continue;
            }

            match reg.widget.update() {
                Ok(data) => {
                    reg.last_polled = Some(now);
                    results.push((reg.widget.name().to_string(), data));
                }
                Err(e) => {
                    tracing::warn!(
                        widget = reg.widget.name(),
                        error = %e,
                        "widget update failed; skipping"
                    );
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ParapetError;
    use crate::widget::{Widget, WidgetData};

    struct MockWidget {
        name: String,
        data: WidgetData,
    }

    impl MockWidget {
        fn new(name: &str, data: WidgetData) -> Self {
            Self {
                name: name.to_string(),
                data,
            }
        }
    }

    impl Widget for MockWidget {
        fn name(&self) -> &str {
            &self.name
        }

        fn update(&mut self) -> Result<WidgetData, ParapetError> {
            Ok(self.data.clone())
        }
    }

    struct ErrorWidget {
        name: String,
    }

    impl Widget for ErrorWidget {
        fn name(&self) -> &str {
            &self.name
        }

        fn update(&mut self) -> Result<WidgetData, ParapetError> {
            Err(ParapetError::SysInfo("simulated failure".into()))
        }
    }

    #[test]
    fn poller_new_is_empty() {
        let mut poller = Poller::new();
        assert!(poller.poll(Instant::now()).is_empty());
    }

    #[test]
    fn poller_polls_on_first_call() {
        let mut poller = Poller::new();
        poller.register(
            Box::new(MockWidget::new(
                "clock",
                WidgetData::Clock {
                    display: "12:00".into(),
                },
            )),
            1000,
        );
        let results = poller.poll(Instant::now());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "clock");
    }

    #[test]
    fn poller_respects_interval() {
        let mut poller = Poller::new();
        poller.register(
            Box::new(MockWidget::new(
                "clock",
                WidgetData::Clock {
                    display: "12:00".into(),
                },
            )),
            500,
        );
        let t0 = Instant::now();
        let _ = poller.poll(t0);
        assert!(
            poller.poll(t0 + Duration::from_millis(100)).is_empty(),
            "too soon — should not fire"
        );
        assert_eq!(
            poller.poll(t0 + Duration::from_millis(600)).len(),
            1,
            "after interval — should fire"
        );
    }

    #[test]
    fn poller_skips_erroring_widget_without_panic() {
        let mut poller = Poller::new();
        poller.register(Box::new(ErrorWidget { name: "bad".into() }), 100);
        assert!(poller.poll(Instant::now()).is_empty());
    }

    #[test]
    fn poller_returns_widget_name_with_data() {
        let mut poller = Poller::new();
        poller.register(
            Box::new(MockWidget::new(
                "cpu",
                WidgetData::Cpu {
                    usage_pct: 42.0,
                    per_core: vec![],
                    temp_celsius: None,
                },
            )),
            100,
        );
        let results = poller.poll(Instant::now());
        assert_eq!(results[0].0, "cpu");
        assert!(matches!(&results[0].1, WidgetData::Cpu { usage_pct, .. } if *usage_pct == 42.0));
    }
}
