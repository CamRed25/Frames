# Parapet — Rule of Law

> **Scope:** All contributors (human and AI agents) working on `parapet/`.
> **Last Updated:** Mar 17, 2026

---

## 1. Purpose

This document codifies **how** the project's standards are applied, enforced, and evolved. Where the other standards define *what* to do, this one defines *the rules about the rules* — governance, verification, change management, and accountability.

Parapet is a ground-up Rust + GTK3 Linux-native status bar. There is no upstream to defer to. Every architectural decision is owned here.

---

## 2. Standards Hierarchy

All `.md` files in `standards/` are authoritative. When conflicts arise, resolve using this precedence:

| Priority | Document | Governs |
|----------|----------|---------|
| 1 | **RULE_OF_LAW.md** (this file) | Meta-rules, enforcement, governance |
| 2 | **CODING_STANDARDS.md** | Rust style, naming, error handling, unsafe rules |
| 3 | **ARCHITECTURE.md** | Module structure, crate graph, dependency chain |
| 4 | **WIDGET_API.md** | Widget trait contract, `WidgetData` boundaries |
| 5 | **BAR_DESIGN.md** | Bar window design, X11 EWMH, widget layout |
| 6 | **CONFIG_MODEL.md** | TOML config structure, field definitions |
| 7 | **BUILD_GUIDE.md** | Cargo workspace, build steps, prerequisites |
| 8 | **TESTING_GUIDE.md** | Test suite structure, headless test policy |
| 9 | **PLATFORM_COMPAT.md** | Distro support, X11/Cinnamon requirements |
| 10 | **UI_GUIDE.md** | GTK3 conventions, CSS theming, widget rendering |

If a standard contradicts a higher-priority standard, the higher-priority document wins. Update the lower-priority document to resolve the conflict.

If a standard is silent on a topic, apply the nearest-scoped standard that addresses a related concern. Document the gap and reasoning in `DOCS/conflict.md` (create if it does not exist) so the relevant standard can be updated explicitly.

**`DOCS/conflict.md` format:**
```markdown
### [Short description of the gap or conflict]
- **Date:** YYYY-MM-DD
- **Standards involved:** [e.g. CODING_STANDARDS §3, WIDGET_API §2]
- **Situation:** [What decision needed to be made]
- **Resolution applied:** [What was done and why]
- **Follow-up:** [Which standard needs updating to close this permanently]
```

Entries in `DOCS/conflict.md` are temporary. Once the relevant standard is updated, move the entry to a `## Resolved` section with the date it was closed.

---

## 3. Core Principles

### 3.1 Build Must Pass

No change is complete until the build succeeds with exit code 0. A broken build blocks all other work.

```bash
cargo build --workspace
# Exit code MUST be 0
```

- Zero compilation errors required.
- Zero warnings on project-owned crates (`-D warnings` enforced in CI).
- Third-party crate warnings do not block merges.
- Build verification happens **after every logical unit of work** — a self-contained change. Not after every line, not only at the end of a session.

### 3.2 Fix, Don't Skip

When code fails, fix the root cause. Do not:

- Comment out failing code to make the build pass
- Delete tests that fail instead of fixing the tested code
- Suppress errors with empty `match _ => {}` arms or `let _ =`
- Use `.unwrap()` to silence a type error and call it fixed
- Mark features as "TODO" when the fix is within reach

**"Fixed" means the code works as intended** — not that the symptom is hidden.

### 3.3 Test Every Change

Every feature and bugfix must have corresponding verification:

| Change Type | Required Verification |
|-------------|----------------------|
| New Rust function | Unit test in `#[cfg(test)]` block |
| New widget type | Widget trait contract test |
| New config field | Config round-trip test (serialize + deserialize) |
| New X11/EWMH behavior | Integration test or manual verification note in PR |
| GTK3 widget renderer | Visual smoke test at 1920×1080 and 2560×1080 |
| Build change | Full `cargo build --workspace` clean |

Test from the **user perspective**. A function that compiles but produces wrong output is not fixed.

### 3.4 Check Before Changing

Before creating or modifying any file:

1. **Read existing code** — understand what's there and why.
2. **Search for usages** — find all call sites and references.
3. **Verify assumptions** — dead code may have hidden consumers.
4. **Ensure no regressions** — new changes must not break existing features.

### 3.5 Error Handling Philosophy

