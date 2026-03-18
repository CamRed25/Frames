# Parapet — Coding Standards

> **Scope:** All Rust code in `parapet/crates/`. Covers naming, error handling, unsafe rules, async patterns, FFI conventions, and toolchain configuration.
> **Last Updated:** Mar 17, 2026

---

## 1. Toolchain Configuration

### 1.1 Rust Edition

All crates use **Rust 2021 edition**.

```toml
# Every crate's Cargo.toml
[package]
edition = "2021"
```

### 1.2 Minimum Supported Rust Version (MSRV)

```toml
# workspace Cargo.toml
[workspace.package]
rust-version = "1.75"
```

Do not use nightly features. Do not use unstable feature flags. All code must compile on stable.

### 1.3 Formatting — rustfmt

All code is formatted with `rustfmt`. A `rustfmt.toml` at the workspace root:

```toml
# rustfmt.toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
reorder_imports = true
reorder_modules = true
newline_style = "Unix"
fn_call_width = 80
chain_width = 80
```

**Rules:**
- `cargo fmt --all` must produce no diff before merge
- Do not suppress formatting with `#[rustfmt::skip]` except for hand-formatted tables or matrices — document why when used
- Line width is 100 characters

### 1.4 Linting — Clippy

Enforced lint groups at workspace level:

```toml
# workspace Cargo.toml
[workspace.lints.clippy]
pedantic = "warn"
correctness = "deny"
```

And in CI:

```bash
cargo clippy --workspace -- -D warnings
```

**Rules:**
- `clippy::correctness` violations are bugs — fix immediately
- `clippy::pedantic` warnings are code quality issues — fix when feasible
- When a pedantic lint is intentionally suppressed:
  ```rust
  // clippy::cast_precision_loss: value is always < 10^6, precision loss is acceptable for display
  #[allow(clippy::cast_precision_loss)]
  let pct = used as f32 / total as f32 * 100.0;
  ```
- Never use `#![allow(clippy::all)]` or `#![allow(warnings)]` at crate root

---

## 2. Naming Conventions

### 2.1 Standard Rust Naming

Follow Rust API Guidelines throughout:

| Item | Convention | Example |
|------|-----------|---------|
| Types, traits, enums | `UpperCamelCase` | `WidgetData`, `ParapetError` |
| Functions, methods | `snake_case` | `update_widget`, `load_config` |
| Variables, parameters | `snake_case` | `charge_pct`, `widget_name` |
| Constants | `SCREAMING_SNAKE_CASE` | `DEFAULT_HEIGHT`, `POLL_INTERVAL_MS` |
| Modules | `snake_case` | `widgets`, `poll` |
| Crates | `snake_case` | `parapet_core`, `parapet_bar` |
| Lifetimes | short lowercase | `'a`, `'cfg`, `'data` |
| Type parameters | single uppercase or short | `T`, `E`, `W` |

### 2.2 Parapet-Specific Conventions

- **Widget data types** use noun phrases describing the data, not the widget:
  - `CpuData`, `MemoryData`, `ClockData` — if split into separate structs
  - Or as `WidgetData::Cpu { ... }` — enum variant names match the widget type
- **Error types** are named for the operation that failed, suffixed with `Error`:
  - `ParapetConfigError`, `PollError`, `BatteryReadError`
- **Config types** are suffixed with `Config`: `BarConfig`, `WidgetConfig`, `ParapetConfig`
- **Renderer types** in `parapet_bar` are suffixed with `Widget`: `ClockWidget`, `CpuWidget`

### 2.3 Module-Level Naming

Public API items exported from a module must read clearly at the call site:

```rust
// Good — reads clearly as parapet_core::poll
pub fn poll_all(&mut self) -> Vec<(String, WidgetData)>

// Avoid — redundant prefix when called as parapet_core::poll::poll_poll_all
pub fn poll_poll_all(&mut self) -> Vec<(String, WidgetData)>
```

---

## 3. Error Handling

### 3.1 Core Philosophy

**Errors are values, not exceptions.** Every fallible function returns `Result<T, E>`. Errors are propagated explicitly, annotated with context, and handled at the boundary closest to the user.

Silent failure is never acceptable.

### 3.2 Library vs Application Errors

| Layer | Crate | Pattern |
|-------|-------|---------|
| Library (`parapet_core`) | `thiserror` | Typed error enums, structured variants |
| Application (`parapet_bar`, `main`) | `anyhow` | Context-annotated propagation |

```rust
// parapet_core — typed library error
#[derive(Debug, thiserror::Error)]
pub enum ParapetConfigError {
    #[error("config file not found at {path}")]
    NotFound { path: PathBuf },
    #[error("config parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("io error reading config: {0}")]
    Io(#[from] std::io::Error),
}

// parapet_bar — application propagation with context
fn start_bar(config_path: &Path) -> anyhow::Result<()> {
    let config = ParapetConfig::load(config_path)
        .context("failed to load bar config")?;
    Ok(())
}
```

