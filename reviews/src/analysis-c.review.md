---
unit_id: U011
crate: sectorforge
paths:
  - src/analysis/control.rs
  - src/analysis/route_control.rs
  - src/analysis/stability.rs
  - src/analysis/intel.rs
  - src/analysis/influence_field.rs
  - src/analysis/power_projection.rs
  - src/analysis/interestingness.rs
  - src/analysis/conflict.rs
  - src/analysis/importance.rs
loc_reviewed: 3781
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 9, low: 11, nit: 6 }
top_risks:
  - "Non-deterministic HashMap iteration in display-bucket aggregation feeds golden-tested legend rendering (F-011-001)"
  - "Tie-break direction differs between derive_world_control and derive_system_control (F-011-002)"
  - "RouteControl `pirate` kind list contains 'chaos' which never matches any real kind (F-011-003)"
---

# Review: src/analysis/ PART C

## Summary
Scoring layer that turns finalised generation data into per-world/per-system/per-faction snapshots. Mostly pure, well-organised. `partial_cmp` defended with `unwrap_or(Equal)` — NaN doesn't crash. Biggest risks: (a) non-deterministic HashMap in `importance::compute_display_buckets` feeding golden writers, (b) divergent tie-break directions in world vs system control, (c) stale `"chaos"` matcher in `route_control` that never matches actual kind catalog so chaos endpoints emit no piracy.

## Findings

### F-011-001 — [HIGH] [Determinism] `compute_display_buckets` uses `std::collections::HashMap` for kind-group aggregation; bucket order is non-deterministic and feeds byte-stable renderers
- **Location:** `src/analysis/importance.rs:139-179`
- **Confidence:** High; consumed by `src/export/bitmap/legend.rs:70`, `src/export/svg_export/legend.rs:61` (golden-tested).
- **Fix:** Replace with `BTreeMap<KindGroup, AggregateAcc>`; derive `PartialOrd, Ord` on `KindGroup`. Add tertiary tie-break to final sort.
- Effort S / Risk Low.

### F-011-002 — [HIGH] [Correctness] Tie-break direction reversed between `derive_world_control` and `derive_system_control`
- **Location:** `src/analysis/control.rs:368-372` vs `:473-481`/`493-497`
- `derive_world_control`: `.then_with(|| a.0.cmp(&b.0))` (descending-id wins via outer descending sort). `derive_system_control::pick`: `.then(b.0.cmp(a.0))` inside ascending `max_by` (ascending-id wins). A system can pick "alpha" while its worlds pick "beta".
- **Fix:** Pick one convention (recommend ascending-id wins); shared helper. S/Low.

### F-011-003 — [HIGH] [Correctness] `route_control::derive_one` checks `kind == "chaos"` but no faction has that kind — chaos endpoints never produce piracy
- **Location:** `src/analysis/route_control.rs:142`
- `let pirate = matches!(kind, "chaos" | "criminal" | "drukhari" | "ork" | "rebel");` — `"chaos"` never matches `chaos_space_marine`/`chaos_knight`/`traitor_guard`/`daemon`/`cult`.
- **Fix:** Expand matcher. S/Low.

### F-011-004 — [MEDIUM] [Correctness] `claim_for` `imperial` branch has dead `disposition == "lawful"` arm
- **Location:** `src/analysis/control.rs:326-331`. Both arms return `ClaimType::ImperialMandate`.
- **Fix:** Fold to single return or make lawful → `LegalSovereignty`. XS/Low.

### F-011-005 — [MEDIUM] [Performance] `influence_field::build` dense `cell_scores` even when most cells are zero
- **Location:** `src/analysis/influence_field.rs:156-198`. 200×200 sector × 30 factions ≈ 19 MB mostly zeros.
- **Fix:** `BTreeMap<(usize, usize), f32>` or hybrid sparse/dense. M/Medium.

### F-011-006 — [MEDIUM] [Correctness] `CellAssignment::score` hard-clipped at 100; multi-anchor overlap saturates
- **Location:** `src/analysis/influence_field.rs:241-247`. Comment says "Normalise"; code clamps.
- **Fix:** Soft saturation `100.0 * raw / (raw + 100.0)` or normalise against field max. S/Medium.

### F-011-007 — [MEDIUM] [Performance] `route_control::derive_route_controls` recomputes `endpoint_aggregates` per route
- **Location:** `src/analysis/route_control.rs:199-240`. For R routes, N systems: aggregate recomputed R/N times.
- **Fix:** Hoist per-system precompute `BTreeMap<&str, Aggregate>`. S/Low.

### F-011-008 — [MEDIUM] [Performance] `conflict::advance_sector` clones `sector.relations` every tick
- **Location:** `src/analysis/conflict.rs:172-187`. 30 factions → 435 cloned pairs per tick.
- **Fix:** Destructure `&mut sector` for independent borrows. S/Low.

