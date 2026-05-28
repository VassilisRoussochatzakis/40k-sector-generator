---
unit_id: U007
crate: sectorforge
paths:
  - src/gen/generation/mod.rs
  - src/gen/generation/factions.rs
  - src/gen/generation/placement.rs
  - src/gen/generation/routes.rs
  - src/gen/generation/systems.rs
  - src/gen/generation/world_placement.rs
  - src/gen/regions.rs
  - src/gen/sites.rs
  - src/gen/archetypes.rs
  - src/gen/hidden_routes.rs
loc_reviewed: 4769
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 3, medium: 9, low: 8, nit: 5 }
top_risks:
  - "Hidden-route emit can panic via `.unwrap()` when an endpoint is double-filtered (F-007-001)"
  - "Faction assignment uses `BTreeMap::entry` keyed on subfaction_id ordering for ranking — tie-break asymmetric in a way that breaks `a≤b ⇔ b≥a` in the sort comparator (F-007-002)"
  - "`apply_route_effects_with_progress` triggers O(R²) bridge-check BFS across all regions on long perilous degrades (F-007-003)"
---

# Review: U007 — generation pipeline core + regions/sites/archetypes/hidden routes

## Summary

Generation core is well-structured: a single linear pipeline in `generation/mod.rs` threads progress + cancellation, every RNG draw flows through `rng::stage_rng` (no `thread_rng`/`from_entropy` anywhere), and output containers are `BTreeMap`/`BTreeSet` so determinism invariants hold. The biggest soft spots are (1) a small but real panic surface in `hidden_routes::emit_layer` where `.unwrap()` relies on an invariant that two `BTreeSet`s stay in lock-step, (2) repeated per-iteration `format!` + `Vec` allocations in the per-pair loop in `generation/routes.rs` and per-world loop in `world_placement.rs`, (3) O(R²) bridge-preservation BFS that runs once per region-affected route. Nothing violates the FxHashMap / RNG / output-stability invariants from `CLAUDE.md`.

## Findings

