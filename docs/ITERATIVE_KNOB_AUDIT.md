# Iterative Generator — Knob Stub Audit & Remediation Plan

> **For a future execution session.** This is an audit *result* + an actionable fix plan.
> The audit was read-only; **no code was changed**. Your job is to execute the tasks below.
> The user's directive: **resolve every stub — dead *and* weak.**

## Execution status — 2026-06-18 (FINAL)

**All actionable tasks are resolved.** The code edits for the DELETE/EXPOSE group
(Tasks 1–8) have landed and gone green, the doc scrub has run, and the
KEEP group (Tasks 9–12) is a confirmed conscious no-op. Net outcome, verified
against the live tree on this date:

- **Task 1 — `PlacementConfig.mode` — DONE.** The field, the `PlacementMode` enum,
  its `as_slug`, and all UI sites are gone from the code (the removal landed
  alongside the `cluster_bias` clustering work). `grep PlacementMode` returns zero
  hits across `src/`, `builder/src/`, `viewer/src/`, and `presets/`/`examples/`;
  `PlacementConfig` now carries only `cluster_bias` + `minimum_system_distance`.
  Stale `PlacementMode` doc mentions were corrected.
- **Task 2 — `WorldSelectionConfig.mode` — DONE.** The field, the single-variant
  `WorldSelectionMode` enum, and both combos (`iterative_gen.rs`, `generation.rs`)
  are gone; `grep WorldSelectionMode` returns zero hits across the crate trees.
  `mode =` is stripped from every `world_selection` TOML table in `presets/` /
  `examples/`. Stale doc mentions (`GUIDE.md` G1 parity table, `BUILDER_REQS.txt`
  field list) scrubbed.
- **Tasks 3–6 — `subsector_width` / `subsector_height` / `strict_world_rows` /
  `allow_partial_rows` — DONE.** All four dead fields, their G1 widgets, and their
  default-write literals are removed; `grep` returns zero hits across the crate
  trees, `presets/`, and `examples/`. Stale doc mentions in `GUIDE.md` (sample
  `sectorforge.toml`, the subsector note) and `BUILDER_REQS.txt` (G1 field list)
  scrubbed; the `subsector_width.is_some()` assertion in `docs/TEST_GAPS.md` was
  corrected.
- **Task 7 — `WorldSelectionConfig.require_complete_rows` — DONE.** The field and
  its checkbox are gone; the engine hardcodes the strict-exclusion path in
  `world_pool.rs` (`build_pool` always excludes rows missing a required field or
  carrying a non-positive/non-finite weight). Generated output is unchanged — only
  the now-redundant `ExclusionReason` branch collapsed. Stale doc mentions in
  `GUIDE.md` (sample TOML + the `worlds.toml` "usable row" prose) scrubbed.
- **Task 8 — `RouteGenerationConfig.stability_targets` — DONE (EXPOSED).** A control
  now lives in the **Routes** wizard step (`iterative_gen.rs` `show_routes_form`):
  an off-by-default "Target a mix" checkbox that writes
  `Some(StabilityTargets { stable, unstable, hazardous, perilous })` with four
  relative-weight DragValues. Routed through the transient session (the command-bus
  carve-out — `iterative_gen` is view state). Default stays `None` → byte-identical
  → goldens green. `GUIDE.md` gains a line for the new control in the iterative
  wizard section (the TOML-key prose was already correct).
- **Golden re-pin — DONE.** The only golden movement from this work was the
  `input_digests["sectorforge.toml"]` line, which flips on *any* config-text edit
  (the deleted serde keys left the checked-in TOML). That diff was proven confined
  to `input_digests` and the affected golden(s) were re-pinned in the dedicated
  re-pin phase. No generation-content bytes changed.
- **Tasks 9–12 — DONE (KEEP / conscious no-op).** Provenance passthroughs
  (`search_base_seed` / `search_candidate_index` / `search_constraints_digest`) and
  the `allow_empty_hexes` validation gate were confirmed *not* stubs and kept per
  the rationale below. No code required.