### F-011-009 — [MEDIUM] [Correctness] `conflict::derive_world_conflict` second-place fragile on ties
- **Location:** `src/analysis/conflict.rs:74-90`. `.fold` with `<` never updates on ties → `gap=0`, `intensity=100`, `attacker==defender`.
- **Fix:** Sort presences and take top 2. S/Low.

### F-011-010 — [MEDIUM] [Correctness] `intel::derive_world_suspected` lets NaN `raw_conf` slip through `< 5.0` filter
- **Location:** `src/analysis/intel.rs:212-226`. `NaN < 5.0` is false; NaN as u32 → 0.
- **Fix:** `if !raw_conf.is_finite() || raw_conf < 5.0 { continue; }`. XS/Low.

### F-011-011 — [MEDIUM] [Performance] `intel::derive_observer_view` walks all presences twice per observer; O(P²)
- **Location:** `src/analysis/intel.rs:147-195`.
- **Fix:** Hoist per-world `BTreeMap<&str, f32>` of `faction_id → visibility` once. M/Low.

### F-011-012 — [LOW] [Idiomatic] `bfs_distances` ignores `_id` parameter; module doc claims per-owner gating that doesn't exist
- **Location:** `src/analysis/power_projection.rs:108-138`.
- **Fix:** Implement owner gating or drop the parameter + misleading doc. S/Low.

### F-011-013 — [LOW] [Performance] `bfs_distances` clones `SystemId` 3× per visited node
- **Location:** `src/analysis/power_projection.rs:108-138`.
- **Fix:** Intern system ids to `u32` indices at `project_sector` start. M/Low.

### F-011-014 — [LOW] [Performance] `power_projection::system_top_reach` linearly scans every faction per call
- **Location:** `src/analysis/power_projection.rs:191-204`.
- **Fix:** Precompute inverse map `BTreeMap<SystemId, (FactionId, f32)>`. S/Low.

### F-011-015 — [LOW] [Performance] `interestingness::profile_targets` uses `BTreeMap<String, MetricTarget>` with `name.to_string()` on `&'static str` literals
- **Location:** `src/analysis/interestingness.rs:154-281`.
- **Fix:** Use `BTreeMap<&'static str, MetricTarget>`. S/Low.

### F-011-016 — [LOW] [Idiomatic] `route_control::merge_endpoints` open-codes a two-element weighted average
- **Location:** `src/analysis/route_control.rs:77-102`.
- **Fix:** `impl AddAssign for Aggregate` + generic `merge(slice)`. S/Low.

### F-011-017 — [LOW] [API design] Scoring fns return bare `f32`; mixing `local_control_score`/`display_importance`/`total_projection` compiles silently
- **Location:** `control.rs:260, 365, 464-469`; `importance.rs:112-115`; `power_projection.rs:53, 167`.
- **Fix:** `ControlScore(f32)`/`DisplayImportance(f32)`/`ProjectedPower(f32)` newtypes. M/Low.

### F-011-018 — [LOW] [Idiomatic] `scale_dimensions` visibility floor coupled inconsistently with caller cap
- **Location:** `src/analysis/control.rs:96-106`. `d.visibility *= k.max(0.3)`; Hidden additionally capped by caller.
- **Fix:** Drop `.max(0.3)` and rely on explicit Hidden override, or document. XS/Low.

### F-011-019 — [LOW] [Documentation] `apply_to_factions` scales only 5 of 9 `PowerProfile` components
- **Location:** `src/analysis/power_projection.rs:165-187`.
- **Fix:** Document or scale all. XS/Low.

### F-011-020 — [LOW] [Performance] `derive_world_claims` allocates a `FactionClaim` even on non-replacing path
- **Location:** `src/analysis/control.rs:269-278`.
- **Fix:** Peek strength first, build claim only on insert/replace. XS/Low.

### F-011-021 — [NIT] [Idiomatic] `claim_for` builds a `String` to run `id.contains(...)`
- **Location:** `src/analysis/control.rs:288-341`.
- **Fix:** `parts.iter().any(|p| p.contains(needle))`. XS/Low.

### F-011-022 — [NIT] [Documentation] `stability::any_kind` mixes subfaction ids and faction kinds silently
- **Location:** `src/analysis/stability.rs:54-72, 125-131`.
- **Fix:** Rename to `any_subfaction_or_kind` + doc. XS/Low.

### F-011-023 — [NIT] [Performance] `should_report_influence_progress` re-derives stride per inner-cell call
- **Location:** `src/analysis/influence_field.rs:201-256, 286-299`.
- **Fix:** Hoist stride outside loop. XS/Low.

