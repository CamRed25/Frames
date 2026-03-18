// Integration tests for Poller interval semantics.
//
// These tests drive the Poller with fake Instants to verify that widgets only
// fire when their interval has elapsed, that multiple widgets are polled
// independently, and that erroring widgets are skipped without panic.

use std::time::{Duration, Instant};

use parapet_core::{ParapetError, Poller, Widget, WidgetData};

// ── MockWidget ──────────────────────────────────────────────────────────────

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

// ── ErrorWidget ─────────────────────────────────────────────────────────────

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

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn poller_polls_all_widgets_on_first_call() {
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
    poller.register(
        Box::new(MockWidget::new(
            "cpu",
            WidgetData::Cpu {
                usage_pct: 0.0,
                per_core: vec![],
                temp_celsius: None,
            },
        )),
        2000,
    );
    let results = poller.poll(Instant::now());
    assert_eq!(results.len(), 2, "first poll should update all registered widgets");
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
    let _ = poller.poll(t0); // initialise last_polled

    // Poll too soon — no results
    let results = poller.poll(t0 + Duration::from_millis(100));
    assert!(results.is_empty(), "should not poll within interval (100ms < 500ms)");

    // Poll after interval has elapsed — one result
    let results = poller.poll(t0 + Duration::from_millis(600));
    assert_eq!(results.len(), 1, "should poll after interval has elapsed (600ms > 500ms)");
    assert_eq!(results[0].0, "clock");
}

#[test]
fn poller_skips_erroring_widget() {
    let mut poller = Poller::new();
    poller.register(Box::new(ErrorWidget { name: "bad".into() }), 100);
    let results = poller.poll(Instant::now());
    assert!(results.is_empty(), "error widget must be skipped, not panic");
}

#[test]
fn poller_independent_intervals_for_each_widget() {
    let mut poller = Poller::new();
    // clock fires every 500 ms, cpu every 1000 ms
    poller.register(
        Box::new(MockWidget::new(
            "clock",
            WidgetData::Clock {
                display: "12:00".into(),
            },
        )),
        500,
    );
    poller.register(
        Box::new(MockWidget::new(
            "cpu",
            WidgetData::Cpu {
                usage_pct: 0.0,
                per_core: vec![],
                temp_celsius: None,
            },
        )),
        1000,
    );
    let t0 = Instant::now();
    let _ = poller.poll(t0); // initialise both

    // After 600 ms only clock should fire
    let results = poller.poll(t0 + Duration::from_millis(600));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "clock", "only clock should fire at 600ms");

    // After 1100 ms cpu should also fire
    let results = poller.poll(t0 + Duration::from_millis(1100));
    assert_eq!(results.len(), 2, "both widgets should fire at 1100ms");
}