### 3.3 The ? Operator

Use `?` for propagation. Do not use `.unwrap()` or `.expect()` in production code paths.

```rust
// Good
let config = ParapetConfig::load(&path)?;

// Never in production
let config = ParapetConfig::load(&path).unwrap();
```

`.expect()` is permitted **only** in:
- `#[cfg(test)]` blocks
- `main()` for fatal startup failures where there is genuinely no recovery
- Mutex lock with an invariant message explaining why poisoning cannot occur

### 3.4 Error Context

Always add context when propagating errors across module boundaries:

```rust
use anyhow::Context;

fn init_cpu_widget(config: &WidgetConfig) -> anyhow::Result<CpuWidget> {
    let poller = CpuPoller::new()
        .with_context(|| format!("failed to initialize CPU poller for widget {:?}", config.name))?;
    Ok(CpuWidget::new(poller))
}
```

### 3.5 Never Silently Discard Errors

```rust
// Never — silently swallows the error
let _ = widget.update();

// Correct — log and continue if non-fatal
if let Err(e) = widget.update() {
    tracing::warn!(widget = widget.name(), error = %e, "widget update failed, skipping");
}

// Correct — propagate if fatal
widget.update().context("widget update failed")?;
```

---

## 4. Cargo and Dependencies

### 4.1 Workspace Structure

All crates are members of the workspace root `Cargo.toml`. Shared dependencies are declared at workspace level:

```toml
# workspace Cargo.toml
[workspace]
members = [
    "crates/parapet_core",
    "crates/parapet_bar",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "~1.0", features = ["derive"] }
toml = "~0.8"
thiserror = "~1.0"
anyhow = "~1.0"
tracing = "~0.1"
```

```toml
# crate Cargo.toml — reference workspace versions
[dependencies]
serde.workspace = true
thiserror.workspace = true
```

### 4.2 Version Pinning Policy

| Dependency Class | Pinning Policy |
|-----------------|---------------|
| Core dependencies (gtk, sysinfo, serde) | Pin minor version: `"~0.18"` |
| Utility crates | Pin major version: `"1"` is acceptable |
| C FFI wrappers | Pin exact version until tested: `"=X.Y.Z"` |
| Dev/test dependencies | Pin major version |

Never use `*` version specifications.

### 4.3 Feature Flags

Optional features must not affect the core build:

```toml
[features]
default = []
ipc = ["dep:tokio"]   # Optional IPC socket for external control
```

`parapet_core` and `parapet_bar` must build and all tests must pass with `--no-default-features`. Optional features are strictly additive.

### 4.4 build.rs Rules

`build.rs` is permitted only for:
- Linking C libraries (GTK3 system library detection via pkg-config)
- Compile-time platform detection

No arbitrary logic, no network access, no file downloads in `build.rs`.

---

## 5. GTK3 Threading Rules

### 5.1 GTK Main Thread

GTK3 is not thread-safe. All GTK operations must occur on the thread that called `gtk::init()` (the main thread).

```rust
// Good — update widget from glib timer callback on main thread
glib::timeout_add_local(Duration::from_millis(1000), move || {
    label.set_text(&get_clock_string());
    glib::ControlFlow::Continue
});

// Never — updating a GtkWidget from a spawned thread
std::thread::spawn(|| {
    label.set_text("foo"); // WRONG — GTK not thread safe
});
```

### 5.2 Background Work

If widget data collection is slow (e.g., slow disk I/O for battery), use `std::thread::spawn` for the data collection and `glib::idle_add_local` to apply results on the main thread:

```rust
let (tx, rx) = std::sync::mpsc::channel::<WidgetData>();

std::thread::spawn(move || {
    let data = collect_slow_data();
    let _ = tx.send(data);
});

glib::idle_add_local(move || {
    if let Ok(data) = rx.try_recv() {
        renderer.apply(data);
    }
    glib::ControlFlow::Break
});
```

Do not block the GTK main thread waiting for data.

---

## 6. Unsafe Code

### 6.1 Policy

`unsafe` blocks are permitted only when:

1. Interfacing with C libraries via FFI not covered by safe gtk-rs bindings
2. The block is accompanied by a `// SAFETY:` comment

Every `unsafe` block without a `// SAFETY:` comment is a bug.

```rust
// Good
// SAFETY: `ptr` is non-null and points to a valid GdkWindow, as guaranteed by
// the caller holding a live reference to the Gdk Window that owns it.
let window = unsafe { gdk_sys::gdk_x11_window_get_xid(ptr) };

// Bug — no safety justification
let window = unsafe { gdk_sys::gdk_x11_window_get_xid(ptr) };
```

