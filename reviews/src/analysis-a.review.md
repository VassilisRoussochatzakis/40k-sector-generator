---
unit_id: U009
crate: sectorforge
paths:
  - src/analysis/economy.rs
  - src/analysis/relations.rs
  - src/analysis/search.rs
loc_reviewed: 4738
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 12, low: 8, nit: 5 }
top_risks:
  - "Per-candidate deep clone of ProjectInput under rayon parallelism (F-009-001)"
  - "u32 underflow in RouteGraphConnected miss computation panics on empty sectors in debug (F-009-002)"
  - "Public enums/structs lack #[non_exhaustive] across the whole API surface (F-009-003)"
  - "Vec allocated per call by get_worlds_for_system inside inner loops (F-009-004)"
---

# Review: src/analysis/ PART A — economy, relations, search

## Summary

Heart of build-time analysis (also re-run inside the egui builder/viewer GUIs). Correctness and determinism mostly handled — RNG through `stage_rng`, sort-before-emit, `BTreeMap` at every store-and-iterate site — but performance has clear, fixable cliffs: per-candidate `ProjectInput` deep-clone under rayon, O(routes) inside a BFS step, repeated `canonical_pair` allocations, re-computed `cooccur` lookups. API hygiene is the weakest area: not a single `#[non_exhaustive]` on growing public enums. One latent panic in debug builds (`u32` underflow when `component_count == 0`).

## Findings

### F-009-001 — [HIGH] [Performance] Per-candidate deep clone of `ProjectInput` across rayon workers
- **Location:** `src/analysis/search.rs:1098-1123, 1186-1209`
- **Blast radius:** CLI + interactive (viewer wishes panel). 8 deep copies live concurrently on default rayon pool.
- **Problem:** `clone_project_with_seed` clones `world_tables`, `world_rows`, `authored_features`, `names`, `factions`, `route_rules`, `relations`, `regions`, `economy`, `history`, `personae`, `sites`, `hooks`, `missions`, `prose`, `input_digests` — all immutable except `config.generation.seed`.
- **Fix:** Wrap immutable arms in `Arc` (single `Arc<ProjectCatalogs>`) so parallel iterator clones only `AppConfig` per candidate.
- Effort M / Risk Medium.

### F-009-002 — [HIGH] [Panic] `u32` underflow when `component_count == 0`
- **Location:** `src/analysis/search.rs:843-855`
- `let miss = if passed { 0.0 } else { (n - 1) as f32 };` with `n: u32 = 0` underflows; debug panics, release wraps to ~4.29e9.
- **Fix:** Guard `if n == 0 { 1.0 }` branch or `n.saturating_sub(1) as f32`.
- S/Low.

### F-009-003 — [HIGH] [API] No `#[non_exhaustive]` on any public enum or DTO struct
- **Location:** Throughout `economy.rs`, `relations.rs`, `search.rs`. `Stance`, `TreatyStatus`, `RelationAttitude`, `SupplyRisk`, `TitheStatus`, `StrategicPriority`, `Constraint`, `EconomyReport`, `WorldEconomy`, `RelationsMatrix`, `SearchOutcome`, etc.
- Every enum one variant away from a SemVer-major bump.
- **Fix:** Apply `#[non_exhaustive]` to every growing public enum and output DTO.
- S/Low.

### F-009-004 — [HIGH] [Performance] `get_worlds_for_system` allocates `Vec` per call inside hot loops
- **Location:** `src/analysis/relations.rs:1170, 1216`; `src/analysis/search.rs:505, 523`; helper at `src/model/sector_model/mod.rs:285-287`
- `pub fn get_worlds_for_system<'a>(...) -> Vec<&'a GeneratedWorld> { sys.worlds.iter().collect() }` — Vec per call.
- **Fix:** Return `&'a [GeneratedWorld]` directly. Iterator API unchanged for callers.
- S/Low.

### F-009-005 — [MEDIUM] [Performance] `count_systems_matching_distance` is O(reachable × routes) per BFS step
- **Location:** `src/analysis/search.rs:565-621`
- **Fix:** Precompute adjacency `BTreeMap<&SystemId, Vec<&SystemId>>` once. S/Low.

### F-009-006 — [MEDIUM] [Performance] `tension_of` redundantly re-runs cooccur lookup that `build_relation` already has
- **Location:** `src/analysis/relations.rs:733-738, 767, 1279-1315`
- **Fix:** Pass `stats: CooccurStats` directly. S/Low.

