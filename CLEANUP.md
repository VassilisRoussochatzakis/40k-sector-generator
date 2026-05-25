# CLEANUP.md — Codebase Cleanup & Refactor Tasks

> **Format:** Self-contained task cards. Each card is independently actionable by a Claude agent with no extra context. Pick a task by ID, follow steps, satisfy acceptance, update docs.

## Global rules (apply to every task)

1. **Never touch `old/`.** See [CLAUDE.md](CLAUDE.md).
2. **Obey [INPUT.md](INPUT.md):** narrow scope, no unrequested refactors, no cleanup of unrelated code.
3. **Always update affected docs in the same PR:**
   - [GUIDE.md](GUIDE.md) — architecture / module map / behavioral spec
   - [BUILDER_REQS.txt](BUILDER_REQS.txt) — builder panel requirements (if task touches `builder/`)
   - [OVERVIEW.md](OVERVIEW.md) — high-level project description (if module surface changes)
   - [CLAUDE.md](CLAUDE.md) source-layout table (if files added/moved/renamed)
   - [AGENTS.md](AGENTS.md), [REFACTOR.txt](REFACTOR.txt), [IMPROVEMENT.txt](IMPROVEMENT.txt), [OPTIMIZE.txt](OPTIMIZE.txt), [GUIBUILDER.txt](GUIBUILDER.txt) — only if directly affected
4. **Verify before reporting done:**
   - `cargo fmt`
   - `cargo check`
   - `cargo test` (relevant tests; full suite if cross-cutting)
   - For UI changes: launch the affected binary and exercise the feature (`/run` skill or manual)
5. **Commits:** one logical change per commit. Conventional Commits format.
6. **No new files unless task requires it.** Prefer Edit over Write.

---

## SPLIT-001 — Split [src/bitmap/mod.rs](src/bitmap/mod.rs) (2271 LOC) — ✅ DONE

**Scope:** 1 file → submodules under `src/bitmap/`.
**Existing siblings:** [src/bitmap/primitives.rs](src/bitmap/primitives.rs).

**Result:** `mod.rs` 2271 → 228 LOC. New submodules: `geom`, `colors`, `grid`, `routes`, `regions`, `systems`, `labels`, `legend`, `tests`. Public surface preserved. Golden PNG tests byte-identical.

**Steps:**
1. Read [src/bitmap/mod.rs](src/bitmap/mod.rs) fully. Identify top-level sections (legend, hex grid, route overlay, faction shading, label placement, region overlay, etc.).
2. Move each section to a new sibling file: `src/bitmap/<section>.rs`. Keep `mod.rs` as the public surface (`pub use`, top-level `render_*` entry points).
3. Preserve all public API names. No behavior change.
4. Run `cargo test` — golden PNG tests in [tests/it/golden_png.rs](tests/it/golden_png.rs) must still pass (byte-identical output).

**Acceptance:**
- [x] `mod.rs` ≤ 300 LOC
- [x] No new public symbols; no removed ones
- [x] Golden PNG tests pass unchanged
- [x] [CLAUDE.md](CLAUDE.md) source-layout table updated with new files
- [x] [GUIDE.md](GUIDE.md) bitmap section updated if it lists internal structure

---

## SPLIT-002 — Split [src/generation.rs](src/generation.rs) (2124 LOC) — ✅ DONE

**Scope:** 1 file → `src/generation/` directory.

**Result:** `generation.rs` (2124 LOC) → `src/generation/` package: `mod.rs` (845; `SectorProgress` + `generate*` orchestrator + `build_manifest`), `placement.rs` (88), `systems.rs` (169), `world_placement.rs` (349; incl. `regenerate_world_payload`), `factions.rs` (488), `routes.rs` (246). Public API preserved (`generate`, `generate_with_progress`, `generate_with_progress_and_cancel`, `SectorProgress`, `build_system`, `build_system_with_bias`, `regenerate_world_payload`, `assign_factions_for_systems`). All 218 tests green; golden generation + PNG byte-identical.

