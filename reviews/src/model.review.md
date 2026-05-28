---
unit_id: U004
crate: sectorforge
paths:
  - src/model/mod.rs
  - src/model/errors.rs
  - src/model/ids.rs
  - src/model/rng.rs
  - src/model/taxonomy.rs
  - src/model/sector_model/mod.rs
  - src/model/sector_model/mutation.rs
loc_reviewed: 2766
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 4, medium: 9, low: 11, nit: 6 }
top_risks:
  - "Two conflicting `MutationError` enums share a name (F-004-001)"
  - "`hex_distance` truncates signed difference via `as u32` (F-004-002)"
  - "O(N) linear scans drive every mutation; mass operations are O(N^2)/O(N^3) (F-004-003)"
  - "`stable_pattern_hash` u32→usize cast is fine on 64-bit but masks pattern selection bias (F-004-010)"
---

# Review: U004 — `src/model/` (domain model + RNG)

## Summary

The model module is structurally sound and the RNG layer is exemplary — `rng.rs` is the single
source of stage-keyed entropy as CLAUDE.md mandates, with no `thread_rng()` leakage and clean
blake3 derivation. The two big issues are (1) a duplicate `MutationError` enum that creates two
parallel error vocabularies in the same crate, with only one actually consumed, and (2)
mutation-bus methods that linearly scan `Vec<GeneratedSystem>` on every call — fine for ten
systems, but the `swap_systems` / `remove_system` / `reindex_*` paths are O(N×R) or worse and
will dominate builder latency at large sector sizes. Determinism invariants are preserved:
all aggregating maps are `BTreeMap`, and no `FxHashMap` ever appears in this subtree. API
surface is mostly idiomatic — `#[must_use]` discipline is good for constructors but missing on
pure-query accessors, and several growable enums need `#[non_exhaustive]`. Mutation methods do
several unnecessary `Arc<Vec<…>>` deep clones to mutate `regions` and `chronicle.events`,
which is by far the largest per-operation cost in this file.

## Findings

### F-004-001 — [HIGH] [API design] Duplicate `MutationError` enum, one is dead code
- **Location:** `src/model/errors.rs:41-52` vs `src/model/sector_model/mutation.rs:21-43`
- **Category:** API design / dead code
- **Confidence:** High
- **Blast radius:** Crate-wide — both types are public, callers can pick the wrong one and get
  cryptic conversion errors.
- **Problem:** Two distinct enums named `MutationError` exist in the same crate. The one in
  `errors.rs` (variants `NotFound` / `Collision` / `InvalidCoord` / `InvalidState`) is
  `Clone + Serialize + Deserialize` and looks designed for transport. The one in
  `mutation.rs` (richer variant set: `SystemNotFound` / `WorldNotFound` / `CoordOccupied` / …)
  is what every actual call site uses (`builder/src/builder/command.rs:16`,
  `builder/src/builder/errors.rs:3`, `builder/src/builder/state/regions_ops.rs:80`). The
  `errors.rs` variant is not referenced anywhere in `src/`, `builder/`, `viewer/`, or
  `gui-core/`.
- **Why it matters:** Anyone reading `errors::MutationError` reasonably assumes it's the
  type the mutation API returns. It isn't. Future contributors will either delete the wrong
  one, write code against the wrong one, or add a third.
- **Evidence:** `grep -rn "MutationError" src builder viewer` — every consumer imports
  `sectorforge::sector_model::mutation::MutationError`.
- **Suggested fix:** Delete the unused `errors::MutationError` (lines 41-52 of
  `errors.rs`) entirely. If a `Clone + Serialize` variant is needed in the future, add
  derives to the mutation-module one (after auditing `std::io::Error` non-Clone constraints
  from `SectorError` — `MutationError` has no `Io` variant, so this is straightforward).
- **Effort:** S
- **Risk of fix:** Low — pure deletion.

### F-004-002 — [HIGH] [Panics / correctness] `hex_distance` truncates negative offsets via `as u32`
- **Location:** `src/model/sector_model/mod.rs:841-850`
- **Category:** Panics / numeric correctness
- **Confidence:** High
- **Blast radius:** Every renderer / route distance calc — `hex_distance` is called from
  `add_route`, `move_system`, `swap_systems`, `swap_route_endpoints`, and indirectly from
  every export path that recomputes distance.
- **Problem:** Line 849 returns `dx.max(dy).max(dz) as u32` where the maxes are `i32`.
  `.abs()` on `i32::MIN` is itself UB-equivalent (returns `i32::MIN` in release), and a
  negative `i32` cast to `u32` is a silent two's-complement wraparound that produces a
  `~4e9` distance. This is reachable: `HexCoord` exposes `pub q: i32, pub r: i32`, and
  `mutation.rs:50-54` only checks `coord.q < 0 || coord.r < 0`, but the cube conversion at
  `offset_r_to_cube` line 853-857 subtracts `(c.r - (c.r & 1)) / 2` — for adversarial
  coords from deserialized JSON, `r = i32::MIN` overflows the subtraction.
- **Why it matters:** Any consumer that calls `hex_distance` on a deserialized
  `GeneratedSector` without first re-validating coords can hit either an arithmetic
  overflow panic (debug) or a silent garbage distance (release) that propagates through
  the route table.
