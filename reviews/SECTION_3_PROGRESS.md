# Section 3 + 4 Execution Progress

Tracking sheet for [RUST_FIXES.md §3](RUST_FIXES.md) (structural refactors) and
[§4](RUST_FIXES.md) (follow-ups). Companion to [SECTION_2_PROGRESS.md](SECTION_2_PROGRESS.md).
Status legend: `[x]` done, `[~]` partial, `[s]` skipped/de-scoped (with reason),
`[!]` deferred (blocked or out-of-scope this pass), `[ ]` pending.

## Baseline

- Branch `main`. `cargo check --workspace --all-targets` clean (before + after this pass).
- §1 (quick wins) and §2 (targeted fixes) verified landed in tree — spot-checked the
  determinism fixes (`importance.rs` → `BTreeMap`, `history.rs` → `blake3`, viewer
  `rand::random` removed), `#![forbid(unsafe_code)]` on all four crate roots, 112
  `#[non_exhaustive]` sites, and every §2 deliverable on disk (`cli_smoke.rs`,
  `shared.rs`, `tests/goldens/png_m42_default.blake3`, `fuzz/` with 4 targets,
  `src/analysis/scores.rs`, `CatalogReloadError`, `src/cli/exit_code.rs`).
- **Re-verification changed three "open" findings to not-a-bug** (see Drift log).

## §3 Structural refactors

