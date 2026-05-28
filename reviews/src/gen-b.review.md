---
unit_id: U008
crate: sectorforge
paths:
  - src/gen/mod.rs
  - src/gen/names.rs
  - src/gen/routes.rs
  - src/gen/orbital_assets.rs
  - src/gen/surface_region.rs
  - src/gen/world_pool.rs
  - src/gen/world_ecs.rs
  - src/gen/faction_style.rs
  - src/gen/factions.rs
loc_reviewed: 2205
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 2, medium: 6, low: 9, nit: 6 }
top_risks:
  - "world_ecs orphan-entity bug when route endpoints missing (F-008-001)"
  - "Notable-feature names silently dropped on parse failure in pool builder (F-008-002)"
  - "Unbounded silent narrowing of usize→u32 in world_ecs FactionComponents (F-008-005)"
---

# Review: src/gen/ Part B — data-side generation helpers

## Summary

The Part-B `src/gen/` files are mostly pure data adapters and deterministic
derivations sitting around the orchestrator in `generation/`. Code quality is
generally good: all iteration uses `BTreeMap` / sorted `Vec`, and only one RNG
call exists in the whole set (none — they're all pure). The most noteworthy
problems are a small handful of correctness slips in `world_ecs.rs` (orphan
entity on missing route endpoints; unchecked usize→u32 narrowing) and one
silent-drop in `world_pool::build_pool` when a `notable_features` key in the
workbook fails both parsers. Everything else is style, cheap-clone reduction,
or visibility minimisation.

`world_ecs` has zero call sites in production or test code (only `pub use`
in `lib.rs`). It is effectively an unused future-facing API surface — flagged
once as a maintenance liability, but not removed by this unit.

## Findings

### F-008-001 — [HIGH] [Correctness] `world_ecs::build` allocates an orphan entity when a route's endpoint system id is unknown
- **Location:** `src/gen/world_ecs.rs:168-185`
- **Category:** Correctness
- **Confidence:** High
- **Blast radius:** Any caller iterating `EntityWorld.kinds`/`names`/`id_lookup` expecting every `EntityKind::Route` to have a `route_components` entry will get a hole. `id_lookup` also gains an entry that resolves to no components.
- **Problem:** The route loop calls `alloc(&mut w, &r.id, EntityKind::Route)` *before* checking that both endpoints resolve in `system_eids`. When either endpoint lookup returns `None`, the `let-else` `continue`s without inserting into `route_components` — so an `EntityKind::Route` row exists in `w.kinds`, `w.names`, and `w.id_lookup` but no component data.
- **Why it matters:** Joining kinds → components by `EntityId` (the documented pattern at `world_ecs.rs:43-54`) will silently miss data; downstream simulators get partial state with no diagnostic.
- **Evidence:** Read of lines 168-185; the `alloc` lambda has unconditional side effects on `next_id`, `kinds`, `names`, and `id_lookup` (lines 100-107).
- **Suggested fix:** Resolve endpoints *before* allocating:
  ```rust
  for r in &sector.routes {
      let from = system_eids.get(&r.from_system_id).copied();
      let to   = system_eids.get(&r.to_system_id).copied();
      let (Some(from), Some(to)) = (from, to) else { continue };
      let eid = alloc(&mut w, &r.id, EntityKind::Route);
      w.route_components.insert(eid, RouteComponents { /* … */ });
  }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-008-002 — [HIGH] [Error handling] `build_pool` silently drops unrecognised notable-feature names from the Key sheet
- **Location:** `src/gen/world_pool.rs:213-219`
- **Category:** Error handling / silent data loss
- **Confidence:** High
- **Blast radius:** Workbook-load path. Every generation run.
- **Problem:** The fallback chain `taxonomy::parse_notable_feature_variant(name).or_else(|| name.parse::<NotableFeature>().ok())` drops the entry on the floor when both parsers fail — no `excluded_rows` entry, no warning. `ExcludedRow` already exists for row-level exclusions but isn't used here.
- **Why it matters:** A typo in `key.notable_features` makes that feature invisible to the generator without ever surfacing to the operator. Compare validate.rs:240/280 which *does* flag the same parse failure during validation; the load path is inconsistent.
- **Evidence:** Read of lines 213-219; cross-check with `src/validate/validation.rs:240` and `:280` which already detect the parse miss.
- **Suggested fix:** Either (a) add a `key_table_feature_misses: Vec<String>` field to `WorldCandidatePool` and push unrecognised names, or (b) reject the workbook with `SectorError::WorldDataLoad` listing the bad names — symmetric with the `validate` path:
  ```rust
  let mut misses = Vec::new();
  for name in &tables.notable_features {
      match taxonomy::parse_notable_feature_variant(name)
          .or_else(|| name.parse::<NotableFeature>().ok())
      {
          Some(f) => pool.feature_pool.key_table_features.push(f),
          None    => misses.push(name.clone()),
      }
  }
  pool.key_table_feature_misses = misses;
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-008-003 — [MEDIUM] [Correctness/Performance] `derive_orbital_assets` re-scans `sys.worlds` four times for orthogonal predicates
- **Location:** `src/gen/orbital_assets.rs:97-117`
- **Category:** Performance / Maintainability
- **Confidence:** High
- **Blast radius:** Hot per-system loop during generation (called once per system on every regen).
- **Problem:** `has_spaceyard`, `war_zone`, and `quarantined` each walk every world's `notable_features` / `tags` arrays in independent passes. With N worlds and F features per world this is 3× the necessary work and three identical match-cascades on `f.as_ref()` substrings.
- **Why it matters:** Cheap per-system today but the count multiplies by sector size; refactoring keeps regen friendly as sectors grow.
- **Evidence:** Read of lines 97-117 (three sequential `sys.worlds.iter()` chains).
- **Suggested fix:** Single fold:
  ```rust
  let mut has_spaceyard = false;
  let mut war_zone = false;
  let mut quarantined = false;
  for w in &sys.worlds {
      for f in &w.world.notable_features {
          match f.as_ref() {
              "MajorSpaceyard"     => has_spaceyard = true,
              "WarZone" | "DaemonicCorruption" => war_zone = true,
              _ => {}
          }
      }
      for t in &w.tags {
          if t.ends_with(":war_zone")    { war_zone = true; }
          if t.ends_with(":quarantined") { quarantined = true; }
      }
  }
  ```
  Also lifts each magic string into a `const` to avoid drift with `notable_features`.
- **Effort:** S
- **Risk of fix:** Low

### F-008-004 — [MEDIUM] [Performance] `WorldCandidate` construction clones 8 enum fields per row when 7 of them are `Clone`-but-not-`Copy`
- **Location:** `src/gen/world_pool.rs:152-188`; cross-reference `src/worlds.rs:56-166`
- **Category:** Performance / Idiomatic Rust
- **Confidence:** High
- **Blast radius:** Workbook load (once per startup) and re-load. ~hundreds–low-thousands of rows.
- **Problem:** `world_type`, `atmosphere`, `temperature`, `biosphere`, `population`, `tech`, `government` are all unit-variant enums (single byte after layout), but they lack `#[derive(Copy)]`. The `.clone()` chain at lines 159-184 reads as an expensive copy when the underlying op is a discriminant byte.
- **Why it matters:** Adding `Copy` is mechanical, eliminates seven syntactic `.clone()`s per row in this file (and other call sites: `world_pool.rs:191-204` in feature pool population, `world_placement.rs`, etc.), and clarifies that the type is value-semantic. Same applies in `WorldCandidate::to_world` (lines 30-43).
- **Evidence:** Reads of `src/worlds.rs:84,95,104,114,124,134` — every enum is unit-only.
- **Suggested fix:** Add `Copy` to the enum derives at `src/worlds.rs:84,95,104,114,124,134` (matches `StarColour` at line 19 which already has it), then drop the `.clone()` calls. `WorldType` would also gain `Copy` but it has many variants — verify it's still ≤ 8 bytes (it is).
- **Effort:** S
- **Risk of fix:** Low — `derive(Copy)` is additive.

### F-008-005 — [MEDIUM] [Correctness] `world_ecs.rs` silently narrows `usize` → `u32` for presence counts and floors `public_order` without re-clamp
- **Location:** `src/gen/world_ecs.rs:151,164,165`
- **Category:** Correctness / Numeric safety
- **Confidence:** Medium
- **Blast radius:** Bounded — counts will not realistically overflow `u32`, and `public_order` is clamped at construction time (`analysis/stability.rs:218`). But the casts violate §3.7's "prefer `TryFrom`/explicit saturation".
- **Problem:**
  - `f.system_presence.len() as u32` and `f.world_presence.len() as u32` (lines 164-165) truncate silently. With > 4 billion presences it wraps — unreachable but lint-bait.
  - `sys.stability.public_order.round() as u8` (line 151) is safe today because `public_order` is pre-clamped to 0..=100 by the producer, but the cast itself doesn't enforce that contract locally. A future change to `public_order`'s range silently produces wrap-around at `as u8` boundary (any negative or > 255 → wrap).
- **Why it matters:** Both are easy to harden with no perf cost; the second is a contract leak.
- **Evidence:** Read of lines 151, 164-165; producer at `src/analysis/stability.rs:218`.
- **Suggested fix:**
  ```rust
  // line 151:
  stability_order: sys.stability.public_order.clamp(0.0, 255.0).round() as u8,
  // lines 164-165:
  system_count: u32::try_from(f.system_presence.len()).unwrap_or(u32::MAX),
  world_count:  u32::try_from(f.world_presence.len()).unwrap_or(u32::MAX),
  ```
- **Effort:** XS
- **Risk of fix:** Low

### F-008-006 — [MEDIUM] [Maintainability/Dead code] Entire `world_ecs` module has no in-tree call sites
- **Location:** `src/gen/world_ecs.rs:1-272` (entire file); re-exported at `src/lib.rs:173`
- **Category:** Dead code / Maintainability
- **Confidence:** High
- **Blast radius:** None today; ~272 LOC of "future-facing" API the team keeps green via tests but no consumer exercises.
- **Problem:** A grep across `src/`, `builder/`, `viewer/`, `gui-core/`, and `tests/` for `build_entity_world` / `world_ecs::build` / `EntityWorld` / `EntityId` finds only the `pub use` line in `src/lib.rs` and the doc-link in `GUIDE.md`. The module's own `#[test]` is its only caller.
- **Why it matters:** The file is invisibly drifting: the `build` walker is the only entry point and its bug (F-008-001) is unreachable from any production code path. Either *use* it (e.g. drive `analytics`/`hooks` off the columnar view) or mark it experimental.
- **Evidence:** `grep -RIn "build_entity_world\|world_ecs::build" .` returns 2 hits, both definitions/exports.
- **Suggested fix:** One of:
  1. Add a doc warning: `//! **Experimental** — no in-tree consumers as of YYYY-MM. API may change without notice.`
  2. Demote `pub use` to `#[doc(hidden)] pub use` until a consumer lands.
  3. Move to an `experimental` sub-module gated by a Cargo feature.
- **Effort:** XS
- **Risk of fix:** Low

### F-008-007 — [MEDIUM] [Idiomatic Rust] `surface_region::derive_regions` allocates a fresh `Vec<(RegionKind, u8, &'static str)>` table on every world
- **Location:** `src/gen/surface_region.rs:66-124`
- **Category:** Performance / Idiomatic Rust
- **Confidence:** High
- **Blast radius:** Generation hot path — called once per world (potentially thousands per sector).
- **Problem:** Each `match wt { … => vec![(…), (…), …] }` arm allocates a fresh `Vec` containing 2–4 static tuples. The table is data, not state — it could live as `&'static [(RegionKind, u8, &'static str)]` slices and be matched once.
- **Why it matters:** Heap churn proportional to world count for tables whose contents are compile-time constant.
- **Evidence:** Read of lines 66-124.
- **Suggested fix:**
  ```rust
  const HIVE: &[(RegionKind, u8, &str)] = &[
      (RegionKind::Capital, 25, "Sector Capital"),
      (RegionKind::Hive, 45, "Primary Hive"),
      (RegionKind::Underhive, 25, "Underhive"),
      (RegionKind::Wilderness, 5, "Outer Wastes"),
  ];
  // … one const per world type …
  let kinds: &[(RegionKind, u8, &str)] = match wt {
      "HiveWorld" => HIVE,
      "CivilisedWorld" | "Civilised" => CIVILISED,
      // …
      _ => DEFAULT,
  };
  let mut out = Vec::with_capacity(kinds.len());
  for &(kind, weight, name) in kinds { … }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-008-008 — [MEDIUM] [Idiomatic Rust] `BlockadeReport`/`SurfaceRegion`/`OrbitalAsset` derives clone-heavy `String` ids per asset; consider `Arc<str>` to match the project's existing `Arc<str>` tag/feature pattern
- **Location:** `src/gen/orbital_assets.rs:36-48`, `surface_region.rs:17-36`
- **Category:** Performance / Ownership
- **Confidence:** Medium
- **Blast radius:** Bounded — assets and regions are O(systems × few) and O(worlds × few). But the rest of the codebase has migrated tags / notable_features to `Arc<str>` (see `orbital_assets.rs:101` `f: &Arc<str>` reads) and these structs are an island of `String`.
- **Problem:** `OrbitalAsset.id`, `SurfaceRegion.name`, `BlockadeReport.blockader/besieged`, etc. all use `String`. Cloning a `BlockadeReport` for the public DTO copies up to 3 `String`s; cloning the entire `Vec<OrbitalAsset>` for serialisation copies one `String` per asset plus inner `ShipStock.hull_class` strings.
- **Why it matters:** Pattern inconsistency makes downstream code less efficient by accident.
- **Evidence:** Compare `orbital_assets.rs:36-48` vs the `Arc<str>` reads on the same file lines 101, 105, 109, 114.
- **Suggested fix:** Migrate the ID/name fields to `Arc<str>` (or `Box<str>`) where cloning is common and content is immutable. Keep the public `format!`-built ids — just collect them into `Arc<str>::from(format!(…))`.
- **Effort:** M
- **Risk of fix:** Medium — touches serde format (`Arc<str>` deserialises fine but golden tests must be re-baselined if anything changes).

### F-008-009 — [LOW] [Idiomatic Rust] `faction_style::hsv_to_rgb` lossy `h as i32 / 60` for sextant selection
- **Location:** `src/gen/faction_style.rs:125-132`
- **Category:** Numeric correctness
- **Confidence:** Medium
- **Blast radius:** Tested code path; will not crash because of the `_ =>` fallback.
- **Problem:** `h as i32 / 60` casts `h: f32` (already normalised to [0, 360) at line 121) to `i32` then integer-divides. For `h = 359.9999`, the cast yields 359 → `/60 = 5`. The `_ =>` arm catches everything ≥ 5 — including 6 if rounding ever produces 360. Works, but the intent ("which sextant of the hue wheel") is hidden.
- **Why it matters:** Determinism is preserved (good — both renderers use this), but the code is brittle and a future edit could break the colour wheel rotation.
- **Evidence:** Read of lines 121-132.
- **Suggested fix:**
  ```rust
  let sextant = ((h / 60.0).floor() as i32).rem_euclid(6);
  let (r1, g1, b1) = match sextant { 0 => (c, x, 0.0), … 5 => (c, 0.0, x), _ => unreachable!() };
  ```
- **Effort:** XS
- **Risk of fix:** Low — verify with `cargo test --test it -- golden` since this touches a render-feeding helper.

### F-008-010 — [LOW] [Performance] `derive_orbital_assets` rebuilds asset-id `String`s via `format!` per loop iteration
- **Location:** `src/gen/orbital_assets.rs:122-184`
- **Category:** Performance
- **Confidence:** High
- **Blast radius:** Once per system × per faction × per asset kind that triggers. Bounded but hot in regen.
- **Problem:** Five `format!("{}-{}-{}", sys.id, kind, id)` calls inline; each allocates. Combined with the `Vec::new()` `ship_inventory` allocation, this is the main heap churn in the function.
- **Why it matters:** Trivially amortised by a small writer.
- **Suggested fix:** Use a reusable `String` buffer (clear/extend) or a small helper:
  ```rust
  let mut buf = String::with_capacity(sys.id.len() + 32);
  let mk_id = |buf: &mut String, kind: &str, fid: &str| -> String {
      buf.clear();
      use std::fmt::Write;
      write!(buf, "{}-{}-{}", sys.id, kind, fid).unwrap();
      buf.clone()
  };
  ```
  Or simply accept the cost and gate this on a profile. Effort/payoff is borderline LOW.
- **Effort:** S
- **Risk of fix:** Low

### F-008-011 — [LOW] [API design] `world_pool::ExcludedRow` and `ExclusionReason` are `pub` but never consumed outside the module
- **Location:** `src/gen/world_pool.rs:53-76`
- **Category:** API surface / visibility minimisation
- **Confidence:** Medium
- **Blast radius:** None today; flagged because U008 special focus calls out re-export visibility.
- **Problem:** `WorldCandidatePool.excluded_rows` is a `pub` field with `pub struct ExcludedRow` and `pub enum ExclusionReason`. `grep` shows zero external consumers — the field is only read by `inspect_workbook` to compute `.len()` (line 367) and by the unit tests in the same file.
- **Why it matters:** Every `pub` item is a permanent contract; over-publishing makes future renames API breaks. The `Display` impl on `ExclusionReason` (lines 67-76) is dead.
- **Suggested fix:** Either expose the diagnostics through `WorkbookStats` (replace `excluded_rows: usize` with `Vec<ExcludedRow>` so the CLI can show *why*), or demote the structs to `pub(crate)`. The former is the better fix because operators do want the "why".
- **Effort:** S
- **Risk of fix:** Low

### F-008-012 — [LOW] [Idiomatic Rust] `factions.rs` keeps `use std::borrow::Cow;` mid-file at line 159
- **Location:** `src/gen/factions.rs:159`
- **Category:** Style / organisation
- **Confidence:** High
- **Blast radius:** None.
- **Problem:** `use` statement appears after a function definition (line 140 returns `Cow<'static, str>` referencing the type before the `use` is in scope — works because the `use` is at module scope regardless of file position, but it's confusing).
- **Suggested fix:** Move to the top of the file alongside `use serde::…` at line 3.
- **Effort:** XS
- **Risk of fix:** Low