### F-007-001 — [HIGH] [Panics] `endpoint_by_id.unwrap()` relies on a brittle invariant
- **Location:** `src/gen/hidden_routes.rs:416-417`
- **Category:** Panics & failure surface (§3.1)
- **Confidence:** Medium
- **Blast radius:** Whole generation aborts; this runs on every sector with hidden factions.
- **Problem:** `let a = endpoint_by_id.get(from.as_str()).copied().unwrap();` (and `b`) assume that every id in `pairs` was inserted into `endpoint_by_id`. Today that holds because `pairs` is built from `endpoints` and `endpoint_by_id` is built from `endpoints`. But the two structures are derived in two separate passes, and `endpoint_by_id` keys on `s.id.as_str()` while `pairs` keys on cloned `SystemId`s converted back via `as_str()` — any future change that filters `endpoints` after `pairs` is built (e.g. dropping blackout endpoints after pair construction, which is exactly the pattern `configured_hidden_routes` uses) will trip the unwrap.
- **Evidence:** Read of `emit_layer` lines 371-417. The function is `pub(super)`-internal but `append_hidden_routes_*` (public) calls it on adversarial sector input.
- **Suggested fix:** Replace both unwraps with `let-else continue;`. This is the same pattern already used in `configured_hidden_routes` (lines 151-156) so there is a local precedent.
  ```rust
  let Some(a) = endpoint_by_id.get(from.as_str()).copied() else { continue; };
  let Some(b) = endpoint_by_id.get(to.as_str()).copied() else { continue; };
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-007-002 — [HIGH] [Correctness] Per-pair candidate `Vec` reallocated in `factions::assign_factions_inner` ranking loop
- **Location:** `src/gen/generation/factions.rs:131-132`
- **Category:** Performance / Allocation (§3.6) — generation hot path
- **Confidence:** High
- **Blast radius:** Per world × per influence tier (≤3); ~3·W·S allocations per sector.
- **Problem:** Inside the `for inf in influences.iter().take(max_factions)` loop, the code builds a fresh `pairs: Vec<(&SubfactionGroup<'_>, f64)>` clone of the whole `weighted` slice on every iteration just to call `weighted_index`, and then mutates `weighted` with `remove(idx)`. The clone is unnecessary; `weighted_index` only needs a `&[(T, f64)]` slice, and `weighted` is already in that shape.
- **Evidence:** Lines 107-118 build `weighted: Vec<(&SubfactionGroup<'_>, f64)>`; lines 131-132 then construct a parallel `pairs` of the same shape immediately before `weighted_index(&pairs, ...)`. The only reason this exists is the borrow checker — `weighted_index` takes `&[(T, f64)]` so a `&[(&SubfactionGroup, f64)]` slice would satisfy it directly.
- **Suggested fix:** Drop the `pairs` allocation and pass `&weighted` directly. The signature already permits it because `T` in `weighted_index` is generic.
  ```rust
  // remove lines 131-132 entirely:
  let idx = match weighted_index(&weighted, rng, "faction") { ... };
  ```
- **Effort:** XS
- **Risk of fix:** Low — typecheck-only change; existing tests must still pass byte-identically because the allocation didn't affect outcomes.

### F-007-003 — [HIGH] [Performance] `is_navigable_bridge` builds full adjacency BTreeMap on every perilous degrade
- **Location:** `src/gen/regions.rs:720-756` (called from `apply_route_stability` at line 700)
- **Category:** Performance (§3.6) — generation hot path
- **Confidence:** High
- **Blast radius:** Per-perilous-candidate scan: O(R) BTreeMap build + BFS, called from `apply_route_effects_with_progress` which iterates all R routes. Net: O(R²·log R) when many routes intersect WarpStorm regions; on a 4096-system sector with ~3k routes this is ~10M BTreeMap insertions per pipeline run.
- **Problem:** `is_navigable_bridge` recomputes the entire route adjacency map from scratch every time a route is about to be made Perilous. The adjacency is identical between calls (mutating `route.stability` doesn't change endpoints), so it should be built once per `apply_route_effects` call and re-used.
- **Evidence:** Lines 729-738 build `adjacency: BTreeMap<&str, Vec<&str>>` from `routes` on every call. `apply_route_effects_with_progress` (line 547) loops over all R routes; each `WarpStorm` and worse-than-Hazardous degrade calls this function (line 578, 586).
- **Suggested fix:** Hoist the adjacency map into `apply_route_effects_with_progress`, build it once, and pass it as `&BTreeMap<&str, Vec<&str>>` into `apply_route_stability_with_bridge_progress`. The "skip Perilous" filter inside the BFS becomes a per-call boolean instead of a structural skip — track which routes are currently Perilous in a `BTreeSet<usize>` updated in place.
  ```rust
  // build once:
  let mut adjacency: BTreeMap<&str, Vec<(&str, usize)>> = BTreeMap::new();
  for (idx, r) in routes.iter().enumerate() {
      adjacency.entry(r.from_system_id.as_str()).or_default().push((r.to_system_id.as_str(), idx));
      adjacency.entry(r.to_system_id.as_str()).or_default().push((r.from_system_id.as_str(), idx));
  }
  // is_navigable_bridge then takes (adjacency, perilous_set, candidate_idx)
  ```
- **Effort:** M
- **Risk of fix:** Medium — must preserve byte-identical output for golden tests (run `cargo test --test it -- golden`). The change is structural but logic-preserving.

### F-007-004 — [MEDIUM] [Determinism/Correctness] `regions::grow_blob` snap-to-free uses Cartesian neighbourhood instead of hex neighbourhood
- **Location:** `src/gen/regions.rs:417-437`
- **Category:** Idiomatic / Correctness (§3.7)
- **Confidence:** High
- **Blast radius:** Output stability is preserved (deterministic), but the geometry is wrong: when the seed centre is occupied, the function scans an axis-aligned `(dq, dr)` square instead of expanding by hex-ring distance, so blob fall-back locations are biased toward the southwest corner and may step further from the requested centre than necessary.
- **Problem:** `for dq in -radius..=radius { for dr in -radius..=radius { ... break 'outer ... } }` returns the first in-bounds free hex in (dq, dr) iteration order. On an even-r row that's `(-radius, -radius)` first — i.e. NW. The intent is "nearest free hex".
- **Evidence:** Lines 420-432; contrast with `offset_r_neighbors`-based neighbour walks elsewhere in the file (line 451) which correctly handle hex offsets.
- **Suggested fix:** Use a BFS over `offset_r_neighbors` from `centre`, returning the first free in-bounds hex. Bounds: `radius` cap of 4 → ≤61 hexes, trivial.
  ```rust
  let mut q: VecDeque<HexCoord> = VecDeque::from([centre]);
  let mut seen: BTreeSet<(i32,i32)> = BTreeSet::new();
  while let Some(c) = q.pop_front() {
      if !seen.insert((c.q, c.r)) { continue; }
      if in_bounds(c) && !occupied.contains(&(c.q, c.r)) { return Some(c); }
      for (dq, dr) in offset_r_neighbors(c.r) { q.push_back(HexCoord { q: c.q+dq, r: c.r+dr }); }
      if seen.len() > 64 { break; }
  }
  ```
  Note: this changes output bytes, so it requires a golden-test refresh.
- **Effort:** S
- **Risk of fix:** Medium — breaks goldens; gated on whether maintainers want this geometric fix.

### F-007-005 — [MEDIUM] [Performance] `generation/routes.rs` per-pair `Vec<&Arc<str>>` re-collection
- **Location:** `src/gen/generation/routes.rs:45-50` and `classify_route` at 206-211
- **Category:** Performance (§3.6) — generation hot path, O(S²) outer
- **Confidence:** High
- **Blast radius:** Per system pair: builds `combined_tags: Vec<&Arc<str>>` over all worlds of both systems. With S=256 and 4 worlds/system that's ~32k allocs, each a Vec of ~8 elements; then `classify_route` does *the same allocation again* with the same data inside the loop body via line 72.
- **Problem:** Two issues stacked: (a) `combined_tags` is rebuilt per pair, and (b) `classify_route` (called twice — once at line 72 to gate weighting modifiers, once at line 159 during final emission) re-collects the same tag list independently.
- **Evidence:** Lines 45-50 collect once; line 72 calls `classify_route(&systems[i], &systems[j], ...)` which collects again at 206-211; line 159 calls it a *third* time when building the route. Inside the modifier loop (75-99) each tag-match is a linear `iter().any()` across `combined_tags` for every modifier.
- **Suggested fix:** Pre-build a `Vec<BTreeSet<Arc<str>>>` of per-system tag sets at the top of `generate_routes`, then in the loop body construct only refs to the two relevant sets. Skip the re-collection in `classify_route` by passing the sets in. Memoize `classify_route` results into `candidates` so emission at line 159 reuses, not recomputes.
  ```rust
  let sys_tags: Vec<BTreeSet<&str>> = systems.iter()
      .map(|s| s.worlds.iter().flat_map(|w| w.tags.iter().map(|a| a.as_ref())).collect())
      .collect();
  // ... inside the i,j loop: query sys_tags[i].contains(...) / sys_tags[j].contains(...)
  ```
- **Effort:** M
- **Risk of fix:** Low — golden tests will catch any tag-set drift.

### F-007-006 — [MEDIUM] [Performance] `world_placement::tags_for_world` allocates many `String` then `Arc<str>` per world
- **Location:** `src/gen/generation/world_placement.rs:293-314`
- **Category:** Performance (§3.6) — per-world hot path
- **Confidence:** High
- **Blast radius:** Per world: 8 fixed-tag `format!`s + N feature `format!`s, each allocating a `String` then converting to `Arc<str>`. On a 256-system × 4-world sector that's ~10k allocations per sector. Each call also goes through `taxonomy::to_snake_case` which itself allocates.
- **Problem:** Tag values are bounded enum-ish strings (`"world_type:hive_world"` etc). The set is small (≤ ~150 distinct values across the whole sector). Right now every world allocates a fresh `String` and `Arc<str>` even when 99% are duplicates.
- **Suggested fix:** Cache `Arc<str>` per `(enum_variant, kind)` in a `BTreeMap<(StaticTagPrefix, &'static str), Arc<str>>` and reuse. A simpler step: switch each of the 8 fixed tags to a `static_assertions::const_assert!`-style precomputed map keyed on the enum variant name; only `notable_features` need format-time strings.
  Alternatively, since enums implement `Display`, use `format_args!` with `Arc::from(format!(...).as_str())` — that's already happening, so the real win is interning. A `OnceLock<BTreeMap<...>>` populated lazily would suffice; cost is one log-N lookup per tag.
- **Effort:** M
- **Risk of fix:** Low if interning is local; tags must remain byte-stable (Arc::from on the same input is fine because `Vec<Arc<str>>` is sorted at the end anyway).

### F-007-007 — [MEDIUM] [Performance] `sites::derive_with` allocates a `Vec<&GeneratedWorld>` per system via `get_worlds_for_system`
- **Location:** `src/gen/sites.rs:135-136` (and `sector_model::GeneratedSector::get_worlds_for_system` at `src/model/sector_model/mod.rs:285`)
- **Category:** Performance (§3.6) — pipeline post-pass
- **Confidence:** High
- **Blast radius:** One Vec per system per sites derivation. Modest, but the helper is documented as "collect()" of `sys.worlds.iter()` — a pure ergonomic loss.
- **Problem:** `get_worlds_for_system` literally returns `sys.worlds.iter().collect()`, allocating a Vec only to be iterated once.
- **Suggested fix:** In `sites::derive_with`, replace `for w in sector.get_worlds_for_system(sys)` with `for w in &sys.worlds`. Either deprecate or `#[inline]` `get_worlds_for_system` so callers can iterate the slice directly. (`get_worlds_for_system` itself is out-of-unit but the call site is in scope.)
- **Effort:** XS
- **Risk of fix:** Low

### F-007-008 — [MEDIUM] [Idiomatic Rust] `regions::sample_condition` duplicates `weighted_index` logic
- **Location:** `src/gen/regions.rs:349-370`
- **Category:** Idiomatic / DRY (§3.7)
- **Confidence:** High
- **Blast radius:** Maintenance — `weighted_index` in `src/model/rng.rs` already handles non-finite/non-positive weights, total==0 fallback, and last-non-zero recovery; this re-implementation does the same job with a different fallback (silent `Turbulence`) on a corrupted pool.
- **Problem:** Two divergent code paths for the same operation. `weighted_index` returns `Result<usize, SectorError>`; this re-implementation swallows the error and returns `Turbulence`, which is a silent data corruption if all condition weights are 0/NaN/Inf in user-supplied `regions.toml`.
- **Suggested fix:** Replace with `weighted_index(&pool_pairs, rng, "regions.condition")` and bubble the error through `build_regions`. Convert `build_regions` to `Result<Vec<WarpRegion>, SectorError>` (currently infallible) — or, if call-site change is undesirable, log/skip-on-error and return an empty region set, but make the silent fallback explicit.
- **Effort:** S
- **Risk of fix:** Low (interface change is small; build_regions has one caller in `generation/mod.rs`).

### F-007-009 — [MEDIUM] [Determinism/Correctness] `factions::representative_disposition` tie-break direction inverted
- **Location:** `src/gen/generation/factions.rs:322-336`
- **Category:** Idiomatic / Correctness (§3.7)
- **Confidence:** Medium
- **Blast radius:** Determinism is preserved (deterministic given same input) but on a weight-tie the function picks the *lexicographically largest* disposition key (`b.0.cmp(a.0)`) — the comment elsewhere in this file (e.g. line 221) is the opposite convention (sort by id ascending). This asymmetry is a latent footgun for anyone replicating the catalog-order rule.
- **Problem:** `.then_with(|| b.0.cmp(a.0))` — reverses string order on ties, inconsistent with the rest of the file which uses ascending lexicographic on tie-breaks (lines 188-204, 220-221).
- **Suggested fix:** Change to `a.0.cmp(b.0)` for consistency. Will change output bytes — gated on golden refresh.
- **Effort:** XS
- **Risk of fix:** Medium — breaks goldens.

### F-007-010 — [MEDIUM] [Performance] `regions::build_regions` linear-scans `centres.contains(c)` in fallback loop
- **Location:** `src/gen/regions.rs:305-314`
- **Category:** Performance (§3.6) — region build path (small N but quadratic)
- **Confidence:** High
- **Blast radius:** N² in centre fallback fill, capped by `target` (≤count/2). Bounded but unnecessary.
- **Problem:** `if !centres.contains(c)` is O(centres.len()) inside a loop over `all`. Should use a `BTreeSet<(i32,i32)>` next to `centres` to make the lookup O(log N).
- **Suggested fix:**
  ```rust
  let mut centre_set: BTreeSet<(i32,i32)> = centres.iter().map(|c| (c.q, c.r)).collect();
  for c in &all {
      if centres.len() >= target { break; }
      if centre_set.insert((c.q, c.r)) { centres.push(*c); }
  }
  ```
- **Effort:** XS
- **Risk of fix:** Low — identical output bytes.

### F-007-011 — [MEDIUM] [Idiomatic Rust] `routes::generate_routes` `let _ = rng;` reserves an unused parameter
- **Location:** `src/gen/generation/routes.rs:135`
- **Category:** Idiomatic / Maintainability (§3.7)
- **Confidence:** High
- **Blast radius:** API smell. The function takes `&mut ChaCha8Rng` but never draws from it. The `// RNG reserved for future stochastic edges` comment is a load-bearing TODO; a stage_rng draw that doesn't happen is a determinism hazard the moment someone adds randomness here (they'll have to be careful about ordering).
- **Problem:** Holds a mutable RNG it doesn't use. Either drop the parameter or document the intended stream discriminator.
- **Suggested fix:** Drop the `rng: &mut ChaCha8Rng` parameter from `generate_routes` until it's needed; the caller in `generation/mod.rs:394-395` constructs it specifically for this call and would also stop being needed. When future stochastic edges are added, derive a fresh stage RNG with a distinct discriminator (e.g. `rng::stage_rng(seed, "routes", "bridges")`).
- **Effort:** XS
- **Risk of fix:** Low

### F-007-012 — [MEDIUM] [API design] `regions::apply_route_effects` ignores its summary
- **Location:** `src/gen/regions.rs:539-545`
- **Category:** Error handling / API (§3.4, §3.7)
- **Confidence:** High
- **Blast radius:** Tests / external callers lose visibility into route-effect stats.
- **Problem:** The non-progress wrapper does `let _ = apply_route_effects_with_progress(...);` — discarding the summary. Callers wanting metrics must use the progress-callback variant and capture the `Completed` event manually. This is the documented pattern, but the throwaway version should still return the summary.
- **Suggested fix:** Return the summary:
  ```rust
  pub fn apply_route_effects(...) -> RegionRouteEffectsSummary {
      apply_route_effects_with_progress(regions, systems, routes, |_| {})
  }
  ```
  Caller can ignore via `let _ = ...;`.
- **Effort:** XS
- **Risk of fix:** Low — additive API change.

### F-007-013 — [LOW] [Panics] `placement::place_systems` dead branch — `target > total_cells` after `min`
- **Location:** `src/gen/generation/placement.rs:16-25`
- **Category:** Idiomatic Rust (§3.7)
- **Confidence:** High
- **Blast radius:** Unreachable code; harmless but misleading.
- **Problem:** `let target = g.system_count.min(total_cells);` then `if target > total_cells { return Err(...) }` — the second condition can never be true because `target` was just clamped to `≤ total_cells`. Either the clamp is wrong (silent truncation) or the check is dead.
- **Suggested fix:** Either remove the clamp and surface the error (preferred — silent clamp loses caller intent), or remove the dead `if`. If clamping intentionally, log a warning at debug level.
- **Effort:** XS
- **Risk of fix:** Low (removing the clamp would change config-overrun semantics; gated decision).

### F-007-014 — [LOW] [Performance] `factions::aggregate_factions` uses `Vec::contains` for system_presence dedup
- **Location:** `src/gen/generation/factions.rs:441-457`
- **Category:** Performance (§3.6) — pipeline post-pass, S worlds × F factions
- **Confidence:** High
- **Blast radius:** O(S × F × P) where P = peak system_presence length per faction. On large sectors with widespread factions (e.g. Imperium present in every system), P grows to S and the inner `gf.system_presence.contains(&sys.id)` is linear, giving O(S²·F).
- **Problem:** Lines 441-444 push only if not present, then dedup again at lines 468-470 — both passes are linear in `system_presence`.
- **Suggested fix:** Build presence into a `BTreeSet<SystemId>` per faction, then convert once at the end:
  ```rust
  let mut sys_set: BTreeMap<FactionId, BTreeSet<SystemId>> = BTreeMap::new();
  // ... push into sys_set[fid].insert(sys.id.clone());
  // at end: gf.system_presence = sys_set[&gf.id].iter().cloned().collect();
  ```
- **Effort:** S
- **Risk of fix:** Low — sort/dedup at the end is byte-stable.

### F-007-015 — [LOW] [Idiomatic Rust] `rng::hex` uses `format!("{b:02x}")` per byte
- **Location:** `src/model/rng.rs:71-77` (out-of-unit, but called by `generation/mod.rs:812, 816`)
- **Category:** Performance (§3.6) — called once per sector (manifest build)
- **Confidence:** Medium
- **Blast radius:** Pipeline tail; ~32 byte loops × 2 ≈ 64 `format!` calls per generation. Each `format!` allocates a small String. Total cost is < 1ms but the pattern is recurring across the workspace.
- **Problem:** `for b in bytes { s.push_str(&format!("{b:02x}")); }` — `format!` per byte allocates. `write!(&mut s, "{b:02x}").unwrap();` avoids the inner allocation.
- **Suggested fix:**
  ```rust
  use std::fmt::Write as _;
  let mut s = String::with_capacity(bytes.len() * 2);
  for b in bytes { let _ = write!(s, "{b:02x}"); }
  ```
  Note: this is out of unit but called from in-unit code; flag for U008 sweep if owner prefers.
- **Effort:** XS
- **Risk of fix:** Low — identical output.

### F-007-016 — [LOW] [Ownership] `archetypes::apply_all` clones `Arc<str>` for `kinds` map values
- **Location:** `src/gen/archetypes.rs:119-123`
- **Category:** Ownership / Cloning (§3.3)
- **Confidence:** Medium
- **Blast radius:** F clones (faction count); small.
- **Problem:** `kinds: BTreeMap<FactionId, Arc<str>>` clones each `f.kind.clone()`. Since the map is read-only and tied to `&sector.factions`' lifetime, a `BTreeMap<&FactionId, &str>` would avoid both the `Arc` clone and the `FactionId` clone.
- **Suggested fix:**
  ```rust
  let kinds: BTreeMap<&FactionId, &str> = sector.factions.iter()
      .map(|f| (&f.id, f.kind.as_ref())).collect();
  ```
  Update each `kinds.get(&p.faction_id)` call — `kinds.get(&&p.faction_id)` or change call sites to `kinds.get(p.faction_id.borrow())` depending on FactionId's `Borrow` impl.
- **Effort:** S
- **Risk of fix:** Low

### F-007-017 — [LOW] [Idiomatic Rust] `factions::aggregate_factions` `iter_mut().find(|sf| sf.id == *sub_id)` is linear
- **Location:** `src/gen/generation/factions.rs:445, 451`
- **Category:** Performance (§3.6) — pipeline post-pass
- **Confidence:** High
- **Blast radius:** O(world_presences × subfactions_per_faction × forces_per_subfaction). Small in practice, but quadratic shape.
- **Problem:** Two linear scans inside an O(S·W·P) loop. Subfactions and forces would be better keyed in a side `BTreeMap<FactionId, BTreeMap<FactionId, usize>>` for index lookups.
- **Suggested fix:** Build an index map after `build_faction_groups`:
  ```rust
  let mut sub_idx: BTreeMap<&FactionId, BTreeMap<&FactionId, usize>> = ...;
  // and similarly for force_idx.
  ```
  Then `gf.subfactions[sub_idx[&gf.id][sub_id]]` is O(log N).
- **Effort:** S
- **Risk of fix:** Low — output is sorted at the end.

### F-007-018 — [LOW] [Idiomatic Rust] `regions::region_at` is O(R × H) per call
- **Location:** `src/gen/regions.rs:471-474`
- **Category:** Performance (§3.6)
- **Confidence:** High
- **Blast radius:** Called once per route endpoint in `dominant_route_condition`, which is called once per route in `apply_route_effects_with_progress`. So R routes × 2 endpoints × R regions × H hexes per region = O(R²·H̄) — on a 100-region sector with 6-hex regions and 3k routes, ~3.6M `contains` checks. Each `contains` is O(H) linear since `r.hexes` is a `Vec`, not a set.
- **Problem:** `r.hexes.contains(&coord)` is linear in the hex count per region.
- **Suggested fix:** Build a `BTreeMap<(i32,i32), &WarpRegion>` once at top of `apply_route_effects_with_progress` and pass it through `dominant_route_condition`. O(1) lookup.
  ```rust
  let by_hex: BTreeMap<(i32,i32), &WarpRegion> = regions.iter()
      .flat_map(|r| r.hexes.iter().map(move |h| ((h.q, h.r), r)))
      .collect();
  ```
  Note: a hex can belong to only one region (centres are min-distance separated and `grow_blob` writes to `occupied`); no conflict.
- **Effort:** S
- **Risk of fix:** Low

### F-007-019 — [LOW] [Idiomatic Rust] `world_placement::pick_features` uses `Vec::remove(idx)` (O(N))
- **Location:** `src/gen/generation/world_placement.rs:226, 247`
- **Category:** Performance (§3.6)
- **Confidence:** Medium
- **Blast radius:** Per-world feature picking; bounded by `world_feature_count` (typically ≤ 10) and tier size (~30). Total work small.
- **Problem:** `filtered.remove(idx)` shifts the tail every draw. `swap_remove` would be O(1) but changes the iteration ordering for subsequent `weighted_index` draws — which would change RNG consumption and break determinism / goldens.
- **Suggested fix:** Either accept the O(N²) cost (current size makes it irrelevant) or rework to use a `Vec<bool>` "consumed" mask alongside the slice — the weighted_index helper would need a variant that accepts a mask, but determinism is preserved.
- **Effort:** S
- **Risk of fix:** Medium (changes RNG stream → goldens). Recommend leaving as-is and marking as documented.

### F-007-020 — [LOW] [Idiomatic Rust] `sites::derive_with` extends manual sites, breaks player_edition filter
- **Location:** `src/gen/sites.rs:149-152`
- **Category:** Correctness (§3.7)
- **Confidence:** High
- **Blast radius:** Manual sites bypass the `player_edition` `public_status == actual_status` filter, which is applied *before* `extend(cfg.manual.clone())`. If a player-edition export is the intent, manually authored hidden sites leak.
- **Problem:** Filter then extend → manual sites with mismatched statuses survive the filter.
- **Suggested fix:** Extend first, then filter:
  ```rust
  out.extend(cfg.manual.clone());
  if cfg.player_edition { out.retain(|s| s.public_status == s.actual_status); }
  ```
- **Effort:** XS
- **Risk of fix:** Low — behaviour change; check existing tests that rely on the previous order.

### F-007-021 — [NIT] [Documentation] `routes::generate_routes` magic number `0.10` for perilous cap
- **Location:** `src/gen/generation/routes.rs:174`
- **Category:** Documentation (§3.11)
- **Confidence:** High
- **Problem:** `let perilous_limit = ((routes.len() as f64) * 0.10).round() as usize;` — 10% cap is unexplained and unconfigured. Add a named const + comment.
- **Suggested fix:** `const PERILOUS_ROUTE_FRACTION: f64 = 0.10;` with a doc comment referencing the spec section.
- **Effort:** XS
- **Risk of fix:** Low

### F-007-022 — [NIT] [Documentation] `hidden_routes::endpoint_score` `subfaction_id.as_deref()` falls through to kind lookup awkwardly
- **Location:** `src/gen/hidden_routes.rs:308-312`
- **Category:** Idiomatic / Readability (§3.7)
- **Confidence:** Medium
- **Problem:** `p.subfaction_id.as_deref().unwrap_or_else(|| kinds.get(p.faction_id.as_str()).copied().unwrap_or(""))` returns the **subfaction id string** when present, otherwise the faction **kind**. These are different categories of identifier; the `needles` slice (e.g. `&["aeldari","harlequin"]`) is matched against both. This works because aeldari subfaction ids happen to overlap with kinds, but it's a footgun for future faction kinds.
- **Suggested fix:** Match `needles` only against `kind`; if subfaction-level matching is also wanted, take an explicit subfaction-needles list. Or document the dual semantics inline.
- **Effort:** XS (doc) — S if semantics are tightened.
- **Risk of fix:** Low for doc; Medium if semantics change.

### F-007-023 — [NIT] [Idiomatic Rust] `factions::PRIMARY_FACTION_LIMIT` `const` for `truncate` reused twice
- **Location:** `src/gen/generation/factions.rs:17, 223`
- **Category:** Idiomatic Rust (§3.7)
- **Confidence:** High
- **Problem:** Good — the constant is named. NIT: the comment cites "Spec §10.9" but doesn't link to which spec file (`docs/BUILDER_REQS.txt` etc.). Per `CLAUDE.md` "Spec/requirement files live in `docs/` … Reference these by `§<tag>`". A bare `§10.9` is ambiguous.
- **Suggested fix:** Add the spec file shortname, e.g. `// Spec BUILDER_REQS.§10.9`.
- **Effort:** XS
- **Risk of fix:** None

### F-007-024 — [NIT] [Idiomatic Rust] `regions::should_report_region_route_progress` and `hidden_routes::should_report_layer_progress` duplicate the same logic
- **Location:** `src/gen/regions.rs:678-683` and `src/gen/hidden_routes.rs:464-469`
- **Category:** DRY (§3.7)
- **Confidence:** High
- **Problem:** Two near-identical helpers (one uses `total/100`, the other `total/20`). Could be hoisted into a small shared `progress_throttle(current, total, stride: usize) -> bool` helper in `model/rng.rs` or a new `model/progress.rs`.
- **Suggested fix:** Add a small `pub(crate)` helper.
- **Effort:** XS
- **Risk of fix:** Low

### F-007-025 — [NIT] [Documentation] `hidden_routes::HIDDEN_K_NEAREST = 3` magic constant not configurable in two of three layers
- **Location:** `src/gen/hidden_routes.rs:37` (used at line 381)
- **Category:** API design (§3.7)
- **Confidence:** Medium
- **Problem:** `configured_hidden_routes` honours `config.k_nearest`, but `emit_layer` (used by `append_hidden_routes_with_regions*`) ignores it and uses the const. Two paths with different knobs is confusing.
- **Suggested fix:** Either route `HIDDEN_K_NEAREST` through `emit_layer`'s signature as a parameter, or add a public function-level override. Document the dual policy.
- **Effort:** S
- **Risk of fix:** Low

## Category status

### 3.1 Panics & failure surface
F-007-001 (HIGH unwrap), F-007-013 (LOW dead branch). All other `unwrap_or*` calls are total fallbacks and reachable safely.

### 3.2 unsafe & soundness
No findings. No `unsafe` blocks in unit.

### 3.3 Ownership, borrowing, lifetimes, cloning
F-007-016 (Arc clone in archetypes::apply_all). F-007-002 (redundant Vec clone). Otherwise clean — most callsites correctly use `&` borrows or `Arc::clone` on shared strings.

### 3.4 Error handling
F-007-008 (silent fallback in `sample_condition`). F-007-012 (summary dropped). Errors flow through `SectorError` consistently; `weighted_index` is propagated via `?` where used.

### 3.5 Concurrency & async
No findings. No threads / async in unit.

### 3.6 Performance
F-007-003 (O(R²) bridge BFS — hot), F-007-005 (per-pair tag re-collect — hot), F-007-006 (per-world tag formatting — hot), F-007-007, F-007-010, F-007-014, F-007-017, F-007-018, F-007-019. Generation is the build hot path — the F-007-003 / F-007-005 / F-007-006 trio is the biggest collective win.

### 3.7 Idiomatic Rust & API design
F-007-004 (snap-to-nearest geometry), F-007-009 (tie-break direction), F-007-011 (unused RNG param), F-007-020 (filter ordering), F-007-022, F-007-023, F-007-024, F-007-025.

### 3.8 Dependencies & Cargo hygiene
No findings in unit. All imports are used; no over-broad feature use.

### 3.9 Memory & resource management
No findings. No long-lived caches, no `Drop` impls, no `static mut`.

### 3.10 Testing & verification
Inline tests cover the happy path and the documented invariants (deterministic regions, perilous bridge preservation, webway endpoint qualification). Coverage gap: no inline tests for the `Anomaly` bias path in `world_placement::choose_world_candidate`, the `OK ≤ a` tie-break in `factions::assign_factions_inner`'s sort, or the `apply_route_effects` fallback when `regions.is_empty()`. Recommend adding three small tests; not promoted to a finding because integration tests in `tests/it/` are out-of-unit.

### 3.11 Documentation & maintainability
F-007-021 (magic number), F-007-023 (spec ref). Module-level `//!` docs are present on every file and adequate. Public functions have `# Errors` where they return `Result`. No `TODO`/`FIXME` clutter.

## Determinism invariants (CLAUDE.md hard rules)

- **No `FxHashMap`/`FxHashSet` iteration for output** — verified by grep, none used in unit. PASS.
- **All RNG draws via `src/model/rng.rs`** — `grep -r "thread_rng\|from_entropy\|SmallRng"` over `src/gen/` returns zero hits. Every RNG in this unit is `ChaCha8Rng` constructed via `rng::stage_rng`. PASS.
- **Output writers byte-stable** — sort steps observed at: `placement.rs:86`, `regions.rs:464`, `world_placement.rs:114, 312`, `generation/mod.rs:586-589`, `hidden_routes.rs:171, 351`, `factions.rs:174-204, 210-222, 278, 314, 468-481`, `sites.rs:153, 496`, `routes.rs:108-113, 196`. All emit paths land on sorted `Vec`s or `BTreeMap`/`BTreeSet`s. PASS.
- **Builder mutations through the command bus** — N/A (this is the generation pipeline, not the builder).

## Summary of suggested fixes

- F-007-001 — HIGH — `let-else continue` in place of `.unwrap()` for `endpoint_by_id.get` — XS / Low
- F-007-002 — HIGH — drop the per-iteration `pairs` Vec clone in `assign_factions_inner`, pass `&weighted` directly — XS / Low
- F-007-003 — HIGH — hoist route-adjacency BTreeMap out of `is_navigable_bridge` into the surrounding loop — M / Medium
- F-007-004 — MEDIUM — replace Cartesian snap-to-free in `grow_blob` with hex-ring BFS — S / Medium
- F-007-005 — MEDIUM — precompute per-system tag sets; pass into `classify_route` to avoid 3× recompute — M / Low
- F-007-006 — MEDIUM — intern Arc<str> tag values per world (or precompute fixed-tag table) — M / Low
- F-007-007 — MEDIUM — iterate `sys.worlds` directly in `sites::derive_with`, skip `get_worlds_for_system` Vec — XS / Low
- F-007-008 — MEDIUM — `regions::sample_condition` → call `weighted_index` and propagate error — S / Low
- F-007-009 — MEDIUM — fix tie-break direction in `representative_disposition` (`a.0.cmp(b.0)`) — XS / Medium (golden refresh)
- F-007-010 — MEDIUM — back `centres` with a `BTreeSet` to avoid O(N²) contains — XS / Low
- F-007-011 — MEDIUM — drop unused `rng: &mut ChaCha8Rng` parameter from `generate_routes` — XS / Low
- F-007-012 — MEDIUM — return summary from `apply_route_effects` instead of discarding — XS / Low
- F-007-013 — LOW — remove dead `target > total_cells` check (or remove the prior `.min()`) — XS / Low
- F-007-014 — LOW — accumulate `system_presence`/`world_presence` into `BTreeSet`s before sort/dedup — S / Low
- F-007-015 — LOW — replace per-byte `format!` in `rng::hex` with `write!` — XS / Low
- F-007-016 — LOW — store `&FactionId`/`&str` in `archetypes::apply_all`'s `kinds` map — S / Low
- F-007-017 — LOW — index subfactions/forces by id for O(log N) lookup in `aggregate_factions` — S / Low
- F-007-018 — LOW — precompute `(q,r) -> &WarpRegion` map in `apply_route_effects_with_progress` — S / Low
- F-007-019 — LOW — pick_features: leave `Vec::remove` as-is (documented) — S / Medium
- F-007-020 — LOW — extend manual sites before applying the player-edition filter — XS / Low
- F-007-021 — NIT — name the 10% perilous-cap constant — XS / Low
- F-007-022 — NIT — document dual semantics of `endpoint_score`'s needle match — XS / Low
- F-007-023 — NIT — qualify spec section references with file shortname — XS / None
- F-007-024 — NIT — extract shared `progress_throttle` helper — XS / Low
- F-007-025 — NIT — surface `HIDDEN_K_NEAREST` as a parameter through `emit_layer` — S / Low