- **Evidence:** Read of line 849 + bounds check in `mutation.rs:50` only guards
  intra-builder coords, not deserialized data.
- **Suggested fix:** Use `i32::checked_sub` chain or `u32::try_from(dx.max(dy).max(dz)).unwrap_or(u32::MAX)`,
  or simply validate coord magnitudes in a `Deserialize` constraint. Minimal:
  ```rust
  let max = dx.max(dy).max(dz);
  u32::try_from(max).unwrap_or(0)  // negative → impossible after .abs()
  ```
  Better: assert `max >= 0` and return `max as u32` with a comment explaining why.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-003 — [HIGH] [Performance] All mutations are O(N) linear scans; bulk ops are O(N²) or O(N³)
- **Location:** `src/model/sector_model/mutation.rs:74-93`, `96-135`, `153-197`, `219-236`,
  `347-365`, `692` (`apply_id_migrations`)
- **Category:** Performance / algorithmic complexity
- **Confidence:** High
- **Blast radius:** Every builder mutation; degrades quadratically with sector size.
- **Problem:** Every lookup is `self.systems.iter().find(|s| s.id == *id)`. That's fine for
  a tiny sector but:
  - `move_system` does **three** linear scans of `self.systems` for each touched route
    (line 124-125 inside a `filter_map` over `self.routes`). For S systems and R routes
    anchored on `id`, this is O(R·S). Then it does another full route scan to apply
    updates (line 130).
  - `swap_systems` does similar O(R·S) work (line 186-189).
  - `remove_system` (line 80-89) does O(S) remove + O(R) route filter + O(F·S) faction
    presence filter + O(F·W·R\_world) — for large sectors this is the slowest single
    mutation.
  - `reindex_sequential` + `apply_id_migrations` is O((S+W) + R·log(S+W) + F·(S+W)),
    acceptable, but the implementation re-iterates `self.routes` and clones
    `sys_map` / `world_map` once per route just to call `route_id`.
- **Why it matters:** §S6 swap-on-collision and the auto-layout system motion path call
  these in tight loops. At 200 systems × 400 routes (a credible upper-bound sector), each
  `move_system` is roughly 80 000 string comparisons.
- **Evidence:** Read of methods cited.
- **Suggested fix:** Maintain `BTreeMap<SystemId, usize>` (id → index into `self.systems`)
  and `BTreeMap<RouteId, usize>` on `GeneratedSector`, rebuilt on `remove`/`reindex`. The
  current `Vec`-only layout is simpler but has reached its scaling ceiling. If that's a
  bigger refactor than wanted, at least:
  - Build a `BTreeMap<&SystemId, HexCoord>` once at the top of `move_system` /
    `swap_systems`, then look up endpoint coords from it instead of re-scanning.
  - Skip the second `.find` loop in `move_system` line 130 by collecting `(usize, u32)`
    pairs (vec index) instead of `(RouteId, u32)`.
- **Effort:** M (lookup index) / S (caching coord map inline)
- **Risk of fix:** Medium — touches the most-tested mutation paths; golden tests must pass.

### F-004-004 — [HIGH] [Performance] `add_region` / `remove_region` / `add_region_hex` / `remove_region_hex` deep-clone an `Arc<Vec<WarpRegion>>` on every call
- **Location:** `src/model/sector_model/mutation.rs:449-501`
- **Category:** Performance / unnecessary clone
- **Confidence:** High
- **Blast radius:** Region editing — the builder hex-paint loop fires `add_region_hex`
  potentially every mouse-move sample.
- **Problem:** Each call does `let mut regions = (*self.regions).clone();` (full deep-clone
  of every `WarpRegion`, every `Vec<HexCoord>`), mutates, then rewraps as a fresh `Arc`.
  The `Arc<Vec<WarpRegion>>` design suggests the original goal was cheap clones for
  derivation snapshots, but the mutation path defeats that by always cloning. The same
  pattern repeats for `chronicle.events` at lines 602-630.
- **Why it matters:** A single `add_region_hex` call on a 50-region sector with average
  100 hexes per region copies ~5 000 `HexCoord`s — every brush stroke.
- **Evidence:** Read of the five `regions`-mutating methods.
- **Suggested fix:** Use `Arc::make_mut(&mut self.regions)` (after wrapping the inner
  `Vec` in something `Arc`-internal supports — i.e. switch the field to
  `Arc<Vec<WarpRegion>>` and rely on `Arc::make_mut`, which clones only when there are
  other strong references). Same fix for `chronicle.events`.
  ```rust
  // before:
  let mut regions = (*self.regions).clone();
  regions.retain(|r| r.id != id);
  self.regions = std::sync::Arc::new(regions);
  // after:
  let regions = std::sync::Arc::make_mut(&mut self.regions);
  regions.retain(|r| r.id != id);
  ```
  When no derivation cache holds a snapshot, this becomes a zero-copy mutation.
- **Effort:** S
- **Risk of fix:** Low — `Arc::make_mut` is the canonical pattern for this exact case.