### F-008-013 — [LOW] [API design] `factions::legacy_top_faction_id` returns `String` where `&'static str` (or `Cow<'static, str>` to match `legacy_top_faction_name`) would suffice
- **Location:** `src/gen/factions.rs:100-137`
- **Category:** Performance / API consistency
- **Confidence:** High
- **Blast radius:** Called once per faction-def whenever `top_faction_id()` falls through. Few allocations, but unnecessary.
- **Problem:** Every match arm is a string literal; the fallback `_ => kind.to_string()` is the only allocating path. Mirror the `Cow<'static, str>` shape used by `legacy_top_faction_name`.
- **Suggested fix:**
  ```rust
  pub fn legacy_top_faction_id(kind: &str) -> Cow<'_, str> {
      match kind {
          "imperial" | … | "collegia_titanica" => Cow::Borrowed("imperial"),
          // …
          _ => Cow::Borrowed(kind),
      }
  }
  ```
  Then `top_faction_id` in line 75 becomes `FactionId::new(legacy_top_faction_id(&self.kind).as_ref())`.
- **Effort:** S
- **Risk of fix:** Low

### F-008-014 — [LOW] [Idiomatic Rust] `surface_region::derive_regions` uses early-return on `Uninhabited` but accepts `wt == "DeadWorld"` despite no `DeadWorld` arm in the match table — falls through to `_` (Capital+Wilderness)
- **Location:** `src/gen/surface_region.rs:62-124`
- **Category:** Correctness / Documentation
- **Confidence:** Medium
- **Blast radius:** Edge: an uninhabited DeadWorld returns two regions named "Capital"/"Hinterland" with no inhabitants.
- **Problem:** The guard `pop == "Uninhabited" && !matches!(wt, "TombWorld" | "DeadWorld" | "WarpLostWorld")` deliberately allows three uninhabited types through, but the match table only has explicit arms for `TombWorld` (line 106) — the other two fall through to the generic `Capital`/`Hinterland` arm at line 120, naming an uninhabited dead world's regions "Capital".
- **Why it matters:** Either intentional (then the generic arm should produce neutral names like "Surface"/"Interior") or a small spec drift.
- **Evidence:** Read of lines 62, 106-124.
- **Suggested fix:** Add explicit arms:
  ```rust
  "DeadWorld" | "WarpLostWorld" => vec![
      (RegionKind::Wilderness, 100, "Dead Surface"),
  ],
  ```
