// Widget contract integration tests.
//
// Verifies the core Widget trait contract against: a minimal mock widget,
// and the concrete ClockWidget implementation that must always be present.

use parapet_core::widgets::clock::ClockWidget;
use parapet_core::{ParapetError, Widget, WidgetData};

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

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn minimal_widget_name_non_empty() {
    let w = MockWidget::new(
        "test",
        WidgetData::Clock {
            display: "12:00".into(),
        },
    );
    assert!(!w.name().is_empty(), "widget name must not be empty");
}

#[test]
fn minimal_widget_update_returns_ok() {
    let mut w = MockWidget::new(
        "test",
        WidgetData::Clock {
            display: "12:00".into(),
        },
    );
    assert!(w.update().is_ok(), "mock widget update must succeed");
}

#[test]
fn clock_widget_satisfies_contract() {
    let mut w = ClockWidget::new("clock", "%H:%M");
    assert!(!w.name().is_empty(), "clock name must not be empty");
    let data = w.update().expect("clock update must succeed");
    match data {
        WidgetData::Clock { display } => {
            assert!(!display.is_empty(), "clock display must not be empty");
        }
        _ => panic!("expected WidgetData::Clock"),
    }
}