### F-011-024 — [NIT] [Documentation] `influence_field::build` comment says "Normalise" but code clamps
- **Location:** `src/analysis/influence_field.rs:237-240`. XS/Low.

### F-011-025 — [NIT] [Documentation] `route_control` module doc says secrecy = `100 - avg(visibility)` but code adds 15-point stealth bonus
- **Location:** `src/analysis/route_control.rs:6-22` vs `:174-177`. XS/Low.

### F-011-026 — [NIT] [UX] `interestingness::describe` formats integer-shaped metrics as `5.00`
- **Location:** `src/analysis/interestingness.rs:355-372`.
- **Fix:** `{:.0}` for count metrics. XS/Low.

## Rubric coverage

- **3.1 Panics:** No reachable `panic!`/`unreachable!`/`todo!`. All `partial_cmp` defended (`unwrap_or(Equal)`).
- **3.2 unsafe:** No findings.
- **3.3 Ownership/clone:** F-011-008, F-011-013, F-011-020.
- **3.4 Error handling:** Only one `Result` returned; clean. No swallowing.
- **3.5 Concurrency:** N/A — single-threaded.
- **3.6 Performance:** F-011-005/007/008/011/013/014/015/023.
- **3.7 Idiomatic/API:** F-011-004/012/016/017/018/019/021/022.
- **3.8 Cargo hygiene:** `use crate::{FxMap, FxSet}` in `power_projection.rs` justified (private BFS, never iterated for output).
- **3.9 Memory:** No findings.
- **3.10 Testing:** Missing strongest-wins tie-break test, chaos-kind piracy test, determinism proptest for `compute_display_buckets`, agreement test for world vs system tie-breaks.
- **3.11 Documentation:** F-011-012, F-011-019, F-011-024, F-011-025.

## Project invariants

- **Fx* not iterated for output:** **F-011-001 violates this** → HIGH.
- **RNG via `model/rng.rs`:** No RNG in this unit.
- **Byte-stable writers:** F-011-001, F-011-002 affect renderer-visible ordering; F-011-006 affects renderer-visible values.
- **Builder command bus:** N/A — read-only.

## Summary of suggested fixes

- F-011-001 — HIGH — Replace `HashMap` with `BTreeMap` in `compute_display_buckets`; add `Ord` to `KindGroup` — S/Low
- F-011-002 — HIGH — Unify tie-break direction across `derive_world_control` and `derive_system_control` — S/Low
- F-011-003 — HIGH — Expand `pirate` matcher in `route_control::derive_one` — S/Low
- F-011-004 — MEDIUM — Remove dead `lawful` branch in `control::claim_for` — XS/Low
- F-011-005 — MEDIUM — Switch `influence_field` to sparse storage on large sectors — M/Medium
- F-011-006 — MEDIUM — Soft saturation or normalisation in `CellAssignment::score` — S/Medium
- F-011-007 — MEDIUM — Hoist `endpoint_aggregates` to per-system precompute — S/Low
- F-011-008 — MEDIUM — Destructure `&mut sector` in `conflict::advance_sector` — S/Low
- F-011-009 — MEDIUM — Sort-then-take-top-2 in `derive_world_conflict` — S/Low
- F-011-010 — MEDIUM — Filter NaN explicitly in `intel::derive_world_suspected` — XS/Low
- F-011-011 — MEDIUM — Cache per-world visibility maps in `intel::derive_observer_view` — M/Low
- F-011-012 — LOW — Implement owner gating or drop unused `_id` in `bfs_distances` — S/Low
- F-011-013 — LOW — Intern system ids to indices in `project_sector` BFS — M/Low
- F-011-014 — LOW — Precompute inverse top-per-system map in `power_projection` — S/Low
- F-011-015 — LOW — Use `&'static str` keys in `interestingness::profile_targets` — S/Low
- F-011-016 — LOW — Add `AddAssign`/`merge` impl on `Aggregate` — S/Low
- F-011-017 — LOW — Introduce score newtypes — M/Low
- F-011-018 — LOW — Document/remove `k.max(0.3)` floor in `scale_dimensions` — XS/Low
- F-011-019 — LOW — Apply reach scaling to all `PowerProfile` components or document — XS/Low
- F-011-020 — LOW — Avoid `FactionClaim` alloc on non-replace path — XS/Low
- F-011-021 — NIT — Match components in `claim_for` without concatenation — XS/Low
- F-011-022 — NIT — Rename `any_kind` + doc — XS/Low
- F-011-023 — NIT — Hoist stride precompute — XS/Low
- F-011-024 — NIT — Fix "normalisation" doc — XS/Low
- F-011-025 — NIT — Document stealth bonus — XS/Low
- F-011-026 — NIT — `{:.0}` for count metrics — XS/Low
