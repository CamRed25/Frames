# Research: Config Migration Infrastructure
**Date:** 2026-03-18
**Status:** Findings complete — decision recommended — ready to close

## Question

What is the right design for `config_version` + a migration chain so that users
receive a readable error (or automatic upgrade) when breaking config changes ship?

---

## Summary

Add a `config_version` integer to `[bar]`, check it in `FramesConfig::load()`,
and run an in-process migration chain before validation. The chain is a
`Vec<fn(&mut toml::Value)>` — one function per version step — applied
sequentially until the document reaches the current schema version. No new
dependencies are required. The TOML file is rewritten in-place after a
successful migration with a brief log message. Implementation lives entirely in
`frames_core` and is display-safe.

---

## Findings

### Background

`WidgetKind` (Phase 2, now complete) is the first structural change. Future
changes that constitute breaking config format changes include:

- Renaming a widget `type` key (e.g. `"vol"` → `"volume"`)
- Removing or renaming a field on a per-widget config struct
- Changing the type of a field (e.g. `interval: String` → `interval: u64`)

Without a migration path, users upgrading Frames get an opaque serde parse error
with no actionable guidance.

---

### Option A — Version integer in `[bar]` + migration chain (recommended)

#### Config surface

```toml
[bar]
config_version = 2
height = 30
```

The field is `#[serde(default)]` with a default of `1` (current schema). When
missing, the config is inferred as version 1 (the initial unversioned schema).

```rust
/// Integer version of the config file schema.
///
/// Bumped whenever a breaking change is made to the config format. Used by
/// the migration chain in [`FramesConfig::load()`] to upgrade old files
/// automatically before validation.
///
/// Current version: `CONFIG_SCHEMA_VERSION`.
#[serde(default = "BarConfig::default_config_version")]
pub config_version: u32,
```

Define a constant in `config.rs`:

```rust
/// The config schema version this build of Frames expects.
///
/// Bump this constant — and add a migration step to `MIGRATIONS` — whenever
/// a breaking change is made to the TOML config format.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
```

#### Migration chain

Store migrations as a static slice of function pointers. Each step receives the
entire TOML document as a `toml::Value` and mutates it to be compatible with the
next schema version.

```rust
/// A migration step that upgrades a TOML document from version N to N+1.
///
/// Each function mutates the `toml::Value` tree in-place. Steps are applied
/// sequentially. Index 0 upgrades from version 1 to version 2, index 1 from
/// version 2 to version 3, and so on.
type MigrationFn = fn(&mut toml::Value);

/// Ordered list of migration functions.
///
/// To add a migration from version N to N+1, append a function here and bump
/// `CONFIG_SCHEMA_VERSION`. Do not reorder or remove existing entries.
static MIGRATIONS: &[MigrationFn] = &[
    // v1 → v2: rename widget type "vol" to "volume" (example)
    migrate_v1_to_v2,
];

fn migrate_v1_to_v2(doc: &mut toml::Value) {
    if let Some(widgets) = doc.get_mut("widgets").and_then(|w| w.as_array_mut()) {
        for widget in widgets.iter_mut() {
            if widget.get("type").and_then(|t| t.as_str()) == Some("vol") {
                if let Some(t) = widget.get_mut("type") {
                    *t = toml::Value::String("volume".to_string());
                }
            }
        }
    }
}
```

#### FramesConfig::load() integration

```rust
pub fn load(path: &Path) -> Result<Self, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound { path: path.to_path_buf() });
    }
    let source = std::fs::read_to_string(path)?;

    // 1. Parse to raw toml::Value for version inspection and migration.
    let mut doc: toml::Value = toml::from_str(&source)?;

    // 2. Extract config_version from [bar].config_version (default 1).
    let file_version = doc
        .get("bar")
        .and_then(|b| b.get("config_version"))
        .and_then(|v| v.as_integer())
        .map(|i| i as u32)
        .unwrap_or(1);

    if file_version > CONFIG_SCHEMA_VERSION {
        return Err(ConfigError::Validation {
            field: "bar.config_version".into(),
            reason: format!(
                "config version {} is newer than this build supports ({}); \
                 upgrade Frames or downgrade your config",
                file_version, CONFIG_SCHEMA_VERSION
            ),
        });
    }

    // 3. Apply migration chain.
    let mut migrated = false;
    for step_idx in (file_version as usize - 1)..MIGRATIONS.len() {
        MIGRATIONS[step_idx](&mut doc);
        migrated = true;
        tracing::info!(
            "config migrated: v{} → v{}",
            file_version as usize + (step_idx - (file_version as usize - 1)),
            file_version as usize + (step_idx - (file_version as usize - 1)) + 1,
        );
    }

    // 4. If migrations were applied, update the version field and rewrite the file.
    if migrated {
        if let Some(bar) = doc.get_mut("bar").and_then(|b| b.as_table_mut()) {
            bar.insert(
                "config_version".into(),
                toml::Value::Integer(i64::from(CONFIG_SCHEMA_VERSION)),
            );
        }
        let new_source = toml::to_string_pretty(&doc)?;
        std::fs::write(path, &new_source)?;
        tracing::info!(
            path = %path.display(),
            "config file rewritten after migration to v{}", CONFIG_SCHEMA_VERSION
        );
    }

    // 5. Deserialize the (possibly migrated) document into FramesConfig.
    let mut config: Self = toml::from_str(&toml::to_string(&doc)?)?;
    config.validate()?;
    Ok(config)
}
```