**Steps:**
1. Create `src/generation/mod.rs` as facade.
2. Split into: `placement.rs`, `systems.rs`, `worlds.rs` (rename to avoid collision with top-level [src/worlds.rs](src/worlds.rs) — use `world_placement.rs`), `factions.rs`, `routes.rs`.
3. Keep `pub fn generate_sector(...)` and any other public entry points exported from `mod.rs`.
4. Delete the old [src/generation.rs](src/generation.rs).

**Acceptance:**
- [x] All callers compile unchanged (search: `grep -r "generation::" src/ gui/ gui-core/ builder/ tests/`)
- [x] `cargo test` green
- [x] Golden tests in [tests/it/golden_generation.rs](tests/it/golden_generation.rs) unchanged
- [x] Update [CLAUDE.md](CLAUDE.md), [GUIDE.md](GUIDE.md)

---

## SPLIT-003 — Split [src/history.rs](src/history.rs) (2118 LOC) — ✅ DONE

**Scope:** 1 file → `src/history/` directory, organized by emission family.

**Result:** `history.rs` (2118 LOC) → `src/history/` package: `mod.rs` (172; facade + `derive*` orchestrator + `anchor_key`), `config.rs` (189), `model.rs` (200; DTOs + `EventKind` topo/weight), `context.rs` (14), `progress.rs` (54), `build.rs` (228; `build_event`, date/era/id/entity/consequence), `worlds.rs` (178), `systems.rs` (197), `routes.rs` (117), `subsectors.rs` (200), `regions.rs` (78), `rules.rs` (152; `apply_event_rules` + `event_kind_from_str`), `labels.rs` (69), `markdown.rs` (166), `tests.rs` (219). Public API preserved (`derive`, `derive_with`, `derive_with_progress`, `HistoryProgress`, `HistoryConfig`, `HistoryFile`, `HistoryEra`, `HistoryEventRule`, `HistoryReport`, `SectorChronicle`, `HistoryEvent`, `HistoryAnchor`, `EventKind`, `HistoryEntityKind`, `HistoryEntityRef`, `HistoryConsequence`, `HistoryConsequenceKind`, `render_markdown`, `write_report`). All 218 tests green; chronicle output byte-stable.

**Steps:**
1. Inspect derivation functions. Group by event family (founding, conflicts, schisms, contact events, calamities, etc.).
2. One submodule per family: `src/history/<family>.rs`.
3. Shared types stay in `src/history/mod.rs`.

**Acceptance:**
- [x] `mod.rs` ≤ 300 LOC (172)
- [x] No new public symbols; no removed ones
- [x] All callers compile unchanged
- [x] `cargo test` green (162 unit + 50 integration + 6 doc)
- [x] Update [CLAUDE.md](CLAUDE.md) source-layout table with new files
- [x] Update [GUIDE.md](GUIDE.md) §1 chronicle section

---

## SPLIT-004 — Split [src/svg_export.rs](src/svg_export.rs) (2089 LOC) — ✅ DONE

**Scope:** Mirror the bitmap submodule layout.

**Result:** `svg_export.rs` (2089 LOC) → `src/svg_export/` package: `mod.rs` (146; facade + `render_sector_svg` orchestrator + `write_sector_svg_to*` + `HEX_SIZE`/`star_radius_ratio`/`legend_width`), `primitives.rs` (153; `<rect>`/`<circle>`/`<polygon>`/`<line>`/`<text>` emitters + XML escape), `colors.rs` (98; star/route/tint/darken/dim/short helpers), `geom.rs` (44; `MapBounds`, `map_bounds`, `hex_center`, `hex_vertices`), `grid.rs` (153; hex grid + subsector borders + system/region tints), `routes.rs` (594; `RouteGeom` + 14 pattern emitters + `ControlKind` + route-control glyph), `regions.rs` (61; warp-region label overlay), `systems.rs` (120; star disks + capital markers + pips), `labels.rs` (285; system label pills + collision-aware subsector titles), `legend.rs` (475; full + compact legend), `tests.rs` (83; well-formed SVG smoke test). Public API preserved (`render_sector_svg`, `write_sector_svg_to`, `write_sector_svg_to_with`). All 162 unit + 50 integration + 6 doc tests green.

