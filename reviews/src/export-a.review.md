---
unit_id: U012
crate: sectorforge
paths:
  - src/export/segmentum.rs
  - src/export/subsectors/mod.rs
  - src/export/subsectors/summary.rs
  - src/export/system_map.rs
loc_reviewed: 3495
reviewed_by: agent
health_score: 3
finding_counts: { critical: 0, high: 3, medium: 9, low: 8, nit: 6 }
top_risks:
  - "Lloyd clustering re-resolves every seed id with O(n) linear scan inside O(n·k·iter) inner loop (F-012-002)"
  - "stitch_pair `index out of range` panic on right/bottom border systems when sector dimension is 0 (saturation arithmetic on u32) (F-012-001)"
  - "Per-system-call `Vec<&GeneratedWorld>` allocations re-issued 4-7× per cell in summary aggregation (F-012-004)"
---

# Review: src/export PART A — segmentum + subsectors + system_map

## Summary

Determinism discipline is good throughout — all output-touching maps are `BTreeMap`/`BTreeSet` and the one Fx-like surface (`scores` in summary) is collected from an already-deterministic source. The structural risks are concentrated in two places: (1) `subsectors/mod.rs::cluster_systems` is `O(n²·k·iter)` because it leans on `sector.get_system(&id)`, which is a linear scan of the system list; that scales badly on large sectors. (2) `summary.rs` calls `sector.get_worlds_for_system(sys)` repeatedly per cell — the helper allocates a fresh `Vec` each call, producing 4-7 throwaway heap allocations per system. There are a handful of unwrap/expect points that look unreachable in normal flow but are not annotated as such, plus some signed-cast paths in `segmentum::stitch_pair` that go through silent `u32 as i32` arithmetic.

## Findings