### F-009-007 — [MEDIUM] [Performance] `canonical_pair` allocates two new Strings every call
- **Location:** `src/analysis/relations.rs:1054-1060`; called from 7+ sites
- **Fix:** Borrowed sibling `canonical_pair_ref<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str)` or switch cooccur key to `Arc<str>` tuple.
- M/Low-Medium.

### F-009-008 — [MEDIUM] [Performance] `pair_overrides` and `overrides` linearly scanned for every pair
- **Location:** `src/analysis/relations.rs:653-668, 771-780`
- **Fix:** Build BTreeMap indexes once per `derive_with_threshold`. S-M/Low.

### F-009-009 — [MEDIUM] [Performance] `system_supply_risk` does full `deps` scan per resource per world
- **Location:** `src/analysis/economy.rs:1196-1234`
- **Fix:** Pre-bucket `deps` by `(to_system_id, resource)` outside loop. S/Low.

### F-009-010 — [MEDIUM] [Performance] Walking `sector.routes` twice to build same valid-routes map
- **Location:** `src/analysis/economy.rs:893-906` and `:1102-1115`
- **Fix:** Build once at top of `derive_with`, pass into `derive_dependency_edges`. S/Low.

### F-009-011 — [MEDIUM] [Idiomatic] Use `let … else` instead of `is_none()` + `unwrap()`
- **Location:** `src/analysis/economy.rs:913-917`
- S/Low.

### F-009-012 — [MEDIUM] [Error handling] Rayon search silently swallows skipped/errored generations
- **Location:** `src/analysis/search.rs:1105-1113`
- Validation+generation failures both return `Slot::Skipped`; `SearchOutcome` neither counts skips nor surfaces error reasons.
- **Fix:** Add `skipped_count` + `skip_reasons` to `SearchOutcome` or propagate via `try_reduce`. S-M/Low.

### F-009-013 — [MEDIUM] [Performance] `Vec::with_capacity` undersized in `derive_dependency_edges`
- **Location:** `src/analysis/economy.rs:1117`
- **Fix:** `Vec::with_capacity(systems.len() * 4)`. S/Low.

### F-009-014 — [MEDIUM] [Performance] `evaluate` allocates many small Strings per constraint per candidate
- **Location:** `src/analysis/search.rs:631-1021`
- 256 × N constraints; only `report_top` (default 5) ever read.
- **Fix:** Two-tier: score first, format only for kept candidates. M/Low-Medium.

### F-009-015 — [MEDIUM] [API] Public `pub fn`s that should be `pub(crate)`
- **Location:** `economy.rs:144, 202, 261, 1507`; `relations.rs:74, 207, 295, 1325, 1443`; `search.rs:419, 1310`
- S/Low.

### F-009-016 — [MEDIUM] [Documentation] `run_search` docstring lies about error propagation
- **Location:** `src/analysis/search.rs:1053-1062`
- Doc claims propagation; code drops. **Fix:** Fix code or doc. S/Low.

### F-009-017 — [LOW] [Performance] `count_world_type_dominant`/friends re-walk all worlds per constraint
- **Location:** `src/analysis/search.rs:498-536, 538-562`
- **Fix:** Build `SectorCounters` once per candidate. M/Low.

### F-009-018 — [LOW] [Determinism] `near_misses` ranking uses `partial_cmp` with `f32` without NaN guard
- **Location:** `src/analysis/search.rs:1149-1154`; cross-ref `relations.rs:632-638`; `economy.rs:1469-1473`
- Tiebreak prevents most damage but defensive NaN→INFINITY mapping is cheap. S/Low.

### F-009-019 — [LOW] [Ownership] Avoidable `String` clones in `compute_pair`
- **Location:** `src/analysis/relations.rs:625-626, 706`
- **Fix:** `let (lo, hi) = if a.id <= b.id { (a, b) } else { (b, a) };`. S/Low.

### F-009-020 — [LOW] [Idiomatic] `DependencyEdge.route_id` is `Option` but always `Some`
- **Location:** `src/analysis/economy.rs:759-768, 1156-1166`
- **Fix:** Tighten to `RouteId` (JSON-shape breaking). S/Low.

### F-009-021 — [LOW] [Performance] `count_faction_presence` triple-nested but expresses an `any`
- **Location:** `src/analysis/search.rs:546-563`
- **Fix:** `flat_map(...).filter(...).count()`. S/Low.

### F-009-022 — [LOW] [Idiomatic] `count() as u32` silent truncation pattern
- **Location:** Multiple in `search.rs` + `economy.rs:990`
- **Fix:** `u32::try_from(...).unwrap_or(u32::MAX)`. S/Low.

