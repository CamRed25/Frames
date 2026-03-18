//! Config round-trip serialization tests.
//!
//! Verifies that each config struct can be serialized to TOML and deserialized
//! back to an equal value (per TESTING_GUIDE §6).

use parapet_core::config::{BarConfig, BarPosition, BarSection, ClockConfig, ParapetConfig, LauncherConfig, WidgetConfig, WidgetKind};

#[test]
fn bar_config_round_trips_through_toml() {
    let config = BarConfig {
        position: BarPosition::Top,
        height: 28,
        monitor: parapet_core::config::MonitorTarget::Primary,
        css: None,
        theme: None,
        widget_spacing: 4,
    };

    let serialized = toml::to_string(&config).expect("serialize BarConfig");
    let deserialized: BarConfig = toml::from_str(&serialized).expect("deserialize BarConfig");

    assert_eq!(config.position, deserialized.position);
    assert_eq!(config.height, deserialized.height);
    assert_eq!(config.widget_spacing, deserialized.widget_spacing);
    assert_eq!(config.css, deserialized.css);
}

#[test]
fn bar_config_bottom_round_trips() {
    let config = BarConfig {
        position: BarPosition::Bottom,
        height: 32,
        monitor: parapet_core::config::MonitorTarget::Index(1),
        css: Some("~/.config/parapet/parapet.css".to_string()),
        theme: None,
        widget_spacing: 6,
    };

    let serialized = toml::to_string(&config).expect("serialize");
    let deserialized: BarConfig = toml::from_str(&serialized).expect("deserialize");

    assert_eq!(config.position, deserialized.position);
    assert_eq!(config.height, deserialized.height);
    assert_eq!(config.css, deserialized.css);
}

#[test]
fn widget_config_clock_round_trips() {
    let config = WidgetConfig {
        kind: WidgetKind::Clock(ClockConfig {
            format: Some("%H:%M:%S".to_string()),
            timezone: Some("local".to_string()),
        }),
        position: BarSection::Center,
        interval: Some(1000),
        label: None,
        on_click: None,
        on_scroll_up: None,
        on_scroll_down: None,
        extra_class: None,
    };

    let serialized = toml::to_string(&config).expect("serialize WidgetConfig");
    let deserialized: WidgetConfig = toml::from_str(&serialized).expect("deserialize WidgetConfig");

    assert!(matches!(deserialized.kind, WidgetKind::Clock(_)));
    if let WidgetKind::Clock(ref clock) = deserialized.kind {
        assert_eq!(clock.format.as_deref(), Some("%H:%M:%S"));
        assert_eq!(clock.timezone.as_deref(), Some("local"));
    }
    assert_eq!(config.interval, deserialized.interval);
}

#[test]
fn full_config_round_trips() {
    let toml_src = r#"
[bar]
position = "bottom"
height = 32
widget_spacing = 6

[[widgets]]
type = "workspaces"
position = "left"

[[widgets]]
type = "clock"
position = "center"
format = "%a %b %d  %H:%M"

[[widgets]]
type = "cpu"
position = "right"
interval = 2000
warn_threshold = 80.0
crit_threshold = 95.0

[[widgets]]
type = "battery"
position = "right"
warn_threshold = 20.0
crit_threshold = 5.0
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse full config");
    assert_eq!(config.bar.height, 32);
    assert_eq!(config.widgets.len(), 4);
    assert!(matches!(config.widgets[2].kind, WidgetKind::Cpu(_)));
    if let WidgetKind::Cpu(ref cpu) = config.widgets[2].kind {
        assert_eq!(cpu.warn_threshold, Some(80.0));
    }
}

#[test]
fn weather_widget_config_parses_latitude_longitude_units() {
    let toml_src = r#"
[[widgets]]
type = "weather"
position = "right"
latitude = 51.5085
longitude = -0.1257
units = "celsius"
interval = 1800000
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse weather config");
    assert_eq!(config.widgets.len(), 1);
    let w = &config.widgets[0];
    assert!(matches!(w.kind, WidgetKind::Weather(_)));
    if let WidgetKind::Weather(ref weather) = w.kind {
        assert_eq!(weather.latitude, Some(51.5085));
        assert_eq!(weather.longitude, Some(-0.1257));
        assert_eq!(weather.units.as_deref(), Some("celsius"));
    }
    assert_eq!(w.interval, Some(1_800_000));
}

#[test]
fn weather_widget_config_fahrenheit() {
    let toml_src = r#"
[[widgets]]
type = "weather"
position = "right"
latitude = 40.71
longitude = -74.01
units = "fahrenheit"
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse weather fahrenheit");
    if let WidgetKind::Weather(ref weather) = config.widgets[0].kind {
        assert_eq!(weather.units.as_deref(), Some("fahrenheit"));
        assert!((weather.latitude.unwrap() - 40.71).abs() < 0.001);
    } else {
        panic!("expected weather widget");
    }
}

