# Research: Config System Overhaul
**Date:** 2026-03-18
**Status:** Findings complete — decision pending

## Question

What concrete improvements can be made to the Frames config system — structure, validation, usability, and tooling — and what is the right phased approach?

---

## Summary

The current config system works but has four meaningful weaknesses: a flat "opt-in everything" `WidgetConfig` struct that gives no type safety, shallow validation that misses many user errors, no schema for editor tooling, and no way to scaffold a starter config. All four can be fixed incrementally without breaking existing configs. The highest-value single change is switching to a **typed per-widget enum** (`WidgetKind`) combined with **`schemars`-generated JSON Schema** — this eliminates the entire class of "wrong field on wrong widget" bugs and enables VS Code autocomplete for free.

---

## Findings

### Problem 1 — The Flat WidgetConfig Sprawl

Every widget shares one struct with dozens of `Option<T>` fields:

```rust
pub struct WidgetConfig {
    pub widget_type: String,
    pub position: BarSection,
    pub latitude: Option<f64>,   // only used by weather
    pub longitude: Option<f64>,  // only used by weather
    pub mount: Option<String>,   // only used by disk
    pub interface: Option<String>, // only used by network
    // ... 15 more optional fields
}
```

Problems:
- A user writing `latitude = 40.7` on a CPU widget gets no error — it silently does nothing
- Adding a new widget means adding more optional fields to a struct that already knows too much
- The struct grows unboundedly with each new widget type

**Option A — Typed `WidgetKind` enum (recommended)**

Replace the flat struct with a tagged union. Each widget gets its own config type:

```toml
# TOML stays the same — TOML arrays of tables with type field are the natural representation
[[widgets]]
type = "weather"
position = "right"
latitude = 40.7
longitude = -74.0
units = "fahrenheit"
```

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WidgetKind {
    Clock(ClockConfig),
    Cpu(CpuConfig),
    Memory(MemoryConfig),
    Network(NetworkConfig),
    Battery(BatteryConfig),
    Disk(DiskConfig),
    Volume(VolumeConfig),
    Brightness(BrightnessConfig),
    Weather(WeatherConfig),
    Media(MediaConfig),
    Workspaces(WorkspacesConfig),
    Launcher(LauncherConfig),
    Separator(SeparatorConfig),
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "default_units")]
    pub units: TempUnit,
}
```

`WidgetConfig` becomes:

```rust
pub struct WidgetConfig {
    #[serde(flatten)]
    pub kind: WidgetKind,          // ← carries all widget-specific fields
    pub position: BarSection,
    pub interval: Option<u64>,
    pub label: Option<String>,
    pub on_click: Option<String>,
    pub on_scroll_up: Option<String>,
    pub on_scroll_down: Option<String>,
    pub extra_class: Option<String>,
}
```

Benefits:
- `latitude` on a CPU widget is now a hard deserialization error, not a silent no-op
- Each widget config type can have non-optional required fields (e.g. `latitude: f64`, not `latitude: Option<f64>`)
- Adding a new widget requires a new `FooConfig` struct, not touching the shared struct
- Serde's `#[serde(deny_unknown_fields)]` can be applied per-widget type to catch typos

Caveats:
- `main.rs` widget construction dispatch changes from `match config.widget_type.as_str()` to `match &config.kind`
- All existing accessor code must be updated (one-time migration, mechanical)
- This is a **breaking config change** only if field names change — the TOML surface is identical since `serde(tag = "type")` uses the existing `type` field as the discriminant

**Option B — Keep flat struct, add `deny_unknown_fields` per widget via custom Deserialize**

Feasible but high implementation cost for marginal gain. Skips the type-safety benefit. Not recommended.

---

### Problem 2 — Shallow Validation

Current validation catches:
- `bar.height == 0`
- CPU `warn >= crit`
- Battery `crit >= warn`