### F-004-005 — [MEDIUM] [Idiomatic Rust] Missing `#[non_exhaustive]` on public, growth-prone enums
- **Location:** `src/model/sector_model/mod.rs:60` (`SystemKind`), `401` (`RouteType`),
  `419` (`RouteKind`), `446` (`RouteViewMode`), `617` (`RoutePattern`), `691`
  (`RouteStability`), `790` (`FactionInfluence`), `926` (`DominanceState`), `961`
  (`ClaimType`), `1020` (`SystemState`); `errors.rs:5` (`SectorError`),
  `mutation.rs:22` (`MutationError`)
- **Category:** API design
- **Confidence:** High
- **Blast radius:** SemVer — adding a new `RouteType` variant (already done once in §3
  NEXT) silently breaks every downstream `match`.
- **Problem:** None of the public model enums carry `#[non_exhaustive]`. `RouteType` has
  visibly grown from `StableWarpLane`/`ChartedPassage`/`SecretPassage` to six variants;
  `ClaimType` has eleven; `RoutePattern` has twenty — these are exactly the kinds of
  enums where `#[non_exhaustive]` exists.
- **Why it matters:** Public crate consumers (workspace builder/viewer) must update every
  match exhaustive on these enums when a new variant lands. With `#[non_exhaustive]`,
  downstream code is forced to write a `_ =>` arm and breakage is opt-in.
- **Suggested fix:** Add `#[non_exhaustive]` to every enum listed above. The internal
  matches in this same module use full enumeration anyway (`RouteType::ALL`), so the
  attribute doesn't cost ergonomics.
- **Effort:** S
- **Risk of fix:** Low — only affects downstream callers that use exhaustive matches.

### F-004-006 — [MEDIUM] [Panics] `DominanceState::from_score` may wrap on NaN / extreme floats
- **Location:** `src/model/sector_model/mod.rs:947-958`
- **Category:** Panics / numeric robustness
- **Confidence:** High
- **Blast radius:** Reachable from any control-score caller; `score` is `f32`, derived
  from `PresenceDimensions::local_control_score` which is unbounded.
- **Problem:** `score.round() as i32` traps on NaN (returns 0 in release, panics in
  some `as` lint configurations) and silently saturates to `i32::MIN/MAX` for huge
  floats. NaN propagation is realistic: any `f32::NAN` field in `PresenceDimensions`
  produces NaN, and `0.round() as i32` is the contract violation point.
- **Why it matters:** A single NaN influence weight (e.g. from `0.0 / 0.0` upstream) maps
  to `Rumored` instead of being detected as invalid input.
- **Evidence:** Read of `local_control_score` line 902-923 — no NaN guard; `from_score`
  line 949 trusts the float.
- **Suggested fix:**
  ```rust
  pub fn from_score(score: f32) -> Self {
      if !score.is_finite() { return Self::Rumored; }
      let s = score.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32;
      ...
  }
  ```
- **Effort:** S
- **Risk of fix:** Low.

### F-004-007 — [MEDIUM] [API design] Public mutation API duplicates `MutationError` across two modules with different variants
- **Location:** `src/model/errors.rs:41` and `src/model/sector_model/mutation.rs:22`
- **Category:** API design
- **Confidence:** High
- **Blast radius:** See F-004-001. Filed separately because the symptom (variant
  divergence) is independent of the dead-code observation; even if both were used, the
  fact that `errors::MutationError::NotFound(String)` collapses what `mutation::MutationError`
  splits into `SystemNotFound`/`WorldNotFound`/`RouteNotFound`/`FactionNotFound`/
  `RegionNotFound` is a regression of information.
- **Suggested fix:** Subsumed by F-004-001 — delete `errors::MutationError`.
- **Effort:** S (already covered)
- **Risk of fix:** Low.

### F-004-008 — [MEDIUM] [Ownership] `apply_id_migrations` clones `BTreeMap` keys unnecessarily; route loop builds two heap strings per touched route
- **Location:** `src/model/sector_model/mutation.rs:736-743`
- **Category:** Ownership / cloning
- **Confidence:** High
- **Blast radius:** Hot on `reindex_sequential` for large sectors.
- **Problem:** `route.from_system_id = SystemId::new(new_from.clone())` clones the
  `String` value out of `sys_map` only to feed it into `Arc::from(String)` inside
  `SystemId::new`. Then `route_id(&route.from_system_id, &route.to_system_id)` formats a
  fresh `String` per route. The double-allocation could be one allocation if the map
  stored `SystemId`/`WorldId` instead of `String`/`String`. Also, lines 692 and 722 pass
  `sys_map.clone()` / `world_map.clone()` to `apply_id_migrations` while the originals
  are still needed for the return value — that's two full map clones per reindex.
- **Why it matters:** With S=200 systems and R=400 routes, each reindex copies ~600
  `String`s twice.
- **Suggested fix:** Change the helper to `apply_id_migrations(&self, sys_map: &BTreeMap…,
  world_map: &BTreeMap…)`; inside, do `let new_id = SystemId::from(new_from.as_str());`
  (one `Arc::from` allocation). Or, more invasively, store
  `BTreeMap<SystemId, SystemId>` in `apply_id_migrations` and convert once at the
  boundary.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-009 — [MEDIUM] [API design / determinism] `reindex_stable` returns empty maps but is shaped as if it returned a tombstone table
