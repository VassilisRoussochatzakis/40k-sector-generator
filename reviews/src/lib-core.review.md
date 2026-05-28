---
unit_id: U003
crate: sectorforge
paths:
  - src/lib.rs
  - src/worlds.rs
  - src/worlds_toml.rs
loc_reviewed: 2542
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 7, low: 7, nit: 5 }
top_risks:
  - "Public surface re-exports modules + types without #[non_exhaustive] on growable error/taxonomy enums (F-003-001, F-003-003)"
  - "Display impls fall through to Debug, coupling user-facing strings to Rust variant identifiers (F-003-002)"
  - "Taxonomy enums (WorldType, Atmosphere, …) are unit-only but not Copy, forcing 50+ .clone() sites workspace-wide (F-003-004)"
---

# Review: U003 — lib facade & worlds taxonomy

## Summary

`src/lib.rs` is a thin, mostly mechanical facade: parent-module re-exports and 30+
one-shot helpers that wrap a `module::function` call with disk I/O and a
`SectorError` mapping. The shape is consistent and well-documented; the main
risks are **public-API surface obligations** — every `pub use` is now a semver
edge — and a few hot pieces of leaked private layout (the bare `pub mod
worlds_toml`, the `pub(crate)` Fx aliases vs. the `pub use crate::FxMap`
pattern emerging elsewhere).

`src/worlds.rs` is the authoritative taxonomy. The variant set, `VARIANTS`
arrays, and `display_name`/`FromStr` round-trip are solid and unit-only. The
biggest cost is structural: the 9 string-payload enums are not `Copy`, which
propagates `.clone()` through `src/gen/world_pool.rs`,
`src/analysis/analytics.rs`, etc. The `Display` impls also fall through to
`Debug`, which silently couples the *type identifier* to user-visible strings
in a few sites (`analysis/search.rs:725`, `gen/generation/world_placement.rs`).

`src/worlds_toml.rs` is the smallest and cleanest of the three; the
`from_str` inherent (rather than `FromStr`) plus the `format!("{v:?}") == s`
matcher for resolving map keys are the two real findings.

No determinism violations were found: the Fx aliases are crate-private,
`worlds.rs` uses `HashMap` only for *lookup* tables (`KeyTables`), and
`worlds_toml.rs` uses `BTreeMap` for the (de)serialized feature pool. No
direct RNG draws.

No panics on user input were found — the only `unwrap`/`expect` are in
`#[cfg(test)]` blocks.

## Findings

### F-003-001 — [HIGH] [API design] Public re-exports are not `#[non_exhaustive]`-protected; one variant addition breaks downstream `match`
- **Location:** `src/lib.rs:143` (`pub use errors::SectorError`), `src/model/errors.rs:5`; same applies to every other re-exported enum (`HistoryEventRule`, `RelationAttitude`, `Stance`, `TreatyStatus`, `RouteLineMode`, `LabelDensity`, `LegendStyle`, `SymbolSet`, `FactionMode`, `BorderOrientation`, `ControlDenominator`, `SubsectorBuildError`, `RegionConditionKind`, `ValidationIssue`, `HistoryConsequenceKind`, `HistoryEntityKind`, `RelationsConfig`/etc.). The CLAUDE.md rubric §3.4 makes `#[non_exhaustive]` mandatory on "growable" public enums.
- **Category:** API design / semver
- **Confidence:** High
- **Blast radius:** Every downstream crate (`builder`, `viewer`, `gui-core`, plus any external consumer once published) sees `non_exhaustive_omitted_patterns` if they `match` on a re-exported enum. Adding any new error kind (e.g. when relations files are loaded) becomes a breaking change.
- **Problem:** `SectorError` and several auxiliary enums are public, plain enums. They are *guaranteed* to grow (the crate is under active spec expansion — `§14 NEW.md`, `§5 NEW2.md`, etc., all added new error and report kinds over time). The downstream `viewer/src/app/lifecycle.rs:244` already matches `SectorError::GenerationCancelled` non-exhaustively (using `_`), which is the right pattern, but the type itself does not enforce it.
- **Evidence:** `src/model/errors.rs` lines 5 and 43 — no `#[non_exhaustive]` attribute. `src/lib.rs:143` re-exports it. Reading the broader crate, `Stance`, `TreatyStatus`, `RelationAttitude` (in `relations.rs`) are similarly bare.
- **Suggested fix:** Add `#[non_exhaustive]` to `SectorError` and `MutationError` first (foundational error path), then sweep across each enum re-exported from `src/lib.rs`. Concretely in `src/model/errors.rs`:
  ```rust
  #[derive(Debug, Error)]
  #[non_exhaustive]
  pub enum SectorError { ... }

  #[derive(Debug, Error, Clone, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  #[non_exhaustive]
  pub enum MutationError { ... }
  ```
  Then update construction sites that use struct-variant syntax (most already do — `SectorError::Io { path, source }`) — the `#[non_exhaustive]` only restricts *external* `match` and construction. Internal construction is unaffected.
- **Effort:** S (for `errors.rs` itself; M if applied workspace-wide as one pass).
- **Risk of fix:** Low — semver-safe addition. Will surface (compile-time) any downstream `match` that omitted `_`.

### F-003-002 — [HIGH] [API design] `Display` impls of public taxonomy enums fall through to `Debug`; user-visible strings are coupled to Rust identifiers
- **Location:** `src/worlds.rs:485-489`, `507-511`, `527-531`, `548-552`, `569-573`, `590-594`, `635-639`, `857-861` (all the `impl Display` blocks for the 8 enums) — every one is the same `write!(f, "{self:?}")`.
- **Category:** API design / correctness / public contract
- **Confidence:** High
- **Blast radius:** Anywhere a taxonomy enum is interpolated with `{}` rather than `display_name()`. Search across `src/` shows at least:
  - `src/analysis/search.rs:725-726` — produces user-facing report strings like `"world_type_exists(HiveWorld)"` rather than `"world_type_exists(Hive World)"`.
  - `src/gen/generation/world_placement.rs:296-302` — `world.world_type.to_string()` is converted via `snake_case` so the *output* tag `world_type:hive_world` happens to be stable, but it depends on `Debug == identifier` and would silently corrupt golden output if any variant is ever renamed.
