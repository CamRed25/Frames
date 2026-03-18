# /audit — Standards Compliance Auditor

You are performing a full standards compliance audit of the Parapet project.
Complete all three phases in order. Do not skip steps. Write the report when done.

---

## Setup

Get today's date:
```bash
date +%Y-%m-%d
```

Set REPORT_DATE to that value. The output file will be:
`/home/cam/Documents/Status/audits/REPORT_DATE_audit.md`

Create the `audits/` directory if it does not exist:
```bash
mkdir -p /home/cam/Documents/Status/audits
```

---

## Phase 1 — Standards Freshness Audit

Read each standard and cross-reference it against the actual codebase. Record every
mismatch between what the standard documents and what actually exists.

### 1.1 ARCHITECTURE.md Freshness

Read `/home/cam/Documents/Status/standards/ARCHITECTURE.md`.

**Workspace member list (§2):**
```bash
grep -A 10 '^\[workspace\]' /home/cam/Documents/Status/Cargo.toml
```
Compare the actual `members = [...]` list against §2's crate list.

**parapet_core module tree (§4):**
```bash
ls /home/cam/Documents/Status/crates/parapet_core/src/
ls /home/cam/Documents/Status/crates/parapet_core/src/widgets/
```
Compare against §4's module table. Note any files present but undocumented, or documented but absent.

**parapet_bar module tree (§5):**
```bash
ls /home/cam/Documents/Status/crates/parapet_bar/src/
ls /home/cam/Documents/Status/crates/parapet_bar/src/widgets/
```
Compare against §5's module table.

**Dependency graph (§3):**
```bash
grep -A 40 '^\[workspace.dependencies\]' /home/cam/Documents/Status/Cargo.toml
```
Compare against §3's documented dependencies. Note any in `Cargo.toml` absent from the standard.

### 1.2 CONFIG_MODEL.md Freshness

Read `/home/cam/Documents/Status/standards/CONFIG_MODEL.md`.

Compare documented `ParapetConfig`, `BarConfig`, and `WidgetConfig` struct fields against the actual Rust source in `crates/parapet_core/src/config.rs`. Note missing or added fields.

### 1.3 BUILD_GUIDE.md Freshness

Read `/home/cam/Documents/Status/standards/BUILD_GUIDE.md`.

**Workspace members in §2.1:**
```bash
grep -A 10 '^\[workspace\]' /home/cam/Documents/Status/Cargo.toml
```
Check whether the template in §2.1 lists all actual workspace members.

**workspace.dependencies (§2.1):**
Compare the `[workspace.dependencies]` example in §2.1 against the actual Cargo.toml.

### 1.4 TESTING_GUIDE.md Freshness

Read `/home/cam/Documents/Status/standards/TESTING_GUIDE.md`.

**Integration test files:**
```bash
ls /home/cam/Documents/Status/crates/parapet_core/tests/ 2>/dev/null || echo "no integration tests yet"
```
Compare actual test files against §2.2.

### 1.5 RULE_OF_LAW.md Freshness

Read `/home/cam/Documents/Status/standards/RULE_OF_LAW.md`.

**Governance files exist:**
```bash
ls /home/cam/Documents/Status/doa/ 2>/dev/null || echo "MISSING: doa/"
test -f /home/cam/Documents/Status/DOCS/cleanup.md && echo "cleanup.md: EXISTS" || echo "MISSING: cleanup.md"
test -f /home/cam/Documents/Status/DOCS/futures.md && echo "futures.md: EXISTS" || echo "MISSING: futures.md"
test -f /home/cam/Documents/Status/DOCS/conflict.md && echo "conflict.md: EXISTS" || echo "MISSING: conflict.md"
```

### 1.6 CODING_STANDARDS.md Freshness

Read `/home/cam/Documents/Status/standards/CODING_STANDARDS.md`.

**rustfmt.toml (§1.3):**
```bash
cat /home/cam/Documents/Status/rustfmt.toml 2>/dev/null || echo "MISSING: rustfmt.toml"
```

**Workspace lints (§1.4):**
```bash
grep -A 5 '\[workspace.lints' /home/cam/Documents/Status/Cargo.toml
```

---

## Phase 2 — Codebase Compliance Audit

Run each check. Record violations with file path and line number.
When a check returns zero results, record it as PASS in the Appendix.

### 2.1 GTK/Display Imports in parapet_core (RULE_OF_LAW §5.3) — Severity: CRITICAL

```bash
grep -rn "gtk\|gdk\|glib\|x11" /home/cam/Documents/Status/crates/parapet_core/src/ --include="*.rs"
```
Any actual import (not in a doc comment) in `parapet_core` is a CRITICAL violation.

### 2.2 .unwrap() in Production Code (CODING_STANDARDS §3.3) — Severity: HIGH

```bash
grep -rn "\.unwrap()" /home/cam/Documents/Status/crates/ --include="*.rs"
```
For each hit, check context. Inside `#[cfg(test)]` → not a violation. All other cases → HIGH.

### 2.3 unsafe Without // SAFETY: Comment (CODING_STANDARDS §6.1) — Severity: CRITICAL

```bash
grep -rn "unsafe {" /home/cam/Documents/Status/crates/ --include="*.rs" -B 3
```
For each `unsafe {`, check whether the preceding 1–3 lines contain `// SAFETY:`.