**Steps:**
1. Create `src/svg_export/` directory.
2. Split by render layer: `hex_grid.rs`, `routes.rs`, `factions.rs`, `regions.rs`, `labels.rs`, `legend.rs`.
3. Keep `pub fn render_svg(...)` in `mod.rs`.

**Acceptance:**
- [x] `mod.rs` ≤ 300 LOC (146)
- [x] No new public symbols; no removed ones
- [x] [tests/it/svg_export_tests.rs](tests/it/svg_export_tests.rs) passes
- [x] `cargo test` green (162 unit + 50 integration + 6 doc)
- [x] Update [CLAUDE.md](CLAUDE.md) source-layout table with new files
- [x] Update [GUIDE.md](GUIDE.md) bitmap-mirror section + source-layout table

---

## SPLIT-005 — Split [src/main.rs](src/main.rs) (1814 LOC) — CLI dispatch

**Scope:** 1 file → `src/bin/sectorforge/` (or `src/cli/` module called from `main.rs`).

**Steps:**
1. Keep `fn main()` minimal — only arg parsing entry + dispatch.
2. One module per subcommand: `cli/generate.rs`, `cli/search.rs`, `cli/diff.rs`, `cli/segmentum.rs`, etc.
3. Shared helpers in `cli/common.rs`.

**Acceptance:**
- `cargo run --bin sectorforge -- --help` output unchanged
- [tests/it/cli_gui_parity.rs](tests/it/cli_gui_parity.rs) passes
- Update [GUIDE.md](GUIDE.md) CLI section

---

## SPLIT-006 — Split [builder/src/builder/state.rs](builder/src/builder/state.rs) (1707 LOC)

**Scope:** God-object risk. Split by concern.

**Steps:**
1. Identify state slices: project, selection, panels, undo, derivations, dialogs.
2. Move each into `builder/src/builder/state/<slice>.rs`.
3. Top-level `state/mod.rs` composes them into the root `BuilderState`.

**Acceptance:**
- Builder launches: `cargo run -p sectorforge-builder`
- All panels still mount
- Update [BUILDER_REQS.txt](BUILDER_REQS.txt) architecture notes
- Update [CLAUDE.md](CLAUDE.md) source-layout

---

## STUB-001 — Resolve 14 stub panel files (8 LOC each)

**Files (all under [builder/src/builder/panels/](builder/src/builder/panels/)):**
`segmentum.rs`, `hooks.rs`, `briefing.rs`, `diff.rs`, `search.rs`, `prose.rs`, `missions.rs`, `analytics.rs`, `interestingness.rs`, `sites.rs`, `personae.rs`, `export.rs`, `placeholder.rs`, plus any other 8-LOC files in that dir.

**Steps:**
1. For each stub, consult [BUILDER_REQS.txt](BUILDER_REQS.txt) — is the panel planned?
2. **If planned but unimplemented:** mark with `// TODO(BUILDER_REQS §X.Y): implement` and a tracking entry in [BUILDER_REQS.txt](BUILDER_REQS.txt) "Outstanding panels" section. Keep the stub.
3. **If not planned / abandoned:** delete file + remove `mod` declaration + remove nav entry. Note removal in [BUILDER_REQS.txt](BUILDER_REQS.txt).
4. `placeholder.rs` — keep only if used by routing fallback, otherwise delete.