- **Location:** `src/model/sector_model/mutation.rs:696-724`
- **Category:** API contract / determinism
- **Confidence:** High
- **Blast radius:** Every caller of `reindex_ids(true)` — see `id_history` propagation.
- **Problem:** `reindex_stable` constructs `old_to_new_sys` / `old_to_new_world` as
  empty `BTreeMap`s (line 699-700), never inserts anything, then calls
  `apply_id_migrations(empty, empty)` which short-circuits at line 731. The function's
  signature promises `(BTreeMap<String,String>, BTreeMap<String,String>)` but the
  "stable" path can never produce a non-empty pair. Callers that branch on tombstone
  presence (e.g. for compat-mode `id_history` accumulation) will silently get empty
  maps even when a system was actually given a new ID at lines 703-707.
- **Why it matters:** Loss of audit trail. A system that started with `index == 0` /
  `id.is_empty()` gets a fresh ID assigned at line 705, but there is no `(old, new)`
  recorded, so `id_history` never learns about it.
- **Evidence:** Read of `reindex_stable`. Test at line 846-851 only checks that the
  existing ID was preserved; nothing tests the freshly-assigned case.
- **Suggested fix:** When an `is_empty`/`index == 0` system gets a new id, push
  `(old_id.to_string(), new_id.to_string())` into `old_to_new_sys`. Same for worlds.
  Add a unit test that constructs a system with an empty `SystemId` and asserts that
  `reindex_ids(true)` records a tombstone.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-010 — [MEDIUM] [Determinism] `stable_pattern_hash` uses `as usize` with no documented portability guarantee
- **Location:** `src/model/sector_model/mod.rs:569-577`, `592-607`
- **Category:** Determinism / portability
- **Confidence:** Medium
- **Blast radius:** Route pattern rendering — affects byte-stable SVG/HTML golden tests
  if anyone ever builds on a 32-bit target.
- **Problem:** `pool[(stable_pattern_hash(self, key) as usize) % pool.len()]` — the
  `u32 → usize` cast is identity on 64-bit, identity on 32-bit, but the FNV-1a
  implementation at lines 592-607 reuses a custom rolling u32 hash with no test that
  it's stable across architectures. The combination is fine in practice but the comment
  block claiming "deterministic" should call out the assumption (or use blake3, which
  the rest of the crate already pulls in).
- **Why it matters:** The crate's golden-test contract is byte-stable output; an
  ad-hoc FNV variant in the model layer is one more thing to audit if a future
  refactor wants to swap in a smaller hash.
- **Suggested fix:** Either (a) call through to `crate::rng::derive_stage_seed` and
  take `u32::from_le_bytes(seed[..4])`, unifying on blake3, or (b) add an inline test
  that asserts the FNV hash matches a known fixture for ASCII keys (lock in cross-arch
  stability). Option (a) is preferred since it removes a parallel implementation.
- **Effort:** S
- **Risk of fix:** Medium — changes golden outputs, requires regenerating fixtures.

### F-004-011 — [MEDIUM] [API design] `add_history_event` / `remove_history_event` / `edit_event` deep-clone `events` on every call
- **Location:** `src/model/sector_model/mutation.rs:602-630`
- **Category:** Performance / ownership
- **Confidence:** High
- **Blast radius:** History editing path; not as hot as region painting but called per-keystroke.
- **Problem:** Same anti-pattern as F-004-004: `let mut events = self.chronicle.events.clone();
  events.push(ev); self.chronicle.events = events;`. The clone is unconditional and full.
  Unlike `regions`, `events` is a plain `Vec<HistoryEvent>` (not behind `Arc`) so this is
  literally `events.push(ev)` masquerading as a clone.
- **Why it matters:** Pure waste — three or four times the memory traffic of a direct
  `self.chronicle.events.push(ev)`.