It misses:
- Unknown `widget_type` strings (e.g. `type = "clcock"` silently creates nothing, logs a WARN at runtime)
- `latitude` / `longitude` out of WGS-84 range (±90 / ±180)
- `interval` of 0 (causes tight busy-loop in the poller)
- `mount` that isn't an absolute path
- `warn_threshold` / `crit_threshold` outside 0–100

**Option A — Extend `FramesConfig::validate()` (recommended, easy wins)**

Add these checks directly in the existing `validate()` method:

```rust
// Reject zero intervals
if w.interval == Some(0) {
    return Err(ConfigError::Validation {
        field: format!("widgets[{i}].interval"),
        reason: "must be > 0".into(),
    });
}
// Validate GPS coords for weather
if w.widget_type == "weather" {
    let lat = w.latitude.unwrap_or(0.0);
    if !(-90.0..=90.0).contains(&lat) {
        return Err(...);
    }
}
```

With the typed `WidgetKind` approach (Problem 1), validation becomes even cleaner because each variant carries only its own fields.

**Option B — `validator` crate with `#[validate]` derive macros**

The `validator` crate (MIT, 6.4M downloads) adds `#[validate(range(min = -90.0, max = 90.0))]` annotations to struct fields. Integrates with serde. Adds a dependency. The existing hand-written validation is only ~40 lines and covers all current cases — the marginal benefit doesn't justify the new dependency at this stage.

---

### Problem 3 — No Editor Schema / Autocomplete

Currently a user editing `config.toml` in VS Code has no autocomplete, no hover docs, and no inline error detection.

**Option A — `schemars` + `taplo` (recommended)**

`schemars = "~0.8"` (MIT, 12.7M downloads) derives JSON Schema from Rust types via `#[derive(JsonSchema)]`. With the typed `WidgetKind` enum, the generated schema would correctly show which fields are available for each widget type.