> **Errors are values, not exceptions.** Every fallible function returns `Result<T, E>`. Errors are propagated explicitly, annotated with context, and handled at the boundary closest to the user. Silent failure is never acceptable.

`thiserror` for library errors, `anyhow` for application-level propagation. `.unwrap()` in production code is a bug.

> **Full error handling rules:** [CODING_STANDARDS.md §3](CODING_STANDARDS.md)

### 3.6 Warnings Policy

| Warning Class | Action |
|---------------|--------|
| Compiler (rustc) | Fix immediately — `-D warnings` is enforced |
| Clippy lints | Fix when feasible; document if intentional with `#[allow(...)]` + comment |
| Deprecation | Replace immediately |
| Unsafe block | Requires safety comment — see CODING_STANDARDS §6 |
| Third-party crate | Document; do not suppress with workspace-level allows |

**Never use `#![allow(warnings)]` at crate root** — it hides real problems.

### 3.7 Widget API Boundary Integrity

Any change to the `Widget` trait, `WidgetData` enum, or widget update protocol:

1. Requires a `WIDGET_API.md` update in the same commit
2. Must document whether the change is breaking or non-breaking
3. Breaking changes to `WidgetData` require updating all widget implementations

This rule exists because Parapet defines its own widget contract. Boundary drift breaks all widgets silently.

---

## 4. Change Management

### 4.1 The doa/ Archive

Code removed from the active build is moved to `doa/`, never deleted:

```
doa/
├── crates/        ← Full crate directories
├── modules/       ← Individual module files
└── misc/          ← Other archived items
```

**Rules:**

- Move, don't delete.
- Comment out the corresponding `Cargo.toml` workspace member with an explanation.
- Update `DOCS/cleanup.md` with the date, item moved, and reason:
  ```
  ### [Item Name]
  - **Date:** YYYY-MM-DD
  - **Source:** original/path/
  - **Destination:** doa/modules/ (or doa/crates/, doa/misc/)
  - **Reason:** [why it was removed from the active build]
  ```
- Update `DOCS/futures.md` if the item was tracked there.

### 4.2 Documentation Updates

Every code change that affects architecture, build, or interfaces must update the relevant standard:

| Code Change | Update Required |
|-------------|----------------|
| New crate added | ARCHITECTURE.md §2, §3 |
| New dependency added/removed | BUILD_GUIDE.md §2, ARCHITECTURE.md |
| Widget API changed | WIDGET_API.md |
| Bar window behavior changed | BAR_DESIGN.md |
| Config field changed | CONFIG_MODEL.md |
| Platform support changed | PLATFORM_COMPAT.md |
| GTK3 convention changed | UI_GUIDE.md |
| Naming convention changed | CODING_STANDARDS.md |
| Crate moved to doa/ | `DOCS/cleanup.md`, `DOCS/futures.md` |

### 4.3 The `DOCS/futures.md` Record

All technical notes, ideas, and future enhancements are recorded in `DOCS/futures.md`:

```markdown
# Project Futures
## Ideas & Enhancements
## Technical Debt & Refactoring
## Known Limitations
## Completed/Integrated
```

**Rules:**

- Record ideas **when they arise**, not later.
- Move items to "Completed/Integrated" when done — do not delete them.
- Include date stamps on entries for traceability.

### 4.4 Commit Messages

```
crate/component: brief description (imperative mood)

Detailed explanation of what changed and why.
Reference any relevant crate version, GTK version, or design doc.
```

Examples:
```
parapet_core/cpu: add per-core usage breakdown to CpuData
parapet_bar/bar: set _NET_WM_STRUT_PARTIAL for bottom-anchored bar
config: add widget refresh_interval field with default 1000ms
ui: fix clock widget text overflow at small font sizes
```

### 4.5 Branch Naming

```
feature/workspace-widget
fix/strut-partial-multi-monitor
docs/architecture-module-table
```

### 4.6 Upstream Policy

Parapet has no upstream. All architectural decisions are owned here. When external crates release breaking changes:

| Situation | Action |
|-----------|--------|
| Dependency minor/patch update | Update freely, run full test suite |
| Dependency major version bump | Evaluate breaking changes, update standards if affected |
| Crate abandoned | Evaluate replacement, document decision in futures.md |
| Security advisory | Fix immediately regardless of breaking changes |

---

## 5. Code Quality Gates

### 5.1 Unsafe Code Policy