### F-012-001 — [HIGH] [Panic] `stitch_pair` casts `border_depth` u32 → i32 without check
- **Location:** `src/export/segmentum.rs:646,656,665,681,688`
- **Category:** Panic / Overflow
- **Confidence:** Medium
- **Blast radius:** segmentum compose path on user-supplied `border_depth`
- **Problem:** `let depth = stitch.border_depth.max(1);` is `u32`, then used as `depth as i32` in `w - depth as i32` and `s.coord.q < depth as i32`. `StitchConfig::border_depth` is a `pub u32` controlled by `segmentum.toml`. A pathological value (e.g. `border_depth = 2_147_483_648`) wraps to negative on the `as i32` cast, making the bound `w - (negative) = w + huge`. With `s.coord.q < 0` allowed elsewhere (the donor filter doesn't bound q below) the predicate then accepts every system. Worse, `(w - 1 - s.coord.q) as u32` casts a negative `i32` to `u32`, producing a huge donor distance that subsequently overflows in `da + db + 1` (line 706, `u32 + u32 + 1` — `da+db` can wrap on overflow in debug builds, panic in release-checked builds; silently wrap in default release). Not memory-unsafe, but turns a config-validation failure into wrong-but-silent stitches.
- **Why it matters:** Output bytes diverge from spec; hard to debug. CLAUDE.md treats config validation as a HIGH bar.
- **Suggested fix:** Validate `border_depth` in `validate_config` (e.g. clamp to `min(width, height)` of any child or reject `> 1024`). Use `i32::try_from(depth).unwrap_or(i32::MAX)` at the boundary, or carry `depth` as `i32` after one checked conversion at the top of `stitch_pair`. Same for the donor-distance arithmetic on 706 — use `da.saturating_add(db).saturating_add(1)`.
- **Effort:** S
- **Risk of fix:** Low

### F-012-002 — [HIGH] [Performance] Lloyd refinement is O(n²·k·iter) because of `sector.get_system` linear scan
- **Location:** `src/export/subsectors/mod.rs:537,574,597-598,624-629`
- **Category:** Performance / Algorithmic
- **Confidence:** High
- **Blast radius:** `build_subsectors`, runs at every export and inside `compose` per child
- **Problem:** Every `sector.get_system(&id)` call is `O(n_systems)` (see `src/model/sector_model/mod.rs:266` — `self.systems.iter().find(...)`). Inside `cluster_systems`:
  - Seeding loop (lines 530-565): for each candidate system, `seeds.iter().map(|sid| sector.get_system(sid).unwrap())` → `O(n·k·n)` per seed picked, `O(n²·k²)` total.
  - Lloyd assign step (lines 571-580): `for sys in &sector.systems { for seed_id in &seeds { let seed_sys = sector.get_system(seed_id).unwrap(); ... } }` → `O(n·k·n) = O(n²·k)` per iteration, multiplied by `max_iterations = 24` → `O(n²·k·24)`.
  - Seed update (lines 594-606): `max_by` calls `sector.get_system(a)/get_system(b)` on every compare — another factor of `n` per comparison.
  - Final seed-id-to-index conversion (lines 622-631): `seeds.iter().map(|sid| sector.systems.iter().position(...))` → `O(n·k)`.
  For a Segmentum with 8 children × ~250 systems × k=20, this becomes minutes of CPU instead of milliseconds.
- **Why it matters:** Export is "build time, not hot loop", but cumulatively this dominates `compose` time on large segmenta. §3.6 PERF rubric.
- **Suggested fix:** Build an `id → index` `BTreeMap` once at the top of `cluster_systems` and keep `seeds` as `Vec<usize>` from the start; look up coordinates via `sector.systems[idx].coord` directly. The score lookups can use the same map. Sketch:
  ```rust
  let id_to_idx: BTreeMap<&str, usize> = sector.systems.iter().enumerate()
      .map(|(i, s)| (s.id.as_str(), i)).collect();
  let mut seed_idx: Vec<usize> = vec![first_seed_index];
  // when comparing distances:
  let seed_sys = &sector.systems[seed_idx[ci]];
  ```
- **Effort:** M
- **Risk of fix:** Low (pure refactor — output ordering is preserved because indices are already deterministic)

### F-012-003 — [HIGH] [Determinism / Stability] `seed_score` uses route_degree by str key but seeds chosen by score without total tie-breaker on every candidate
- **Location:** `src/export/subsectors/mod.rs:507-525,594-606`
- **Category:** Determinism / Correctness
- **Confidence:** Medium
- **Blast radius:** Subsector ids / labels / capitals (output bytes)
- **Problem:** The seed-update phase at lines 594-606 ties on `(score, sys.index)` using `sys_b.index.cmp(&sys_a.index)` — but `members` is built from a `BTreeMap<SystemId, usize>` iteration (deterministic) and `max_by` returns the **last** element under ties. The third tie-breaker is `sys_b.id.cmp(&sys_a.id)` — i.e. **descending** id. The initial seeding loop (line 523) breaks ties **ascending** id. Two different tie-break orders for "same kind of decision" makes the seed set order-sensitive in a way that's surprising — and if the system list is ever reordered (e.g. by a future tweak in generation), the seeds will flip. Determinism tests exist (line 962-977) but only assert same-input-same-output, not stability across cosmetic input permutations.
- **Why it matters:** It is currently deterministic, but the asymmetry is a foot-gun: a refactor to switch seeding to also use `max_by` would silently change output bytes. Golden tests would catch it; design smell remains.
- **Suggested fix:** Pick one tie-break direction across both seeding and refinement. Concretely, in line 603-604 change to `sys_a.index.cmp(&sys_b.index).reverse()` so the comment "tie by lowest sys.index" is correctly expressed; or refactor both seeding/refinement to a shared `fn cmp_candidate(a, b) -> Ordering`.
- **Effort:** S
- **Risk of fix:** Medium (changes output bytes; needs golden refresh)

### F-012-004 — [MEDIUM] [Performance] `sector.get_worlds_for_system(sys)` allocates fresh `Vec` 4-7× per system in summary
- **Location:** `src/export/subsectors/summary.rs:45,58,201,214,520-535,602-622,627,656,667-670` and `src/export/subsectors/mod.rs:641-645` (via `seed_score`)
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** Per-cell summary loop; scales with `system_count × max_iterations` indirectly
- **Problem:** `GeneratedSector::get_worlds_for_system` (`src/model/sector_model/mod.rs:285-287`) returns `sys.worlds.iter().collect::<Vec<_>>()` — a heap allocation for every call. `populate_summary` calls it twice on the same `sys` per loop iteration (lines 201, 214). `pick_capital` calls it five times per system (lines 602, 607, 613, 619, 627, 656, 667). `resolve_system_owners` calls it twice (lines 45, 58). `seed_score` is only called once per system but uses the same pattern via `&sys.worlds` (correct there).
- **Why it matters:** Export build time. For a 250-system sector that's ~2000 short-lived `Vec` allocs per `build_subsectors`. Easy win.
- **Suggested fix:** Replace `sector.get_worlds_for_system(sys)` usages here with direct `&sys.worlds` (it's just `sys.worlds.iter().collect()` anyway — these paths don't need the abstraction). Or change the helper to return `&[GeneratedWorld]` rather than `Vec<&GeneratedWorld>`. Sketch for the local call sites:
  ```rust
  total_worlds += sys.worlds.len() as u32;
  for w in &sys.worlds { ... }
  ```
- **Effort:** S
- **Risk of fix:** Low (output unchanged)

### F-012-005 — [MEDIUM] [Performance / Allocation] `pick_capital` allocates `lower(t)` strings + new `Vec<String>` per system via `inferred_prosperity_rank`
- **Location:** `src/export/subsectors/summary.rs:755-799`
- **Category:** Performance / Allocation
- **Confidence:** High
- **Blast radius:** `pick_capital` is run once per cell at build, but `inferred_prosperity_rank` is called for every world inside every system inside every cell (line 616), and each call builds `tokens = Vec<String>` from `to_ascii_lowercase` on each tag.
- **Problem:** Lines 757-763 build a `Vec<String>` of lowercased tags then run two `iter().any(|t| t.contains(...))` passes (lines 764-779) and one more (lines 787-794). For a sector with ~250 systems × ~3 worlds × ~5 tags ≈ 3700 short String allocations per `build_subsectors` call.
- **Why it matters:** Same blast radius as F-012-004 — export build time.
- **Suggested fix:** Don't materialise a `Vec<String>` — fold the lowercase + contains check into a single pass. Or stash a `&str` slice and use `eq_ignore_ascii_case` / case-insensitive substring matching (see e.g. https://docs.rs/aho-corasick) — but simplest:
  ```rust
  let any_token = |needles: &[&str]| -> bool {
      let sources = w.tags.iter().chain(w.world.notable_features.iter()).chain(sys.tags.iter());
      sources.any(|t| {
          let lo = t.to_ascii_lowercase();
          needles.iter().any(|n| lo.contains(n))
      })
  };
  if any_token(&["trade", "market", "commerce", "port", "hub"]) { bonus += 1; }
  ```
  The inner lowercase still allocates but one String per tag instead of a full `Vec` per call.
- **Effort:** S
- **Risk of fix:** Low

### F-012-006 — [MEDIUM] [Performance] `format!` in inner loops for hex-cluster value (`subsector-tmp-{i}`) and stitch link ids
- **Location:** `src/export/subsectors/mod.rs:232,336` and `src/export/segmentum.rs:753`
- **Category:** Performance / Allocation
- **Confidence:** Medium
- **Blast radius:** Once per cluster (subsectors) / once per accepted link (stitch). Bounded by `k` and `max_links_per_pair × pairs`.
- **Problem:** `format!("subsector-tmp-{i}").into()` on line 232 creates a String for an id that's overwritten on line 336 (`cell.id = format!("subsector-{}", slugify(...)).into()`). The temporary id is never observable. Similar throwaway formatting on line 753 (`format!("sl-{:04}", *link_idx - 1)`) is unavoidable but writes through `String → Arc<str>` for `id: String` (no `Arc`).
- **Why it matters:** Minor — k is bounded — but the tmp-id allocation is pure waste; just set `Arc::from("")`.
- **Suggested fix:** Replace the temp id with an `Arc::from("")` placeholder. Better: skip the placeholder entirely by building cells in two phases (compute capitals first, build with final id at construction time).
- **Effort:** S
- **Risk of fix:** Low

### F-012-007 — [MEDIUM] [Panic] `.expect("seed id missing")` and `.expect("missing sys")` are reachable on logic errors
- **Location:** `src/export/subsectors/mod.rs:227,537,574,597-598,629,660`
- **Category:** Panic / Failure surface
- **Confidence:** Medium
- **Blast radius:** Library code in `build_subsectors`; export aborts entire compose
- **Problem:** Eight `.expect`/`.unwrap` calls on `sector.get_system(seed_id)`. The invariant (seeds are subset of `sector.systems`) is locally true but not enforced by a type. A future change to `cluster_systems` (e.g. allowing pinned seeds passed externally) would turn these into bugs. §3.1 in CLAUDE.md — "panic in library code on common input" is a HIGH; here the input is internal so I'm rating MEDIUM.
- **Why it matters:** Crash on what should be `Err(SubsectorBuildError::...)`.
- **Suggested fix:** Switch to `id → index` (see F-012-002), eliminating most expects. Where lookups remain, return `SubsectorBuildError::InvalidState(...)` instead. Add a new variant if needed.
- **Effort:** S (after F-012-002)
- **Risk of fix:** Low

### F-012-008 — [MEDIUM] [Determinism / Style] `unwrap()` on inner-loop `scores.get(&sys.id)` and `assignment.get(&sys.id)`
- **Location:** `src/export/subsectors/mod.rs:518-519,544,599-602,618`
- **Category:** Panic / Failure surface
- **Confidence:** High
- **Blast radius:** Same as F-012-007
- **Problem:** Same class. `scores` and `assignment` are built from `sector.systems` and indexed by `sys.id`. The invariant holds today but is enforced only by reading-the-code; a missing entry crashes.
- **Why it matters:** Same.
- **Suggested fix:** `.expect("score by sys.id — populated in this function")` at minimum; or restructure to iterate the map (which already implies presence). Better: index by `usize` via the new id→index map.
- **Effort:** S
- **Risk of fix:** Low

### F-012-009 — [MEDIUM] [Concurrency / Maintainability] `system_to_cluster.iter_mut()` rebinding then `hex_cluster.into_iter()` rebuilds two maps
- **Location:** `src/export/subsectors/mod.rs:342-353`
- **Category:** Maintainability
- **Confidence:** Medium
- **Blast radius:** `build_subsectors`
- **Problem:** After relabel, we mutate `system_to_cluster` in place but rebuild `hex_cluster_new` from scratch. The asymmetry plus `new_index_by_old[ci]` indexing (`Index` panics on missing key) is a subtle hazard if the relabel ever fails to cover all old indices. Logic is correct today.
- **Why it matters:** Reader has to walk both paths to convince themselves they're equivalent.
- **Suggested fix:** Unify by using one helper:
  ```rust
  let remap = |old: usize| -> usize { new_index_by_old[&old] };
  let hex_cluster_new: BTreeMap<_,_> = hex_cluster.iter().map(|(&k, &v)| (k, remap(v))).collect();
  for v in system_to_cluster.values_mut() { *v = remap(*v); }
  ```
- **Effort:** S
- **Risk of fix:** Low

### F-012-010 — [MEDIUM] [Determinism] `sort` of `inter_sector_links` is by emission order, not by canonical key
- **Location:** `src/export/segmentum.rs:579-625` (no explicit sort), serialised at `:900-905`
- **Category:** Determinism / Output stability
- **Confidence:** Medium
- **Blast radius:** `segmentum.json` byte stability across cosmetic config edits
- **Problem:** `links` is built by iterating `by_slot` (BTreeMap, deterministic) then pushing in stitch-pair order, with link ids assigned via `*link_idx += 1`. The final `Vec` is **not** explicitly sorted by a canonical key — the order depends entirely on the iteration of `by_slot` (which is `(col, row)` ascending — fine) and the per-pair RNG-driven choice. That's stable for one config but tying *link ids* to insertion order means re-ordering children in `segmentum.toml` (without changing what gets stitched) shuffles ids `sl-0001..sl-NNNN`. The README/spec claim is "same config ⇒ same bytes" — and that holds — but the implicit contract "same stitches ⇒ same link ids" does not.
- **Why it matters:** Surprises downstream consumers diffing two `segmentum.json` outputs.
- **Suggested fix:** Compute a content-derived id, e.g. `format!("sl-{from_child}-{to_child}-{from_sys}-{to_sys}")` slugified, instead of a counter. If a counter is required for human readability, sort `links` by `(from_child_id, to_child_id, from_system_id, to_system_id)` before assigning the counter index. Document the invariant either way.
- **Effort:** S
- **Risk of fix:** Medium (output bytes change — needs golden refresh)

### F-012-011 — [MEDIUM] [Error model] `compose` writes child outputs inside the loop but does not roll back on later failure
- **Location:** `src/export/segmentum.rs:432,360`
- **Category:** Resource / Error model
- **Confidence:** High
- **Blast radius:** Failed `compose` leaves a half-written `output_dir`
- **Problem:** `export_sector(&sector, &output_cfg, &child_out)?;` (line 432) commits N-1 children to disk before child N fails. A later `stitch_children` or `write_report` failure also leaves the child dirs untouched. There is no `.tmp` directory + rename pattern. §3.9 / §3.4 — IO error context is OK (paths included via `SectorError::io`), but partial output is not — and the user has no obvious way to know "everything before child K is good, K is bad".
- **Why it matters:** Re-running `compose` re-generates from scratch — extra cost — but worse, can mask the failure if the consumer reads files without checking the manifest first.
- **Suggested fix:** Write all children into `<output_dir>.partial/`, then `fs::rename` to `<output_dir>` only after `write_report` succeeds. If atomicity is too heavy, at least delete a stale `<output_dir>/super_manifest.json` on entry so a half-written tree is detectable.
- **Effort:** M
- **Risk of fix:** Low

### F-012-012 — [MEDIUM] [Idiomatic] `_systems: &[GeneratedSystem]` parameter is unused
- **Location:** `src/export/subsectors/mod.rs:420` (in `capital_or_seed_coord`)
- **Category:** Idiomatic / API hygiene
- **Confidence:** High
- **Blast radius:** API noise
- **Problem:** Dead parameter prefixed with `_` to silence the warning. The function ignores it entirely.
- **Why it matters:** Suggests an abandoned refactor; readers wonder if it's load-bearing.
- **Suggested fix:** Drop the parameter and the call site argument.
- **Effort:** S
- **Risk of fix:** Low

### F-012-013 — [LOW] [Idiomatic] `as u32` / `as i32` casts without `try_from`
- **Location:** `src/export/segmentum.rs:592,654,656,665,681,688,706,716` and `src/export/subsectors/mod.rs:178-179,234,235,326,328,501,651,663-668,664,696,723,727`
- **Category:** Idiomatic / Soundness
- **Confidence:** High
- **Blast radius:** Silent truncation on unrealistic inputs
- **Problem:** Pervasive `as` casts between `i32`/`u32`/`u64`/`usize`. Most are bounded by sector dimensions (already 32-bit) but the `subsector_label` (`mod.rs:723-727`) uses `index as i64 + 1` — safe — while `mod.rs:651 deg * 4 + max_pop * 5 + max_tech * 2 + worlds.len() as i32` can overflow for `worlds.len() > i32::MAX / something` (not realistic for worlds but the construct is brittle). CLAUDE.md §3.7 calls these out.
- **Why it matters:** Mostly cosmetic but masks a real overflow in F-012-001.
- **Suggested fix:** Where input is trusted, prefer `i32::try_from(x).expect("invariant: ...")` so the invariant is documented. For arithmetic outputs, prefer `checked_add` / `saturating_add`.
- **Effort:** M (scope-wide)
- **Risk of fix:** Low

### F-012-014 — [LOW] [Performance] `format!` in `render_markdown` hot loop
- **Location:** `src/export/segmentum.rs:773-861`
- **Category:** Performance / Allocation
- **Confidence:** Medium
- **Blast radius:** Once per compose — not hot.
- **Problem:** ~25 `s.push_str(&format!(...))` calls. Each `format!` allocates a String then copies it into `s`. `write!(&mut s, ...)` skips the intermediate.
- **Why it matters:** Tens of small allocs per compose — well under any real budget.
- **Suggested fix:** `use std::fmt::Write;` then `writeln!(s, ...).unwrap();` everywhere — same byte output, fewer allocations. Same pattern recurs in `format_super_grid` (line 877).
- **Effort:** S
- **Risk of fix:** Low (output identical)

### F-012-015 — [LOW] [Determinism / Maintainability] `Default` `..Default::default()` spread on `GeneratedSector` in tests reaches into many fields
- **Location:** `src/export/segmentum.rs:986`, `src/export/subsectors/mod.rs:818-820`, `src/export/system_map.rs:467`
- **Category:** Maintainability / Test brittleness
- **Confidence:** Medium
- **Blast radius:** Tests only
- **Problem:** `..Default::default()` after manually populated fields silently absorbs new `GeneratedSector` fields without forcing the test author to think about them. Acceptable for forward-compat but in golden-byte territory it's a footgun — a new field with a non-trivial default flips outputs without any test failure noise.
- **Why it matters:** Less alarming because this is `#[cfg(test)]`, but if the production `GeneratedSector` gains a new field that participates in subsector clustering, the cluster tests would silently start passing under the wrong assumption.
- **Suggested fix:** Replace `..Default::default()` with explicit field assignments in tests; add a `#[non_exhaustive]` to `GeneratedSector` itself to force a thinking-pause on consumers (separate concern — would be flagged at the model level).
- **Effort:** S
- **Risk of fix:** Low

### F-012-016 — [LOW] [Error model] `pick_capital` swallows `world_score < 0` silently
- **Location:** `src/export/subsectors/summary.rs:633-637`
- **Category:** Error model / Correctness
- **Confidence:** Low
- **Blast radius:** Subsector summary
- **Problem:** The pattern `Some((s, _)) if s >= 0 => (s, Some(w.id))` / `Some((s, _)) => (s, None)` says "if the best world scored negative, keep its score but drop the world id". The capital then has `subsector_capital_world_id = None` but the system is still chosen. This is intentional (frontier rule, line 685) but the score-only-with-no-world pathway also drops the world entirely without recording the reason. Hard to debug from output.
- **Why it matters:** Output looks "correct" but loses information about *why* no world was chosen.
- **Suggested fix:** Add a `notes` push on the Subsector when this happens, or a `tags.push("frontier_capital".into())`. Make the rule observable.
- **Effort:** S
- **Risk of fix:** Low

### F-012-017 — [LOW] [Idiomatic] `system_map::write_system_maps` clones `opts` per system
- **Location:** `src/export/system_map.rs:61`
- **Category:** Performance / Cloning
- **Confidence:** Medium
- **Blast radius:** Once per system at export time
- **Problem:** `let img = render_system(sys, &sector.factions, scale, opts.clone());` — `SystemRenderOptions` contains a `MapTheme` (rgba colours) so the clone is small, but unnecessary: `render_system` only reads `opts`. Should take `&SystemRenderOptions`.
- **Why it matters:** Trivial.
- **Suggested fix:** Change `render_system` and `write_one_system_png` to accept `&SystemRenderOptions`. Removes the clones in the loop and at line 75.
- **Effort:** S
- **Risk of fix:** Low

### F-012-018 — [LOW] [Idiomatic] `seen: Vec<&str>` linear-scan dedup in `draw_legend`
- **Location:** `src/export/system_map.rs:324-330`
- **Category:** Idiomatic / Algorithm
- **Confidence:** Low
- **Blast radius:** Per-system legend render
- **Problem:** `seen.contains(&t)` is `O(n²)` over world types. `BTreeSet<&str>` would be `O(n log n)` — but we want stable insertion order. The existing pattern is fine; flag as a clarity nit.
- **Suggested fix:** Add a `BTreeSet` companion for the membership check, keep the `Vec` for ordering. Or document that order matters and leave it.
- **Effort:** S
- **Risk of fix:** Low

### F-012-019 — [LOW] [Documentation] `pub const RESOLUTION_*` lack rustdoc + `pub` ranks not exported in `lib.rs` usage clear
- **Location:** `src/export/system_map.rs:44-49`
- **Category:** Documentation
- **Confidence:** Medium
- **Blast radius:** Caller ergonomics
- **Problem:** The constants document themselves in a comment block ("720p / 1440p / 4K") but the docstring is on `RESOLUTION_720P` only — `1440P` / `4K` / `MAX_SCALE` get no per-item rustdoc.
- **Suggested fix:** Add `///` on each constant.
- **Effort:** S
- **Risk of fix:** Low

### F-012-020 — [LOW] [Idiomatic] `subsector_label`'s `String::from_utf8(buf).expect("ascii")` could be `unsafe { from_utf8_unchecked }` but isn't worth it; flag the expect as documented invariant
- **Location:** `src/export/subsectors/mod.rs:731`
- **Category:** Idiomatic
- **Confidence:** Low
- **Problem:** The bytes pushed are `b'A' + (n % 26) as u8`, always ASCII. The `expect("ascii")` is correct but the comment would help.
- **Suggested fix:** Add `// SAFETY: pushed bytes are b'A'..=b'Z' by construction` (no `unsafe` needed — `from_utf8` checked is fine for non-hot code).
- **Effort:** S
- **Risk of fix:** None

### F-012-021 — [NIT] [Style] Stray "// ... (rest of logic)" comment
- **Location:** `src/export/subsectors/summary.rs:531`
- **Category:** Style
- **Confidence:** High
- **Problem:** Leftover placeholder comment from a refactor.
- **Suggested fix:** Delete.
- **Effort:** S
- **Risk of fix:** None

### F-012-022 — [NIT] [Style] Speculative comment + commented reasoning in `system_map::render_system`
- **Location:** `src/export/system_map.rs:159-164`
- **Category:** Style
- **Confidence:** High
- **Problem:** Block contains:
  ```rust
  // fill_polygon not available? I'll use draw_line segments.
  // ...
  // I'll just draw the outline for now.
  // Actually I should probably add a primitive for this if needed.
  ```
  Internal monologue checked in as code.
- **Suggested fix:** Replace with a single line: `// No filled-polygon primitive; outline-only diamond is intentional.`
- **Effort:** S
- **Risk of fix:** None

### F-012-023 — [NIT] [Style] `mod summary;` placed mid-file after `use` block, between `push_unique` and tests
- **Location:** `src/export/subsectors/mod.rs:740-744`
- **Category:** Style / Organisation
- **Confidence:** High
- **Problem:** Submodule declaration mixed with helper functions and tests in the middle of the file. Convention is top-of-file.
- **Suggested fix:** Move `mod summary;` + `use summary::{...};` to the top with the other imports.
- **Effort:** S
- **Risk of fix:** None

### F-012-024 — [NIT] [Style] `fn _unused(_w: WorldDto) {}` in tests
- **Location:** `src/export/subsectors/mod.rs:823`
- **Category:** Style
- **Problem:** Dead test fixture.
- **Suggested fix:** Delete it (and the `WorldDto` import if it then goes unused).
- **Effort:** S
- **Risk of fix:** None

### F-012-025 — [NIT] [Style] `mk_child` test helper takes `id: &str` and unused `_systems: &[GeneratedSystem]` shape
- **Location:** `src/export/segmentum.rs:990-1006`
- **Category:** Style
- **Problem:** `project: "/tmp/none".into()` — not Windows-friendly even in tests (compiles fine because it's `Utf8PathBuf::from`, but reads as Unix-only).
- **Suggested fix:** Use a temp dir or just an opaque token like `"unused"`.
- **Effort:** S
- **Risk of fix:** None

### F-012-026 — [NIT] [Style] `fn is_capital_like_tag` is `fn`, not `pub(super)`; `any_capital_like` is generic over `&'a S` with `S: AsRef<str>`
- **Location:** `src/export/subsectors/summary.rs:803-811`
- **Category:** Style / API
- **Problem:** The generic helper is more flexible than its single caller pattern (`w.tags.iter().chain(w.world.notable_features.iter())`) needs — both produce `&Arc<str>` and `&Arc<str>` respectively, both `AsRef<str>`. Fine, but consider a simpler signature `fn any_capital_like<I: IntoIterator<Item = impl AsRef<str>>>(iter: I) -> bool` to drop the explicit lifetime + outer reference dance.
- **Suggested fix:** Simplify the generics.
- **Effort:** S
- **Risk of fix:** None

## Rubric coverage

- §3.1 Panics: F-012-001, F-012-007, F-012-008, F-012-020 (low). Several `.expect`/`.unwrap` paths are guarded by local invariants but lack documentation.
- §3.2 unsafe: None present. No findings.
- §3.3 Ownership / cloning: F-012-017 (per-system opts.clone), F-012-004 (per-call Vec).
- §3.4 Error handling: F-012-011 (no rollback on partial export), F-012-001 (config validation gap). I/O paths in segmentum carry path context via `SectorError::io` — good.
- §3.5 Concurrency / async: None present. No findings.
- §3.6 Performance: F-012-002 (algorithmic), F-012-004 / F-012-005 (alloc), F-012-006 / F-012-014 (format!), F-012-017 (clone).
- §3.7 Idiomatic / API: F-012-012, F-012-013, F-012-018, F-012-020, F-012-026.
- §3.8 Deps: No findings. Imports are tight.
- §3.9 Memory / resources: F-012-011 (resource cleanup on error). `save_png_fast` uses `BufWriter` + RAII close — good.
- §3.10 Testing: F-012-015 (test brittleness via `..Default::default()`), F-012-024 / F-012-025 (dead test code). Tests cover determinism, classification, label scheme, and rendering. No property tests for clustering invariants (e.g. "every system assigned exactly once" is asserted once, not as a strategy).
- §3.11 Docs: F-012-019. Module-level `//!` comments are good in all three files. Public API `pub fn compose / compose_with_progress / build_subsectors / write_system_maps` have `# Errors` sections — good.
- **Determinism invariants (CLAUDE.md hard rule):** No `FxHashMap`/`FxMap` iterated for output in this unit. All collections used for output are `BTreeMap`/`BTreeSet` or `Vec` constructed in deterministic order. RNG draw at `src/export/segmentum.rs:714` routes through `rng::stage_rng` — compliant. F-012-010 flags an implicit-ordering risk that's not a hard-rule violation but worth pinning.

## Summary of suggested fixes

- F-012-001 — HIGH — Validate stitch.border_depth + use saturating arithmetic in stitch_pair — S/Low
- F-012-002 — HIGH — Replace sector.get_system lookups in cluster_systems with id→index map — M/Low
- F-012-003 — HIGH — Unify seed-vs-refinement tie-break order to one direction — S/Medium
- F-012-004 — MEDIUM — Drop sector.get_worlds_for_system in favor of &sys.worlds in summary hot path — S/Low
- F-012-005 — MEDIUM — Avoid per-world Vec<String> in inferred_prosperity_rank token scan — S/Low
- F-012-006 — MEDIUM — Skip the tmp subsector id format!; use placeholder Arc — S/Low
- F-012-007 — MEDIUM — Replace .expect on get_system with SubsectorBuildError variant — S/Low (after F-012-002)
- F-012-008 — MEDIUM — Same for scores/assignment get().unwrap() — S/Low
- F-012-009 — MEDIUM — Factor a single remap helper in mod.rs relabel block — S/Low
- F-012-010 — MEDIUM — Sort inter_sector_links by canonical key before assigning sl-NNNN ids — S/Medium
- F-012-011 — MEDIUM — Stage compose output in <dir>.partial then rename on success — M/Low
- F-012-012 — MEDIUM — Drop _systems parameter from capital_or_seed_coord — S/Low
- F-012-013 — LOW — Replace as casts with try_from / saturating ops where bounds matter — M/Low
- F-012-014 — LOW — Replace push_str + format! with writeln! in render_markdown — S/Low
- F-012-015 — LOW — Drop ..Default::default() in test fixtures to force explicitness — S/Low
- F-012-016 — LOW — Surface frontier-capital rule via a note/tag on the Subsector — S/Low
- F-012-017 — LOW — Accept &SystemRenderOptions in render_system / write_one_system_png — S/Low
- F-012-018 — LOW — Use BTreeSet companion for legend dedup membership check — S/Low
- F-012-019 — LOW — Add per-constant rustdoc to RESOLUTION_1440P / RESOLUTION_4K / MAX_SCALE — S/Low
- F-012-020 — LOW — Document the ASCII invariant on subsector_label's expect — S/None
- F-012-021 — NIT — Delete stray "// ... (rest of logic)" comment in summary.rs — S/None
- F-012-022 — NIT — Replace internal-monologue comments in system_map render_system — S/None
- F-012-023 — NIT — Move `mod summary;` declaration to top of mod.rs — S/None
- F-012-024 — NIT — Delete `_unused` test fixture in mod.rs — S/None
- F-012-025 — NIT — Replace "/tmp/none" with platform-neutral token in mk_child — S/None
- F-012-026 — NIT — Simplify any_capital_like generic signature — S/None
