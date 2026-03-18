# Research: Weather Widget
**Date:** 2026-03-17
**Status:** Decision recommended — ready to close

## Question

What HTTP client crate and weather data source should be used to implement a weather widget in `parapet_core`, and how should the widget integrate with the existing `Widget` trait and Poller architecture?

## Summary

Use `ureq ~3.2` (MIT/Apache-2.0, blocking HTTP, pure Rust, 103M downloads) to call the Open-Meteo free API (no API key, CC-BY 4.0, non-commercial OK). A `WeatherWidget` implemented in `parapet_core::widgets::weather` stores a cached `WeatherData` value and returns it on Poller-driven `update()` calls; the cache is refreshed only when the HTTP fetch succeeds, providing graceful degradation on network errors. A 30-minute poll interval (1 800 000 ms) safely fits within Open-Meteo's 10 K calls/day free tier.

---

## Findings

### Option A: `ureq ~3.2` + Open-Meteo (recommended)

**HTTP crate — `ureq v3.2.0`**

- **License:** MIT OR Apache-2.0
- **Downloads:** 103.5 M
- **MSRV:** 1.71 (workspace minimum is 1.75 — no conflict)
- **Blocking:** Yes — synchronous I/O; `agent.get(url).call()` blocks until response
- **TLS default:** rustls + ring (pure Rust; no OpenSSL system library required)
- **JSON:** `features = ["json"]` — backed by `serde_json`; `response.body_mut().read_json::<T>()` deserialises directly into a typed struct
- **Timeout:** Per-`Agent` config: `ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()`
- **Connection reuse:** `Agent` maintains a connection pool; storing `Agent` in the widget struct avoids per-request TCP overhead
- **RUSTSEC:** No advisories for `ureq` or `serde_json` found

```toml
# parapet_core/Cargo.toml
[dependencies]
ureq = { version = "~3.2", features = ["json"] }
```

`serde_json` is a transitive dependency already present via `serde` features; `ureq`'s `json` feature enables its `body.read_json()` convenience method using the same `serde_json` crate.

**Data source — Open-Meteo**

- **URL:** `https://api.open-meteo.com/v1/forecast`
- **Auth:** None — no API key required
- **License:** CC-BY 4.0 (data); free tier for non-commercial use
- **Free tier limits:** < 10 000 calls/day, < 5 000/hour, < 600/min
- **Rate analysis:** 30-minute interval = 48 calls/day per bar instance — safely within limits
- **Parameters:** `?latitude=LAT&longitude=LON&current=temperature_2m,weather_code,wind_speed_10m,relative_humidity_2m`
- **Response shape:**
  ```json
  {
    "current": {
      "time": "2026-03-17T14:00",
      "temperature_2m": 12.3,
      "weather_code": 61,
      "wind_speed_10m": 18.5,
      "relative_humidity_2m": 74
    }
  }
  ```
- **Weather codes:** WMO standard — 0 = clear sky, 1–3 = partly cloudy, 45/48 = fog, 51–57 = drizzle, 61–67 = rain, 71–77 = snow, 80–82 = showers, 95/96/99 = thunderstorm
- **No official Rust SDK:** A search of crates.io confirmed no `open-meteo` crate exists; raw HTTP + typed serde structs is the correct approach

### Option B: A paid/key-based API (OpenWeatherMap, WeatherAPI, etc.)

Requires an API key stored in config — a worse UX and a potential secrets-in-config risk. Open-Meteo's free tier is fully adequate for a personal desktop bar. **Not recommended.**

---

## Recommendation

Use **`ureq ~3.2` + Open-Meteo**.

**Implementation sketch for `parapet_core::widgets::weather`:**