**Pros:**
- Zero new dependencies (`toml::Value` is already available via the existing `toml`
  dep; `toml::to_string_pretty` is available via `toml = "~0.8"` with the
  `display` feature, which is already enabled through `serde` usage)
- Migration steps are pure functions on a TOML document — no Rust struct versioning
  required
- Readable error message when file version > binary version
- Automatic rewrite means users only see migration warnings in logs; the bar
  continues to start normally
- Round-trips correctly: after rewrite, `config_version` in the file matches
  `CONFIG_SCHEMA_VERSION`

**Cons:**
- File rewrite on startup (for migrated configs only) creates a brief I/O
  reservation — negligible in practice
- Rewriting the file loses any non-standard comments/formatting from the user's
  original file (TOML round-trips are not comment-preserving). This is an
  expected trade-off.
- Migration functions must be kept around forever (unlike dead code that can be
  moved to `doa/`) because they are required for progressive upgrades. Store them
  in a `migrations.rs` submodule to avoid cluttering `config.rs`.

---

### Option B — Refuse to start with a version mismatch + printed instructions

Simpler (no rewrite), but forces users to manually fix their config with no
tooling. Not acceptable for a breaking change that is purely mechanical (field
rename).

---

### Option C — Semver string field (`config_version = "1.0.0"`)

Adds complexity without benefit. Integer version is sufficient since Frames
controls the only schema; semver is only useful when there are third parties
implementing the format. Rejected.

---

## Recommendation

**Option A.** The implementation is contained to `frames_core/src/config.rs`
(plus a new sibling `frames_core/src/migrations.rs`). No new dependencies.
Current schema starts at `CONFIG_SCHEMA_VERSION = 1`. The `MIGRATIONS` slice is
empty at first; the infrastructure merely validates the version field and
produces a readable error. The first real migration function is added when Phase
3 introduces the first breaking field change.

### Implementation steps

1. Add `config_version: u32` to `BarConfig` with `#[serde(default = "...")]`
   returning `1`.
2. Add `pub const CONFIG_SCHEMA_VERSION: u32 = 1` to `config.rs`.
3. Create `crates/frames_core/src/migrations.rs` with an empty
   `pub static MIGRATIONS: &[MigrationFn] = &[]` and an explanatory comment.
4. Refactor `FramesConfig::load()` as shown above.
5. Extend `ConfigError` with a `ConfigTooNew` variant for the "file version >
   binary version" case (distinct from `Validation`, giving callers a typed
   handle).
6. Add round-trip tests:
   - `config_version_missing_defaults_to_1`
   - `config_version_newer_than_binary_returns_err`
   - Empty MIGRATIONS slice does not mutate the document.

---

## Standards Conflict / Proposed Update

`CONFIG_MODEL.md §4.1` (`[bar]` fields table) should gain a row for
`config_version: integer (default 1, auto-managed)` once this is implemented.

`ARCHITECTURE.md §4.1` should note that `FramesConfig::load` runs the migration
chain before deserialization.

---

## Sources

- `toml` crate 0.8 docs — `toml::Value`, `to_string_pretty`: confirmed available
  without additional features
- `frames_core/src/config.rs`: current `load()` and `validate()` structure
- `DOCS/research/config-overhaul.md §Problem 5`: original discovery of this need
- `DOCS/futures.md`: "Config migration infrastructure" entry that prompted this research

## Open Questions

1. Should the file rewrite be opt-in (`--auto-migrate` flag) rather than
   automatic? Automatic is safer for users but loses comments. A log line at
   `INFO` level plus a backup copy (`config.toml.bak`) before rewrite would
   mitigate this.
2. When `MIGRATIONS` is empty and `CONFIG_SCHEMA_VERSION = 1`, a file with no
   `config_version` field silently passes. The first real migration will confirm
   the round-trip behaviour works as intended.