- **Suggested fix:**
  ```rust
  pub fn add_history_event(&mut self, ev: crate::history::HistoryEvent) {
      self.chronicle.events.push(ev);
  }
  pub fn remove_history_event(&mut self, idx: usize) -> Result<(), MutationError> {
      if idx >= self.chronicle.events.len() {
          return Err(MutationError::EventNotFound(idx));
      }
      self.chronicle.events.remove(idx);
      Ok(())
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — strictly removing waste.

### F-004-012 — [MEDIUM] [Concurrency / API] `GeneratedSector` carries `Arc<…>` derived-overlay fields that mutation methods sometimes deep-clone, breaking copy-on-write
- **Location:** `src/model/sector_model/mod.rs:31-46`, mutation paths cited above
- **Category:** API contract / shared ownership
- **Confidence:** Medium
- **Blast radius:** Builder undo/redo + derivation cache (LD1) snapshotting.
- **Problem:** The `Arc<InfluenceField>`, `Arc<PowerProjectionMap>`, `Arc<RelationsMatrix>`,
  `Arc<Vec<WarpRegion>>`, `Arc<EconomyReport>` layout exists so that a snapshot taken
  by the derivation cache can be O(1) cloned and held for incremental diffing. But the
  mutation methods break this in two ways: (a) `add_region*` always allocates a fresh
  `Arc<Vec<…>>` (F-004-004), and (b) nothing prevents a panel from grabbing a `&mut
  GeneratedSector` and writing one of these `Arc` fields directly — there's no setter
  contract.
- **Why it matters:** The whole `Arc` design pays a complexity cost (every read site
  does `&*sector.regions` instead of `&sector.regions`) for a benefit the mutation
  layer doesn't honour.
- **Suggested fix:** Either (a) commit to `Arc::make_mut` everywhere mutation occurs
  (F-004-004 covers regions; also chronicle, economy, etc.), or (b) drop the `Arc`
  wrapping for fields the mutation API edits, keeping it only on truly derivation-cached
  fields (`influence_field`, `power_projection`, `relations`, `economy`). Recommend
  option (b) for `regions` — it's mutated by user action, not by derivation.
- **Effort:** M
- **Risk of fix:** Medium — public field type change.

### F-004-013 — [LOW] [Idiomatic Rust] `get_world` is O(S·W); a `WorldId` → `(sys_idx, world_idx)` lookup would be O(log N) or O(1)
- **Location:** `src/model/sector_model/mod.rs:274-283`
- **Category:** Performance / API
- **Confidence:** High
- **Blast radius:** Any consumer that walks worlds by id in a loop (panels, exports).
- **Problem:** Nested `for sys ... for w ...` linear scan.
- **Suggested fix:** Same as F-004-003 — `BTreeMap<WorldId, (usize, usize)>` lookup
  index rebuilt on mutation. Lower priority than the system index since worlds are
  fetched less often.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-014 — [LOW] [Idiomatic Rust] `get_worlds_for_system` returns `Vec` instead of an iterator
- **Location:** `src/model/sector_model/mod.rs:285-287`
- **Category:** API design
- **Confidence:** High
- **Blast radius:** Any caller is forced into a heap allocation.
- **Problem:** `pub fn get_worlds_for_system<'a>(&self, sys: &'a GeneratedSystem) -> Vec<&'a GeneratedWorld>`
  is equivalent to `sys.worlds.iter().collect()`. Every caller forces a `Vec`
  allocation only to immediately iterate.
- **Suggested fix:**
  ```rust
  pub fn get_worlds_for_system<'a>(&self, sys: &'a GeneratedSystem) -> impl Iterator<Item = &'a GeneratedWorld> {
      sys.worlds.iter()
  }
  ```
  Or just inline the `.iter()` at call sites and delete this helper — it adds no value.
- **Effort:** S
- **Risk of fix:** Low — but does change the return type, so a quick downstream sweep
  is needed.

### F-004-015 — [LOW] [Performance] `rng::hex` uses `format!("{b:02x}")` in a per-byte loop
- **Location:** `src/model/rng.rs:71-77`
- **Category:** Performance / hot path
- **Confidence:** High
- **Blast radius:** `digest_bytes` (line 27-29) is called from every manifest write and
  the builder derivation cache; reasonably hot.
- **Problem:** `format!` allocates an intermediate `String` per byte; `push_str` then
  copies it again. For a 32-byte blake3 hash that's 64 allocations to produce one
  string.
- **Suggested fix:**
  ```rust
  pub fn hex(bytes: &[u8]) -> String {
      const HEX: &[u8; 16] = b"0123456789abcdef";
      let mut s = String::with_capacity(bytes.len() * 2);
      for b in bytes {
          s.push(HEX[(b >> 4) as usize] as char);
          s.push(HEX[(b & 0xF) as usize] as char);
      }
      s
  }
  ```
  Or pull in `hex` crate (already a transitively likely dep) and use `hex::encode`.
- **Effort:** S
- **Risk of fix:** Low — covered by `stage_seed_is_stable` and other determinism tests.

### F-004-016 — [LOW] [API design] `add_faction` silently no-ops on collision and still returns the input id
- **Location:** `src/model/sector_model/mutation.rs:331-345`
- **Category:** API design / error swallowing
- **Confidence:** High
- **Blast radius:** Builder faction creation path.
- **Problem:** `if !self.factions.iter().any(|f| f.id == id) { … push … }; id`. The
  caller cannot tell whether the faction was newly inserted or already existed; the
  `name` and `kind` arguments are silently discarded on collision. Other mutation
  methods return `Result<…, MutationError::Collision>` for similar shapes.
- **Suggested fix:**
  ```rust
  pub fn add_faction(&mut self, id: FactionId, name: &str, kind: &str)
      -> Result<FactionId, MutationError> {
      if self.factions.iter().any(|f| f.id == id) {
          return Err(MutationError::FactionNotFound(format!("collision: {id}")));  // or new Duplicate variant
      }
      self.factions.push(GeneratedFaction { … });
      Ok(id)
  }
  ```
  Add a `DuplicateFaction(String)` variant since collision is its own concept.
- **Effort:** S
- **Risk of fix:** Low — small downstream sweep.

### F-004-017 — [LOW] [Idiomatic Rust] Public model `get_*` methods lack `#[must_use]`
- **Location:** `src/model/sector_model/mod.rs:266-291`
- **Category:** Idiomatic Rust
- **Confidence:** High
- **Blast radius:** None at runtime; missed lint signal.
- **Problem:** `get_system`, `get_system_mut`, `get_world`, `get_worlds_for_system`,
  `all_worlds` all return data and have no side effects, but are not annotated. The
  `empty` / `new_at` / `new` / `pattern_with_salt` / `pattern` constructors are
  annotated, so the project already cares about this lint category.