```rust
use serde::Deserialize;
use ureq::Agent;

#[derive(Deserialize)]
struct ApiResponse {
    current: CurrentWeather,
}

#[derive(Deserialize)]
struct CurrentWeather {
    temperature_2m: f32,
    weather_code: u16,
    wind_speed_10m: f32,
    relative_humidity_2m: u8,
}

pub struct WeatherWidget {
    agent: Agent,
    latitude: f64,
    longitude: f64,
    units: TempUnit,             // Celsius | Fahrenheit
    last: Option<WeatherData>,   // stale-cache fallback
}

impl Widget for WeatherWidget {
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast\
             ?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m",
            self.latitude, self.longitude
        );
        match self.agent.get(&url).call()
            .and_then(|mut r| r.body_mut().read_json::<ApiResponse>())
        {
            Ok(resp) => {
                let data = WeatherData {
                    temp: resp.current.temperature_2m,
                    weather_code: resp.current.weather_code,
                    wind_speed: resp.current.wind_speed_10m,
                    units: self.units,
                };
                self.last = Some(data.clone());
                Ok(WidgetData::Weather(data))
            }
            Err(e) => {
                tracing::warn!("weather fetch failed: {e}");
                self.last.clone()
                    .map(WidgetData::Weather)
                    .ok_or_else(|| ParapetError::io("weather: no data yet"))
            }
        }
    }
}
```

**`WidgetData` variant to add** (minor bump → WIDGET_API 1.3.0):

```rust
/// Current weather conditions from a remote provider.
Weather {
    /// Current temperature in the configured unit.
    temp: f32,
    /// WMO weather interpretation code.
    weather_code: u16,
    /// Wind speed in km/h (always, regardless of temp unit).
    wind_speed: f32,
    /// Temperature unit selected in widget config.
    units: TempUnit,
},
```

Add `TempUnit` as a plain `#[derive(Debug, Clone, Copy)] pub enum TempUnit { Celsius, Fahrenheit }` in `parapet_core::widgets::weather`.

**`WidgetConfig` fields to add:**

```toml
[[widgets]]
type = "weather"
position = "right"
interval = 1800000      # 30 minutes (required)
latitude = 51.5         # required
longitude = -0.1        # required
units = "celsius"       # optional, default "celsius"
```

**Widget name for CSS:** `.widget-weather`

**GTK renderer display** (suggestion for `parapet_bar`):
- Map `weather_code` to a UTF-8 symbol: ☀ (0), ⛅ (1–3), 🌫 (45–48), 🌧 (51–67), 🌨 (71–77), ⛈ (95+)
- Display: `☀ 12°C`

**Standards reference:** ARCHITECTURE §3 (display isolation — `ureq` is pure Rust, no display dep, safe in `parapet_core`); BUILD_GUIDE §1.2 (no new system libraries required); WIDGET_API §5 (5-step checklist for new widgets).

---

## Standards Conflict / Proposed Update

None. `ureq` and Open-Meteo fit cleanly into the existing architecture without requiring any standards change.

The `BUILD_GUIDE §1.2` system dependency table does not need updating — `ureq` uses rustls (pure Rust TLS) and has no C library requirement.

---

## Sources

- [crates.io/crates/ureq](https://crates.io/crates/ureq): v3.2.0 — MIT/Apache-2.0, blocking HTTP client, rustls default
- [crates.io/crates/serde_json](https://crates.io/crates/serde_json): v1.0.149 — MIT/Apache-2.0, 780M downloads, no advisories
- [open-meteo.com/en/docs](https://open-meteo.com/en/docs): API endpoint, parameters, WMO weather codes
- [open-meteo.com/en/terms](https://open-meteo.com/en/terms): CC-BY 4.0, free tier limits
- [rustsec.org/advisories](https://rustsec.org/advisories): checked — no advisories for `ureq` or `serde_json`

---

## Open Questions

1. **`ureq` TLS / `ring` version** — `ureq v3.2` ships with rustls; confirm the transitive `ring` dependency is ≥ 0.17 (RUSTSEC-2025-0010 notes < 0.17 is unmaintained). Run `cargo tree -p ureq` before pinning to verify.
2. **Open-Meteo CC-BY attribution** — The licence requires attribution for public distribution. For a personal bar this is a non-issue; if Parapet is packaged and redistributed, a notice in `README.md` or `--version` output should name Open-Meteo as the data source.
3. **Network unavailability at startup** — The stale-cache fallback handles mid-session failures; widgets show the last known value. On cold start with no network, the widget returns `Err` until the first successful fetch. The bar renderer should display a neutral placeholder (e.g. `? --°`) rather than hiding the widget.