### F-009-023 — [LOW] [Documentation] Missing `# Panics`/`# Errors` rustdoc on `derive`/`derive_with`/`derive_with_threshold`
- S/Low.

### F-009-024 — [NIT] [Style] Repeated `mul_add` chains hurt readability
- **Location:** `economy.rs:261-285, 1261-1270`; `relations.rs:933-987, 1295-1313`

### F-009-025 — [NIT] [Style] Trivial one-call helpers — `match_cause`/`cross_kinds`

### F-009-026 — [NIT] [Style] Magic numbers without named constants — `economy.rs:843, 862, 867`...

### F-009-027 — [NIT] [Testing] Determinism tests round-trip through JSON instead of `PartialEq`

### F-009-028 — [NIT] [Idiomatic] `for view in [a_to_b, b_to_a]` non-obvious

## Rubric coverage

- **3.1 Panics:** F-009-002 only real one.
- **3.2 unsafe:** None.
- **3.3 Ownership:** F-009-001, 004, 007, 019.
- **3.4 Errors:** F-009-012, 016.
- **3.5 Concurrency:** rayon use is order-deterministic; `IndexedParallelIterator::collect()` preserves order. No shared mutable state.
- **3.6 Performance:** F-009-001, 004, 005, 006, 007, 008, 009, 010, 013, 014, 017, 021.
- **3.7 Idiomatic/API:** F-009-003, 011, 015, 020, 022, 028.
- **3.8 Deps:** `HashMap` use is `FactionIndex` lookup-only.
- **3.9 Memory:** F-009-001 dominates.
- **3.10 Tests:** Coverage gaps deferred to U022.
- **3.11 Docs:** F-009-016, 023, 024, 026.

## Project invariants

- **Fx*/HashMap not iterated for output:** PASS — only `FactionIndex` lookup.
- **RNG via stage_rng:** PASS.
- **Byte-stable writers:** PASS — `BTreeMap` everywhere; `partial_cmp` ranks have tiebreaks.
- **Builder command bus:** N/A.

## Summary of suggested fixes

| id | severity | short | effort | risk |
|---|---|---|---|---|
| F-009-001 | HIGH | Wrap immutable arms of ProjectInput in Arc | M | Medium |
| F-009-002 | HIGH | Guard `n - 1` underflow when component_count == 0 | S | Low |
| F-009-003 | HIGH | Apply `#[non_exhaustive]` to public enums + DTOs | S | Low |
| F-009-004 | HIGH | Return `&[GeneratedWorld]` from get_worlds_for_system | S | Low |
| F-009-005 | MEDIUM | Precompute route adjacency in count_systems_matching_distance | S | Low |
| F-009-006 | MEDIUM | Pass stats into tension_of | S | Low |
| F-009-007 | MEDIUM | Borrow-key canonical_pair / Arc<str> tuple | M | Low-Medium |
| F-009-008 | MEDIUM | Build BTreeMap indexes for overrides | S-M | Low |
| F-009-009 | MEDIUM | Pre-bucket deps by (consumer, resource) | S | Low |
| F-009-010 | MEDIUM | Build valid_routes_by_sys once | S | Low |
| F-009-011 | MEDIUM | `let Some(sys) = ... else { continue }` | S | Low |
| F-009-012 | MEDIUM | Surface or propagate rayon errors | S-M | Low |
| F-009-013 | MEDIUM | Bump Vec::with_capacity | S | Low |
| F-009-014 | MEDIUM | Defer per-constraint format! to kept candidates | M | Low-Medium |
| F-009-015 | MEDIUM | Downgrade in-crate-only pub to pub(crate) | S | Low |
| F-009-016 | MEDIUM | Fix run_search docstring | S | Low |
| F-009-017 | LOW | Pre-walk into SectorCounters once per candidate | M | Low |
| F-009-018 | LOW | NaN-guard total_miss | S | Low |
| F-009-019 | LOW | Replace canonical_pair alloc with swap | S | Low |
| F-009-020 | LOW | Tighten DependencyEdge.route_id (non-Option) | S | Low |
| F-009-021 | LOW | Rewrite count_faction_presence | S | Low |
| F-009-022 | LOW | Guard count() as u32 truncations | S | Low |
| F-009-023 | LOW | Add `# Panics`/`# Errors` rustdoc | S | Low |
| F-009-024 | NIT | Break long mul_add chains | S | Low |
| F-009-025 | NIT | Inline trivial helpers | S | Low |
| F-009-026 | NIT | Hoist magic numbers to consts | S | Low |
| F-009-027 | NIT | PartialEq compare in tests | S | Low |
| F-009-028 | NIT | Named helper for direction loop | S | Low |
