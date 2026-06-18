# Code-Quality Review — 2026-06-05

Agentic full-workspace review (`main`, ~117k LOC across `src/`, `builder/`, `viewer/`, `gui-core/`, `tests/`). Seven parallel area-reviews, deduped into themes below. Finding IDs (`A1`, `E3`…) map to the per-area tables at the bottom.

## Baseline health

| Signal | Result |
|---|---|
| `cargo clippy --workspace --all-targets` | **0 warnings** |
| `unsafe` blocks (non-test) | **0** |
| `TODO`/`FIXME`/`HACK`/`XXX` | **0** |
| `#[allow(...)]` | 17 (10× `too_many_arguments`, 2× `large_enum_variant`) |
| Determinism invariants (Fx-lookup-only, RNG via `rng.rs`, square sectors, byte-stable writers) | respected in scope (agents A & C verified) |

Surface quality is already maxed. Everything below is structural — lint cannot see it.

---

## P0 — Correctness: command-bus bypass (undo/redo holes)

Document state mutated outside `state.run(BuilderCommand::…)` → not undoable, can desync. Flagged HIGH by two independent agents. **Verify each against §R4 / §D3 before fixing** — some may be deliberate carve-outs.

- **`E2`** `builder/src/builder/panels/control.rs:769` — `state.sector.systems[i].primary_factions = derived.clone()` written **every frame** when unlocked. Document state, per-frame, off-bus. Strongest smell. → move to derivation/read-model, or commit via `EditSystem` only on change.
- **`E1`** `builder/src/builder/panels/control.rs:957` — `apply_faction_power(...)` + manual `state.dirty=true`. Faction power totals are document state. → add `ApplyFactionPower` command.
- **`D1`** `builder/src/builder/state/regions_ops.rs:19,47,61,84` — region add/paint/erase bypass the bus while `SetRegionKind`/`RenameRegion` use it. Same domain, inconsistent. → add region commands, or document the carve-out (code cites §D3; confirm that's authoritative vs CLAUDE.md listing regions as bus-mandatory).
- **`D2`/`D11`** `builder/src/builder/state/derivations.rs:373` — `recompute_chronicle` overwrites `sector.chronicle` (holds user `manual` events) directly; can interleave with the undoable `EditChronicle`. → funnel through command or document precedence.
- **`D4`** `builder/src/builder/command.rs` — `MoveSystem`/`SwapSystems` mutate coords without recomputing incident-route `controls` → control overlay may desync after move/undo. Verify recompute/invalidate.

---

## P1 — High-leverage: kill the boilerplate

Largest LOC-and-drift win in the repo. Ordered by safety × payoff.

1. **`E3` panel `labeled()` ×32 byte-identical copies** → hoist one `gui_core::ui_kit::labeled` (+ `labeled_inline`/`hint_label`). Trivial, mechanical, shrinks every god-file. **Do first.**
2. **`B-S3` / `A4` / `C6` / `E6` enum→slug boilerplate ×20+** — every status enum hand-writes `as_slug()` + `Display` + a parallel parse table (drift hazard: a new variant silently fails to parse). Codebase already accepts macros (`enum_slug!`). → add `enum_slug! { … }` (or `strum::EnumString`/`AsRefStr`). Deletes hundreds of lines.
3. **`E-S3` the `EditWorld` clone-mutate-dispatch idiom ×26** (+ `EditSystem` ×9; modal error string reinvented ×23) → `state.edit_world(wid, |w| {…})` / `edit_system(...)` helper that clones, runs closure, dispatches, surfaces uniform error. ~200 lines.
4. **`B-S1` / `B-S2` analysis report skeleton** — 9 modules duplicate `derive → score → render_md → write_report`; `hooks.rs` and `missions.rs` are nearly the same file twice (`cap_per_anchor` copy-pasted verbatim). → `SectorReport` trait (`type Config; const BASE_NAME; fn derive_with; fn render_markdown`) with default `derive`/`write_report`, + generic `load_config_file<T: DeserializeOwned>`.
5. **`C2` / `C3` CLI runner boilerplate** — `(project,sector)` resolve match + the `"pass exactly one of --project…"` string ×7 + the `out/json/markdown` emit triple ×13 → `common::resolve_sector_with_cfg` + `common::emit_report(...)`.
6. **`F1` / `F2` / `F7` viewer ships two editing stacks** — `App` live-edit (`Arc::make_mut`, `live_dirty`, `write_sector_to_path`) vs the separate `editor::` module (`EditorState`, `Dialog::SaveAs`, `save_project_sector`): two dirty flags, two save paths, two `empty_*` constructors, dup add-route/drag-move/distance logic. Dominant viewer hazard. → unify on one surface; extract shared `worlds.toml` grid widget into gui-core (`enum_combo` is byte-identical across builder & viewer).
7. **`B5`** field-wise `add`/`scale`/`clamp` hand-rolled across ~10 fields in `StrategicOutput`/`ResourceVector`/`PresenceDimensions` (NaN/typo-prone — `.clamp(0.0,100.0)` ×10) → derive macro or `fields_mut() -> [&mut f32; N]`.

---

## P1.5 — Type safety: stop stringifying typed data

Enums `to_string()`'d at boundaries then string-compared downstream — reintroduces the drift `ids.rs` newtypes set out to kill.

- **`C1` (real data-loss class)** `src/validate/diff.rs:369` — resource diff matches fields by string with `_ => 0.0`; a new resource in `RESOURCE_KEYS` **silently vanishes from the diff**, no compile error. → `SectorBalance::get(key) -> Option<f32>` co-located with the keys.
- **`A3`** `src/gen/generation/routes.rs:52` — route weighting matches feature tags by raw literals (`s == "feature:trade_hub"`) → compare against `NotableFeature` variants (renamed feature becomes a compile error).
- **`A2`** `src/model/sector_model/mod.rs:248` — `WorldDto`'s 9 enum-valued fields stored as `Arc<str>`, stringified on build, string-compared later → hold the real `worlds.rs` enums (`#[serde(into/from)]`).
- **`E4`** `builder/src/builder/panels/world.rs` — `format!("{v:?}")` debug-name used as a storage key ×12 → explicit slug fns.

---

## P2 — God-files, hot paths, test rigor

**Split (mechanical; do alongside P1 edits):**

| Finding | File | LOC | Split into |
|---|---|---|---|
| `A1` | `src/model/sector_model/mod.rs` | 1516 | DTO layer / `routes_view.rs` (render vocab + FNV hash) / `scoring.rs` |
| `B11` | `src/analysis/economy.rs`, `relations.rs` | 1785/1689 | `config`/`tables`/`derive`/`risk`/`render` |
| `D3` | `builder/src/builder/command.rs` | 1922 | per-command `trait Command { dep_classes; apply; revert }` or paired-arm macro |
| `D5` | `builder/src/builder/state/mod.rs` | — | ~110-field god-struct; fold panel scratch into sibling `*State` structs |
| `E7`/`E4` | panels `system.rs`/`world.rs` | 2033/1625 | archetype/preview; identity/environment/society/features/claims |
| `F3` | `gui-core/src/sector_view.rs` | 1711 | 27-field no-`Default` god-widget → add `Default`/builder; split `show()` body |
| `F8` | `gui-core/src/info_panel.rs` | 1156 | split formatting module from `Ui` render fns |
| `C6` | `src/worlds.rs` | 1371 | taxonomy enums vs `worlds.toml` IO |

**Per-frame / algorithmic cost:**
- `E8`/`E9` — `route_component_count` + full `chronicle.events.clone()` recomputed every frame, no cache → memoize behind a derivation keyed on a slice digest.
- `F4`/`F10` — `SectorView` callers passing `cache:None` (`planner_view.rs:100`, `editor/map_panel.rs:63`) hit O(systems·regions) per-hex scans every frame; centers map + star-dust rebuilt per frame on the map hot path → make `SectorMapCache` mandatory / cache star-dust into a `Shape`.
- `B1` (HIGH) — `economy.rs:1254` `system_supply_risk` is O(systems·worlds·resources·edges) → pre-bucket deps into a `BTreeMap` once.
- `B3`/`B6` — O(F²·rules) pair scans; `String`-allocating map keys in the hot co-occurrence loop → index rules once; key on `(u32,u32)` faction indices.

**Test rigor:**
- `G2` (HIGH) — RESOLVED (2026-06-18): committed content goldens for `sector.json`/`sector.md` are now in place and tested at `tests/it/golden_generation.rs:348` (`sector_json_matches_committed_golden`) and `:354` (`sector_md_matches_committed_golden`), with git-tracked `tests/goldens/sector_m42_default.json` and `.md` blessed behind `UPDATE_*`. (Was: no committed content golden — only run-to-run equality + counts, so a deterministic-but-*wrong* text change passed.)
- `G3` (HIGH) — RESOLVED (2026-06-18): the cheap segmentum cases are un-ignored and run by default — `segmentum_tests.rs:103` (`segmentum_example_parses_and_children_fit_grid`) and `:142` (`duplicate_child_slot_is_rejected`), covering parse/grid-fit + the rejection path. The remaining `compose_segmentum` tests are inherently composition-bound (full-m42 compose) and stay `#[ignore]` by policy (run via `--ignored`).
- `G1` (HIGH) — 4 suites' docs claim "many random seeds (proptest)" but the test re-derives one memoized fixture (idempotency ≠ reproducibility); no `proptest!` exists → add real seed-varying `proptest!` or fix the docs.
- `G4` — `Html`/`Bitmap` never exercised through `export_sector` dispatch. `G5` — fixture `OnceLock` boilerplate dup ×5 → move to `shared.rs`.

---

## P3 — Good first issues

- **Viewer ignores `palette::{success,warning,danger,info}()`** — ~20 hardcoded amber/red `Color32::from_rgb(...)` triples re-introduce the SPRUCE-D7 defect (`F5`/`F6`/`F9`/`F11`). Theme-unaware. Mechanical fix.
- `A6` `String→Arc` double-alloc ×4 (`Arc::from(&'static str)`); `A11` per-byte `format!` in `rng.rs` hex hot path; `C5` dead branch in `redact_for_observer`; `D7`/`D8` dead `let _ =` bindings; `B12`/`B10` magic thresholds → named consts; `C7` unescaped `|` in markdown tables.

---

## Suggested sequence

1. **Verify + fix P0 bus bypasses** (start E2/E1 — clearest).
2. **`labeled()` widget extraction** (safe warm-up, touches everything).
3. **`enum_slug!` macro** + **`C1` diff drift fix** (kills two silent-drift classes).
4. **`G2` content golden** before any god-file split.
5. Then dedup waves 3–7 of P1, god-file splits behind the new golden net.

---

## Appendix — full per-area findings

### AREA A — `src/model` + generation
- A1 [MED] `sector_model/mod.rs:700` — render vocab (`RoutePattern`/`strides`/`stable_pattern_hash`) lives in the data-model module → extract to `routes_view.rs`/`export/`.
- A2 [MED] `sector_model/mod.rs:248` — `WorldDto` stores 9 enum fields as `Arc<str>`, stringified then string-compared → hold real enums.
- A3 [MED] `gen/generation/routes.rs:52` — feature tags matched by raw string literals → compare against `NotableFeature` variants.
- A4 [MED] `model/taxonomy.rs:48` — four hand-written variant→enum tables, no exhaustiveness guard → `strum` or round-trip test.
- A5 [MED] `sector_model/mod.rs:16` — ~199 `pub` fields make the command-bus invariant unenforceable at type level → accessors or document.
- A6 [LOW] `gen/generation/mod.rs:619` — `GENERATOR_NAME.to_string().into()` double-allocs ×4 → `Arc::from`.
- A7 [LOW] `sector_model/mod.rs:332` — O(n) linear scans in `get_system`/`get_world`; a `BTreeMap<SystemId,usize>` index caps worst case.
- A8 [LOW] `sector_model/mod.rs:332` — read accessors lack `#[must_use]`.
- A9 [LOW] `sector_model/mutation.rs:457` — `(*self.regions).clone()` full deep-copy per hex edit → `Arc::make_mut`.
- A10 [LOW] `gen/hidden_routes.rs:455` — infallible `.unwrap()` ×2 → `expect(...)` documenting the invariant.
- A11 [LOW] `model/rng.rs:71` — `hex()` `format!("{b:02x}")` per byte → `write!` into pre-sized buffer.
- A12 [LOW] `sector_model/mod.rs:303` vs `gen/generation/mod.rs:837` — `GenerationManifest` built field-by-field in two places with divergent defaults → single constructor.
- Determinism/RNG/square invariants all respected in scope.

### AREA B — `src/analysis`
- B-S1 systemic — 9 report modules duplicate `load→derive→score→render→write`; extract `SectorReport` trait + generic `load_config_file<T>`.
- B-S2 systemic — `hooks.rs` ≈ `missions.rs` (`cap_per_anchor` verbatim); extract `WeightedAnchored` trait + `merge_manual`/`cap_per_anchor`/`rank`.
- B-S3 systemic — ~20 hand-written `as_slug`+`Display` enums; add `enum_slug!` macro.
- B1 [HIGH] `economy.rs:1254` — O(systems·worlds·resources·edges) `system_supply_risk` → pre-bucket deps.
- B3 [MED] `relations.rs:714` — O(F²·rules) pair scans → index rule lists once.
- B4 [MED] `hooks.rs:204` & `missions.rs:232` — `cap_per_anchor` duplicated → generic.
- B5 [MED] `economy.rs:219` / `1399` / `control.rs:110` — hand-rolled field-wise math ×10 fields → macro/`fields_mut()`.
- B6 [MED] `relations.rs:1228` — `canonical_pair` allocates two `String`s per pair-event in hot loop → key on indices.
- B7 [LOW] analytics/relations — `format!("{}",x).into()` map keys for closed enums → intern via `as_slug`.
- B8 [LOW] `search.rs:1305` — O(candidates·top) insert scan → `BinaryHeap` if `top` grows.
- B9 [LOW] 15 sites — `partial_cmp().unwrap_or(Equal)` on f32 → centralize `cmp_f32_desc`.
- B10 [LOW] `economy.rs:261` — 9-deep `mul_add` unreadable → `WEIGHTS.iter().zip(...)`.
- B11 [LOW] `economy.rs`/`relations.rs` god-modules → sub-module split.
- B12 [LOW] scattered magic thresholds → named `const` bands.

### AREA C — export / validate / worlds / cli
- C-S1 systemic — CLI runner boilerplate not absorbed by `common.rs` → `load_or_regenerate_with_cfg` + `emit_report`.
- C-S2 systemic — stringly-typed economy mirror in `diff.rs` silently drops on drift.
- C-S3 systemic — `bitmap/` vs `svg_export/` duplicate label/layout geometry → `render_core/`.
- C1 [MED] `validate/diff.rs:369` — resource diff `_ => 0.0` silently drops new resources → `SectorBalance::get`.
- C2 [MED] `cli/{economy,relations,...}.rs` — `(project,sector)` match + string ×7 → hoist helper.
- C3 [MED] `cli/*` — `out/json/markdown` emit triple ×13 → `common::emit_report`.
- C4 [MED] `export/{bitmap,svg_export}/labels.rs` — `system_label_visible` + placement geometry duplicated → shared predicate.
- C5 [LOW] `export/html_export.rs:257` — dead branch in `redact_for_observer`.
- C6 [LOW] `worlds.rs` 1371 — god-file mixes taxonomy + IO; per-enum `FromStr`/`Display`/`VARIANTS` triples desync-prone.
- C7 [LOW] `export/segmentum.rs:840` & `diff.rs` — unescaped `|`/newline in markdown table rows.
- C8 [LOW] `export/html_export.rs:252` — magic `20.0` observer-visibility default → name it.
- C9 [LOW] `cli/common.rs:107` — wildcard arm on closed `Severity` enum masks future variant.
- Non-issues confirmed clean: fixed-precision float formatting, BTree ordering, Result propagation, XML/HTML/JSON escaping, `exit_code::from_error` wired.

### AREA D — builder command bus + state
- D-S1 systemic — hand-maintained three-way match symmetry across 35 variants (~107 arms), apply/revert pairing convention-only → per-command trait or macro.
- D-S2 systemic — document state mutated outside the bus (regions, chronicle, economy recompute).
- D-S3 systemic — `BuilderState` ~110-field god-struct mixes document/cache/transient UI.
- D1 [HIGH] `state/regions_ops.rs:19` — region edits bypass bus → not undoable.
- D2 [HIGH] `state/derivations.rs:373` — `recompute_chronicle` overwrites serialized chronicle off-bus.
- D3 [MED] `command.rs:363/410/857` — three parallel matches → `Command` trait / paired-arm macro.
- D4 [MED] `command.rs` — `MoveSystem`/`SwapSystems` don't recompute incident-route controls → overlay desync.
- D5 [MED] `state/mod.rs:152` — ~110 `pub(crate)` fields → fold panel scratch into `*State` structs.
- D6 [MED] `state/derivations.rs:306` — verify staleness gate prevents per-frame economy recompute.
- D7 [LOW] `command.rs:478` — dead `let _ = si;`.
- D8 [LOW] `state/derivations.rs:311` — dead `sys_idx` BTreeMap.
- D9 [LOW] `command.rs:94` — no `size_of` guard / `large_enum_variant` lint on cloned-per-redo enum.
- D10 [LOW] `state/derivations.rs:493` — validation silently skipped on missing catalog, no status hint.
- D11 [LOW] `command.rs:817` vs `derivations.rs:373` — `EditChronicle` vs `recompute_chronicle` precedence undefined.

### AREA E — builder panels
- E-S1 systemic — `labeled()` copy-pasted byte-identical into 32 files → `ui_kit::labeled`.
- E-S2 systemic — list/detail/manual-editor master-detail shell repeats ~8 panels → `roster_detail` + `add_row_scratch<T>` helpers.
- E-S3 systemic — `EditWorld` clone-mutate-dispatch idiom ×26 (+`EditSystem` ×9) → `state.edit_world(...)` helper.
- E1 [HIGH] `control.rs:957` — `apply_faction_power` off-bus.
- E2 [HIGH] `control.rs:769` — `primary_factions` written every frame off-bus.
- E3 [HIGH] panels ×32 — `labeled` duplicated → hoist.
- E4 [MED] `world.rs:735` — 1625-line god-file + `format!("{v:?}")` storage keys ×12 → split + slug fns.
- E5 [MED] `control.rs:478` & `world.rs:1050` — claim/presence tables + chip colours duplicated → shared `presence_widgets.rs`.
- E6 [MED] `history.rs:60` & `control.rs:69` — `SYSTEM_STATES` + label/key duplicated → shared module.
- E7 [MED] `system.rs:106` — 2033-line god-file → extract archetype/preview sub-modules.
- E8 [MED] `routes.rs:91` — `route_component_count` recomputed every frame → memoize.
- E9 [MED] `history.rs:582` — full `chronicle.events.clone()` per frame → iterate by ref.
- E10 [MED] `control.rs:1203` — >150-line filter/list fns → shared `filter_bar` helper.
- E11 [LOW] `map/context_menu.rs` — 177-line menu builders → table-drive items.
- E12 [LOW] `search.rs` — 308-line `show` fn → extract blocks.
- E13 [LOW] catalog dirty-marking boilerplate ×6 → `state.mark_catalog_dirty`.
- E14 [LOW] `world.rs:1197` & `control.rs:1479` — duplicate `claim_chip_colours` → `claim_chip` widget.
- Note: catalog panels writing `data_catalogs.*` directly is intentional per documented contract — not flagged. `ModalKind` already a single enum — no bool-flag sprawl.

### AREA F — viewer + gui-core
- F-S1 systemic — two parallel sector-editing stacks (App live-edit vs `editor::`) → unify.
- F-S2 systemic — `palette::{success,warning,danger,info}` exist but viewer uses ~20 hardcoded `Color32` → replace.
- F-S3 systemic — `SectorView` 27-field god-widget, no `Default`/builder.
- F1 [HIGH] `app/editor_views.rs:30` + `editor/map_panel.rs` — second editing+save stack → unify, delete one.
- F2 [HIGH] `data_editor.rs:141` vs `builder/.../worlds_editor.rs:188` — `worlds.toml` grid + `enum_combo` reimplemented per crate → shared gui-core widget.
- F3 [MED] `gui-core/sector_view.rs:136` — 27-field config, no `Default` → add builder.
- F4 [MED] `sector_view.rs:264` — `cache:None` callers hit O(N·regions) per-hex per frame → mandatory cache.
- F5 [MED] viewer ×11 sites — hardcoded amber/red colors → `palette::warning/danger/success`.
- F6 [MED] `editor/dialogs.rs:195` — SaveAs error raw `Color32` → `palette::danger`.
- F7 [MED] `editor/map_panel.rs:163` vs `app/sector_view.rs:583` — drag-move/add-route recompute duplicated → hoist shared helper into `sectorforge`.
- F8 [LOW] `info_panel.rs` 1156 — formatting fused with layout → split.
- F9 [LOW] `editor/factions_panel.rs:308` — local `palette_dim()` redefines a shared color → call `chrome_text_dim()`.
- F10 [LOW] `sector_view.rs:382` — per-frame centers map + star-dust rebuild → memoize in cache.
- F11 [LOW] `factions_overview.rs:399` — same amber for success & error → branch + semantic colors.
- F12 [LOW] `palette.rs:771` — `stability_color` greens/ambers overlap `StatusColors` → document the split.

### AREA G — tests
- G-S1 systemic — doc-vs-reality determinism gap: 4 suites claim proptest seed-varying but re-derive one memoized fixture.
- G-S2 systemic — fixture boilerplate duplicated 5×.
- G-S3 systemic — writer/format coverage gap; no on-disk content golden for `sector.json`/`sector.md`.
- G1 [HIGH] economy/hooks/personae/relations `_tests.rs` — false "many random seeds" docs → real `proptest!` or fix docs.
- G2 [HIGH] RESOLVED (2026-06-18): committed content goldens for text outputs now tested at `golden_generation.rs:348` (`sector_json_matches_committed_golden`) / `:354` (`sector_md_matches_committed_golden`), blessed behind `UPDATE_*` with git-tracked `tests/goldens/sector_m42_default.{json,md}`.
- G3 [HIGH] RESOLVED (2026-06-18): cheap cases un-ignored and run by default — `segmentum_tests.rs:103` (`segmentum_example_parses_and_children_fit_grid`) / `:142` (`duplicate_child_slot_is_rejected`); remaining `compose_segmentum` tests stay `#[ignore]` by policy (composition-bound; run via `--ignored`).
- G4 [MED] `OutputFormat::Html`/`Bitmap` — never exercised via `export_sector` dispatch → extend `export_writes_all_expected_files`.
- G5 [MED] economy/hooks/personae/relations/invariants_proptest — `fixture_dir()`+`OnceLock` dup → move to `shared.rs`.
- G6 [MED] `viewer/.../lifecycle.rs:109` — document-write paths have zero tests → extract path→bytes core, round-trip test.
- G7 [MED] `invariants_proptest.rs:60` — `system_count` derived deterministically, never fuzzed → add strategy dimension.
- G8 [LOW] `search_and_diff.rs:202` — asserts only "doesn't crash" → assert concrete non-empty delta.
- G9 [LOW] `relations_tests.rs:81` — skips the documented 0..=100 clamp assertion → assert the range.
- G10 [LOW] `svg_export_tests.rs:7` — substring-only, no hash pin → add `UPDATE`-gated blake3 pin.
