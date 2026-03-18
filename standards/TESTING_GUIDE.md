# Parapet — Testing Guide

> **Scope:** Test suite structure, test types, headless test policy, required coverage by change type, and display-required test handling.
> **Last Updated:** Mar 17, 2026

---

## 1. Philosophy

Tests verify behavior from the **user perspective**. A function that compiles and returns without panicking is not tested — it is type-checked. A test must assert that the function produces the correct output for a given input, including failure cases.

**Rules (from RULE_OF_LAW §3.3):**
- Every new Rust function has a unit test in a `#[cfg(test)]` block
- Every failing test is fixed, not deleted or ignored
- Tests are not removed to make a build pass
- `#[ignore]` requires a comment explaining the condition that will un-ignore it

---

## 2. Test Types

### 2.1 Unit Tests

Live in `#[cfg(test)]` modules within the source file under test:

```rust
// src/config.rs

pub fn default_height() -> u32 { 30 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_height_returns_thirty() {
        assert_eq!(default_height(), 30);
    }

    #[test]
    fn config_parses_valid_toml() {
        let toml = r#"
            [bar]
            position = "top"
            height = 28
        "#;
        let config: ParapetConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.bar.height, 28);
        assert_eq!(config.bar.position, BarPosition::Top);
    }

    #[test]
    fn config_uses_defaults_when_optional_fields_absent() {
        let toml = "[bar]\nposition = \"top\"";
        let config: ParapetConfig = toml::from_str(toml).expect("valid toml");
        assert_eq!(config.bar.height, BarConfig::default().height);
    }
}
```

**Rules:**
- Three or more test cases per function — happy path, failure path, edge case
- No global mutable state in tests — tests must be order-independent
- Use `tempfile::TempDir` for any test that needs a real filesystem

### 2.2 Integration Tests

Live in `crates/<crate>/tests/`. Access only the public API — no `use super::*`.

```
crates/
└── parapet_core/
    └── tests/
        ├── config_roundtrip.rs    ← serialize → deserialize → assert equality
        ├── widget_update.rs       ← Widget trait contract tests
        └── poller_intervals.rs    ← Poller scheduling behavior
```

Integration tests in `parapet_core` must not require a display (no GTK). All `parapet_core` tests must pass with `--no-default-features`.

### 2.3 Widget Trait Contract Tests

Every type that implements the `Widget` trait must have a contract test:

```rust
// crates/parapet_core/tests/widget_update.rs

struct MinimalWidget;
impl Widget for MinimalWidget {
    fn name(&self) -> &str { "minimal" }
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        Ok(WidgetData::Clock { display: "00:00".to_string() })
    }
}

#[test]
fn widget_name_returns_non_empty_string() {
    let w = MinimalWidget;
    assert!(!w.name().is_empty());
}

#[test]
fn widget_update_returns_ok_for_minimal_implementation() {
    let mut w = MinimalWidget;
    assert!(w.update().is_ok());
}
```

---

## 3. Required Coverage by Change Type

| Change Type | Required Tests | Location |
|-------------|---------------|----------|
| New Rust function | Unit test — happy path + failure + edge case | Same file, `#[cfg(test)]` |
| New widget type | Widget trait contract test | `crates/parapet_core/tests/` |
| New config field | Config round-trip test (serialize + deserialize) | `crates/parapet_core/tests/config_roundtrip.rs` |
| Config field default change | Test the new default value | Same file, `#[cfg(test)]` |
| Polling interval logic | Poller scheduling test | `crates/parapet_core/tests/poller_intervals.rs` |
| X11 EWMH behavior | Visual verification note in PR description | Manual |
| GTK3 renderer | Visual smoke test note in PR description | Manual |
| Build change | `cargo build --workspace` clean | CI |

---

## 4. Test Helpers and Fixtures

### 4.1 Temp Config File

```rust
// Test helper — creates a minimal valid config in a temp directory
pub fn temp_config() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().expect("tempdir creation");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, r#"
        [bar]
        position = "top"
        height = 30
    "#).expect("config write");
    (dir, path)
}
```

### 4.2 Mock Widget

```rust
pub struct MockWidget {
    name: String,
    data: WidgetData,
}

impl MockWidget {
    pub fn new(name: &str, data: WidgetData) -> Self {
        Self { name: name.to_string(), data }
    }
}

impl Widget for MockWidget {
    fn name(&self) -> &str { &self.name }
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        Ok(self.data.clone())
    }
}
```

---

## 5. Headless Test Policy

`parapet_core` must be fully testable without a display. All tests in `parapet_core` must pass with:

```bash
cargo test -p parapet_core --no-default-features
```

**Rules:**
- No GTK imports in `parapet_core` — so GTK init is never required for `parapet_core` tests
- Tests that require a display are only in `parapet_bar`
- `parapet_bar` tests requiring GTK must check for display availability and skip gracefully

```rust
// In parapet_bar integration tests
#[test]
fn bar_window_creates_without_panic() {
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        eprintln!("SKIP: no display available");
        return;
    }
    gtk::init().expect("GTK init");
    // ... test body
}
```

**Print a `SKIP:` message to stderr** so CI logs show why the test did not run. Do not use `#[ignore]` for display-availability skips — use runtime checks.

`#[ignore]` is reserved for tests that are explicitly deferred to a future state, with a comment explaining when they will be un-ignored.

---

## 6. Config Round-Trip Tests

Every config struct must have a round-trip test verifying that serialized TOML can be deserialized back to the same struct:

```rust
#[test]
fn bar_config_round_trips_through_toml() {
    let config = BarConfig {
        position: BarPosition::Top,
        height: 28,
        monitor: MonitorTarget::Primary,
        css_path: None,
        widget_spacing: 4,
    };

    let serialized = toml::to_string(&config).expect("serialize");
    let deserialized: BarConfig = toml::from_str(&serialized).expect("deserialize");

    assert_eq!(config.position, deserialized.position);
    assert_eq!(config.height, deserialized.height);
    assert_eq!(config.monitor, deserialized.monitor);
}
```

---

## 7. Running the Test Suite

```bash
# All tests (headless baseline — no display required)
cargo test --workspace --no-default-features

# parapet_core only
cargo test -p parapet_core

# Specific test
cargo test -p parapet_core config_parses_valid_toml

# With output (for SKIP messages)
cargo test --workspace -- --nocapture

# Full suite (requires display for parapet_bar tests)
cargo test --workspace
```

---

## 8. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and test mandate | [RULE_OF_LAW.md §3.3](RULE_OF_LAW.md) |
| Test naming conventions | [CODING_STANDARDS.md §8.3](CODING_STANDARDS.md) |
| Test suppression policy | [CODING_STANDARDS.md §8.4](CODING_STANDARDS.md) |
| Config model | [CONFIG_MODEL.md](CONFIG_MODEL.md) |
| Widget trait contract | [WIDGET_API.md](WIDGET_API.md) |
| Build verification protocol | [BUILD_GUIDE.md §3.3](BUILD_GUIDE.md) |