## How this was produced

A 53-agent workflow enumerated all 33 serde/`pub` knobs of `GenerationConfig` + nested
`PlacementConfig` / `WorldSelectionConfig` / `RouteGenerationConfig` /
`RelationsGenerationConfig`, traced each (UI → config → engine read sites), had a second
agent re-confirm every DEAD/WEAK call with a different search strategy, and **empirically**
diffed the suspects at min-vs-max on a **fixed seed** (`presets/_full`, `--light`, blake3
of the raw config text normalized out of the diff).

Result: **6 DEAD · 5 WEAK · 21 LIVE · 1 reclassified** (`allow_empty_hexes` is a live
validation gate, not a stub). `placement.cluster_bias` is **LIVE** (reference knob — already
wired into `place_systems_reroll`; do not touch).

> ⚠️ **Line numbers below are a snapshot — they drift.** Re-grep by symbol name before
> editing. Treat `path:line` as "start here", not gospel.

---

## Invariants (do not violate — copy into any subagent brief)

- **Never read or modify anything in `old/`.**
- **All RNG goes through `src/model/rng.rs`** (stage-keyed via blake3). No `thread_rng`.
- **Output writers must stay byte-stable.** After *every* change run the golden suite and it
  must stay **green without updating**:
  ```bash
  cargo test --test it -- golden
  ```
  Live map render goldens (gui-core) are a separate suite — only touch with
  `UPDATE_MAP_SNAPSHOTS=1`, and you should **not** need to here (these knobs are dead).
- **Sectors are square** (`sector_width == sector_height`). None of these tasks may break that.
- **Builder mutations go through the command bus** (`state.run(BuilderCommand::…)`). Removing
  widgets is fine; do not introduce direct writes to document state.

### Global execution rules (the `cluster_bias` playbook)

1. **Byte-identical default.** Every removal/implementation must leave the *default-valued*
   output unchanged. Dead-field deletes are byte-safe by definition (the engine never read
   them) — **prove it**: generate a sector at a fixed seed before and after, diff, expect
   identical (normalize the `input_digests["sectorforge.toml"]` line, which flips on *any*
   config-text edit — see `src/loading/input.rs:74`).
2. **Strip deleted serde keys from presets.** Deleting a field can break loading of any TOML
   that still carries the key **iff** the struct has `#[serde(deny_unknown_fields)]`.
   **Check that first.** Then:
   ```bash
   rg -n '<deleted_key>' presets/ examples/        # strip every hit
   ```
3. **Preserve RNG draw order.** Some "dead config field" deletes also touch
   `src/gen/random_sector.rs`, which constructs the config and may draw RNG. Do **not** change
   the number/order of RNG draws there or the `random` SectorSize path churns its goldens.
4. **One knob per commit** (or per tight cluster), golden-green between each.

---

## Decisions at a glance

| # | knob | verdict | decision | surfaced in iterative wizard? |
|---|---|---|---|---|
| 1 | `PlacementConfig.mode` | DEAD | **DELETE** | **yes — step 2** |
| 2 | `WorldSelectionConfig.mode` | DEAD | **DELETE** | **yes — step 4** |
| 3 | `GenerationConfig.subsector_width` | DEAD | **DELETE** | no (G1 panel) |
| 4 | `GenerationConfig.subsector_height` | DEAD | **DELETE** | no (G1 panel) |
| 5 | `GenerationConfig.strict_world_rows` | DEAD | **DELETE** (dup) | no (G1 panel) |
| 6 | `WorldSelectionConfig.allow_partial_rows` | DEAD | **DELETE** | no (G1 panel) |
| 7 | `WorldSelectionConfig.require_complete_rows` | WEAK | **DELETE** (collapse) | no (G1 panel) |
| 8 | `RouteGenerationConfig.stability_targets` | WEAK | **EXPOSE** (engine works) | no |
| 9 | `GenerationConfig.search_base_seed` | WEAK | **KEEP** (provenance) | no |
| 10 | `GenerationConfig.search_candidate_index` | WEAK | **KEEP** (provenance) | no |
| 11 | `GenerationConfig.search_constraints_digest` | WEAK | **KEEP** (provenance) | no |
| 12 | `GenerationConfig.allow_empty_hexes` | LIVE\* | **KEEP** (validation gate) | no |