- **Effort:** XS
- **Risk of fix:** Low — touches generated data; re-baseline goldens if applicable.

### F-008-015 — [LOW] [Idiomatic Rust] `WorldCandidatePool` exposes 4 `pub` fields rather than offering accessors; combined with `Default::default()` + field-by-field assign in tests, this makes invariants implicit
- **Location:** `src/gen/world_pool.rs:45-51`
- **Category:** API design
- **Confidence:** Medium
- **Blast radius:** None today, but locks the type to its current layout for downstream consumers.
- **Problem:** `candidates`, `excluded_rows`, `feature_pool`, `star_colour_weights` are all `pub`. The constructor (`build_pool`) is the *only* legitimate producer because `star_colour_weights` is derived from `candidates`. A caller mutating `candidates` directly will silently desync `star_colour_weights`.
- **Suggested fix:** Make `star_colour_weights` `pub(crate)` and expose a `pub fn star_colour_weights(&self) -> &[(StarColour, f64)]` accessor; or recompute lazily.
- **Effort:** S
- **Risk of fix:** Low

### F-008-016 — [NIT] [Style] Operator precedence on combined `&&`/`||` predicates is correct but parens-free
- **Location:** `src/gen/orbital_assets.rs:131,142,165-167`
- **Category:** Style
- **Confidence:** High
- **Problem:** Lines like `(has_spaceyard && d.industrial >= 30.0) || d.industrial >= 60.0 && d.orbital >= 30.0` parse as intended (Rust binds `&&` tighter than `||`), but the human eye trips. Clippy would flag this under `nonminimal_bool` / readability lints.
- **Suggested fix:** Add explicit parens around the AND clusters.
- **Effort:** XS
- **Risk of fix:** None