`unsafe` blocks are permitted only when:

1. Interfacing with C libraries via FFI (GTK3 internals not covered by gtk-rs bindings)
2. The block is accompanied by a `// SAFETY:` comment explaining the invariants upheld

Every `unsafe` block without a `// SAFETY:` comment is a bug.

> **Full unsafe rules and patterns:** [CODING_STANDARDS.md §6](CODING_STANDARDS.md)

### 5.2 Dead Code Elimination

Dead code rots. Remove it when encountered — but move to `doa/`, never delete outright.

- Unreachable match arms: remove them
- Unused imports: `cargo fix` removes them automatically
- Feature-gated code for features that no longer exist: archive to `doa/`

### 5.3 Display Isolation Enforcement

`parapet_core` must not import anything from `gtk`, `gdk`, `glib` (UI), or `x11`. Core contains only system info collection, config parsing, widget data types, and error types.

A `parapet_core` module that imports a display library is a bug in the crate boundary design.

---

## 6. Build System Rules

### 6.1 Cargo Discipline

Key policies:

| Rule | Rationale |
|------|-----------|
| Workspace-level `[patch]` only for forks | Prevents version conflicts across crates |
| Pin minor versions for core dependencies | Reproducible builds across machines |
| `build.rs` only for C FFI | No arbitrary build logic |
| No `*` version specifications | Always breaks eventually |

### 6.2 Build Verification Protocol

After any modification:

```bash
# 1. Incremental build
cargo build --workspace

# 2. Full check with clippy
cargo clippy --workspace -- -D warnings

# 3. If Cargo.toml changed, verify dependency tree
cargo tree | grep -E "duplicate|conflict"

# 4. Full test suite (headless baseline)
cargo test --workspace --no-default-features

# 5. Full test suite before merge
cargo test --workspace
```

Always verify exit code 0. Do not proceed to the next task while the build is broken.

---

## 7. Operational Rules

### 7.1 Standards Review — Lookup Table

| Work Type | Read Before Starting |
|-----------|---------------------|
| New Rust crate or module | ARCHITECTURE.md, CODING_STANDARDS.md |
| Widget system work | WIDGET_API.md, CODING_STANDARDS.md §3 |
| Bar window / X11 work | BAR_DESIGN.md, PLATFORM_COMPAT.md |
| Config / TOML work | CONFIG_MODEL.md |
| GTK3 / UI rendering work | UI_GUIDE.md, CODING_STANDARDS.md |
| New test suite | TESTING_GUIDE.md |
| Starting a major feature | Full read of all relevant standards |
| Returning after a break | RULE_OF_LAW.md + feature-relevant standards |

### 7.2 Batch, Don't Thrash

- Group related file edits into a single logical change.
- Read enough context to understand before modifying.
- Test once after each **logical unit of work**.
- Only request user input after features are 100% confirmed working.

---

## 8. Enforcement

### 8.1 Violation Severity

| Severity | Examples | Action |
|----------|----------|--------|
| **Critical** | Build broken, GTK import in `parapet_core`, unsafe without SAFETY comment | Fix immediately, block all work |
| **High** | Dead code introduced, tests removed, standards ignored | Fix before proceeding |
| **Medium** | Missing doc comment, naming violation, futures.md not updated | Fix in current session |
| **Low** | Minor formatting, optional optimization | Fix when convenient |

### 8.2 Accountability

Every code change is attributable. Whether made by a human or an AI agent:

- The change must follow all applicable standards.
- The build must pass after the change.
- Documentation must be updated if affected.

**No exceptions for "quick fixes."**

---

## 9. Cross-References

| Topic | Standard |
|-------|----------|
| Rust style & conventions | [CODING_STANDARDS.md](CODING_STANDARDS.md) |
| Module structure & crate graph | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Widget trait & data contract | [WIDGET_API.md](WIDGET_API.md) |
| Bar window & X11 design | [BAR_DESIGN.md](BAR_DESIGN.md) |
| TOML config model | [CONFIG_MODEL.md](CONFIG_MODEL.md) |
| Build prerequisites & steps | [BUILD_GUIDE.md](BUILD_GUIDE.md) |
| Test suite & verification | [TESTING_GUIDE.md](TESTING_GUIDE.md) |
| Distro & X11 support | [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) |
| GTK3 & UI conventions | [UI_GUIDE.md](UI_GUIDE.md) |
