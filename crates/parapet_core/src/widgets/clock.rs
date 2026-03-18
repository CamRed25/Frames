//! Clock widget — date and time data provider.
//!
//! Returns the current local time formatted with a `chrono` format string.
//! Has no sysinfo or sysfs dependencies — this is the simplest widget and
//! the best first implementation to prove the `Widget → WidgetData` pipeline.

use chrono::Local;

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Provides the current date and time as a formatted string.
///
/// The format string follows `chrono::format` / strftime conventions
/// (e.g. `"%H:%M:%S"`, `"%a %b %d  %H:%M"`).
pub struct ClockWidget {
    name: String,
    format: String,
}

impl ClockWidget {
    /// Create a new `ClockWidget`.
    ///
    /// - `name`: stable identifier used in logs and CSS widget names.
    /// - `format`: `chrono::format` pattern string (e.g. `"%H:%M"`).
    #[must_use]
    pub fn new(name: impl Into<String>, format: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            format: format.into(),
        }
    }
}

impl Widget for ClockWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Return the current local time formatted with this widget's format string.
    ///
    /// Always succeeds — `chrono::Local::now()` does not fail on supported
    /// platforms.
    ///
    /// # Errors
    ///
    /// This implementation never returns `Err`. The signature requires it for
    /// trait compatibility.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let display = Local::now().format(&self.format).to_string();
        Ok(WidgetData::Clock { display })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_update_returns_non_empty_display() {
        let mut w = ClockWidget::new("clock", "%H:%M");
        let data = w.update().expect("clock update should not fail");
        if let WidgetData::Clock { display } = data {
            assert!(!display.is_empty(), "display string must not be empty");
        } else {
            panic!("expected WidgetData::Clock");
        }
    }

    #[test]
    fn clock_format_is_applied() {
        let mut w = ClockWidget::new("clock", "%Y");
        let data = w.update().expect("clock update should not fail");
        if let WidgetData::Clock { display } = data {
            // A 4-digit year is exactly 4 characters and parses as a number
            assert_eq!(display.len(), 4, "year format must produce 4 chars; got: {display}");
            assert!(display.parse::<u32>().is_ok(), "year must be numeric; got: {display}");
        } else {
            panic!("expected WidgetData::Clock");
        }
    }

    #[test]
    fn clock_name_is_stable() {
        let w = ClockWidget::new("my-clock", "%H:%M");
        assert_eq!(w.name(), "my-clock");
        assert_eq!(w.name(), "my-clock"); // same value on repeated calls
    }

    #[test]
    fn clock_widget_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClockWidget>();
    }
}