### F-008-017 — [NIT] [Style] `roman_numeral(0)` returns `"0"` (Arabic) — silently wrong for a "roman" function
- **Location:** `src/gen/names.rs:60-87`
- **Category:** Style / contract
- **Confidence:** High
- **Problem:** The function's name promises Roman; the 0 case returns "0". Romans had no zero. Either document the convention (`# Notes: 0 maps to "0" because Roman has no zero — used as a placeholder for satellite indexing`) or `Option<String>`.
- **Suggested fix:** Add doc comment explaining the choice; or return `"N"` (medieval nulla) for consistency.
- **Effort:** XS
- **Risk of fix:** Low (verify no caller depends on the literal "0").

### F-008-018 — [NIT] [Style] `faction_style::glyph_for_kind` indexes a static slice by `(salt as usize) % pool.len()` without asserting the pool is non-empty
- **Location:** `src/gen/faction_style.rs:80-109`
- **Category:** Style / defensiveness
- **Confidence:** High
- **Problem:** Every arm provides a non-empty slice, so `% pool.len()` cannot panic. But the contract is implicit. A future maintainer adding an empty pool gets a runtime panic.
- **Suggested fix:** Convert to `[char; N]` per arm (compile-time size) or add a `debug_assert!(!pool.is_empty())`.
- **Effort:** XS
- **Risk of fix:** None