The two that matter most are **#1 and #2**: dropdowns a user can change in the wizard for
**zero effect** — the UI lies. Do those first.

---

## GROUP A — DELETE (dead stubs)

### Task 1 — `PlacementConfig.mode` (DEAD, wizard step 2) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field, `PlacementMode` enum, `as_slug`, and all UI
> sites are gone from the tree; zero `PlacementMode` hits remain in code or presets.
> Stale `PlacementMode` doc mentions corrected the same day.
The known suspect. Engine reads only `cluster_bias` + `minimum_system_distance`; clustering is
gated on `cluster_bias > 0.0` (`placement.rs:59`), **never** on `mode`. Empirically
byte-identical uniform-vs-clustered. The live lever is the `cluster_bias` slider, which already
sits next to this dropdown — `mode` is a redundant coarse duplicate.

- **Config field:** `src/loading/config.rs:143`
- **Enum `PlacementMode` + generated `as_slug`:** `src/loading/config.rs:169` (decl nearby) —
  `as_slug` has **zero callers on a `PlacementMode` value**; only the `#[cfg(test)]` serde-parity
  test at `src/macros.rs:192` and the `enum_slug!` registration use it.
- **UI to remove:**
  - `builder/src/builder/panels/iterative_gen.rs:376` (combo call) + combo helper
    `placement_mode_combo` `iterative_gen.rs:1246` + options `iterative_gen.rs:1240`
  - `builder/src/builder/panels/generation.rs:220`
  - `viewer/src/editor/generation_panel.rs:120,126,132,138` (set-sites) **and `:142`**, which
    gates the cluster_bias slider's visibility on `mode == Clustered` → after removal, **show the
    slider unconditionally** (builder iterative slider at `iterative_gen.rs:381-389` is already
    not mode-gated, so it's unaffected).
- **`random_sector.rs` coupling (careful):** `src/gen/random_sector.rs:339-348` builds a local
  `PlacementMode` and at `:344` uses `if mode == Clustered` to decide whether to roll a nonzero
  `cluster_bias` (else `0.0`); the struct literal sets `mode:` at `:394`. Remove the `mode:`
  field from the literal, but **keep the exact same RNG draw sequence** that decides the bias
  value, or the `random` path goldens churn. Simplest: keep the local decision variable, just
  stop storing it in the struct.
- **Done when:** field + enum + 4 UI sites + parity test gone; `mode=` stripped from
  `presets/`+`examples/`; `cargo check --workspace` clean; golden suite green **without update**;
  fixed-seed before/after sector diff identical.

### Task 2 — `WorldSelectionConfig.mode` (DEAD, wizard step 4) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field, single-variant `WorldSelectionMode` enum,
> parity test, and both combos (`iterative_gen.rs`, `generation.rs`) removed; zero
> `WorldSelectionMode` hits remain in code or presets. `mode =` stripped from
> `world_selection` TOML tables; stale doc mentions scrubbed. Golden green.
Never read **and** the enum has a single variant (`WeightedRows`) — it cannot vary even in
principle. Pure UI noise.

- **Config field:** `src/loading/config.rs:187`; `Default` writes it at `config.rs:200`
- **Enum `WorldSelectionMode`:** `#[non_exhaustive] enum { WeightedRows }` at
  `src/loading/config.rs:215-218`; `as_slug` via `src/macros.rs:193`
- **UI to remove:** `builder/src/builder/panels/iterative_gen.rs:627` (combo) + helper
  `world_selection_mode_combo` `iterative_gen.rs:1263` + `WORLD_SELECTION_MODES`
  `iterative_gen.rs:1260-1261`; `builder/src/builder/panels/generation.rs:254` (read-only `&`,
  renders a static "Weighted rows" label `generation.rs:412-422`)
- **Write site:** `src/gen/random_sector.rs:399` (`mode: WeightedRows`) — drop from literal.
- **Done when:** field + single-variant enum + parity test + 2 UI sites gone; `mode=` stripped
  from `world_selection` TOML tables; golden green without update.

### Task 3 — `GenerationConfig.subsector_width` (DEAD) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field + G1 widget + test assertion removed; zero
> hits remain in code or presets. Stale doc mentions (`GUIDE.md`, `TEST_GAPS.md`)
> scrubbed. Golden green (only `input_digests` moved, re-pinned later).
Assigned/serialized, **zero** engine reads. Empirically byte-identical 1 vs 64. No subsector
concept exists anywhere in the engine.

- **Config field:** `src/loading/config.rs:108`
- **UI:** `builder/src/builder/panels/generation.rs:128-142` ("Subsector width" DragValue;
  `0` maps to `None` at `:138`)
- **Other refs (non-reads):** `src/gen/random_sector.rs:336,385` (default); test assertion
  `tests/it/loading_tests.rs:98` — update/remove the assertion.
- **Done when:** field + G1 widget + test assert gone; `subsector_width` stripped from presets;
  golden green without update.

### Task 4 — `GenerationConfig.subsector_height` (DEAD) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field + G1 widget removed; zero hits remain in
> code or presets. Stale doc mentions (`GUIDE.md`) scrubbed. Golden green (only
> `input_digests` moved, re-pinned later).
Identical shape to Task 3.

- **Config field:** `src/loading/config.rs:110`
- **UI:** `builder/src/builder/panels/generation.rs:143-157` (writes via `(sh > 0).then_some(sh)`
  at `:153`)
- **Other refs:** `src/gen/random_sector.rs:386` (default); same test file.
- **Done when:** as Task 3.

### Task 5 — `GenerationConfig.strict_world_rows` (DEAD, duplicate) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field + checkbox + the three default-`true` write
> literals removed; zero hits remain in code or presets. Doc mentions (`GUIDE.md`,
> `docs/BUILDER_REQS.txt`) scrubbed. Golden green (only `input_digests` moved).
A dead **duplicate** — the advertised behavior is actually implemented by the live
`WorldSelectionConfig.require_complete_rows` (`config.rs:189`, read at `world_pool.rs:131`).
Toggling `strict_world_rows` changes only serialized TOML.

- **Config field:** `src/loading/config.rs:119`
- **UI:** `builder/src/builder/panels/generation.rs:208-211` (checkbox)
- **Write sites (default `true` literals):** `src/gen/random_sector.rs:392`,
  `builder/src/builder/project_io.rs:85`, `builder/src/builder/state/mod.rs:611`
- **Docs mentioning it (update/remove):** `GUIDE.md:1175`, `docs/BUILDER_REQS.txt:406`
- **Done when:** field + checkbox + 3 literal writes + doc mentions gone; stripped from presets;
  golden green without update.

### Task 6 — `WorldSelectionConfig.allow_partial_rows` (DEAD, unimplemented) — DELETE  ✅ DONE
> **DONE (verified 2026-06-18).** Field + checkbox + the two write sites + the stale
> `world_pool.rs` comment removed; zero hits remain in code or presets. Done together
> with Task 7. Docs scrubbed. Golden green.
Explicitly never built — comment at `src/gen/world_pool.rs:141`: *"Partial-row mode
(allow_partial_rows) is not implemented in the first cut; exclude and report."* The
`require_complete_rows == false` branch (`world_pool.rs:140`) unconditionally excludes partial
rows regardless of this flag.

- **Config field:** `src/loading/config.rs:191`
- **UI:** `builder/src/builder/panels/generation.rs:269-272` (checkbox + tooltip `:269`)
- **Write sites:** `src/loading/config.rs:205` (Default), `src/gen/random_sector.rs:401`
- **Also delete:** the stale comment `world_pool.rs:141`.
- **Note:** this is entangled with Task 7 — do them together.
- **Alternative (only if the feature is actually wanted):** *implement* partial-row inclusion in
  the `world_pool.rs:140` branch (include rows with missing fields instead of excluding), gated
  so default `false` reproduces today's output. That's a real feature, not a cleanup.

---

## GROUP B — RESOLVE WEAK

### Task 7 — `WorldSelectionConfig.require_complete_rows` (WEAK) — DELETE / collapse  ✅ DONE
> **DONE (verified 2026-06-18).** Field + checkbox removed; `world_pool.rs` hardcodes
> the strict-exclusion path (`build_pool` always excludes incomplete/bad-weight
> rows). Generated output unchanged — only the redundant `ExclusionReason` branch
> collapsed; any shifted `WB_EXCLUDED_ROWS` diagnostic-test expectations updated.
> Doc mentions scrubbed. Golden green.
Read and reachable (`world_pool.rs:131`), **but** both states converge on the same world pool:
the lenient path is tied to the dead `allow_partial_rows`, so `true` and `false` both exclude
incomplete rows. The *only* observable difference is the `ExclusionReason` **diagnostic string**
(`validation.rs:214` → `WB_EXCLUDED_ROWS_SEVERE` `:233` / `WB_EXCLUDED_ROWS` `:241`) — not the
generated sector.

- **Config field:** `src/loading/config.rs:189`
- **UI:** `builder/src/builder/panels/generation.rs:262` (checkbox)
- **Engine read:** `src/gen/world_pool.rs:131` (`if cfg.require_complete_rows`); else-branch `:140`.
  `build_pool` call sites: `mod.rs:388`, `lib.rs:350`, `validation.rs:163`; preview-only:
  `builder/src/builder/state/generation_ops.rs:187`, `builder/src/builder/panels/world/features.rs:197`.
- **Decision:** with Task 6 gone, hardcode the strict exclusion path and delete the field +
  checkbox. **Generated output is unchanged** (both branches already exclude incomplete rows);
  only the diagnostic reason text collapses to one variant — so **validation/diagnostic tests
  may shift, not goldens.** Grep `WB_EXCLUDED_ROWS` test expectations and update.
- **Alternative:** keep + actually implement partial rows (couples with Task 6's implement
  alternative — same feature). Pick one direction for the pair.

### Task 8 — `RouteGenerationConfig.stability_targets` (WEAK) — EXPOSE  ✅ DONE
> **DONE (verified 2026-06-18).** A control now lives in the **Routes** wizard step
> (`iterative_gen.rs` `show_routes_form`): an off-by-default "Target a mix" checkbox
> that sets `Some(StabilityTargets { stable, unstable, hazardous, perilous })` via
> four relative-weight DragValues, written to the transient session (command-bus
> carve-out). Default `None` → byte-identical → golden green. `GUIDE.md` iterative
> wizard section gains a line for the control (TOML-key prose was already correct).
**Inverse of a dead stub: live engine code, dead value.** The Stage-9 rebalance path works and
*does* change per-route `RouteStability` when `Some`, but every production constructor sets
`None`, so it's only reachable by hand-authoring TOML.

- **Config field:** `src/loading/config.rs:273-274`; `StabilityTargets` struct
  `config.rs:244-255` (has `#[serde(deny_unknown_fields)]`)
- **Engine (already works):** gate `routes.rs:232` (`is_none()` → legacy `cap_perilous_routes`);
  Stage 9 `mod.rs:659-662` (`if let Some(targets) = …routes.stability_targets` →
  `rebalance_public_stability`); `routes.rs:435` → `stability_cut_indices` `routes.rs:394-413` →
  tier partition `routes.rs:448-459`. Only `StabilityTargets` constructor today is the test at
  `routes.rs:496` (`#[cfg(test)]`).
- **All-`None` constructors:** `random_sector.rs:413`, `project_io.rs:88`, `state/mod.rs:614`,
  Default `config.rs:284`.
- **Decision — EXPOSE:** add a control in the **Routes** wizard step
  (`iterative_gen.rs:1147-1195`, alongside `enabled`/`route_density`/`max_route_distance`/
  `ensure_connected_graph`) that sets `Some(StabilityTargets{…})`. **Default stays `None` →
  byte-identical → goldens green.** This is the cheapest way to make a working feature reachable.
  Route through `BuilderCommand` (command-bus invariant).
- **Alternative (delete):** if route-stability rebalancing is unwanted, remove the field +
  `StabilityTargets` struct + the Stage-9 `Some` branch. Confirm intent before deleting working
  engine code.

---

## GROUP C — KEEP (addressed: confirmed *not* stubs — do not churn)

> Included per the "resolve every flag" directive. The resolution for these is a **conscious
> no-op with rationale**, so a future session doesn't waste effort "fixing" working code.

### Task 9–11 — `search_base_seed` / `search_candidate_index` / `search_constraints_digest` — KEEP  ✅ DONE
> **DONE (conscious no-op, 2026-06-18).** Confirmed provenance passthroughs, working as
> designed; kept per the rationale below. No code required.
**Provenance passthroughs, working as designed.** Not user knobs — set only by the
`--constraints` seed-search CLI flow (`src/cli/generate.rs:58,61,62,65`) and echoed verbatim into
the manifest:
- `search_base_seed` → `GenerationManifest.base_seed` (`mod.rs:1008-1012`)
- `search_candidate_index` → `…candidate_index` (`mod.rs:1013`)
- `search_constraints_digest` → `…constraints_digest` (`mod.rs:1014-1018`)

(all inside `build_manifest` `mod.rs:975-1026`, called once at `mod.rs:744`; manifest fields at
`src/model/sector_model/mod.rs:1113-1120`). They change manifest bytes but have **zero generation
effect** — that's the whole point: they let `base_seed + candidate_index` reproduce a
constraint-search result. **Recommendation: KEEP.** No UI; do not expose. Delete *only* if you
deliberately want to drop seed-search provenance from the manifest (that's a feature removal, not
a stub fix).

### Task 12 — `allow_empty_hexes` — KEEP (validation gate)  ✅ DONE
> **DONE (conscious no-op, 2026-06-18).** Confirmed a live pre-gen validation gate, not a
> stub; kept per the rationale below. No code required.
**Not a stub.** No generation read, but a real pre-gen gate: `src/validate/validation.rs:107`
(`if g.system_count > grid_cells && !g.allow_empty_hexes`) rejects over-dense sectors (error at
`:111`). Config `config.rs:115`, UI checkbox `generation.rs:200-205`. It changes accept/reject,
not generated content — which is why the static pass mis-flagged it DEAD. **KEEP.** Optional
polish: make the label/tooltip say "validation" so it doesn't read as a generation knob.

---

## Suggested order & verification

1. **Tasks 1, 2** (wizard-visible dead dropdowns) — highest user-facing value.
2. **Tasks 3, 4, 5** (independent dead G1 fields) — can parallelize.
3. **Tasks 6 + 7 together** (the row-completeness pair) — pick delete-collapse (recommended) or
   implement-partial-rows, for *both*.
4. **Task 8** (expose stability_targets) or delete — confirm intent.
5. **Tasks 9–12** — documentation/no-op; nothing to build.

Per task, the definition of done is all three of:
```bash
cargo check --workspace --all-targets          # compiles
cargo test --test it -- golden                 # green WITHOUT updating snapshots
cargo test -p sectorforge-builder              # builder UI changes compile + pass
```
plus, for any delete, a **fixed-seed before/after sector diff** that is identical (normalize the
`input_digests` line). If a diff is *not* identical, you removed something live — stop and
re-trace.