### 6.2 FFI Wrappers

All C library calls must be wrapped in safe Rust APIs:

```rust
// ffi.rs — raw bindings, internal only
mod ffi {
    extern "C" {
        fn gdk_x11_window_get_xid(window: *mut GdkWindow) -> u64;
    }
}

// x11.rs — safe wrapper
pub fn get_xid(window: &gdk::Window) -> u64 {
    unsafe {
        // SAFETY: window is a valid GdkWindow pointer for the lifetime of the reference.
        ffi::gdk_x11_window_get_xid(window.as_ptr())
    }
}
```

---

## 7. Documentation

### 7.1 Doc Comments

Every public item must have a doc comment:

```rust
/// Returns the current memory usage as bytes used and total bytes available.
///
/// Refreshes the `sysinfo::System` before sampling. Swap statistics are
/// included in the returned `MemoryData`.
///
/// # Errors
///
/// Returns [`ParapetError::SysInfo`] if memory information cannot be read.
///
/// # Panics
///
/// Does not panic. All error conditions return `Err`.
pub fn memory_usage(&mut self) -> Result<WidgetData, ParapetError> {
```

Required sections for non-trivial public functions:
- Description (first line, concise)
- `# Errors` — every `Err` variant that can be returned
- `# Panics` — if the function can panic, explain when; if it cannot, say so

### 7.2 Inline Comments

Comment the *why*, not the *what*:

```rust
// Good — explains the decision
// sysinfo requires a full refresh before sampling; calling cpu_usage()
// without refresh returns stale data from the previous poll cycle.
system.refresh_cpu();
let usage = system.global_cpu_info().cpu_usage();

// Useless — restates the code
// Call refresh_cpu
system.refresh_cpu();
```

### 7.3 TODO / FIXME Policy

`TODO` and `FIXME` comments in committed code must reference a `DOCS/futures.md` entry:

```rust
// TODO(DOCS/futures.md#wayland-support): implement layer-shell for Wayland
```

Standalone `TODO` comments with no reference are not permitted.

---

## 8. Testing

### 8.1 Unit Tests

Unit tests live in `#[cfg(test)]` modules in the same file as the code under test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_loads_from_valid_toml_string() {
        let toml = r#"
            [bar]
            position = "top"
            height = 30
        "#;
        let config: ParapetConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.bar.position, BarPosition::Top);
    }
}
```

### 8.2 Integration Tests

Integration tests live in `crates/<crate>/tests/`. They test public API only — no `use super::*` in integration tests.

### 8.3 Test Naming

Test names are full sentences describing the behavior under test:

```rust
#[test]
fn config_parse_fails_on_missing_required_field() { }

#[test]
fn cpu_widget_data_returns_usage_between_zero_and_one_hundred() { }
```

### 8.4 No Test Suppression

Do not `#[ignore]` tests without a comment explaining why and what condition will un-ignore them. Do not delete failing tests — fix them.

> **Full test suite structure and headless policy:** [TESTING_GUIDE.md](TESTING_GUIDE.md)

---

## 9. Logging

### 9.1 tracing Conventions

Use `tracing` macros throughout. Never use `println!` for diagnostic output in library code.

```rust
use tracing::{debug, info, warn, error};

// Structured fields preferred over format strings
tracing::info!(widget = widget.name(), interval_ms = interval, "widget poller registered");
tracing::warn!(widget = widget.name(), error = %e, "widget update failed, using stale data");
tracing::error!(path = %config_path, error = %e, "config file could not be read");
```

### 9.2 Log Levels

| Level | Use For |
|-------|---------|
| `error` | Unrecoverable failures visible to the user |
| `warn` | Recoverable problems, degraded operation |
| `info` | Significant lifecycle events (bar start, config reload, widget init) |
| `debug` | Detailed operational flow for developer diagnosis |
| `trace` | High-frequency events (per-poll, per-widget update) — off by default |

### 9.3 Sensitive Data

Never log:
- Full filesystem paths that contain usernames (truncate or hash)
- Raw widget data that may contain sensitive system information

---

## 10. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Module structure and crate graph | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Widget trait and data contract | [WIDGET_API.md](WIDGET_API.md) |
| Bar window and X11 design | [BAR_DESIGN.md](BAR_DESIGN.md) |
| Build prerequisites and Cargo workspace | [BUILD_GUIDE.md](BUILD_GUIDE.md) |
| Test suite structure and headless policy | [TESTING_GUIDE.md](TESTING_GUIDE.md) |
| GTK3 and UI conventions | [UI_GUIDE.md](UI_GUIDE.md) |