### F-008-019 — [NIT] [Docs] Public structs in `factions.rs` lack `#[non_exhaustive]` despite being TOML-parsed and likely to grow new optional fields
- **Location:** `src/gen/factions.rs:11-68`
- **Category:** API design
- **Confidence:** Medium
- **Problem:** `FactionDef` has 18 fields and a clear pattern of "added in §F2 / §F7" via `#[serde(default, skip_serializing_if = …)]`. External pattern-matchers will break each time a field is added. Marking `#[non_exhaustive]` future-proofs.
- **Suggested fix:** `#[non_exhaustive] pub struct FactionDef { … }`. Same for `FactionsFile` (line 6).
- **Effort:** XS
- **Risk of fix:** Low — only struct literal callers break; this codebase constructs `FactionDef` via deserialisation and one test (line 200, 226).

### F-008-020 — [NIT] [Docs] `inspect_workbook` is the only function in `world_pool.rs` whose `# Errors` is undocumented despite returning `Result`
- **Location:** `src/gen/world_pool.rs:317`
- **Category:** Documentation
- **Confidence:** High
- **Problem:** Public function returning `Result<_, SectorError>` with no `# Errors` section.
- **Suggested fix:** Add:
  ```rust
  /// # Errors
  ///
  /// Returns `SectorError::WorldDataLoad` when the workbook at `path` cannot be
  /// read or parsed.
  ```