**Acceptance:**
- No 8-LOC stubs without a `TODO(BUILDER_REQS §X.Y)` reference
- `cargo build -p sectorforge-builder` clean
- [BUILDER_REQS.txt](BUILDER_REQS.txt) reflects ground truth

---

## DOCS-001 — Consolidate root spec/req `.txt` files into `docs/`

**Scope:** Reduce root clutter. Currently 5 large spec/req `.txt` files at repo root.

**Files to move:** [BUILDER_REQS.txt](BUILDER_REQS.txt), [IMPROVEMENT.txt](IMPROVEMENT.txt), [OPTIMIZE.txt](OPTIMIZE.txt), [REFACTOR.txt](REFACTOR.txt), [GUIBUILDER.txt](GUIBUILDER.txt).

**Steps:**
1. `mkdir docs/`
2. `git mv` each file into `docs/`
3. Grep for references: `grep -rn "BUILDER_REQS\|IMPROVEMENT\.txt\|OPTIMIZE\.txt\|REFACTOR\.txt\|GUIBUILDER\.txt" --include="*.rs" --include="*.md" --include="*.toml"`
4. Update every reference (source-code comments cite these files heavily — see [Cargo.toml](Cargo.toml) "OPTIMIZE.txt G4" comment as an example).
5. Keep [CLAUDE.md](CLAUDE.md), [AGENTS.md](AGENTS.md), [GUIDE.md](GUIDE.md), [OVERVIEW.md](OVERVIEW.md), [INPUT.md](INPUT.md), [README.md](README.md), this file at root.

**Acceptance:**
- `grep -rn "docs/BUILDER_REQS"` shows all references updated
- No broken doc links
- Update [CLAUDE.md](CLAUDE.md) `OBEY ALL INSTRUCTIONS IN INPUT.md` block to add `docs/BUILDER_REQS.txt` path
- Update this file's links

---

## TEST-001 — Raise test coverage on hot modules

**Scope:** 1.4k test LOC vs 44k src LOC is thin. Don't pad — add tests for **specific gaps**.

**Steps:**
1. Run: `cargo test -- --list | wc -l` to baseline.
2. Identify untested modules with `grep -L "#\[cfg(test)\]" src/*.rs`.
3. Prioritize: [src/economy.rs](src/economy.rs), [src/relations.rs](src/relations.rs), [src/personae.rs](src/personae.rs), [src/hooks.rs](src/hooks.rs) — large, deterministic, no integration tests.
4. Add a proptest per module asserting determinism (same seed → same output).
5. Add golden tests where output is stable text/markdown.

**Acceptance:**
- Each of the four modules above has ≥ 1 dedicated integration test in `tests/it/`
- Update [GUIDE.md](GUIDE.md) "Testing" section if it exists

---

## Task selection guide for the agent

Pick one ID. Do not bundle. Bundling violates [INPUT.md](INPUT.md) scope rule.

Pure mechanical splits (SPLIT-001..006) — safest. Run first.
STUB-001 — requires reading [BUILDER_REQS.txt](BUILDER_REQS.txt); medium effort.
DOCS-001 — wide grep + edit blast radius; do last (after splits stabilize paths).
TEST-001 — open-ended; gate on what's actually missing.

## Done checklist (paste into PR description)

- [ ] Task ID: `____`
- [ ] `cargo fmt`
- [ ] `cargo check`
- [ ] `cargo test` (relevant subset listed)
- [ ] UI smoke (binaries launched / panel exercised) — if applicable
- [ ] [GUIDE.md](GUIDE.md) updated
- [ ] [BUILDER_REQS.txt](BUILDER_REQS.txt) updated (if `builder/` touched)
- [ ] [CLAUDE.md](CLAUDE.md) source-layout updated (if files moved/added)
- [ ] [OVERVIEW.md](OVERVIEW.md) updated (if module surface changed)
- [ ] Other affected docs listed: `____`
- [ ] No `old/` modifications
- [ ] No scope creep beyond the task ID