Steps:
1. Add `schemars` to `frames_core` (no display dependency — it's a derive macro)
2. `#[derive(JsonSchema)]` on `FramesConfig`, `BarConfig`, `WidgetConfig`, and all per-widget config types
3. Add a `frames_bar --dump-schema` subcommand that writes the schema to stdout
4. Users drop the schema in their `.vscode/` folder and add this to `settings.json`:
   ```json
   "evenBetterToml.schema.associations": {
     ".*/frames/config.toml": "./frames-config.schema.json"
   }
   ```
   Or use `taplo`'s inline schema association comment in the config:
   ```toml
   #:schema https://example.com/frames-config.schema.json
   ```

This gives:
- Autocomplete for field names and values
- Hover documentation
- Inline error squiggles for unknown fields or wrong types
- Free — no runtime cost, schema generation is a compile-time derive

`taplo` (MIT) is the TOML language server used by the "Even Better TOML" VS Code extension — no installation step beyond enabling the existing extension. The `taplo-cli` binary can also validate a config file from the command line: `taplo check --schema frames.schema.json ~/.config/frames/config.toml`.

**Option B — Manually document the config as JSON Schema YAML**

Tedious to keep in sync, error-prone. Not recommended.

---

### Problem 4 — No Starter Config / Scaffolding

New users must write a config file from scratch or copy the example in `CONFIG_MODEL.md §6`.

**Option A — `frames_bar --init-config` subcommand (recommended)**

Writes a well-commented starter config to `~/.config/frames/config.toml` (or the path in `FRAMES_CONFIG`) if the file does not exist. Exits with an error if the file already exists to prevent accidental overwrite.

The config written is the same as the `CONFIG_MODEL.md §6` example, rendered from a `const` string baked into the binary. No external file required.

```bash
frames_bar --init-config
# → Wrote config to /home/cam/.config/frames/config.toml
```

Low implementation cost: ~20 lines in `main.rs`, no new dependencies.

**Option B — Ship a `config.toml.example` file**

Already partially done via the docs. Does not help users who don't read the README first.

---

### Problem 5 — No Config Migration Infrastructure

The `Config Migration` agent exists but there is no runtime migration path. When `WIDGET_API_VERSION` bumps with a breaking config change, users get a parse error with no guidance.

**Option A — Version field + migration runner (future, not v0.1.0)**

Add `config_version = "1"` to the `[bar]` section. On load, check the version. If it is older than current, attempt a migration chain (a `Vec<Box<dyn MigrationStep>>` applied in sequence). Write the migrated config back and continue.

This is non-trivial and belongs in a separate plan. File in `futures.md`.

---

### Problem 6 — Environment Variable Expansion (Minor)

Only `~` is expanded in `bar.css`. The `FRAMES_CONFIG` env var override works, but inside the config file itself, `$HOME` inside a path is not expanded.

**Option A — Expand `$HOME` / `~` in all path-type string fields at validation time**

Add a `expand_path()` helper that handles both `~` and `$HOME`. Applied in `validate()` to `bar.css`, `bar.theme`, and any future path fields. Pure Rust, zero new dependencies.

---

## Recommendation

Do this in two phases:

### Phase 1 — High value, low risk (v0.1.x)

| Item | Effort | Value |
|------|--------|-------|
| Extend `validate()` with zero-interval, GPS range, and threshold range checks | Low | High |
| Add `frames_bar --init-config` subcommand | Low | High |
| Expand `$HOME` / `~` in all path fields | Low | Medium |
| Add `schemars` derives + `--dump-schema` subcommand | Medium | High |

Phase 1 requires no structural breaking changes and improves UX immediately.

### Phase 2 — Structural (v0.2.0)

| Item | Effort | Value |
|------|--------|-------|
| Migrate from flat `WidgetConfig` to typed `WidgetKind` enum | Medium | Very High |
| Wire `deny_unknown_fields` on per-widget configs | Low (after Phase 2 struct change) | High |
| Config migration infrastructure (`config_version` + migration chain) | High | Medium |

Phase 2 is a breaking change to internal Rust types (not the TOML surface) and should be versioned.

The single most impactful change is the **`WidgetKind` enum** (Phase 2) because it eliminates an entire class of silent misconfiguration bugs and makes the codebase extensible without a growing flat struct. But Phase 1's validation and `--init-config` are shippable today with minimal risk.

---

## Standards Conflict / Proposed Update

`CONFIG_MODEL.md §5` (Validation Rules) should be extended with the Phase 1 validation additions once implemented (zero interval, GPS range, absolute path check for `mount`).

`ARCHITECTURE.md §4.4` should be updated in Phase 2 to document the `WidgetKind` enum as the config dispatch mechanism, replacing the `match config.widget_type.as_str()` pattern.

---

## Sources

- [`schemars` crate](https://docs.rs/schemars): JSON Schema derive macro — what it contributed: schema generation approach
- [`taplo` TOML tooling](https://taplo.tamasfe.dev): TOML LSP and CLI validator — what it contributed: editor integration strategy
- [`validator` crate](https://docs.rs/validator): field-level validation derives — considered and deferred (unnecessary dep for current scale)
- [`figment` crate](https://docs.rs/figment): layered config (file + env + CLI) — considered as full config replacement; overkill for current single-file model; worth revisiting if Frames gains a CLI-override requirement
- Serde documentation on `#[serde(tag = "type")]` internally-tagged enums: confirms the discriminant field (`type`) survives round-trip through TOML

---

## Open Questions

1. Should the `WidgetKind` typed enum be a Phase 2 internal refactor only, or should it be done as part of a larger v0.2.0 config schema stabilisation? Affects whether the existing TOML format is considered "stable" before migration.
2. Should `--dump-schema` write a versioned schema URL (e.g. `https://github.com/CamRed25/Frames/releases/download/v0.2.0/config.schema.json`) so users can reference a pinned URL instead of generating locally?