- **Effort:** XS
- **Risk of fix:** None

### F-008-021 — [NIT] [Idiomatic Rust] `WorldCandidate.to_world` takes ownership of `features` but uses `&self` to clone its enum fields — asymmetric
- **Location:** `src/gen/world_pool.rs:30-43`
- **Category:** API ergonomics
- **Confidence:** Medium
- **Problem:** `features: Vec<NotableFeature>` is owned (good), but every other field is `self.xxx.clone()` — the function is `&self`. With `Copy` on the unit enums (see F-008-004) the cloning vanishes; for now, mark which call sites benefit.
- **Suggested fix:** Combined with F-008-004 — once enums are `Copy`, this becomes a one-line struct literal with no clones.
- **Effort:** XS (paired with F-008-004)
- **Risk of fix:** Low

## Rubric coverage

- **3.1 Panics & failure surface:** `world_pool.rs:155-184` uses `.expect("invariant: …")` deliberately after `first_missing_field` guards — safe and documented. No unguarded indexing or unchecked arithmetic found. `f32 as u8` casts are clamped (`orbital_assets.rs:193`, `surface_region.rs:164`) except F-008-005. No `todo!`/`unreachable!`.
- **3.2 unsafe & soundness:** No `unsafe` in any file. ✓
- **3.3 Ownership, borrowing, lifetimes, cloning:** See F-008-004, F-008-008, F-008-013.
- **3.4 Error handling:** See F-008-002. `top_by_weight` / `top_by_count` use `.unwrap_or(Ordering::Equal)` in NaN-safe spots — acceptable since `world_pool` rejects NaN at intake.
- **3.5 Concurrency & async:** N/A — these files are single-threaded pure helpers.
- **3.6 Performance:** See F-008-003, F-008-007, F-008-010. Determinism-positive: every BTreeMap iteration is sorted; no Fx aliases used here.
- **3.7 Idiomatic Rust & API design:** See F-008-006, F-008-011, F-008-013, F-008-015, F-008-019. Visibility minimisation is the largest theme (multiple `pub` items have no external consumers).
- **3.8 Dependencies & Cargo hygiene:** No unused imports. `factions.rs:159` has a misplaced `use` (NIT F-008-012).
- **3.9 Memory & resource management:** No `Drop`, no caches, no static globals. Clean.
- **3.10 Testing & verification:** Inline tests in `names.rs`, `routes.rs`, `orbital_assets.rs`, `surface_region.rs`, `world_pool.rs`, `world_ecs.rs`, `faction_style.rs`, `factions.rs` exercise happy paths and a handful of edge cases. Gaps: `world_pool::apply_authored_features` has zero direct tests; `world_pool::build_pool` does not test the `NaNOrInfiniteWeight` branch (it has `MissingWeight` and `NonPositiveWeight` only); `surface_region` does not test all 11 region kinds or the `Uninhabited`-but-allowed types (see F-008-014); `faction_style::hsv_to_rgb` has no direct test. No `#[ignore]`d tests, no sleep-based tests. No findings raised — adding coverage is desirable but not gating.
- **3.11 Documentation & maintainability:** Module docs are present everywhere. `# Errors` missing on `inspect_workbook` (F-008-020). Magic strings for notable features (e.g. `"MajorSpaceyard"`, `"WarZone"`, `"DaemonicCorruption"`) are duplicated across `orbital_assets.rs` — would benefit from `const`s next to `NotableFeature` enum (see F-008-003 suggested fix).