- **Suggested fix:** Add `#[must_use]` to all pure accessors.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-018 — [LOW] [API design] `FactionInfluence::weight` is missing `#[must_use]`
- **Location:** `src/model/sector_model/mod.rs:799-809`
- **Category:** Idiomatic Rust
- **Confidence:** High
- **Suggested fix:** Add `#[must_use]`. Same for `PowerProfile::total_projection` — it
  has the annotation. `weight` is the lone exception.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-019 — [LOW] [API design] `add_region` returns the input `id` as `String`; callers already have it
- **Location:** `src/model/sector_model/mutation.rs:449-466`
- **Category:** API design
- **Confidence:** High
- **Suggested fix:** Return `()` and let the caller carry the id; or, if the goal is
  symmetry with `add_system → Result<SystemId>`, validate uniqueness and return
  `Result<String, MutationError::Collision>`. As written, the function can never fail
  and the return value is always `id.to_string()`.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-020 — [LOW] [Idiomatic Rust] `weighted_index` skips invalid weights but emits no diagnostic on every-zero pool
- **Location:** `src/model/rng.rs:33-68`
- **Category:** Error handling / debuggability
- **Confidence:** Medium
- **Problem:** When every weight is zero or NaN, the function returns
  `Err(WeightedSelectionFailed { context })`. The `context` string is the only
  diagnostic; the caller doesn't learn whether the pool was empty, all-zero, or
  contained NaN. Useful for triaging "stage RNG failed" bug reports.
- **Suggested fix:** Add a debug-only `eprintln!`/`tracing::debug!` carrying pool length
  and sum, or extend `WeightedSelectionFailed` with `pool_len: usize, total: f64`.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-021 — [LOW] [Documentation] `derive_stage_seed` doesn't document the format string contract
- **Location:** `src/model/rng.rs:8-12`
- **Category:** Documentation / determinism
- **Confidence:** High
- **Problem:** `format!("sectorforge:{root_seed}:{stage}:{discriminator}")` is the
  load-bearing string for every deterministic draw. Changing it (even reformatting
  whitespace) invalidates every golden output the project produces. The doc comment
  says "Derive a 32-byte stage seed" but doesn't warn that the format is a stability
  contract.
- **Suggested fix:** Add a `# Stability` doc section: "The literal format string and
  delimiter are part of the public stability contract. Changing it requires
  regenerating every golden fixture."
- **Effort:** S
- **Risk of fix:** Low.

### F-004-022 — [LOW] [Idiomatic Rust] `taxonomy.rs` has three near-identical 30-90 line `parse_<variant>` match arms; consider `strum::EnumString`
- **Location:** `src/model/taxonomy.rs:48-217`
- **Category:** Idiomatic Rust / maintenance
- **Confidence:** High
- **Problem:** `parse_world_type_variant`, `parse_government_variant`,
  `parse_notable_feature_variant` each repeat every enum variant as a string literal
  match arm. Adding a new world type or notable feature requires touching both
  `worlds.rs` and `taxonomy.rs`, with no compile-time check that they stay in sync.
- **Why it matters:** This has rotted before (the upstream `worlds::NotableFeature`
  has more than 90 variants and was almost certainly the source of historical
  bugs/typos here).
- **Suggested fix:** Annotate the upstream enums with `#[derive(strum::EnumString)]`
  (or just `#[derive(Deserialize)]` and route through `serde_json::from_value`) so the
  parser is generated from the enum definition. Or, at minimum, add a unit test that
  walks every variant of every enum and round-trips it through the parser.
- **Effort:** M (strum dep) / S (round-trip test)
- **Risk of fix:** Low.

### F-004-023 — [LOW] [Documentation] `# Errors` and `# Panics` sections missing across mutation API
- **Location:** `src/model/sector_model/mutation.rs` — every public `pub fn` returning `Result<_, MutationError>`
- **Category:** Documentation
- **Confidence:** High
- **Problem:** Standard Rustdoc convention dictates that any function returning
  `Result` should document `# Errors` and any panicking function should document
  `# Panics`. None of the mutation methods carry these sections.
- **Suggested fix:** Add brief `# Errors` blocks listing the variants each method can
  return. Bulk task, easily worth an hour for the readability gain.
- **Effort:** M
- **Risk of fix:** Low.

### F-004-024 — [NIT] [Idiomatic Rust] `apply_id_migrations(sys_map.clone(), world_map.clone())` could pass by ref
- **Location:** `src/model/sector_model/mutation.rs:692, 722`
- **Category:** Idiomatic Rust
- **Confidence:** High
- **Problem:** See F-004-008. The clone is solely so the same map can be returned to
  the caller.
- **Suggested fix:** Change `apply_id_migrations` to take `&BTreeMap<String, String>`;
  delete the `.clone()` calls.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-025 — [NIT] [Idiomatic Rust] `digest_bytes` returns `String` allocated via `format!` chain
- **Location:** `src/model/rng.rs:27-29`
- **Category:** Performance / nit
- **Confidence:** Medium
- **Problem:** See F-004-015 (already covered); the fix to `hex` automatically fixes
  `digest_bytes`.