#[test]
fn weather_widget_config_defaults_when_coords_absent() {
    // latitude and longitude are optional; build_widget defaults them to 0.0
    let toml_src = r#"
[[widgets]]
type = "weather"
position = "right"
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse weather no coords");
    if let WidgetKind::Weather(ref weather) = config.widgets[0].kind {
        assert_eq!(weather.latitude, None);
        assert_eq!(weather.longitude, None);
        assert_eq!(weather.units, None);
    } else {
        panic!("expected weather widget");
    }
}

#[test]
fn media_widget_config_parses_minimal() {
    let toml_src = r#"
[[widgets]]
type = "media"
position = "center"
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse media config");
    assert_eq!(config.widgets.len(), 1);
    let w = &config.widgets[0];
    assert!(matches!(w.kind, WidgetKind::Media(_)));
    assert_eq!(w.interval, None);
}

#[test]
fn media_widget_config_with_interval_and_actions() {
    let toml_src = r#"
[[widgets]]
type = "media"
position = "center"
interval = 2000
on_click = "playerctl play-pause"
on_scroll_up = "playerctl next"
on_scroll_down = "playerctl previous"
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse media with actions");
    let w = &config.widgets[0];
    assert_eq!(w.interval, Some(2000));
    assert_eq!(w.on_click.as_deref(), Some("playerctl play-pause"));
    assert_eq!(w.on_scroll_up.as_deref(), Some("playerctl next"));
    assert_eq!(w.on_scroll_down.as_deref(), Some("playerctl previous"));
}

#[test]
fn disk_widget_config_parses_mount() {
    let toml_src = r#"
[[widgets]]
type = "disk"
position = "right"
mount = "/home"
format = "percent"
interval = 30000
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse disk config");
    assert_eq!(config.widgets.len(), 1);
    let w = &config.widgets[0];
    assert!(matches!(w.kind, WidgetKind::Disk(_)));
    if let WidgetKind::Disk(ref disk) = w.kind {
        assert_eq!(disk.mount.as_deref(), Some("/home"));
        assert_eq!(disk.format.as_deref(), Some("percent"));
    }
    assert_eq!(w.interval, Some(30000));
}

#[test]
fn disk_widget_config_defaults_when_mount_absent() {
    let toml_src = r#"
[[widgets]]
type = "disk"
position = "right"
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse disk no mount");
    if let WidgetKind::Disk(ref disk) = config.widgets[0].kind {
        assert_eq!(disk.mount, None);
        assert_eq!(disk.format, None);
    } else {
        panic!("expected disk widget");
    }
}

#[test]
fn launcher_config_round_trips_all_fields() {
    let config = LauncherConfig {
        max_results: Some(15),
        button_label: Some("Apps".to_string()),
        popup_width: Some(320),
        popup_min_height: Some(300),
        pinned: vec!["firefox".to_string(), "code".to_string()],
    };

    let serialized = toml::to_string(&config).expect("serialize LauncherConfig");
    let deserialized: LauncherConfig =
        toml::from_str(&serialized).expect("deserialize LauncherConfig");

    assert_eq!(config.max_results, deserialized.max_results);
    assert_eq!(config.button_label, deserialized.button_label);
    assert_eq!(config.popup_width, deserialized.popup_width);
    assert_eq!(config.popup_min_height, deserialized.popup_min_height);
    assert_eq!(config.pinned, deserialized.pinned);
}

#[test]
fn launcher_config_new_fields_default_when_only_max_results_set() {
    let toml_src = r#"
[[widgets]]
type = "launcher"
position = "left"
max_results = 5
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse launcher minimal");
    if let WidgetKind::Launcher(ref launcher) = config.widgets[0].kind {
        assert_eq!(launcher.max_results, Some(5));
        assert_eq!(launcher.button_label, None);
        assert_eq!(launcher.popup_width, None);
        assert_eq!(launcher.popup_min_height, None);
        assert!(launcher.pinned.is_empty());
    } else {
        panic!("expected launcher widget");
    }
}

#[test]
fn launcher_config_pinned_parses_vec() {
    let toml_src = r#"
[[widgets]]
type = "launcher"
position = "left"
pinned = ["firefox", "code", "org.gnome.Nautilus"]
"#;

    let config: ParapetConfig = toml::from_str(toml_src).expect("parse launcher pinned");
    if let WidgetKind::Launcher(ref launcher) = config.widgets[0].kind {
        assert_eq!(launcher.pinned.len(), 3);
        assert_eq!(launcher.pinned[0], "firefox");
        assert_eq!(launcher.pinned[2], "org.gnome.Nautilus");
    } else {
        panic!("expected launcher widget");
    }
}

#[test]
fn launcher_config_rejects_unknown_field() {
    let toml_src = r#"
[[widgets]]
type = "launcher"
position = "left"
bad_field = true
"#;

    let result: Result<ParapetConfig, _> = toml::from_str(toml_src);
    assert!(
        result.is_err(),
        "expected parse failure for unknown field 'bad_field' on launcher config"
    );
}