## Summary of suggested fixes

- F-008-001 — HIGH — `world_ecs::build` resolve route endpoints before alloc — S/Low
- F-008-002 — HIGH — `build_pool` surface unrecognised feature names instead of dropping — S/Low
- F-008-003 — MEDIUM — `derive_orbital_assets` single-pass world scan — S/Low
- F-008-004 — MEDIUM — `derive(Copy)` on unit-variant world enums to kill clones — S/Low
- F-008-005 — MEDIUM — `world_ecs` use `try_into` / clamp before numeric casts — XS/Low
- F-008-006 — MEDIUM — Mark `world_ecs` experimental or wire a consumer — XS/Low
- F-008-007 — MEDIUM — `derive_regions` use `const` slices instead of per-call `vec!` — S/Low
- F-008-008 — MEDIUM — Migrate asset/region id strings to `Arc<str>` for consistency — M/Medium
- F-008-009 — LOW — `hsv_to_rgb` use `.floor() as i32 .rem_euclid(6)` for sextant — XS/Low
- F-008-010 — LOW — `derive_orbital_assets` reuse a `String` buffer for asset ids — S/Low
- F-008-011 — LOW — Surface `ExcludedRow` through `WorkbookStats` or demote to `pub(crate)` — S/Low
- F-008-012 — LOW — Move `factions.rs` mid-file `use std::borrow::Cow;` to top — XS/Low
- F-008-013 — LOW — `legacy_top_faction_id` return `Cow<'_, str>` to match name pair — S/Low
- F-008-014 — LOW — Add explicit `DeadWorld`/`WarpLostWorld` region table arms — XS/Low
- F-008-015 — LOW — `star_colour_weights` field should be `pub(crate)` + accessor — S/Low
- F-008-016 — NIT — Add explicit parens to mixed `&&`/`||` predicates in `orbital_assets.rs` — XS/None
- F-008-017 — NIT — Document/justify `roman_numeral(0) == "0"` — XS/Low
- F-008-018 — NIT — Defensive `debug_assert!` (or arrays) in `glyph_for_kind` — XS/None
- F-008-019 — NIT — `#[non_exhaustive]` on `FactionDef` / `FactionsFile` — XS/Low
- F-008-020 — NIT — Add `# Errors` doc on `inspect_workbook` — XS/None
- F-008-021 — NIT — Paired with F-008-004: drop `to_world`'s field-by-field clones — XS/Low