- **Problem:** Two parallel canonical strings exist — `display_name()` returns `"Hive World"` (the human-readable, FromStr-round-trippable form), and `Display` returns `"HiveWorld"` (the Rust identifier). Callers cannot tell which they're getting without inspecting the trait impl. Worse, the `Debug` derive output is *not stable across `derive`-driven changes* — rename a variant and golden tests + report files churn.
- **Evidence:** `src/worlds.rs:487` `write!(f, "{self:?}")` — same pattern repeated for every enum. Compare with the carefully maintained `display_name` tables at lines 911-1321.
- **Suggested fix:** Replace each `Display` impl with a thin wrapper over `display_name`:
  ```rust
  impl std::fmt::Display for WorldType {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          f.write_str(self.display_name())
      }
  }
  ```
  Then `world_placement.rs:296` becomes `format!("world_type:{}", snake(world.world_type.display_name()))` — still stable, but no longer depends on `Debug`. Run `cargo test --test it -- golden` after, since `analysis/search.rs` user labels will change (they were arguably wrong before).
- **Effort:** S
- **Risk of fix:** Medium — golden output for at least `search.md` (constraint labels) will shift; this is the *correct* shift but the test will need a re-bless. Worth doing once now rather than letting more golden output accrue against `Debug`.

### F-003-003 — [HIGH] [API design] `pub mod worlds_toml` exposes the *entire* serde wire layout; no `#[non_exhaustive]` on the structs that downstream code constructs
- **Location:** `src/lib.rs:49` (`pub mod worlds_toml;`), `src/worlds_toml.rs:70-96` (`WorldsConfig`, `FeaturePoolConfig`, `WeightedFeatureEntry`).
- **Category:** API design / semver
- **Confidence:** High
- **Blast radius:** `builder/src/builder/data_catalogs.rs:17`, `builder/src/builder/project_io.rs:644`, `builder/src/builder/panels/generation.rs:556`, `viewer/src/data_editor.rs:16` all construct or pattern-match on these. Adding *any* field requires a synchronized change across four files.
- **Problem:** `WorldsConfig`, `FeaturePoolConfig`, and `WeightedFeatureEntry` are `pub struct` with all-`pub` fields. Adding a new feature axis (e.g. `by_government`, which would be a natural extension given `§45 WD3`) is currently a breaking change because every construction site (`WorldsConfig { generation, features }`) must be updated. The `Default` derive lets us claim "use `..Default::default()`", but downstream callers don't, and there is no compile-time hint they should.
- **Evidence:** `src/worlds_toml.rs:70` lacks `#[non_exhaustive]`. The test in the same file (line 187) constructs the struct positionally, demonstrating the brittle pattern.
- **Suggested fix:** Add `#[non_exhaustive]` to all three structs, and provide a builder or `with_features` setter for downstream construction:
  ```rust
  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  #[non_exhaustive]
  pub struct WorldsConfig { pub generation: Vec<GenerationRow>, pub features: FeaturePoolConfig }

  impl WorldsConfig {
      pub fn new(generation: Vec<GenerationRow>, features: FeaturePoolConfig) -> Self {
          Self { generation, features }
      }
  }
  ```
  Then audit `builder/`, `viewer/` for construction sites and convert to `WorldsConfig::new(...)` or `..WorldsConfig::default()`.
- **Effort:** M
- **Risk of fix:** Medium — touches 4 downstream files (visible via `grep "WorldsConfig {"`).

### F-003-004 — [HIGH] [Performance / Ownership] 8 taxonomy enums are unit-only but not `Copy`; ~50 `.clone()` sites workspace-wide are forced
- **Location:** `src/worlds.rs:56` (`WorldType`), `84` (`Atmosphere`), `95` (`Temperature`), `104` (`Biosphere`), `114` (`Population`), `124` (`TechLevel`), `134` (`Government`), `168` (`NotableFeature`). All derive `Clone` but not `Copy`. Compare with `StarColour` (line 19) which is `Copy`.
- **Category:** Performance (allocation-free) / Ownership / API
- **Confidence:** High
- **Blast radius:** Once-per-build (taxonomy) — but the *clones* propagate into hot paths:
  - `src/gen/world_pool.rs:33-39` — 7 enum clones per candidate built (cold but on every regen).
  - `src/gen/world_pool.rs:185, 197` — clones inside the pool build loop.
  - `src/analysis/analytics.rs:190, 199` — clones per world during *analysis* (cold but per-export).
  - `src/export/subsectors/summary.rs:217, 221, 229` — clones per world per subsector summary (also cold-ish).
  - `src/gen/generation/world_placement.rs:79` — clone inside a hot placement loop.