### 2.4 println! in Library Code (CODING_STANDARDS §9.1) — Severity: MEDIUM

```bash
grep -rn "println\!" /home/cam/Documents/Status/crates/parapet_core/src/ --include="*.rs"
```
Filter out occurrences inside `#[cfg(test)]` blocks.

### 2.5 TODO/FIXME Without futures.md Reference (CODING_STANDARDS §7.3) — Severity: MEDIUM

```bash
grep -rn "TODO\|FIXME" /home/cam/Documents/Status/crates/ --include="*.rs"
```
`TODO(DOCS/futures.md#...)` format → compliant. Any other format → MEDIUM violation.

### 2.6 Missing /// Doc Comments on pub fn (CODING_STANDARDS §7.1) — Severity: MEDIUM

```bash
grep -rn "^\s*pub fn" /home/cam/Documents/Status/crates/parapet_core/src/ --include="*.rs" -B 1 | head -80
```
For each `pub fn`, check whether the preceding line contains `///`.

### 2.7 Missing //! Module-Level Comments — Severity: MEDIUM

```bash
grep -rL "^//!" /home/cam/Documents/Status/crates/parapet_core/src/ --include="mod.rs"
grep -rL "^//!" /home/cam/Documents/Status/crates/parapet_bar/src/ --include="mod.rs"
```

### 2.8 anyhow in parapet_core Public API (CODING_STANDARDS §3.2) — Severity: MEDIUM

```bash
grep -rn "anyhow::Error\b\|anyhow::Result" /home/cam/Documents/Status/crates/parapet_core/src/ --include="*.rs"
```
`anyhow` as a public return type in `parapet_core` is a violation. Internal propagation with `?` is acceptable.

### 2.9 Removed Code Tracking (RULE_OF_LAW §4.1) — Severity: HIGH

```bash
cat /home/cam/Documents/Status/DOCS/cleanup.md 2>/dev/null || echo "no cleanup.md yet"
ls /home/cam/Documents/Status/doa/ 2>/dev/null || echo "no doa/ yet"
```
For each entry in `cleanup.md`, verify the listed DOA path exists.

---

## Phase 3 — Write the Audit Report

```bash
mkdir -p /home/cam/Documents/Status/audits
```

Write the complete report to:
`/home/cam/Documents/Status/audits/YYYY-MM-DD_audit.md`

Use this structure:

```markdown
# Audit Report — YYYY-MM-DD

> Generated by `/audit`. Covers standards freshness and codebase compliance.
> Standards root: `standards/` | Severity: RULE_OF_LAW.md §8.1

## Summary Table

| Area | Status | Findings |
|------|--------|---------|
| ARCHITECTURE.md Freshness | PASS / FINDINGS | N |
| CONFIG_MODEL.md Freshness | PASS / FINDINGS | N |
| BUILD_GUIDE.md Freshness | PASS / FINDINGS | N |
| TESTING_GUIDE.md Freshness | PASS / FINDINGS | N |
| RULE_OF_LAW.md Freshness | PASS / FINDINGS | N |
| CODING_STANDARDS.md Freshness | PASS / FINDINGS | N |
| GTK in parapet_core | PASS / FINDINGS | N |
| .unwrap() in Production | PASS / FINDINGS | N |
| unsafe / SAFETY Comments | PASS / FINDINGS | N |
| println! in Library Code | PASS / FINDINGS | N |
| TODO/FIXME Policy | PASS / FINDINGS | N |
| Missing Doc Comments | PASS / FINDINGS | N |
| Error Handling (anyhow) | PASS / FINDINGS | N |
| Removed Code Tracking | PASS / FINDINGS | N |

---

## Standards Freshness Findings

### ARCHITECTURE.md
#### Finding: [Short descriptive title]
- **Severity:** critical | high | medium | low
- **Standard says:** `[exact quote]`
- **Reality:** [what actually exists]
- **Evidence:** `file:line` or bash output
- **Recommendation:** [Specific action]

[Repeat per standard. Use PASS if none.]

---

## Codebase Compliance Findings

### GTK/Display Imports in parapet_core
[Per violation: file:line, import found, CRITICAL severity]

### .unwrap() in Production Code
[Per violation: file:line, context, HIGH severity]

### unsafe Without // SAFETY:
[Per violation: file:line, CRITICAL severity]

[... remaining checks ...]

---

## Appendix: Passing Checks

| Check | Evidence |
|-------|---------|
| [Check name] | [e.g. "grep returned 0 results"] |
```

---

## Severity Reference (RULE_OF_LAW.md §8.1)

| Severity | Definition |
|----------|-----------|
| **critical** | GTK in parapet_core, unsafe without SAFETY, data loss risk |
| **high** | .unwrap() in production, standards violation, test removed |
| **medium** | Missing doc comment, wrong TODO format, naming violation |
| **low** | Minor style, optional improvement |

---

## Auditor Notes

- Prefer concrete evidence (file:line, grep output) over inference.
- If evidence is absent, record PASS — do not fabricate findings.
- Do not report the same violation twice.
- Passing checks must appear in the Appendix with brief evidence.
- Use relative paths from the project root (e.g. `crates/parapet_core/src/config.rs:42`).
