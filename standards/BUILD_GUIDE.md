# Frames — Build Guide

> **Scope:** Build prerequisites, Cargo workspace setup, dependency compilation, and CI build steps.
> **Last Updated:** Mar 17, 2026

---

## 1. Prerequisites

### 1.1 Rust Toolchain

```bash
# Install rustup if not present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install the pinned stable toolchain
rustup toolchain install stable
rustup default stable

# Required components
rustup component add clippy rustfmt
```

The workspace `rust-version` field in `Cargo.toml` enforces the minimum. Do not use nightly.

### 1.2 System Dependencies

These libraries must be present on the build host. They are detected via `pkg-config`.

| Library | Package (Fedora) | Package (Ubuntu/Debian) | Package (Arch) | Notes |
|---------|-----------------|------------------------|----------------|-------|
| GTK3 | `gtk3-devel` | `libgtk-3-dev` | `gtk3` | UI toolkit |
| GDK3 | (included with GTK3) | (included) | (included) | GTK display backend |
| pkg-config | `pkgconf` | `pkg-config` | `pkgconf` | Required for C library detection |
| glib2 | `glib2-devel` | `libglib2.0-dev` | `glib2` | GLib (usually transitive) |

> **Note:** The `ureq` (HTTP) and `zbus` (D-Bus) crates added in v0.1.0 are pure Rust and
> require no additional system libraries. `zbus` communicates with the D-Bus session daemon
> over a Unix socket — `libdbus` is not required.
>
> **Note:** `schemars` (JSON Schema derive) and `serde_json` added in v0.1.x are pure Rust
> with no system library prerequisites. They are used only by `frames_core` for schema
> generation and are not linked into the GTK3 display path.

```bash
# Fedora
sudo dnf install gtk3-devel pkgconf

# Ubuntu / Debian
sudo apt install libgtk-3-dev pkg-config

# Arch
sudo pacman -S gtk3 pkgconf
```

### 1.3 Task Runner (just)

```bash
cargo install just
```

`just` encodes all build and verification recipes. See `justfile` at the workspace root.

After cloning, run `just install-hooks` once to install the pre-commit hook (§3.4).

---

## 2. Cargo Workspace

### 2.1 Workspace Root

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "crates/frames_core",
    "crates/frames_bar",
]
resolver = "2"

[workspace.package]
edition = "2021"
rust-version = "1.75"
authors = ["Frames Contributors"]
license = "GPL-3.0-or-later"
repository = "https://github.com/cam/frames"

[workspace.lints.clippy]
pedantic = "warn"
correctness = "deny"

[workspace.dependencies]
# Pin minor versions for core dependencies — see CODING_STANDARDS.md §4.2
serde = { version = "~1.0", features = ["derive"] }
toml = "~0.8"
thiserror = "~1.0"
anyhow = "~1.0"
tracing = "~0.1"
tracing-subscriber = { version = "~0.3", features = ["env-filter"] }
sysinfo = "~0.30"
chrono = { version = "~0.4", features = ["serde"] }
notify = "~6.1"
ureq = { version = "~3.2", features = ["json"] }
zbus = { version = "~5.1", features = ["blocking-api"] }
schemars = "~0.8"
serde_json = "~1.0"
gtk = { version = "~0.18", features = ["v3_22"] }
gdk = "~0.18"
glib = "~0.18"
```

### 2.2 Crate Cargo.toml Templates

```toml
# crates/frames_core/Cargo.toml
[package]
name = "frames_core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
serde.workspace = true
toml.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
sysinfo.workspace = true
chrono.workspace = true
notify.workspace = true

[dev-dependencies]
tempfile = "3"
```

```toml
# crates/frames_bar/Cargo.toml
[package]
name = "frames_bar"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true

[lints]
workspace = true

[dependencies]
frames_core = { path = "../frames_core" }
gtk.workspace = true
gdk.workspace = true
glib.workspace = true
gio.workspace = true
fuzzy-matcher.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

---

## 3. Building

### 3.1 Development Build

```bash
# Standard incremental build
cargo build --workspace

# Release build (optimized)
cargo build --workspace --release
```

### 3.2 Check and Lint

```bash
# Preferred: run all checks via just
just check

# Or individually:
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

### 3.3 Full Verification Protocol

Run this before any merge:

```bash
# Preferred: one-liner
just check            # build + clippy + fmt-check + tests
just check-headless   # repeat without default features (no display required)

# Or manually (all steps must exit 0):
# 1. Clean incremental build
cargo build --workspace

# 2. Clippy
cargo clippy --workspace -- -D warnings

# 3. Formatting
cargo fmt --all -- --check

# 4. Dependency tree (if Cargo.toml changed)
cargo tree | grep -E "duplicate"

# 5. Headless test suite (safe baseline)
cargo test --workspace --no-default-features

# 6. Full test suite
cargo test --workspace
```

### 3.4 Pre-commit Hook

Install once per clone:

```bash
just install-hooks
```

The hook (`scripts/pre-commit`) runs `cargo fmt --check` and `cargo clippy -- -D warnings` on every commit. The full test suite is a pre-merge step (§3.3), not a commit-time step.

---

## 4. Feature Flags

```toml
# crates/frames_core/Cargo.toml
[features]
## Default-on: none currently.
default = []

## Optional IPC socket for external control (e.g. reload, query widget state).
## Adds tokio. Strictly additive.
ipc = ["dep:tokio"]
```

Build without optional features:
```bash
cargo build --workspace --no-default-features
cargo test --workspace --no-default-features
```

Both must succeed. Optional features are strictly additive — disabling them must never break the build.

---

## 5. Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `FRAMES_LOG` | Log level filter for `tracing-subscriber` | `info` |
| `FRAMES_CONFIG` | Override config file path | `~/.config/frames/config.toml` |
| `RUST_BACKTRACE` | Enable backtraces on panic | `0` |

Set `FRAMES_LOG=debug` during development for verbose output.

---

## 6. Running the Bar

```bash
# Development run (from workspace root)
cargo run -p frames_bar

# With a custom config
FRAMES_CONFIG=/path/to/config.toml cargo run -p frames_bar

# Release build + run
cargo build --workspace --release
./target/release/frames_bar
```

The bar requires a running X11 display. `DISPLAY` must be set. Running without a display will fail at `gtk::init()`.

For headless testing:
```bash
cargo test --workspace --no-default-features
```

---

## 7. CI

CI runs the full verification protocol from §3.3 on every push. Minimum CI steps:

```yaml
# .github/workflows/ci.yml (conceptual)
steps:
  - cargo build --workspace
  - cargo clippy --workspace -- -D warnings
  - cargo fmt --all -- --check
  - cargo test --workspace --no-default-features  # headless — no display needed
  - cargo build --workspace --no-default-features
```

CI does not deploy. Deployment is manual.

**Note:** The full `cargo test --workspace` (with default features) requires a real X11 display and is not run in CI. GTK3 tests that require a display are marked `#[ignore]` with a comment explaining the condition for running them. See TESTING_GUIDE.md §5.

---

## 8. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Cargo version pinning policy | [CODING_STANDARDS.md §4.2](CODING_STANDARDS.md) |
| Workspace crate structure | [ARCHITECTURE.md §2](ARCHITECTURE.md) |
| Test suite structure | [TESTING_GUIDE.md](TESTING_GUIDE.md) |
| Distro package availability | [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) |