- **Suggested fix:** Covered by F-004-015.
- **Effort:** -
- **Risk of fix:** -

### F-004-026 — [NIT] [Documentation] `RouteType::pattern_key` exists in its own `impl` block at line 609-613, separate from `RouteType::key` at line 473-482
- **Location:** `src/model/sector_model/mod.rs:609-613`
- **Category:** Documentation / maintainability
- **Problem:** `pattern_key` returns `self.key()` and is only used inside
  `stable_pattern_hash`. The separate `impl` block makes it visually look unrelated to
  the main `RouteType` impl, and the method just delegates.
- **Suggested fix:** Inline `pattern_key` into `stable_pattern_hash` (call `.key()`
  directly), delete the lone `impl RouteType` block.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-027 — [NIT] [Idiomatic Rust] `ids.rs` macro `From<$name> for String` allocates twice via `.to_string()`
- **Location:** `src/model/ids.rs:100-110`
- **Category:** Performance nit
- **Problem:** `value.0.to_string()` on an `Arc<str>` does `<str as ToString>::to_string`
  which is one allocation; that's fine. The clone of `value.0` is implicit and free
  because `Arc` clone is a refcount bump, but `From<&$name>` does a full
  `value.0.to_string()` deep copy. That's correct but worth a `#[inline]`.
- **Suggested fix:** Add `#[inline]` and document the allocation behaviour. Or expose
  `as_arc_str(&self) -> &Arc<str>` for callers that want to share ownership without
  reallocating.
- **Effort:** S
- **Risk of fix:** Low.

### F-004-028 — [NIT] [Documentation] `into_string` on ID newtypes is misleadingly named (it `to_string`s a `&str`, doesn't move)
- **Location:** `src/model/ids.rs:46-49`
- **Category:** API naming
- **Problem:** Convention is `into_string(self) -> String` should consume `self` and
  yield the owned string with zero extra allocation when possible. Here, `self.0` is
  `Arc<str>`, so the only zero-copy path is `Arc::try_unwrap` then `Box<str>::into()`.
  Currently the method does `self.0.to_string()` — always allocates. Either rename
  to `to_owned_string` to signal allocation, or implement the `Arc::try_unwrap` happy
  path:
  ```rust
  pub fn into_string(self) -> String {
      match Arc::try_unwrap(self.0) {
          Ok(s) => s.into(),  // Box<str> → String, no alloc
          Err(arc) => arc.to_string(),
      }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low.

### F-004-029 — [NIT] [Idiomatic Rust] Bounds check in `add_system` / `move_system` uses `coord.q < 0 || (coord.q as u32) >= self.width`; the cast is dead
- **Location:** `src/model/sector_model/mutation.rs:50-54, 97-101`
- **Category:** Idiomatic Rust
- **Problem:** After `coord.q < 0` is checked, the cast `coord.q as u32` is
  non-truncating but reads as if it might wrap. Reader has to convince themselves it's
  safe.
- **Suggested fix:**
  ```rust
  fn in_bounds(coord: HexCoord, w: u32, h: u32) -> bool {
      u32::try_from(coord.q).is_ok_and(|q| q < w)
          && u32::try_from(coord.r).is_ok_and(|r| r < h)
      }
  ```
  Then `if !in_bounds(...) { return Err(...) }` at both sites — extracts the
  duplicated check.
- **Effort:** S
- **Risk of fix:** Low.

## Per-rubric coverage

- **3.1 Panics:** F-004-002 (HIGH, `as u32` truncation), F-004-006 (MEDIUM, NaN cast),
  F-004-029 (NIT, bounds-check clarity). `unwrap()`/`expect()` in production model
  code: none in non-test code — all `unwrap`s are in `#[cfg(test)]`. Pass.
- **3.2 unsafe:** No `unsafe` blocks anywhere in the subtree. Pass.
- **3.3 Ownership / cloning:** F-004-004 (regions deep-clone), F-004-008
  (apply_id_migrations clones), F-004-011 (chronicle events clone), F-004-027 (id
  macro allocations). Largest impact: F-004-004.
- **3.4 Error handling:** F-004-001 / F-004-007 (duplicate enum), F-004-016
  (add_faction silently no-ops), F-004-020 (weighted_index diagnostic).
- **3.5 Concurrency / async:** N/A — no threading in this subtree. `Arc<…>` fields
  exist for derivation-cache snapshotting only; see F-004-012.
- **3.6 Performance:** F-004-003 (O(N²) mutations), F-004-013 (O(S·W) get_world),
  F-004-014 (Vec instead of iterator), F-004-015 (hex format!), F-004-004
  (region clones).
- **3.7 Idiomatic / API:** F-004-005 (non_exhaustive), F-004-017 (must_use on
  getters), F-004-018 (must_use on weight), F-004-019 (add_region return value),
  F-004-022 (taxonomy duplication), F-004-026 (split impl block), F-004-028
  (into_string naming).
- **3.8 Dependencies:** No unused imports observed in any reviewed file. The
  `rand`/`rand_chacha`/`blake3` deps in `rng.rs` are all used. Pass.
- **3.9 Memory:** F-004-012 (Arc contract violations). No `Drop` impls in this
  subtree, no static muts, no growing caches without eviction.
