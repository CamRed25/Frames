# Parapet — Release Guide

> **Scope:** Release preparation checklist, version tagging, binary packaging, and distribution guidelines.
> **Last Updated:** Mar 17, 2026

---

## 1. Overview

This document defines the release process for Parapet. Releases are cut manually from `master` when the project reaches a shippable state.

**Release types:**

| Type | Version format | When |
|------|---------------|------|
| Alpha | `v0.x.0-alpha` | Early testing, not feature-complete |
| Beta | `v0.x.0-beta` | Feature-complete, bugs expected |
| Release Candidate | `v0.x.0-rc.N` | Final testing before stable |
| Stable | `v0.x.0` or `v1.0.0` | Production-ready |

---

## 2. Pre-Release Checklist

Before tagging any release, complete every item:

### 2.1 Code Quality Gates

- [ ] `cargo build --workspace --release` exits 0
- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo fmt --all -- --check` exits 0
- [ ] `cargo test --workspace --no-default-features` exits 0 (all tests pass headlessly)
- [ ] No `.unwrap()` in production paths outside of documented invariants
- [ ] No `unsafe` blocks without `// SAFETY:` comments

### 2.2 Documentation Gates

- [ ] `CHANGELOG.md` entry written for this release
- [ ] `standards/` documents are current with the code
- [ ] `DOCS/futures.md` updated — completed items moved to Completed section
- [ ] `README.md` describes current features and installation

### 2.3 Functional Verification

- [ ] Bar starts on Cinnamon without errors
- [ ] `_NET_WM_STRUT_PARTIAL` is respected (maximized windows don't overlap the bar)
- [ ] All configured widget types display correct data
- [ ] Config hot-reload works (modify `config.toml`, bar updates without restart)
- [ ] Bar exits cleanly on SIGTERM (no orphaned X11 windows)
- [ ] CSS theming applies correctly

---

## 3. Version Tagging

Versions follow [Semantic Versioning](https://semver.org/). Until `v1.0.0`, the API is not stable and breaking changes are acceptable between minor versions.

```bash
# Tag a release
git tag -a v0.2.0 -m "v0.2.0 — clock and CPU widgets"

# Push the tag
git push origin v0.2.0
```

Update the version in `Cargo.toml` before tagging:

```toml
# workspace Cargo.toml
[workspace.package]
version = "0.2.0"
```

---

## 4. Binary Distribution

### 4.1 Release Binary

Build the release binary:

```bash
cargo build --workspace --release
```

The output binary is at `target/release/parapet_bar`.

The binary links against GTK3 dynamically. The target system must have GTK3 installed. This is a reasonable assumption for any Cinnamon desktop.

### 4.2 Packaging for Distros

**Fedora / RPM:** Create a `.spec` file in `packaging/rpm/`. The spec should declare `Requires: gtk3 >= 3.22`.

**Debian / Ubuntu:** Create a `debian/` directory. `Depends: libgtk-3-0 (>= 3.22)`.

**Arch (AUR):** `PKGBUILD` in `packaging/arch/`. `depends=('gtk3')`.

Until a stable release, do not publish to distro repos or AUR. Package only for direct download.

### 4.3 GitHub Releases

For each tagged version, create a GitHub release with:
- The compiled binary for `x86_64-unknown-linux-gnu`
- The `CHANGELOG.md` section for this version as the release description
- SHA256 checksum of the binary

```bash
# Generate checksum
sha256sum target/release/parapet_bar > parapet_bar.sha256
```

---

## 5. CHANGELOG.md Format

Follow [Keep a Changelog](https://keepachangelog.com/en/1.1.0/):

```markdown
# Changelog

All notable changes to Parapet are documented here.
This project adheres to [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.2.0] - 2026-MM-DD
### Added
- `parapet_core/cpu` — CPU usage widget with per-core breakdown
- `parapet_bar/bar` — `_NET_WM_STRUT_PARTIAL` for correct window maximization

### Fixed
- `parapet_bar/css` — built-in default theme no longer overrides user CSS

## [0.1.0] - 2026-03-17
### Added
- Initial bar window with clock widget
- GTK3 bar window with X11 EWMH dock type
- TOML configuration
```

All new entries go under `## [Unreleased]` until a release is cut.

---

## 6. Post-Release

After a release is tagged and pushed:

1. Add a new `## [Unreleased]` section to `CHANGELOG.md`
2. Update `DOCS/futures.md` — move items completed in this release to Completed section
3. Open any follow-up issues for known bugs or planned next features

---

## 7. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| Build verification protocol | [BUILD_GUIDE.md §3.3](BUILD_GUIDE.md) |
| Test suite | [TESTING_GUIDE.md](TESTING_GUIDE.md) |
| Platform requirements | [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) |
