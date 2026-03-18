//! Weather data provider widget.
//!
//! Fetches current weather conditions from the [Open-Meteo](https://open-meteo.com)
//! free API (no API key required). Results are cached between fetches so that
//! transient network failures return stale data rather than clearing the display.
//!
//! See `DOCS/research/weather.md` for crate selection rationale and API details.

use std::time::Duration;

use serde::Deserialize;

use crate::error::ParapetError;
use crate::widget::{TempUnit, Widget, WidgetData};

// ── API response shapes ───────────────────────────────────────────────────────

/// Top-level Open-Meteo `/v1/forecast` response.
#[derive(Deserialize)]
struct ApiResponse {
    current: CurrentWeather,
}

/// The `current` object within the Open-Meteo response.
#[derive(Deserialize)]
struct CurrentWeather {
    temperature_2m: f32,
    weather_code: u16,
    wind_speed_10m: f32,
    relative_humidity_2m: u8,
}

// ── WeatherWidget ─────────────────────────────────────────────────────────────

/// Data provider for the weather widget.
///
/// Queries the Open-Meteo API on each `update()` call. The response is cached
/// so that HTTP failures return the last known conditions rather than an error.
/// The first call after construction performs the initial fetch.
pub struct WeatherWidget {
    name: String,
    agent: ureq::Agent,
    latitude: f64,
    longitude: f64,
    unit: TempUnit,
    /// Last successfully fetched data, used as a fallback on network errors.
    last: Option<WidgetData>,
}

impl WeatherWidget {
    /// Create a new `WeatherWidget`.
    ///
    /// # Parameters
    /// - `name` — stable widget name used in logging and Poller registration.
    /// - `latitude` / `longitude` — WGS-84 geographic coordinates for the forecast.
    /// - `unit` — temperature unit to request and display (`Celsius` or `Fahrenheit`).
    ///
    /// # Returns
    ///
    /// A ready-to-use `WeatherWidget`. The first `update()` call performs the
    /// initial network request.
    ///
    /// # Side effects
    ///
    /// Constructs a `ureq::Agent` with a 10-second connect+read timeout. No
    /// network I/O occurs at construction time.
    #[must_use]
    pub fn new(name: &str, latitude: f64, longitude: f64, unit: TempUnit) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        let agent = config.new_agent();
        Self {
            name: name.to_string(),
            agent,
            latitude,
            longitude,
            unit,
            last: None,
        }
    }

    /// Build the Open-Meteo forecast URL for the configured coordinates and unit.
    ///
    /// # Returns
    ///
    /// A fully-qualified URL string ready to be passed to `ureq::Agent::get`.
    fn build_url(&self) -> String {
        let temp_unit = match self.unit {
            TempUnit::Celsius => "celsius",
            TempUnit::Fahrenheit => "fahrenheit",
        };
        format!(
            "https://api.open-meteo.com/v1/forecast\
             ?latitude={lat}\
             &longitude={lon}\
             &current=temperature_2m,weather_code,wind_speed_10m,relative_humidity_2m\
             &temperature_unit={temp_unit}",
            lat = self.latitude,
            lon = self.longitude,
        )
    }
}

impl Widget for WeatherWidget {
    /// Return the widget's name.
    fn name(&self) -> &str {
        &self.name
    }

    /// Fetch current weather from the Open-Meteo API and return it as
    /// [`WidgetData::Weather`].
    ///
    /// On success the result is stored in an internal cache. On HTTP or parse
    /// failure, the cached value is returned (if available) so the display
    /// shows stale data rather than clearing. Only returns `Err` when the
    /// request fails *and* no cached value is available.
    ///
    /// # Errors
    ///
    /// Returns [`ParapetError::Http`] if the HTTP request fails and no cached
    /// data is available.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let url = self.build_url();

        match self.agent.get(&url).call() {
            Ok(mut response) => match response.body_mut().read_json::<ApiResponse>() {
                Ok(api) => {
                    let data = WidgetData::Weather {
                        temperature: api.current.temperature_2m,
                        weather_code: api.current.weather_code,
                        wind_speed: api.current.wind_speed_10m,
                        humidity: api.current.relative_humidity_2m,
                        unit: self.unit,
                    };
                    self.last = Some(data.clone());
                    Ok(data)
                }
                Err(e) => {
                    tracing::warn!(
                        widget = self.name,
                        error = %e,
                        "weather JSON parse failed; returning stale data"
                    );
                    self.last
                        .clone()
                        .ok_or_else(|| ParapetError::Http(format!("json parse error: {e}")))
                }
            },
            Err(e) => {
                tracing::warn!(
                    widget = self.name,
                    error = %e,
                    "weather HTTP request failed; returning stale data"
                );
                self.last
                    .clone()
                    .ok_or_else(|| ParapetError::Http(format!("http request failed: {e}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_widget_name_returns_name() {
        let w = WeatherWidget::new("my-weather", 51.5, -0.1, TempUnit::Celsius);
        assert_eq!(w.name(), "my-weather");
    }

    #[test]
    fn weather_widget_name_non_empty() {
        let w = WeatherWidget::new("weather", 0.0, 0.0, TempUnit::Celsius);
        assert!(!w.name().is_empty());
    }

    #[test]
    fn weather_url_contains_latitude_longitude() {
        let w = WeatherWidget::new("w", 51.5085, -0.1257, TempUnit::Celsius);
        let url = w.build_url();
        assert!(url.contains("51.5085"), "URL must contain latitude; got: {url}");
        assert!(url.contains("-0.1257"), "URL must contain longitude; got: {url}");
        assert!(url.contains("celsius"), "URL must contain temperature unit; got: {url}");
    }

    #[test]
    fn weather_url_fahrenheit_unit() {
        let w = WeatherWidget::new("w", 40.71, -74.01, TempUnit::Fahrenheit);
        let url = w.build_url();
        assert!(url.contains("fahrenheit"), "URL must use fahrenheit unit; got: {url}");
    }

    #[test]
    fn weather_url_contains_required_fields() {
        let w = WeatherWidget::new("w", 0.0, 0.0, TempUnit::Celsius);
        let url = w.build_url();
        assert!(url.contains("temperature_2m"), "missing temperature_2m; got: {url}");
        assert!(url.contains("weather_code"), "missing weather_code; got: {url}");
        assert!(url.contains("wind_speed_10m"), "missing wind_speed_10m; got: {url}");
        assert!(url.contains("relative_humidity_2m"), "missing humidity; got: {url}");
    }
}