- **3.10 Testing:** Inline tests are present for `rng.rs`, `ids.rs`, `taxonomy.rs`,
  `sector_model/mod.rs`, `mutation.rs`. Gaps: F-004-009 (reindex_stable tombstone
  not tested), F-004-002 (no negative-coord round-trip), `weighted_index` is tested
  only for trivial pools (no fuzz on weight distribution). No `#[ignore]` markers,
  no sleeps. Reasonable coverage overall.
- **3.11 Documentation:** F-004-021 (rng stability contract), F-004-023 (missing
  `# Errors` on mutation API), F-004-028 (into_string naming). Module-level docs
  exist on `ids.rs`, `taxonomy.rs`, `rng.rs`, `mod.rs`. Pass on the structural
  basics.

## CLAUDE.md determinism invariant audit

- **No `FxHashMap`/`HashMap` iteration for output:** Confirmed. The entire model
  subtree uses `BTreeMap` exclusively (`mod.rs:5, 248, 829`, `mutation.rs:9, 665,
  699, 728-729`). Pass.
- **All RNG draws through `model/rng.rs`:** Confirmed. `grep -rn "thread_rng\|seed_from_entropy" src/model/` 
  returns nothing. Pass.
- **`stage_rng`/`derive_stage_seed`/`hash_root_seed` are all stage-keyed via blake3:**
  Confirmed at `rng.rs:9-22`. The public surface is minimal and correct: every public
  RNG factory requires a stage discriminator. Pass.
- **No public `RngCore` returned without a stage key:** Confirmed. `stage_rng` is the
  only public function returning a `ChaCha8Rng`, and it takes all three required
  parameters by `&str`. No `Default`/`new`/`from_entropy` escape hatch exists. Pass.
- **Builder mutations through the command bus:** Out of scope for this unit (model
  exposes mutation methods; the bus discipline is enforced in builder/). The fact
  that `apply_id_migrations` is `pub(crate)`-by-omission (actually `pub` because
  it's an inherent method on `GeneratedSector`) is borderline — but it's invoked
  only internally, so not a finding.

## Summary of suggested fixes

- F-004-001 — HIGH — delete dead duplicate `errors::MutationError` — S/Low
- F-004-002 — HIGH — guard `hex_distance` against `i32::MIN.abs()` / negative → u32 wrap — S/Low
- F-004-003 — HIGH — add `BTreeMap<SystemId, usize>` lookup index; eliminate O(N²) mutations — M/Med
- F-004-004 — HIGH — switch region/chronicle Arc mutations to `Arc::make_mut` — S/Low
- F-004-005 — MEDIUM — add `#[non_exhaustive]` to all 12 public growable enums — S/Low
- F-004-006 — MEDIUM — `DominanceState::from_score` guard against NaN / huge floats — S/Low
- F-004-007 — MEDIUM — covered by F-004-001 — S/Low
- F-004-008 — MEDIUM — `apply_id_migrations` take maps by `&`; eliminate clone-then-allocate — S/Low
- F-004-009 — MEDIUM — `reindex_stable` must record tombstones for newly-assigned IDs — S/Low
- F-004-010 — MEDIUM — replace ad-hoc FNV `stable_pattern_hash` with blake3, or test cross-arch — S/Med
- F-004-011 — MEDIUM — chronicle event mutations are bare clones; remove them — S/Low
- F-004-012 — MEDIUM — clarify `Arc<…>` ownership contract on `GeneratedSector` overlay fields — M/Med
- F-004-013 — LOW — `get_world` should use a `WorldId` lookup index — S/Low
- F-004-014 — LOW — `get_worlds_for_system` should return `impl Iterator` — S/Low
- F-004-015 — LOW — `rng::hex` should avoid per-byte `format!` — S/Low
- F-004-016 — LOW — `add_faction` should return `Result<…, Collision>` rather than silent no-op — S/Low
- F-004-017 — LOW — add `#[must_use]` to model getter methods — S/Low
- F-004-018 — LOW — add `#[must_use]` to `FactionInfluence::weight` — S/Low
- F-004-019 — LOW — `add_region` return value is pointless; return `()` — S/Low
- F-004-020 — LOW — extend `WeightedSelectionFailed` with pool_len/total for triage — S/Low
- F-004-021 — LOW — document `derive_stage_seed` format string as a stability contract — S/Low
- F-004-022 — LOW — collapse `taxonomy.rs` parsers via `strum::EnumString` or round-trip test — M/Low
- F-004-023 — LOW — add `# Errors` rustdoc sections to mutation API — M/Low
- F-004-024 — NIT — pass `BTreeMap`s by reference in `apply_id_migrations` — S/Low
- F-004-025 — NIT — covered by F-004-015 — -/-
- F-004-026 — NIT — fold lone `pattern_key` `impl` block into main `impl RouteType` — S/Low
- F-004-027 — NIT — `#[inline]` on ID `From` impls; expose `as_arc_str` — S/Low
- F-004-028 — NIT — `into_string` either rename or implement `Arc::try_unwrap` happy path — S/Low
- F-004-029 — NIT — extract `in_bounds` helper; replace `as u32` with `u32::try_from` — S/Low