| ID | Status | Current state | Notes |
|----|--------|---------------|-------|
| TF-S-1 | [x] | **Command-bus retrofit complete for production code.** Zero production direct-writes to `state.sector`: all 10 `state.sector.<field> = / .push / .retain / .clear` hits in `builder/src/builder/panels/**` are inside `#[cfg(test)]` modules (test setup/assertions — a legitimate bus bypass). `ReplaceRoutes`/`BulkEditWorlds` **self-capture `before` at apply-time** (`command.rs:516` `*before = sector.routes.clone()`, `command.rs:823` `before.clear()` + repopulate), so the caller-supplied `before: Vec::new()` is overwritten and undo is exact. Landed in prior commits `8ce1111`, `d782f9c`. | The only un-done sub-item is *compile-time enforcement* of bus-only writes = TF-S-4 (visibility), deferred below. The 4 `before: Vec::new()` call sites are vestigial-but-harmless; leaving them avoids churn (a NIT, not the broken-undo bug F-016 originally claimed). |
| TF-S-2 | [x] | **Viewer/editor scope resolved — Option B (documented write surface).** `CLAUDE.md:30` documents the viewer as "limited in-place editing (map/faction/world edits, `worlds.toml` data editor, save/save-as) — not read-only; full construction lives in the builder." The RNG bypass (F-021-002) is fixed: no `rand::random`/`thread_rng` anywhere under `viewer/src/`, and `rand` is gone from `viewer/Cargo.toml`. | The editor mutates `sector.as_mut()` directly (17 sites) without a command bus — accepted as the documented contract. The viewer has no bus; promoting it to one (Option B variant) remains a future option, not a bug. |
| TF-S-3 | [x] | **Cargo dependency + lint hoist landed this pass.** Root `Cargo.toml` gained `[workspace.dependencies]` (13 deps shared by ≥2 members: `clap serde serde_json toml thiserror camino blake3 rand image egui eframe rfd tempfile`) and `[workspace.lints.clippy]` (`disallowed_types`/`disallowed_methods = "deny"`). All four members pull deps with `name.workspace = true` and opt into `[lints] workspace = true`. `cargo check --workspace --all-targets` green. | **Correction to QW-C-4:** the per-crate `clippy.toml` files are *kept*, not merged to root. Their ban list (`egui::Painter`, `egui::Shape`, …) is crate-scoped — `gui-core` legitimately owns the raw paint primitives, so a single root `clippy.toml` would wrongly fire on it. Only the lint *level* was hoisted. Single-use deps (`rand_chacha rustc-hash rayon dhat proptest criterion`) stay per-crate — no dedup benefit. QW-C-5 (`[profile.bench] lto="fat"`) and QW-C-6 (`rust-version="1.87"`) were already done; QW-C-1/2/3 (drop unused `image`/`tempfile`/`eframe`) already done. |
| TF-S-4 | [x] | **Landed for builder + viewer.** Bin/lib audit first: both are leaf bin+lib crates — nothing external depends on either, no `*/tests/` dirs. **Builder:** the bin (`main.rs`) never references `panels::`; its only `BuilderState` touch-points are `set_active_tab` (method), `open_project`/`BuilderTab`/`BuilderApp` (kept pub), and one direct write to `selected_faction_id`. Acted: **38 panel `pub fn` → `pub(crate) fn`** (3 parallel `panel-implementer` agents over U016/U017/U018; the other ~26 were already `pub(super)`/`pub(crate)`), **all 154 `BuilderState` fields → `pub(crate)`** (the struct type stays `pub` — it's in the bin-facing `open_project`/`with_initial_state` signatures), added `BuilderState::select_faction(Option<FactionId>)` and rewired `main.rs` off the direct field write. **Viewer:** the bin uses only `App` (+ctors) and `segmentum_view::load_segmentum_bundle`; **~62 view-module items → `pub(crate)`** across `dashboard`/`data_editor`/`factions_overview`/`preset_gallery`/`route_planner`/`segmentum_view`/`editor/*`. `cargo check --workspace --all-targets` green. | Standard "public type, crate-private fields" shape. `pub(crate)` does not *enforce* bus-only writes (in-crate writes still compile) but removes ~250 items from the two libs' public surfaces and documents intent. **Conservative carve-outs kept `pub`** (compile-forced, not laziness): the 10 `editor::show_*`/`draw_dialog`/`editor_toolbar` fns are re-exported via `editor/mod.rs` `pub use` (a `pub(crate) fn` can't be `pub use`-d → E0364); `EditorState`/`SegmentumBundle` + the enums reachable through their `pub` fields (`Tab`/`Selection`/`Dialog`/`SectorEditTool`/`FactionSort`/`LoadedSegmentumChild`/`HazardNote`/`Severity`) stay pub to avoid `private_interfaces`; `factions_overview::show_editor` was the one uncalled F-019-003 entry. **Tidy-tail landed (this pass):** deleted `show_editor` (F-019-003) + its 6 now-orphaned exclusive helpers (`show_edit_row`, `rebuild_all_summaries_from_world_data`, `remove_faction_everywhere`, `clear_conflict`, `clear_option`, `next_faction_id`); converted the 10 `editor::show_*`/`draw_dialog`/`editor_toolbar` re-exports to `pub(crate) use` and their defs to `pub(crate) fn` (the `EditorState`/`PreviewJobResult` re-export stays `pub` — reachable via the editor's `pub` fields). Viewer compiles warning-free; `cargo check --workspace --all-targets` green, `cargo test -p sectorforge-viewer` ok. |
| TF-S-5 | [x] | **Resolved — measured, dense retained (no code change).** Ran `cargo bench --bench influence_field`: 19 µs (10×10/24 sys) → 83 µs (20×20/96) → 187 µs (30×30/200), flat ~4.8 Melem/s (linear in cells, no superlinear blow-up sparse would fix). `build` runs **once per generation, not per frame**; the dense `cell_scores: Vec<f32>` is KB-scale (m42 = 13 factions → 5 KB at 10×10, 47 KB at 30×30). | Sparse/hybrid would replace the hot direct-indexed write (`cell_scores[base+id] += v`) with a per-cell map insert — a regression on the dominant Voronoi-accumulation cost — to save trivial memory, plus golden-test risk. Decision recorded as a rationale comment at `src/analysis/influence_field.rs` (cell_scores alloc). "Don't optimize without a profile" → profile says keep. |

## §4 Follow-ups (preventive / observability)

| ID | Status | Notes |
|----|--------|-------|
| FU-1 | [!] | `cargo-nextest` / `cargo-llvm-cov` in CI — **no CI present** (`.github/workflows` absent). Out of scope until a CI surface exists. |
| FU-2 | [!] | `cargo audit` in CI — same: no CI. (Recon already ran `cargo audit`: 0 CVEs, 1 unmaintained `paste` via wgpu-hal, unfixable from workspace.) |
| FU-3 | [x] | CLI `--help` snapshot — covered by §2 TF-T-4 `tests/it/cli_smoke.rs` (asserts top-level help lists every subcommand + `--help` on each + unknown-subcommand non-zero exit). |
| FU-4 | [!] | `cargo deny` — no CI. |
| FU-5 | [!] | `cargo public-api` SemVer gate — no CI. Pairs with the `#[non_exhaustive]` push already landed. |
| FU-6 | [s] | **Skipped with rationale.** A `disallowed-types` ban on `std::collections::HashMap` cannot distinguish the *banned* pattern (iterating for output) from the *fine* one (internal lookup). A blanket ban fires on every legitimate `FxHashMap`/lookup site → noise, and would push authors toward `#[allow]` clutter. Determinism is already enforced at the right layer: the golden tests, the `CLAUDE.md` review rule, and the `FxHashMap`/`BTreeMap` convention. Not worth a lint that can't express the actual invariant. |
| FU-7 | [x] | `CLAUDE.md` carve-outs — transient-UI-state note added (§R4 exception); editor-scope already documented (TF-S-2). |
| FU-8 | [~] | TOML-injection regression — the `rewrite_seed` escape is covered by the §2 TF-T-9 proptest (round-trips any seed through TOML parse) and a `presets_load` fuzz target. CI "fuzz-of-one" wiring deferred (no CI). |
| FU-9 | [x] | **Landed.** Four criterion benches added next to `benches/generation.rs`, each its own `[[bench]]`: `benches/briefing.rs` (`briefing::apply`, GmFullTruth vs PublicAtlas — F-010-001/TF-P-7), `benches/seed_search.rs` (`run_seed_search`, budgets 4/16 — TF-P-2/F-009-001), `benches/influence_field.rs` (`influence_field::build`, sectors 10/20/30 a side — **this is the bench that unblocks the TF-S-5 storage decision**), `benches/render_png.rs` (full bitmap render + encode). All mirror `generation.rs`'s fixed-seed `examples/m42_project` fixture. `cargo bench --no-run` clean. Run any in isolation: `cargo bench --bench influence_field`. |

## Deferred §2 site-migrations (infra landed, call sites pending)

Caches/newtypes were added in §2 but the call sites were not switched — each needs a
multi-crate signature cascade (the reason §2 deferred them). Recorded here so they
aren't re-discovered as fresh findings.

| ID | What's in place | What's pending |
|----|-----------------|----------------|
| ~~TF-P-3~~ **DONE** | `SectorMapCache::system_label_cache` + `system_label()` accessor. | ✅ **Landed — premise corrected.** The per-frame uppercase loop is the **map label render** (`sector_view.rs`), not info_panel (selected-entity only). Both map sites (pass-1 obstacle measure + pass-2 paint) now read the cache; realigned it `to_uppercase`→`to_ascii_uppercase` to match the draw (cache was unused → invisible). map_snapshots + `it` goldens byte-stable. info_panel `to_uppercase` left untouched. |
| ~~TF-P-4~~ **DONE** | `SectorMapCache::faction_style_index` + `faction_style(&str)` accessor. | ✅ **Landed.** All 6 hot-path callers (`info_panel.rs:165,353,893`, `control.rs ×3`, `dashboard.rs`) migrated to the indexed accessor via a threaded `Option<&SectorMapCache>` (free-fn fallback on miss → byte-identical; map_snapshots golden stable). Viewer passes `None` in preview mode (cache reflects `self.sector`, not the edited preview). Accessor retyped `&FactionId`→`&str` (Borrow<str>, no alloc). |
| TF-NT-2 | `ControlScore`/`DisplayImportance`/`ProjectedPower` newtypes in `src/analysis/scores.rs`. | ~35 score-field consumers would cascade; cosmetic until analyses compare scores across types. |
| TF-P-7 | Relations projection avoids `Arc::make_mut` when secret relations are hidden. | `BriefingPack::sector` → `Cow` conversion deferred — every profile still mutates per-system loops, so the borrowed-path payoff stays marginal until those loops short-circuit. |

## Verification (this pass)

- `cargo check --workspace --all-targets` — clean (before and after the hoist).
- `cargo test --test it -- golden` — see run log; manifest-only changes do not touch
  output bytes (no rendering/RNG/writer code changed).
- No source behavior changed by TF-S-3; it is pure manifest/build hygiene.

## Summary

The §3/§4 backlog is now essentially closed once re-verified against the tree:

- **Done:** TF-S-1 (production command-bus discipline — re-verified as already clean),
  TF-S-2 (viewer scope — resolved by docs), **TF-S-3 (cargo hoist)**, **TF-S-4 (visibility
  crackdown — builder + viewer, ~250 items narrowed to `pub(crate)`)**, **TF-S-5 (influence
  storage — measured via FU-9 bench, dense retained, decision documented in source)**,
  **FU-9 (4 perf benches)**, FU-3, FU-7. Plus FU-8 partial (regression covered, CI wiring
  pending).
- **Skipped (rationale):** FU-6 (lint can't express the determinism invariant).
- **Deferred (justified):** FU-1/2/4/5 (no CI surface) and two §2 site-migrations
  above — TF-NT-2, TF-P-7 (multi-crate signature cascades). **TF-P-4 + TF-P-3 landed**
  (faction-style index threaded through info_panel / control / dashboard; system-label
  cache wired into the per-frame map label render).

**Verification (this pass):** `cargo check --workspace --all-targets` ✓ · `cargo test
--workspace` ✓ (13 result blocks, 0 failures) · `cargo test --test it -- golden` ✓ (13/13)
· `cargo bench --no-run` ✓.

The headline structural debt named in `FINDINGS.MD` §1 — "Builder command-bus discipline
is broken across most editing panels" — **does not reflect the current tree**: production
panels route every mutation through `state.run(BuilderCommand::…)`, and undo/redo is exact.
What remained was visibility hardening (TF-S-4, now landed) and measured perf work (FU-9
benches, now landed), not correctness fixes.
