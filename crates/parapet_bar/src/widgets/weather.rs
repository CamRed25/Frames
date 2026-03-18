//! Weather widget renderer — displays current temperature, wind speed, and a
//! WMO weather-code icon in a GTK label.

use gtk::prelude::*;

use parapet_core::widget::TempUnit;
use parapet_core::{WeatherConfig, WidgetData};

/// GTK3 renderer for the weather widget.
pub struct WeatherWidget {
    label: gtk::Label,
}

impl WeatherWidget {
    /// Create a new weather renderer from the given weather config.
    ///
    /// CSS classes `.widget` and `.widget-weather` are applied to the label.
    ///
    /// # Errors
    ///
    /// Currently infallible. Returns `anyhow::Result` for a consistent renderer
    /// contract with constructors that can fail.
    // clippy::unnecessary_wraps: consistent renderer contract — other constructors are fallible
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(_config: &WeatherConfig) -> anyhow::Result<Self> {
        let label = gtk::Label::new(None);
        label.set_widget_name("weather");
        label.style_context().add_class("widget");
        label.style_context().add_class("widget-weather");
        Ok(Self { label })
    }

    /// Return a reference to the root GTK widget.
    pub fn widget(&self) -> &gtk::Widget {
        self.label.upcast_ref()
    }

    /// Apply new weather data: update label text with icon, temperature, and wind.
    pub fn update(&self, data: &WidgetData) {
        if let WidgetData::Weather {
            temperature,
            weather_code,
            wind_speed,
            unit,
            ..
        } = data
        {
            let icon = wmo_icon(*weather_code);
            let unit_sym = match unit {
                TempUnit::Celsius => "C",
                TempUnit::Fahrenheit => "F",
            };
            self.label
                .set_text(&format!("{icon} {temperature:.0}°{unit_sym} 💨{wind_speed:.0}"));
        }
    }
}

/// Map a WMO weather interpretation code to a display icon.
///
/// WMO code ranges follow the Open-Meteo documentation. Unknown codes return
/// a generic thermometer icon.
fn wmo_icon(code: u16) -> &'static str {
    match code {
        0 => "☀",
        1..=3 => "⛅",
        45 | 48 => "🌫",
        51..=57 => "🌦",
        61..=67 | 80..=82 => "🌧",
        71..=77 => "❄",
        95 | 96 | 99 => "⛈",
        _ => "🌡",
    }
}