- **Problem:** Each of these enums is a single-byte tag enum (`#[repr]` is default but all variants are payload-free). They satisfy every `Copy` requirement; `Copy` was clearly omitted by oversight. Removing the manual `.clone()` calls is a free idiomatic win plus marginal perf.
- **Evidence:** `git grep "world_type.clone\|atmosphere.clone\|temperature.clone\|biosphere.clone\|population.clone\|government.clone\|tech_level.clone\|notable_feature.clone"` in `src/` returns 16 sites; `notable_features` (the `Vec<NotableFeature>`) drives further allocation that drops to `Copy` once the inner type is `Copy`.
- **Suggested fix:** Add `Copy` to the derive:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
  pub enum WorldType { ... }
  ```
  Same for `Atmosphere`, `Temperature`, `Biosphere`, `Population`, `TechLevel`, `Government`, `NotableFeature`. Several derives lack `PartialOrd`/`Ord` (only `StarColour`, `WorldType`, `NotableFeature` have it) — consider adding to all eight while editing, so they can sort deterministically when emitted. After that, delete the now-redundant `.clone()` calls in `gen/world_pool.rs`, `analysis/analytics.rs`, `export/subsectors/summary.rs`, `gen/generation/world_placement.rs`. `worlds_toml.rs:133, 141` (`v.clone()`) and `worlds.rs:1335-1357` (the `KeyTables::from_enums` body) also simplify.
- **Effort:** S (deriving) / M (workspace cleanup pass).
- **Risk of fix:** Low — `Copy` is purely additive, no semver hazard. Cleanup pass is mechanical.

### F-003-005 — [MEDIUM] [API design / Errors] `FromStr` impls return `Err = ()`, losing the bad input string entirely
- **Location:** `src/worlds.rs:384` (`StarColour`), `453`, `492`, `514`, `534`, `555`, `576`, `597`, `642` (every `impl FromStr` block uses `type Err = ();`).
- **Category:** Error handling / API design
- **Confidence:** High
- **Blast radius:** Any future caller that does `s.parse::<WorldType>()` will get back `Err(())` — no context to log, no surface to wrap. Today there are no callers (`grep -rn "FromStr\|.parse::<.*WorldType" src/ builder/ viewer/ gui-core/` returns only the impls themselves), so this is latent — but the `FromStr` impls are *public API* and presented as the primary way to deserialize variant names alongside `display_name`.
- **Problem:** `()` is one of the standard "don't do this" smells: callers can't display, log, or wrap the error. The TOML wire format already has a structured error (`WorldsTomlError::BadVariant { kind, value }`), and the resolver in `worlds_toml.rs:128-141` is forced to wrap `parse_world_type_variant` with an `ok_or_else` instead of using `FromStr`.
- **Evidence:** `src/worlds.rs:384` — `type Err = ();`. Same pattern in 8 more places.
- **Suggested fix:** Either:
  1. **(Preferred)** Introduce a small structured error and use it consistently:
     ```rust
     #[derive(Debug, Clone, thiserror::Error)]
     #[error("unknown {kind}: {value:?}")]
     pub struct UnknownVariant { pub kind: &'static str, pub value: String }

     impl std::str::FromStr for WorldType {
         type Err = UnknownVariant;
         fn from_str(s: &str) -> Result<Self, Self::Err> {
             /* existing match */ ;
             Err(UnknownVariant { kind: "WorldType", value: s.to_owned() })
         }
     }
     ```
     Then `parse_world_type_variant`/`parse_star_colour_variant` in `worlds_toml.rs:166-179` collapse to `s.parse().ok()` (or propagate the error directly).
  2. Or, if "I just want the boolean" is the only use case, document it on the impl and leave it.
- **Effort:** M
- **Risk of fix:** Low — additive on the error type; `FromStr` is currently unused outside the impls.

### F-003-006 — [MEDIUM] [API design / Dead code] `worlds::WorldEntry` and `worlds::System` are public but never referenced
- **Location:** `src/worlds.rs:331-356`.
- **Category:** Dead code / API surface bloat
- **Confidence:** High
- **Blast radius:** Once-per-build (compile time, doc clutter).
- **Problem:** `grep -rn "worlds::System\|worlds::WorldEntry"` returns *zero* hits in `src/`, `builder/`, `viewer/`, `gui-core/`, `tests/`. The crate's actual system/world types live in `src/model/sector_model/` (`GeneratedSystem`, `GeneratedWorld`). These struct definitions add ~25 lines, three more `Debug, Clone` impls per `cargo doc`, and a public-API obligation. They also have a `pub system_seed: Option<f64>` field that looks like residual sheet-derived data (`col L in template`).
- **Evidence:** `src/worlds.rs:331-356`; no consumers found in the workspace.
- **Suggested fix:** Delete both types and the `system_index`/`system_seed`/`location_name` `Option<String>` fields. If they were retained for future API parity, mark with `#[deprecated(note = "...")]` and a clear migration path to `GeneratedSystem`/`GeneratedWorld`. Run `cargo check --workspace` to confirm no users.
- **Effort:** S
- **Risk of fix:** Low.

### F-003-007 — [MEDIUM] [Performance / Ownership] `generate_system_standalone` clones the system twice for a one-element factions pass
- **Location:** `src/lib.rs:323-340`.
- **Category:** Ownership / Performance
- **Confidence:** High
- **Blast radius:** Once-per-call (cold — single-system standalone generation; not in the sector pipeline). Still gratuitous.
- **Problem:** The function does:
  ```rust
  let mut sys = generation::build_system(...)?;     // owned
  let mut single = [sys.clone()];                    // CLONE #1
  generation::assign_factions_for_systems(&mut single, ...);
  sys = single[0].clone();                           // CLONE #2
  Ok(sys)
  ```
  The first clone is needed only because `single` is created before `sys` is consumed; the second is a slice-`Clone` to avoid moving out of a fixed-size array. Both are avoidable.
- **Evidence:** `src/lib.rs:324-340`.
- **Suggested fix:** Wrap the single system in an array directly and own it through to the end:
  ```rust
  let sys = generation::build_system(...)?;
  let mut single = [sys];                            // move, no clone
  generation::assign_factions_for_systems(&mut single, ...);
  let [sys] = single;                                // destructure (or use into_iter().next().unwrap())
  Ok(sys)
  ```
  Bonus: this also fixes the clippy `needless_pass_by_value` on `project: ProjectInput` (clippy.txt:17608) — if callers don't actually need `project` to be owned, take `&ProjectInput` and remove all `&project.x` borrows from the body; if they do, the body should consume it.
- **Effort:** XS
- **Risk of fix:** Low — same observable behaviour. `[sys]` destructure works since Rust 1.42.

### F-003-008 — [MEDIUM] [API design] `pub mod worlds_toml` should re-export selectively rather than exposing the whole module
- **Location:** `src/lib.rs:49` (`pub mod worlds_toml;`).
- **Category:** API design / encapsulation
- **Confidence:** Medium
- **Blast radius:** Every internal helper (`parse_world_type_variant`, `parse_star_colour_variant`) becomes *named* in `cargo doc` even though they're `fn` (currently private — OK), and the test module module name is exposed. More importantly, the *structural shape* of `WorldsConfig` becomes part of the public commitment.
- **Problem:** Unlike the rest of `lib.rs`, which carefully re-exports specific items (`pub use config::AppConfig;`), `worlds` and `worlds_toml` are exposed as full `pub mod`. Downstream then writes `sectorforge::worlds_toml::WorldsConfig` — a strictly worse import than a top-level `sectorforge::WorldsConfig`.
- **Evidence:** `src/lib.rs:49`; compare to `lib.rs:79-83` (`pub use loading::config`) etc.
- **Suggested fix:** Either:
  - Apply the same `pub use worlds_toml::{WorldsConfig, WeightedFeatureEntry, FeaturePoolConfig, ResolvedFeaturePool, WorldsTomlError, DEFAULT_FILENAME};` pattern and switch `pub mod` to `mod` (or keep `pub mod` if direct paths are still desired); or
  - Document that `worlds_toml::*` *is* the public surface and accept the obligation.
  The first option is consistent with the established `pub use parent::child` pattern documented in `lib.rs:26-30`.
- **Effort:** S
- **Risk of fix:** Low — re-exports are additive; converting `pub mod` to `mod` would force one round of `cargo fix` across builder/viewer to drop the `worlds_toml::` segment.

### F-003-009 — [MEDIUM] [API design] `worlds::WorldsLoad.authored_features` leaks `worlds_toml::ResolvedFeaturePool` across the module boundary
- **Location:** `src/worlds.rs:411-415`.
- **Category:** API design / encapsulation
- **Confidence:** Medium
- **Blast radius:** Public API; pins `WorldsLoad` to whatever `worlds_toml` chooses to make `ResolvedFeaturePool`. Downstream `src/loading/input.rs:24` already mirrors this leak.
- **Problem:** `pub struct WorldsLoad { pub authored_features: Option<crate::worlds_toml::ResolvedFeaturePool> }` makes `worlds.rs` depend on `worlds_toml` — which is fine — but it also leaks the *exact wire-shaped pool type* into a struct that's notionally format-agnostic. The original intent of `worlds.rs` (per the file-header doc) is that it's the canonical taxonomy; `worlds_toml` is a *concrete loader*. Re-introducing the loader type at this layer means a second TOML-independent loader can't reuse `WorldsLoad`.
- **Evidence:** `src/worlds.rs:411`, `src/worlds_toml.rs:153` (`ResolvedFeaturePool`), `src/loading/input.rs:24`.
- **Suggested fix:** Move `ResolvedFeaturePool` (or an equivalent format-neutral `AuthoredFeaturePool`) into `worlds.rs` itself; have `worlds_toml.rs` build it from the TOML config. The `WeightedFeatureEntry` likewise belongs alongside the taxonomy. Then `worlds_toml` depends on `worlds` (already does) but not vice-versa.
- **Effort:** M
- **Risk of fix:** Medium — touches `loading/input.rs`, `gen/world_pool.rs:235`, builder/viewer through re-exports.

### F-003-010 — [MEDIUM] [API design] `WorldsConfig::from_str` is inherent rather than implementing `FromStr`
- **Location:** `src/worlds_toml.rs:107` (`#[allow(clippy::should_implement_trait)] pub fn from_str(...)`).
- **Category:** API design
- **Confidence:** High
- **Blast radius:** Low (one-shot loader) but the `#[allow(clippy::should_implement_trait)]` makes the smell official.
- **Problem:** `from_str` is the conventional `FromStr` method name; calling it on the inherent impl shadows the trait and forces callers to write `WorldsConfig::from_str(text)` instead of `text.parse()`. The clippy suppression admits this.
- **Evidence:** `src/worlds_toml.rs:106-109`.
- **Suggested fix:** Either rename the inherent fn (`fn parse_toml(text: &str)` / `fn from_toml(text: &str)`) or actually implement `FromStr`:
  ```rust
  impl std::str::FromStr for WorldsConfig {
      type Err = WorldsTomlError;
      fn from_str(text: &str) -> Result<Self, Self::Err> {
          toml::from_str(text).map_err(|e| WorldsTomlError::Parse(e.to_string()))
      }
  }
  ```
  Then drop the `clippy::should_implement_trait` allow. Update `builder/src/builder/project_io.rs:653, 845` accordingly.
- **Effort:** S
- **Risk of fix:** Low.

### F-003-011 — [MEDIUM] [Performance / API] `parse_world_type_variant` / `parse_star_colour_variant` allocate a `String` per probe via `format!("{v:?}")`
- **Location:** `src/worlds_toml.rs:166-179`.
- **Category:** Performance (cold but quadratic over feature map)
- **Confidence:** High
- **Blast radius:** Per `resolved_features()` call, this runs `O(map_keys × variants)` and each comparison allocates a `format!` string. With ~24 `WorldType` variants and a fully-authored `by_world_type` map, that's a couple hundred small heap allocations per load. Cold path (project open) but easy to remove.
- **Problem:** Comparing via `format!("{v:?}") == s` is the smelly half of "use `Debug` as canonical id". It also depends on `Debug` matching the source identifier — same risk as F-003-002.
- **Evidence:** `src/worlds_toml.rs:169`, `178`.
- **Suggested fix:** Use the existing `display_name()` for human-readable, or add a `variant_name()` method that returns the Rust identifier as `&'static str`. The latter is the more "right" choice here since the TOML wire format uses identifiers:
  ```rust
  impl WorldType {
      pub const fn variant_name(&self) -> &'static str {
          match self {
              Self::AgriWorld => "AgriWorld",
              // ...
          }
      }
  }
  fn parse_world_type_variant(s: &str) -> Option<WorldType> {
      WorldType::VARIANTS.iter().copied().find(|v| v.variant_name() == s)
  }
  ```
  No allocation, no `Debug` dependence. Same for `StarColour::variant_name`. Even simpler: lift the matching to a `match s { "AgriWorld" => Some(Self::AgriWorld), ... }` style — see F-003-005 for the `FromStr`-based unification.
- **Effort:** S
- **Risk of fix:** Low.

### F-003-012 — [LOW] [API / Dead code] `load_generation_rows` and `into_legacy_tuple` are documented as legacy compatibility shims with no remaining external users
- **Location:** `src/worlds.rs:403-407` (`load_generation_rows`), `417-420` (`into_legacy_tuple`).
- **Category:** Dead code / API surface
- **Confidence:** High
- **Blast radius:** Public API today; once removed, `gen/world_pool.rs:319` would call `load_worlds_data` directly.
- **Problem:** Both functions exist solely to preserve the CSV-era tuple shape `(KeyTables, Vec<GenerationRow>)`. Per the doc comment on line 399-402, the CSV path has been removed. The only caller is `inspect_workbook` (`gen/world_pool.rs:319`), which is inside the crate and could call `load_worlds_data` directly.
- **Suggested fix:** Inline both into `gen/world_pool.rs::inspect_workbook`:
  ```rust
  let WorldsLoad { tables, rows, .. } =
      crate::worlds::load_worlds_data(std::path::Path::new(path))
          .map_err(|e| SectorError::WorldDataLoad { path: path.to_string(), message: e.to_string() })?;
  ```
  Then mark `load_generation_rows` and `into_legacy_tuple` `#[deprecated]` for a release, or delete them outright (they have no external callers).
- **Effort:** XS
- **Risk of fix:** Low.

### F-003-013 — [LOW] [API design] `KeyTables` exposes 9 `pub` fields, leaking the `HashMap` internal representation
- **Location:** `src/worlds.rs:361-381`.
- **Category:** API design / encapsulation / determinism (latent)
- **Confidence:** Medium
- **Blast radius:** Public API; switching from `HashMap<String, _>` to a different store becomes breaking. Also, since these are *lookup-only*, exposing `HashMap` invites accidental iteration that would silently violate the determinism invariant (CLAUDE.md). The doc comment on each field says "→ enum variants", but does not mark them as "lookup only — do not iterate for output."
- **Evidence:** `src/worlds.rs:362-381` (all fields `pub`, no warning).
- **Suggested fix:** Either:
  - Add `#[non_exhaustive]` + change `pub` fields to `pub(crate)` and expose `pub fn get_world_type(name: &str) -> Option<WorldType>` etc. (preferable; mirrors `from_enums` constructor + lookups only), or
  - At minimum, add a `// LOOKUP ONLY — do not iterate for output (determinism invariant; see CLAUDE.md)` comment on the struct.
  Also worth checking that `KeyTables::default()` (used because of `#[derive(Default)]`) is reachable and produces *empty* tables — currently it does, but `KeyTables::from_enums()` (line 1329) is the only valid constructor; making `default()` private/explicit would prevent silent misuse.
- **Effort:** S
- **Risk of fix:** Medium (touches accessor sites — `analysis/economy.rs` etc. — but `grep` shows only `tables.world_types.contains_key(...)` style usage).

### F-003-014 — [LOW] [Documentation] `# Errors` sections elide concrete variants, conflicting with the documented contract
- **Location:** `src/lib.rs:200-202`, `219-221`, `240-242`, `253-261`, `302-306` (some) — e.g. `validate_project` says "Currently infallible — the result is wrapped in `Result` to leave room for future fatal-validation cases" but offers no `# Errors` content; `load_segmentum_json` (377-384) lists no error variants.
- **Category:** Documentation
- **Confidence:** Medium
- **Blast radius:** Doc-only.
- **Problem:** `lib.rs` is otherwise meticulous about `# Errors`; the inconsistency is a small but real polish issue. Also several functions (`generate_sector_with_progress`, `write_segmentum`, etc.) say "Same as [`X`]" but `X` is two pages down and varies — easier to enumerate inline.
- **Evidence:** Spot-checks above.
- **Suggested fix:** Sweep: every `Result`-returning `pub fn` in `lib.rs` gets a one-bullet `# Errors` block naming each `SectorError` variant it can return. Cross-reference is fine for *append-only* variants but unhelpful for `validate_project` which is documented infallible.
- **Effort:** S
- **Risk of fix:** Trivial.

### F-003-015 — [LOW] [Idiomatic Rust] Inconsistent `display_name` receivers — some `self`, some `&self`
- **Location:** `src/worlds.rs:879` (`pub fn display_name(self)` on `StarColour` — `Copy`), `911` (`pub fn display_name(&self)` on `WorldType` — `Clone` only), `951`, `972`, `992`, `1013`, `1034`, `1079`, `1218` (all `&self`).
- **Category:** Idiomatic Rust / API consistency
- **Confidence:** High
- **Blast radius:** Low — both work — but the inconsistency forces callers to remember which is which.
- **Problem:** Once F-003-004 makes all taxonomy enums `Copy`, the receivers can all be `self`, matching `StarColour`. Per RFC 430 / clippy `wrong_self_convention`, `Copy` enums conventionally take `self`.
- **Suggested fix:** After F-003-004, change every `display_name(&self)` to `display_name(self)`. The borrow at call sites (`v.display_name()` where `v: &WorldType`) auto-derefs anyway, so callers are unaffected.
- **Effort:** XS
- **Risk of fix:** Low.

### F-003-016 — [LOW] [Performance] `KeyTables::from_enums` allocates a `String` per variant via `to_owned()` even though `display_name() -> &'static str`
- **Location:** `src/worlds.rs:1329-1361`.
- **Category:** Performance / allocation (startup; cold)
- **Confidence:** High
- **Blast radius:** Once per project load (~120 small heap allocations). Cold but fixable for free.
- **Problem:** `KeyTables` uses `HashMap<String, Enum>`. Since the keys are all `&'static str` from `display_name()`, the map could be `HashMap<&'static str, Enum>` (or even a `phf::Map` for true compile-time lookup, but that's overkill). All 120 `.to_owned()` calls go away.
- **Evidence:** `src/worlds.rs:1332-1357`.
- **Suggested fix:** Change `KeyTables` field types from `HashMap<String, X>` to `HashMap<&'static str, X>`. Audit consumers (`grep` shows `analysis/economy.rs`, `validate/validation.rs`, plus crate-internal lookups) — they're all `tables.world_types.get(key)` style which works identically with `&str` keys vs `String` keys.
  Or, more aggressively, replace `KeyTables` entirely with const lookup functions on each enum (since `from_enums` is the *only* constructor and reflects compile-time data anyway).
- **Effort:** S
- **Risk of fix:** Low.

### F-003-017 — [LOW] [Performance / Allocation] `world_pool::inspect_workbook` rebuilds `KeyTables` from scratch when it's already compile-time-derivable
- **Location:** `src/worlds.rs:434` (`let (tables, rows) = cfg.to_loader_inputs();`) — and the matching `to_loader_inputs` body at `src/worlds_toml.rs:120-122` calls `KeyTables::from_enums()` every time.
- **Category:** Performance (cold, but redundant)
- **Confidence:** Medium
- **Blast radius:** Once per project load. With the F-003-016 fix it goes from "120 small allocations" to "rebuild a small struct each call" — still wasteful given `from_enums()` returns a deterministic constant.
- **Problem:** `KeyTables::from_enums()` is a pure function of compile-time data, yet it allocates 9 `HashMap`s on every call.
- **Suggested fix:** After F-003-016 (`&'static str` keys), wrap behind `OnceLock`:
  ```rust
  pub fn from_enums() -> &'static Self {
      static CELL: OnceLock<KeyTables> = OnceLock::new();
      CELL.get_or_init(|| { /* existing body */ })
  }
  ```
  Change the return type accordingly (or leave the owned constructor and add a `static_ref()` companion). `to_loader_inputs` returns `(&'static KeyTables, Vec<GenerationRow>)`.
- **Effort:** S
- **Risk of fix:** Low (lifetime adjustment in 2-3 callers).

### F-003-018 — [LOW] [Idiomatic Rust] `Government::None` shadows `Option::None` — confusing in match arms
- **Location:** `src/worlds.rs:155` (variant) and every match site (e.g. `worlds.rs:619`).
- **Category:** Idiomatic Rust / naming
- **Confidence:** Medium
- **Blast radius:** Maintainability.
- **Problem:** `Government::None` reads ambiguously in `match` arms — e.g. `Government::None => ...` looks like a missing-value case rather than "the government is *literally* an anarchy." Most Rust style guides recommend renaming such variants (`Anarchy`, `NoGovernment`, `Stateless`).
- **Suggested fix:** Rename to `Government::Anarchy` (or `NoGovernment`); update `from_str` key (`"None"` → `"None"` or `"Anarchy"`; pick a wire-stable name and document the breaking change), `display_name`, `VARIANTS`. This is a breaking rename — keep `"None"` as the wire string but rename the Rust identifier.
- **Effort:** S
- **Risk of fix:** Low for in-tree (Rust identifier only), Medium if `"None"` wire string changes (it shouldn't — keep both names mapping to `"None"`).

### F-003-019 — [NIT] [Documentation] `src/worlds.rs` opens with a `///` doc comment that's not attached to any item (technically `//!` is the right form)
- **Location:** `src/worlds.rs:1-2`.
- **Category:** Documentation
- **Confidence:** High
- **Blast radius:** None — but rustdoc may complain.
- **Suggested fix:** Change `/// World parameter types.` to `//! World parameter types.` at the very top of the file (module-level doc) so it becomes the module's rustdoc summary.
- **Effort:** Trivial.
- **Risk of fix:** None.

### F-003-020 — [NIT] [Documentation] Fx alias docs should call out determinism invariant explicitly
- **Location:** `src/lib.rs:51-56`.
- **Category:** Documentation / determinism
- **Confidence:** High
- **Blast radius:** Low — these are `pub(crate)` so the comment is for *internal* maintainers, who are the exact audience for the determinism rule.
- **Problem:** The current comment says "internal lookup-only structures" which is correct, but `CLAUDE.md` uses the more emphatic "never iterate for output." Matching the wording makes grep across the codebase land here.
- **Suggested fix:** Update the comment to:
  ```rust
  // Fast non-cryptographic hash for internal maps/sets. LOOKUP ONLY — never
  // iterate an `FxMap`/`FxSet` for output (CLAUDE.md determinism invariant).
  // Output writers must use `BTreeMap`/`BTreeSet` or sort keys explicitly.
  pub(crate) type FxMap<K, V> = rustc_hash::FxHashMap<K, V>;
  pub(crate) type FxSet<T> = rustc_hash::FxHashSet<T>;
  ```
- **Effort:** Trivial.
- **Risk of fix:** None.

### F-003-021 — [NIT] [Code style] `worlds.rs` huge `FromStr`/`AsRef`/`display_name` tables triplicate the variant name; consider a single source via macro
- **Location:** `src/worlds.rs:641-748` (`FromStr` for `NotableFeature`, 102 lines), `750-855` (`AsRef<str>`, 102 lines), `1116-1217` (`VARIANTS`, 100 lines), `1218-1321` (`display_name`, 103 lines).
- **Category:** Code style / maintainability
- **Confidence:** High
- **Blast radius:** Adding a notable feature requires editing four ~100-line tables in lockstep — high churn / easy to miss one. (Clippy already flags two of these as exceeding the 100-line threshold.)
- **Suggested fix:** A small declarative macro:
  ```rust
  macro_rules! taxonomy_enum {
      (
          $vis:vis enum $name:ident { $( $variant:ident = $display:literal ),* $(,)? }
      ) => {
          #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
          $vis enum $name { $( $variant ),* }
          impl $name {
              pub const VARIANTS: &'static [Self] = &[ $( Self::$variant ),* ];
              pub const fn display_name(self) -> &'static str {
                  match self { $( Self::$variant => $display ),* }
              }
              pub const fn variant_name(self) -> &'static str {
                  match self { $( Self::$variant => stringify!($variant) ),* }
              }
          }
          impl std::str::FromStr for $name {
              type Err = UnknownVariant;
              fn from_str(s: &str) -> Result<Self, Self::Err> {
                  match s { $( $display => Ok(Self::$variant), )* _ => Err(UnknownVariant { kind: stringify!($name), value: s.to_owned() }) }
              }
          }
          impl AsRef<str> for $name { fn as_ref(&self) -> &str { match self { $( Self::$variant => stringify!($variant) ),* } } }
          impl std::fmt::Display for $name { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.display_name()) } }
      };
  }
  ```
  Reduces `worlds.rs` from ~1361 LOC to ~300, eliminates F-003-002, -004, -005, -011, -015 in one stroke. Note: the `Display` impl above gives the human form (`"Hive World"`), and `AsRef<str>` gives the identifier — flip those if existing callers depend on the current `AsRef<str>` shape (which returns `"AgriWorld"` etc.).
- **Effort:** M (write the macro and migrate; verify golden tests).
- **Risk of fix:** Medium — must verify byte-stable output. Recommend doing this *after* F-003-002 has been merged so `Display`'s correctness is established first.

### F-003-022 — [NIT] [Idiomatic Rust] `pub const VARIANTS: &'static [Self]` is conventionally `pub const ALL: &'static [Self]` in Rust (Clippy `must_use_candidate`)
- **Location:** `src/worlds.rs:870, 885, 942, 965, 984, 1005, 1026, 1047, 1116`.
- **Category:** Naming convention
- **Confidence:** Low
- **Blast radius:** None functional; `VARIANTS` is also a defensible name. Mentioning in case of style sweep.
- **Suggested fix:** Either standardize on `ALL` (matches `strum`'s convention) or keep `VARIANTS` and add `#[must_use]` to the methods clippy flagged (baseline shows `must_use_candidate` warnings on every `display_name`).
- **Effort:** Trivial.
- **Risk of fix:** Low.

### F-003-023 — [NIT] [Code style] `WorldError::Invalid(String)` is stringly-typed; `WorldsTomlError` is structured — the two parallel error types should converge
- **Location:** `src/worlds.rs:14` (`Invalid(String)`), `src/worlds_toml.rs:55-67` (structured).
- **Category:** Error handling
- **Confidence:** Medium
- **Blast radius:** API.
- **Problem:** `WorldError` is the *outer* error type returned from `load_worlds_data`. It has just two variants (`Io` and `Invalid(String)`), and the `Invalid` payload is always `format!("worlds.toml: {e}")` (line 433) where `e` is already a structured `WorldsTomlError`. Promoting `WorldsTomlError` into a variant of `WorldError` preserves the structure for callers.
- **Suggested fix:**
  ```rust
  #[derive(Debug, Error)]
  #[non_exhaustive]
  pub enum WorldError {
      #[error("I/O error: {0}")]
      Io(#[from] std::io::Error),
      #[error("worlds.toml: {0}")]
      Toml(#[from] crate::worlds_toml::WorldsTomlError),
      #[error("invalid data: {0}")]
      Invalid(String), // kept for non-TOML-sourced validation failures
  }
  ```
  Then `load_worlds_data` becomes `WorldsConfig::from_path(...)?` plus a `?` for features.
- **Effort:** S
- **Risk of fix:** Low — additive; `WorldError::Invalid` retained for back-compat.

## Category coverage

- §3.1 Panics: No findings in non-test code. All `unwrap`/`expect` are in `#[cfg(test)]` (`worlds_toml.rs:235-258`). The `index == 0` guard in `lib.rs:307` returns `SectorError::InvalidConfig`, not panics. No integer overflow risk; no slicing.
- §3.2 unsafe: None.
- §3.3 Ownership: F-003-004 (Copy elide), F-003-007 (double clone in standalone path). `lib.rs:303` clippy `needless_pass_by_value` for `project: ProjectInput` is folded into F-003-007's fix sketch.
- §3.4 Errors: F-003-001 (no `#[non_exhaustive]`), F-003-005 (`type Err = ()`), F-003-023 (stringly-typed `WorldError::Invalid`).
- §3.5 Concurrency: N/A.
- §3.6 Performance: F-003-011, F-003-016, F-003-017.
- §3.7 Idiomatic & API: F-003-002, F-003-003, F-003-006, F-003-008, F-003-009, F-003-010, F-003-013, F-003-015, F-003-018, F-003-022.
- §3.8 Deps: No findings.
- §3.9 Memory/Drop: No findings (no `Drop` impls; no caches; no `static mut`).
- §3.10 Tests: `worlds_toml.rs` has 4 inline tests — good coverage of the happy and unknown-variant paths. `worlds.rs` has zero inline tests for `FromStr`/`display_name` round-trips or `KeyTables::from_enums`. F-003-024 below.
- §3.11 Docs: F-003-014, F-003-019, F-003-020.

### F-003-024 — [LOW] [Testing] `worlds.rs` has no inline tests for `FromStr`/`display_name`/`VARIANTS` round-trips
- **Location:** `src/worlds.rs` — entire file. No `#[cfg(test)] mod tests` block.
- **Category:** Test coverage
- **Confidence:** High
- **Blast radius:** Bugs in any of the ~700 lines of hand-written variant tables would only surface via downstream failures, not a clean local test.
- **Suggested fix:** Add:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      #[test]
      fn world_type_display_name_round_trip() {
          for v in WorldType::VARIANTS {
              let s = v.display_name();
              assert_eq!(s.parse::<WorldType>().ok().as_ref(), Some(v),
                         "display_name {s:?} did not parse back");
          }
      }
      // similar for every taxonomy enum
  }
  ```
  This catches every "I added a variant to `VARIANTS` but forgot to add it to `from_str`" defect at compile-test time. ~50 lines covers all 9 enums.
- **Effort:** S
- **Risk of fix:** Trivial.

## Determinism audit (CLAUDE.md hard rules)

- Fx aliases: only declared at `src/lib.rs:55-56` as `pub(crate)`. **OK.** Comment could be sharper (F-003-020).
- HashMap iteration in `worlds.rs`/`worlds_toml.rs`: `worlds.rs` has zero `iter()/keys()/values()/into_iter()`; `worlds_toml.rs:128, 136` iterates `BTreeMap` (deterministic). **OK.**
- RNG: zero `rand::thread_rng()` or seed code in these files. **OK.**
- Output writers: none of these three files write user-facing output. **OK.**

## Summary of suggested fixes

- F-003-001 — HIGH — Add `#[non_exhaustive]` to `SectorError`, `MutationError`, and every re-exported enum in `lib.rs` — S/Low
- F-003-002 — HIGH — Replace `Display` impls that delegate to `Debug` with `display_name()`-based ones — S/Medium
- F-003-003 — HIGH — Add `#[non_exhaustive]` to `WorldsConfig`, `FeaturePoolConfig`, `WeightedFeatureEntry` + provide constructors — M/Medium
- F-003-004 — HIGH — Derive `Copy` (and `PartialOrd`/`Ord`) on all 8 unit-only taxonomy enums; sweep `.clone()` call sites — S/Low (derive) + M (cleanup)
- F-003-005 — MEDIUM — Replace `type Err = ()` on all 9 `FromStr` impls with a structured `UnknownVariant` error — M/Low
- F-003-006 — MEDIUM — Delete `worlds::WorldEntry` and `worlds::System` (zero callers) — S/Low
- F-003-007 — MEDIUM — Remove the two clones in `generate_system_standalone` via `[sys]` move + destructure; consider `&ProjectInput` — XS/Low
- F-003-008 — MEDIUM — Switch `pub mod worlds_toml` to `mod worlds_toml` + explicit `pub use` selection — S/Low
- F-003-009 — MEDIUM — Move `ResolvedFeaturePool` / `WeightedFeatureEntry` into `worlds.rs` to invert the dep — M/Medium
- F-003-010 — MEDIUM — `WorldsConfig::from_str` should be `FromStr` trait, not inherent — S/Low
- F-003-011 — MEDIUM — Replace `format!("{v:?}") == s` matchers with a `variant_name()` const fn or `FromStr` after F-003-005 — S/Low
- F-003-012 — LOW — Inline `load_generation_rows`/`into_legacy_tuple` into `inspect_workbook`; delete or `#[deprecated]` — XS/Low
- F-003-013 — LOW — Make `KeyTables` fields `pub(crate)` or document "lookup only" — S/Medium
- F-003-014 — LOW — Add concrete `# Errors` bullets to every `Result`-returning fn in `lib.rs` — S/Trivial
- F-003-015 — LOW — Make `display_name(&self)` → `display_name(self)` after F-003-004 — XS/Low
- F-003-016 — LOW — `KeyTables` fields → `HashMap<&'static str, _>` to drop ~120 startup allocations — S/Low
- F-003-017 — LOW — Cache `KeyTables::from_enums()` behind a `OnceLock` — S/Low
- F-003-018 — LOW — Rename `Government::None` → `Government::Anarchy` (keep `"None"` wire string) — S/Low
- F-003-019 — NIT — Convert leading `///` doc on `src/worlds.rs:1` to `//!` — Trivial/None
- F-003-020 — NIT — Sharpen Fx-alias comment with the CLAUDE.md "never iterate for output" wording — Trivial/None
- F-003-021 — NIT — Collapse the four parallel variant tables behind a `taxonomy_enum!` macro — M/Medium
- F-003-022 — NIT — Either rename `VARIANTS` → `ALL` (strum convention) or add `#[must_use]` to flagged `display_name`s — Trivial/Low
- F-003-023 — NIT — Promote `WorldsTomlError` into a `WorldError` variant via `#[from]` — S/Low
- F-003-024 — LOW — Add inline `FromStr` ↔ `display_name` round-trip tests for every taxonomy enum — S/Trivial
