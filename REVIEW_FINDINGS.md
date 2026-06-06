# sectorforge — Codebase Audit (REVIEW.md-driven)

*Consolidated from 23 per-dimension reviews + adversarial verification passes. Verdicts applied: refuted findings dropped, downgrades/upgrades reflected, surviving High/Blocker findings marked **(verified)**. Cross-cutting issues (idioms / errors / perf / determinism vs. per-area) are stated once in their most natural home and cross-referenced.*

Date: 2026-06-06 · Scope: whole workspace (~122k LoC) · Severity rubric: Blocker / High / Medium / Low / Nit

---

## 1. Executive summary

**Overall assessment.** sectorforge is a large, **mature, and genuinely healthy** codebase. Across 23 audited dimensions the median score is **8/10**, with the deterministic engine (`src/gen`, `src/analysis`, `src/export`) and the Rust-idiom / error-handling / concurrency disciplines scoring **9**. The architecture is textbook: a four-crate split (engine+CLI / builder / viewer / shared gui-core) with the dependency arrow pointing the correct way (the engine has zero `egui`/`eframe`/`rfd` deps), `#![forbid(unsafe_code)]` in all four crates, one sanctioned RNG entry point, and a command-bus + undo/redo design in the builder that is compiler-enforced for completeness. **There are no Blockers and no findings that threaten the integrity of the deterministic engine's output.** The High-severity findings are real but localized, and cluster into five themes rather than being scattered rot.

**Main risks (the five themes, in priority order):**
1. **No CI** (verified). A codebase whose entire value proposition is byte-stable deterministic output guards that property only with golden/proptest/fuzz suites that run when a human remembers. This is the single highest-leverage gap — it is what lets every *other* regression class land unnoticed.
2. **Malformed-input availability** (verified). Two reachable process-abort paths under `panic="abort"`: an unbounded sector dimension that allocates a multi-GB `Vec`, and a UTF-8-slice panic in `parse_hex_rgb` on a crafted faction colour. Plus no GUI panic hook and no `catch_unwind` around background workers, so any panic is a silent hard crash with no breadcrumb.
3. **Geometry & command-bus invariant leaks in the GUI** (verified). The "sectors must be square" invariant is breakable through two shipping UI paths (viewer "Irregular dimensions" checkbox; builder 8×10 new-project default), and the entire data-catalog editor family mutates document state off the command bus, so catalog edits are silently non-undoable.
4. **Viewer is under-tested and partially duplicated** (verified). ~10k LoC of viewer edit/save/export has 9 tests and none cover any write path; two parallel map-edit stacks hand-roll near-identical mutations with a divergent ID-reindex contract.
5. **Front-door documentation** (verified). The README is a 22-byte title line and GUIDE.md contains a confidently-wrong "examples are bundled into the binaries" claim — the first things a new developer reads are empty or false, despite excellent material one hop away.

**Best qualities.** Determinism discipline is the crown jewel and it largely holds: every RNG draw is blake3-stage-keyed through `src/model/rng.rs`, output accumulators are `BTreeMap`/`BTreeSet` (Fx maps appear only as order-independent lookups), sorts carry unique tiebreaks, and float emission is fixed-precision/locale-independent. The typed-ID newtype system (`define_id!`), the command-bus/undo ledger (LD1–LD4 + off-thread re-derivation), and the golden/proptest/fuzz test infrastructure are all best-in-class for an app this size.

**Highest-priority recommendation.** Stand up a **minimal CI pipeline** (one Ubuntu job: `clippy -D warnings`, `cargo test --workspace`, `cargo test --test it -- golden`, plus a build-only `cargo +nightly fuzz build`), pinned to the declared 1.87 MSRV. It is additive, touches no source, and converts every determinism/correctness safety-net in the repo from opt-in to enforced — which is the precondition that makes safely landing the other fixes possible.

**Where coverage was thin.** This was a static read-only audit; no builds or tests were executed, so the *magnitudes* in the performance findings (auto-save cost, region-clone tax) are reasoned from `path:line` inspection and named benches rather than measured. Per-panel builder/viewer **interaction** logic is largely outside unit-test reach (egui immediate-mode), so behavioral coverage there leans on map-snapshot goldens. Negative validation (rules firing on bad input) and the `Err` arms of headline library APIs are lightly exercised in-tree.

---

## 2. Repository map

```
40k-sector-generator/            (repo dir; crate/binary name is "sectorforge")
├── src/                         sectorforge (lib + bin) — the deterministic engine + CLI
│   ├── model/                   domain types: ids, rng, sector_model, taxonomy, errors, macros
│   ├── gen/                     generation pipeline: placement, regions, routes, factions, world_pool, sites
│   ├── analysis/                pure derivations: economy, relations, history, personae, hooks, missions,
│   │                            interestingness, search, analytics, influence_field, power_projection
│   ├── export/                  byte-stable writers: render(md), svg_export, bitmap, html_export,
│   │                            heatmap, subsectors, segmentum, render_core
│   ├── validate/                pre-gen validation + post-gen invariants + diff
│   ├── loading/                 config.rs, input.rs (load_project, path-traversal guard), presets, sector_save
│   ├── cli/                     clap Command enum + ~24 thin per-runner modules + exit_code
│   ├── worlds.rs / worlds_toml.rs   world taxonomy enums + worlds.toml schema
│   └── lib.rs                   the 941-line public facade + flat compat-alias block
├── builder/                     sectorforge-builder — full sector construction (writes; egui)
│   └── src/builder/             state/ (command bus, undo, derivations), panels/ (~30 panels), command.rs
├── viewer/                      sectorforge-viewer — limited in-place editing (egui)
│   └── src/                     app/, editor/ (map/world/data panels), factions_overview, segmentum_view
├── gui-core/                    sectorforge-gui-core — shared widgets; SOLE owner of raw paint primitives
│   └── src/                     sector_view/ (view, cache, render), palette, info_panel, jobs, map_theme
├── tests/it/                    single-binary integration suite (golden, proptest, cli parity, invariants)
├── fuzz/                        out-of-workspace cargo-fuzz targets (nightly)
├── benches/ + */benches/        criterion benches (generation, influence_field, seed_search, render, mutations)
├── presets/ + examples/         living-documentation data (scaffolded by `new`/`list-presets`)
└── docs/                        MAP.md + spec/.txt files (§-tagged) + some dated process docs
```

**Crate responsibilities.**
- **`sectorforge` (`src/`)** — owns the entire deterministic domain: generation, analysis, exports, validation, loading, and the CLI binary. No GUI dependencies. The CLI is a clean leaf (`mod cli;` in `main.rs`, never re-exported).
- **`sectorforge-builder`** — the authoritative editor; every document mutation flows through `BuilderState::run(BuilderCommand::…)` with full undo/redo and an off-thread derivation ledger.
- **`sectorforge-viewer`** — a limited editor (map/world/data edits, save/save-as, exports) with a `dirty`-flag model instead of a command bus (deliberate: no undo/redo).
- **`sectorforge-gui-core`** — shared egui widgets; the only crate permitted to touch raw `egui::Painter`/`Shape` (enforced by per-crate `clippy.toml` bans in builder + viewer).

---

## 3. Architecture assessment

**Score: 8/10.** The macro-architecture is strong and the layering honest. (Dimension: arch — no High/Blocker findings.)

**Layers (top to bottom):** GUI shells (builder/viewer) → shared widgets (gui-core) → engine facade (`lib.rs`) → engine internals (model/gen/analysis/export/validate/loading). **Dependency direction is correct and manifest-enforced:** the engine pulls in no GUI crate (all 10 GUI mentions in `src/` are doc comments noting cross-renderer parity, [src/gen/faction_style.rs:1](src/gen/faction_style.rs)); the GUIs depend on `sectorforge`; `cli` is a binary-only leaf nothing imports.

**The one real structural weakness is encapsulation, not layering.** [src/lib.rs](src/lib.rs) presents **two contradictory public surfaces**: a curated 61-`pub fn` facade (lines 209–941) that only the CLI uses, and a flat `pub use model::*; pub use gen::*;` alias block (lines 79–181) through which the GUIs reach straight into generation-pipeline internals (`generation::generate`, `generate_with_progress_and_cancel`, `regenerate_world_payload`, `world_pool::build_pool`). Two of the GUI entry points are not in the facade at all, so there is no enforced "engine API vs. engine internals" boundary and any refactor of `generation` internals is a silent breaking change to two downstream crates. **(Medium — Architecture #1.)**

Secondary: the `analysis` layer's module doc promises "side-effect-free… deterministic" yet several submodules co-locate `fs::read_to_string` loaders and `render_markdown` presentation alongside the pure `derive` ([src/analysis/economy/derive.rs:7,42](src/analysis/economy/derive.rs), [src/analysis/relations/derive.rs:8,628](src/analysis/relations/derive.rs)). The contract should be made honest (doc fix + optionally propagate the `economy/` `io.rs`/`render.rs` split). **(Medium — Architecture #2.)** A dangling `[`cli`]` intra-doc link in the crate root and the 941-line delegation-table facade are Low/maintainability notes.

---

## 4. File & module organization

**Score: 7/10.** For a 122k-LoC desktop app this is functional and mostly cohesive. The two scariest-looking files are exactly what the brief predicted and are **not** split targets: `command.rs` (2407 lines, 46% tests) is flat declarative command/undo plumbing with compiler-enforced 39/39 symmetric `apply`/`revert` dispatch, and `derivations.rs` (1295 lines) is a well-decomposed 35-method derivation impl. `map/mod.rs` ranks #3 by raw lines purely because it is **77% tests** (prod is ~380 lines) — flagging it for a split would be a measurement artifact.

The genuine problems are localized: one true monolith (`SectorView::show`), a long tail of egui `show_*` panel functions that interleave layout with command-bus mutation, and several legitimately-declarative-but-long data tables.

| File | Lines | Assessment | Recommendation |
|---|---|---|---|
| [gui-core/src/sector_view/view.rs](gui-core/src/sector_view/view.rs) (`SectorView::show`) | **807** (fn) | **Genuine monolith** — one fn does viewport cull + 6 paint passes + `'outer` label-collision search + pointer hit-test, ~12-level nesting, 36 inline closures. Highest-traffic live-render path. **High (verified).** | Extract phase blocks into `pub(super)` helpers on a `RenderCtx` (`paint_hex_fills`, `paint_routes`, `place_subsector_labels`, `dispatch_click`). Pure cut/paste; sequence behind `UPDATE_MAP_SNAPSHOTS` goldens. |
| [builder/src/builder/command.rs](builder/src/builder/command.rs) | 2407 (46% tests) | Cohesive declarative enum + dispatch; **not** a god-file. Twin 39-arm `apply`/`revert` are correct-but-fragile (compiler forces presence, not inverse-correctness). | Do **not** split. Close the pairing gap with one parametric `apply_then_revert_is_identity_for_every_variant` test over an `all_variants()` fixture. |
| [builder/src/builder/panels/history.rs](builder/src/builder/panels/history.rs) | 1794 | Cohesive router; `show_add_event_wizard` (~289) + `show_selected_event_inspector` (~253) mix layout + mutation. | Split the wizard's distinct stages into `wizard_step_*` helpers opportunistically. |
| [builder/src/builder/panels/map/mod.rs](builder/src/builder/panels/map/mod.rs) | 1643 (77% tests) | **Measurement artifact** — prod ~380 lines; only `show_map_inspector` (201) is oversized. | Leave file as-is. Apply the model/layout split to `show_map_inspector` only. |
| [builder/src/builder/panels/routes.rs](builder/src/builder/panels/routes.rs) | 1516 | ~45 well-named sub-functions; size reflects UI breadth, not responsibility-mixing. | No split. |
| [src/analysis/personae.rs](src/analysis/personae.rs) | 1553 | Dominated by parallel string-keyed faction-kind tables (`default_pool` 321 lines) + a divergent kind vocabulary (Analysis #2). | Normalize the kind vocabulary; optionally lift `default_pool` literals to a `static`/TOML. |
| [src/export/segmentum.rs](src/export/segmentum.rs) | 1206 | Five responsibilities (config / DTO / compose / stitch algorithm / md+json writers) in one file; the byte-stable surface is buried. | Split into `segmentum/{config,dto,compose,stitch,render,writers}.rs` as a pure move behind `segmentum_tests`. |
| [src/validate/validation.rs](src/validate/validation.rs) (`validate`) | 352 (fn) | Flat declarative rule sequence; low per-rule complexity, borderline scannability. | Optionally group banner sections into `validate_<area>` fns preserving push order. |
| [viewer/src/factions_overview.rs](viewer/src/factions_overview.rs) | 1032 | Genuinely mixes view + editor-state + file I/O + TOML serialization. | Peel the persistence/serde half into `factions_designer_io.rs`. |

The ~80 files over 500 lines largely reflect per-domain panels and declarative data, not rot.

---

## 5. Back-end / engine review

The engine is the healthiest part of the codebase. Scores: **model 8, gen 9, analysis 8, export 9, validate 8, cli 8, data 7.**

### Model (`src/model/`) — 8/10
Exemplary typed-ID newtypes (`define_id!`: `#[serde(transparent)]` + `Arc<str>` + ordered constructors), a controlled `mutation.rs` write API, and a small correct `rng.rs`. Weaknesses are *enforcement gaps*, not rot:
- **Two divergent "empty sector" constructors bake different generator metadata** ([src/model/sector_model/scaffold.rs:27,34](src/model/sector_model/scaffold.rs) hardcodes `"sectorforge"`/`"0.1.0"` vs [mod.rs:413-414](src/model/sector_model/mod.rs) which uses `crate::GENERATOR_NAME`/`GENERATOR_VERSION`). The scaffold path is reached from production ([builder/.../project_io.rs:259](builder/src/builder/project_io.rs), [viewer/.../dialogs.rs:162](viewer/src/editor/dialogs.rs)) and writes a stale version into persisted manifests after any version bump. **(Medium)** Fix: delegate `empty_sector` to `GeneratedSector::empty`.
- **Per-enum wire string has two independent sources of truth** (`enum_slug!` literals vs `#[serde(rename_all)]`) with no test asserting they agree; `RouteType` has a third copy in `key()`. **(Medium)** Fix: one cheap per-enum `to_string == as_slug` guard test (cross-referenced under §7 / §8). Lower: `HexCoord` signed-but-non-negative, `add_faction` silent no-op on duplicate, history-event mutations clone the whole vec per edit.

### Generation (`src/gen/`) — 9/10
**Airtight on its two load-bearing invariants.** Every one of ~52 RNG sites routes through `stage_rng` (blake3-keyed); zero `thread_rng`/`SystemTime` seeding; **zero** `Fx`/`HashMap` usage — accumulators are `BTreeMap`/`BTreeSet`, output collections end in a total-order sort with a unique tiebreaker. Only Low/Nit findings: `mint_seed()` entropy is weak (pid + same-stack-slot + coarse-clock nanos can collide under scripted loops — add an `AtomicU64` counter); a reserved-but-unused `rng` in the route stage is a latent determinism trap (`let _ = rng`); a `centres.contains` O(n²) fallback nit.

### Analysis (`src/analysis/`) — 8/10
Determinism handled with real care (explicit `.then_with(id)` tiebreaks via shared `cmp_f32_*`, `BTreeMap` outputs, stage-keyed per-anchor RNG). The one item worth scrutiny:
- **`run_search` parallel determinism is reasoned, not tested.** [src/analysis/search.rs:1150-1285](src/analysis/search.rs): `near_misses` *membership* depends on which candidates a `lowest_winner` atomic race let workers skip, so two runs may surface different near-miss sets into `search.json`/`search.md`. `winning`/`candidates_evaluated` are provably safe; near-miss set is not guarded by a byte-equality test. **(Medium)** Fix: add a parallel-vs-single-thread byte-equality test; if it flakes, make near-miss selection derive from a fixed evaluated set. (See also Concurrency, which credits the same `into_par_iter` site as exemplary for the *winner* path.)
- **Faction-kind string dispatch duplicated across 5 `match` tables with divergent key sets** — `"genestealer_cult"` (control.rs spelling) falls through personae's `"genestealer"|"gsc"` tables to generic placeholder names/titles. Latent data-quality bug. **(Medium)** Fix: a `canonical_kind` normalizer + a coverage test. Lower: `economy_resource_min` hand-match silently reports `0.0` for unknown resource keys (use the existing `sector_balance.get`).

### Export (`src/export/`) — 9/10
**Unusually disciplined byte-stability.** Every `Fx`/`HashMap` traced is `.get()`-only against a deterministically-ordered driver; float formatting is uniformly fixed-precision; `{:?}` only on stable enums; HTML/JSON explicitly engineer stability (`BTreeMap` palette). All findings Low/Nit: two float sorts lack a unique tiebreak (trade-lane [render.rs:533](src/export/render.rs), SVG label [svg_export/labels.rs:162](src/export/svg_export/labels.rs)) — deterministic today via stable-sort-over-sorted-input but FMA-tie-fragile, add the explicit tiebreak the rest of the file uses. **Coverage gap (carried to Testing #5):** the committed SVG/PNG goldens run with `heatmap: Off`, so the heatmap tint path has no byte-golden.

### Validate (`src/validate/`) — 8/10
Pre-gen `validate` and post-gen `check_sector` are pure and well-organized; `GEN_SECTOR_NOT_SQUARE` is correctly the single canonical pre-gen geometry gate; diff is provably deterministic (`BTreeMap` matching, fixed `RESOURCE_KEYS`, explicit tiebreaks, a `diff_is_deterministic` test). Findings: **`top_faction_deltas` is a wired, user-settable config knob that does nothing** — no `.take()`/truncate anywhere, so the builder "top 10" control is a silent no-op **(Medium)**; `min_faction_delta` overloaded as the economy-resource threshold (Low); split-brain validation-code centralization (26 of ~48 codes bypass the `ValidationCode` enum its own doc says to use, no guard test) (Low).

### CLI (`src/cli/`) — 8/10
Disciplined thin-runner pattern, sysexits exit-code mapping, deterministic stdout, all 24 subcommands smoke-tested.
- **`hooks` and `missions` silently drop project-authored config on the `--project` path.** **(High — verified.)** [src/cli/hooks.rs:16-20](src/cli/hooks.rs) and [src/cli/missions.rs:16-21](src/cli/missions.rs) build `*Config::default()` and discard `input.catalogs.{hooks,missions}` (which carry operator-authored `manual: Vec<…>` + tuned caps), while every sibling command (`personae`/`sites`/`economy`/…) and the `random` runner correctly preserve them. A GM who authored manual hooks gets library defaults with no warning. *Verifier confirmed against all six cited files; scope is precisely these two runners (`interestingness`/`briefing` correctly have no project catalog).* Fix: switch both to `resolve_sector_with_cfg(...|input| input.catalogs.{hooks,missions}.clone())`.
- Medium: `--heatmap`/`--profile` token parsers live in the CLI decoupled from their core enums (move to `parse_token` next to the enum). Low/Nit: `analyze`/`search` `--strict` bypass the centralized exit mapper with `Ok(ExitCode::from(1))`; `--seed`+`--constraints` precedence is silent; `--observer` unvalidated; the `cli_smoke` `SUBCOMMANDS` list is hand-maintained.

### Data layer & schema (`src/loading`, `worlds*.rs`) — 7/10 *(also §7)*
Above-average serde discipline (no `flatten`/`untagged`, `#[serde(alias)]` on renames, the `WorldDto`/`WorldDtoRaw` shim, path-traversal guard). Dominant gap is the **`deny_unknown_fields`** issue — see §7.

---

## 6. Front-end / GUI review

Scores: **builder-arch 9, builder-panels 8, viewer 7, gui-core 8.**

### Builder architecture & command bus — 9/10
**Exemplary.** Every mutation flows through `BuilderState::run`; each `BuilderCommand` carries its own `before` payload so `revert` is an exact inverse; three parallel exhaustive `match self` blocks (`dep_classes`/`apply`/`revert`, no `_` arm) force the compiler to handle every new variant, backstopped by `dep_classes_cover_all_variants` and a 256-byte size cap. The LD1–LD4 derivation ledger and LD3 off-thread re-derivation (shared pure `compute_*` fns, dispatch-time fingerprint re-checked on drain, deterministic `DerivationKind::ALL` drain order) are equally disciplined. Findings cluster on the seam between off-bus derived-state installs and the state-restoring paths:
- **Watcher-driven catalog reload does not invalidate the derivation ledger** ([project_io.rs:886-981](builder/src/builder/project_io.rs)) — a silently reloaded `relations.toml` keeps pre-reload derived overlays until an unrelated command happens to invalidate them. **(Medium)**
- **Undo does not restore off-bus derived state; economy can persist stale via auto-save** ([state/undo.rs:98-113](builder/src/builder/state/undo.rs)) — after undo on a non-Economy tab, `sector.economy` describes the pre-undo sector and `trigger_auto_save` serializes that inconsistent pair to `sector.json`. Self-healing on the UI; the persisted artifact in that window is not. **(Medium)** Lower: off-bus installs auto-save transiently-inconsistent state; a redundant `status()` match arm (Nit).

### Builder panels — 8/10
Disciplined: every structural sector/route/region/faction-power edit routes through the bus; the map context menu is textbook; the region brush previews off-bus then commits one undoable `EditRegion`. The one broad, *intentional-but-undocumented* gap is the data-catalog + economy-override editor family bypassing the bus — **this is the same issue as Determinism #3 and is stated there.** Lower here: catalog-edit boilerplate triplet duplicated across 7+ panels (extract a shared helper); `AddFaction`/`RemoveFaction` commands are dead in production while `factions.toml` edits go off-bus.

### Viewer — 7/10
The limited-edit architecture is deliberate and sound (no command bus by design; edits cascade route/presence cleanup correctly; the big files reuse engine types rather than duplicating them). Two real problems:
- **Two parallel map-edit stacks duplicate mutation logic with a divergent ID-reindex contract.** **(High — verified.)** App-side stack ([app/sector_view.rs:464-625](viewer/src/app/sector_view.rs), reindexes IDs) vs editor-panel stack ([editor/map_panel.rs:151-235](viewer/src/editor/map_panel.rs), deliberately does not) hand-roll the same add/remove system+route mutations against the same `editor.sector`. *Verifier confirmed a behavioral divergence, not just maintenance overhead: the editor-panel delete omits the faction `system_presence`/`world_presence` pruning the App path performs.* Fix: lift the *mutations* into `GeneratedSector` methods in `mutation.rs` (which already hosts `reindex_ids`); both stacks keep only their thin reindex-or-not finalizer.
- **`editor/enums.rs` forks the canonical `worlds.rs` enum tables.** **(Downgraded High → Medium.)** *Verifier: the duplication and the UX label inconsistency (inspector shows `"AgriWorld"`, data-editor shows `"Agri-World"`) are real, but a scripted diff shows every array currently matches its `VARIANTS` exactly — no active drift, no silent-no-op today, and the E4 `as_ref` attribution was a misattribution. Latent, not active.* Fix: make the inspector dropdowns consume `T::VARIANTS`+`display_name()` via the shared `enum_combo`, deleting the arrays. Other Medium items: viewer edits mutate the source `sector.json` with no validation gate on save; world-inspector save stores the rounded *display* star colour; `factions_overview.rs` mixes four responsibilities.

### gui-core — 8/10
The raw-paint boundary is genuinely enforced (per-crate `clippy.toml` bans + `#![forbid(unsafe_code)]`); the map-snapshot golden suite is well-built and self-blessing; the render-path caching is real and benched. Findings: **`RenderMapTheme::from_map_theme` silently drops most overlay colours and hardcodes `region_tint_strength = 0.5`** ([map_theme.rs:129](gui-core/src/map_theme.rs), [view.rs:193](gui-core/src/sector_view/view.rs)) — a light data theme (`print_mono`) renders dark label chips and over-strong tints live, disagreeing with the exported PNG. **(Medium)** The `SectorView::show` monolith is the same finding as §4 (gui-core rated it Medium; the filesize verifier corroborated 807 lines — carried as **High** in §4). Route-control category list triplicated/stringly-typed (Low); process-wide `RwLock` chrome/status globals (Low, documented trade-off); 24-pub-field `SectorView` (Low); viewer `cache: None` per-frame region scan (Low, perf — see §12).

---

## 7. Shared / API-contract review (sector.json + TOML schema)

**Score: 7/10.** The on-disk contract shows above-average serde discipline: zero `flatten`/`untagged` (both notorious for silent breakage), `#[serde(alias)]` on every renamed field/variant so old files still load, the `WorldDto`/`WorldDtoRaw` shim that stores real enums in-memory while keeping `sector.json` byte-stable, weight finiteness/sign validation at pool-build, a `read_relative` path-traversal guard, and **one shared type set across engine + both GUIs** (eliminating schema drift between them).

- **No `#[serde(deny_unknown_fields)]` anywhere — every typo'd config key is silently dropped.** **(High — verified.)** Workspace grep returns 0 hits; `AppConfig` and all sub-structs deserialize permissively ([src/loading/config.rs:5-127](src/loading/config.rs)), so `route_densty = 0.9` is ignored and the `#[serde(default)]` (`0.30`) silently applies. *Verifier confirmed the load path, the defaults, and that zero `flatten` uses mean the fix has no serde conflict.* For a tool whose entire contract is hand-authored TOML edited by humans **and** round-tripped by the GUIs, this is the highest-probability real-world failure. Fix: add `deny_unknown_fields` to the hand-authored *config* structs (not the machine-written `sector.json` model types, which use `skip_serializing_if` for forward-compat); sweep `presets/`+`examples/` first since it turns any tolerated junk key into a hard error.
- **Enum taxonomy hand-encoded in 3–4 parallel string vocabularies** (`worlds.rs` `FromStr`/`display_name`/`VARIANTS` in *display* form + `taxonomy.rs` `parse_*_variant` in *Rust-identifier* form + serde derive). `docs/ADDING_A_WORLD_TYPE.md` documents the multi-site sync requirement, so it is *managed* not silent — **(Medium)**. Fix: a `macro_rules!` table per enum (mirroring `enum_slug!`) so all string forms derive from one variant list; roll out one enum per PR behind goldens.
- **No `schema_version` on any config or `sector.json` envelope** — backward-compat rests entirely on `default`+`alias`, which cannot express a breaking shape change. **(Medium)** Fix: an additive `schema_version: Option<u32>` gate.
- Low: `enum_slug!` vs `rename_all` dual-source (same as Model #3 — one guard test fixes both); `worlds.toml` double-read in the load path; unknown `style_border`/colour spellings fall back silently; no single `CONFIG_SCHEMA.md` reference doc; feature-pool weights validated only at pool-build, not at load.

---

## 8. Rust idioms review

**Score: 9/10.** An exceptionally disciplined, idiomatic codebase. Domain IDs are `#[serde(transparent)]` newtypes via `define_id!`; score wrappers (`ControlScore`/`ProjectedPower`) prevent cross-domain `f32` mixups; ~40 closed enums get exhaustive-match `as_slug()` via `enum_slug!` (a missing slug is a compile error); `#![forbid(unsafe_code)]` in all four crates; exactly one `panic!` in `src/` (test-only); the ~32 non-test `.expect()`s are uniformly genuinely-infallible with invariant-explaining messages; granular visibility (146 `pub(super)`, 95 `#[non_exhaustive]`). High clone counts were verified legitimate (undo before/after snapshots; cross-thread `ctx.clone()`/`cfg.clone()` moves into LD3 workers).

Findings are localized type-design inconsistencies, none structural:
- **Closed-set `style_border` persisted as `Option<String>` despite an existing `FactionBorder` enum + parser** ([src/gen/factions.rs:59-62](src/gen/factions.rs); enum at [faction_style.rs:176](src/gen/faction_style.rs)) — the one place closed-ness should be enforced (the persisted DTO) is the one place it isn't, so typos survive serialization and re-parse on every render. **(Medium)** Fix: retype to `Option<FactionBorder>` with `#[serde(rename_all="lowercase")]` + an `Unknown`/lenient fallback for forward-compat; behind goldens. Low/Nit: `EntityWorld.id_lookup` raw-`String`-keyed inside a typed view; `RouteComponents.kind: String` shadows a typed enum; `define_id!` exposes the `Arc<str>` payload as `pub` (drop `pub` — grep confirms no external `.0` access). Correctly **not** flagged: `kind`/`disposition` as strings (catalog-extensible by design); `SystemTime::now()` (the sanctioned seed-mint).

---

## 9. Error handling review

**Score: 9/10.** Mature and disciplined, well above the REVIEW.md baseline. A proper `thiserror` enum (`SectorError`, [src/model/errors.rs:5](src/model/errors.rs)) with `#[source]`/`#[from]` chaining and path context on every IO/parse/export variant; the CLI maps every variant to a stable sysexits exit code; **zero** `unwrap`/`expect` on user-supplied args/files in CLI runners; both GUIs surface IO/load/save failures via `ModalKind::Message` / `export_status` rather than swallowing.

- **GUI file-load has no panic boundary; a malformed `sector.json` that trips an engine `expect` aborts the whole app.** This is the error-handling framing of the same root issue covered as **Observability #1 (no panic hook)** — serde rejects structurally-invalid JSON cleanly, but a *structurally-valid* sector with inconsistent IDs can reach e.g. [src/export/subsectors/mod.rs:652](src/export/subsectors/mod.rs) `expect("missing sys")` and, under `panic="abort"`, kill the editor with unsaved work in other tabs. Fix is shared with Observability (panic hook + optionally a `catch_unwind` at the GUI ingestion points). **(Medium)** Lower: a few `Result<_, String>` stringly boundaries flatten `toml::de::Error` span info (UX is fine — the string carries line/col and a test asserts it); one swallowed `create_dir_all` in the viewer ([lifecycle.rs:169](viewer/src/app/lifecycle.rs)) — though the downstream write error is still surfaced. The `writeln!`-into-`String` and best-effort `prefs.save()` discards are correct idioms, not defects.

---

## 10. Security review (local threat model)

**Score: 7/10.** For a local desktop/CLI app with no network surface, the posture is solid where it matters most: `#![forbid(unsafe_code)]` eliminates the entire memory-unsafety class; the `read_relative` path-traversal guard rejects absolute paths and `..` components ([src/loading/input.rs:248-257](src/loading/input.rs)); RNG is stage-keyed; the primary TOML deserializers are fuzzed. The real weaknesses are **availability**, not confidentiality — none crosses a privilege boundary, so severities are bounded by the local model.

- **Unbounded sector dimensions allocate an arbitrarily large `Vec` → process abort.** **(High — verified.)** `validate()` caps only `grid_cells==0`/square/`system_count` — **no upper dim bound** ([src/validate/validation.rs:58-108](src/validate/validation.rs)); `MAX_CUSTOM_DIM=80` is enforced only at the CLI/builder front-ends, never on disk-loaded configs. A hand-edited square `sectorforge.toml` with `dim=50000` passes validation and reaches `Vec::with_capacity(2.5×10⁹)` ([placement.rs:14,27](src/gen/generation/placement.rs)), OOM-aborting under `panic="abort"`. *Verifier confirmed the full chain.* A shared/imported project file — the realistic distribution vector — becomes a one-line crash bomb hitting both CLI and builder. Fix: one `GEN_SECTOR_TOO_LARGE` validation rule near the existing geometry block.
- **`parse_hex_rgb` panics on a 6-byte multibyte string (UTF-8 slice on non-char-boundary).** **(High — verified.)** [src/gen/faction_style.rs:159-168](src/gen/faction_style.rs) checks `t.len()!=6` (bytes) then slices `&t[0..2]` with no `is_ascii()` guard (unlike sibling `parse_color`). *Verifier confirmed `style_fill="a😀b"` panics mid-emoji during render via the builder Factions panel's `color_override`; the `.unwrap_or(derived)` catches only `None`, not the inner slice panic; CLI export is unaffected (it ignores `style_fill`).* Fix: one-line `if !t.is_ascii() { return None; }`.
- **`i32` multiply overflow in placement cell-count** ([placement.rs:12-14](src/gen/generation/placement.rs)) — `(width*height) as i32` wraps before the `usize` cast (release: silent corruption; debug: panic). Mostly subsumed once dims are capped. **(Medium)** Fix: compute `total_cells` in `usize`.
- **Fuzz targets miss the JSON sector-load path and ~10 TOML catalog deserializers** — the richest deserialization surface (`GeneratedSector` JSON) has zero fuzz coverage, exactly the post-parse semantic-panic class that findings #1/#2 inhabit. **(Medium)** Fix: add 2–3 targets (`sector_json_parse`, `factions_toml_parse`). Low/Nit: atomic-write predictable PID temp name (no privilege boundary crossed in the single-user model); the `grid_cells` product is computed with inconsistent integer widths in three places.

---

## 11. Testing review

**Score: 8/10.** A genuinely strong, determinism-aware suite (~784 tests): blake3-pinned `sector.json`/`md`/PNG/SVG goldens, a deterministic CPU rasterizer for gui-core map snapshots, same-seed/different-seed checks at both JSON and render layers, 6 cross-seed proptest blocks, full-JSON CLI↔library parity, and 4 fuzz targets matching the untrusted-TOML threat model. The thinness is concentrated and identifiable.

- **Viewer edit, save, and export write-paths are effectively untested.** **(High — verified.)** The viewer has exactly 9 tests and none exercise `save_project_sector` (whose path-traversal guard rejecting `/`,`\`,`.`,`..` at [file_ops.rs:83-87](viewer/src/editor/file_ops.rs) is real and unverified), `DataEditor::save`, or any export writer. *Verifier confirmed the count and the untested guard.* Fix: a focused unit module hitting the pure-data seams (happy path + 4 forbidden-name `Err` cases + data-editor round-trip) — no egui harness needed.
- **Conflict-state commands have zero coverage; undo/redo *stack* integration is thinly tested.** **(Downgraded High → Medium.)** *Verifier: the central evidence was wrong — the four conflict commands ARE constructed in `dep_classes_cover_all_variants`, and `assert_round_trip` drives the full `run()→undo()→redo()` stack across ~10 commands. Residual gap is narrow: no dedicated apply/round-trip test for the 4 conflict commands, `advance_conflict_ticks` is never called by a test, and redo-truncation-after-new-command is untested.*
- Medium: negative validation is shallow (~4 of ~67 rule codes asserted to fire; the flagship `GEN_SECTOR_NOT_SQUARE` has no negative test); the `MAX_CUSTOM_DIM` resource guard is front-end-only and untested at the library boundary (ties to Security #1); export-writer goldens are uneven (heatmap/system_map/HTML/segmentum lack pinned byte-goldens — ties to Export); no test exercises the `Err` path of `load_project`/`generate_sector`/`export`. Low/Nit: per-panel coverage skews to pure helpers (inherent to egui — keep extracting decision logic into testable fns); zero `#[should_panic]`/in-tree panic-safety tests (delegate the no-panic property to deterministic `from_str(...).is_err()` cases seeded from the fuzz corpus).

---

## 12. Performance review

**Score: 9/10.** A maturely-optimized codebase whose hot paths have demonstrably been profiled, not guessed. The two scarcest paths are well-engineered: seed-search uses the workspace's single correct rayon parallelization with a lowest-winner short-circuit and an `Arc`-shared catalog (heavy data never re-cloned per candidate); the GUI render path is backed by a digest-gated `SectorMapCache` hoisting hex/label/faction-style lookups out of the per-frame loop, with a realistic PERF2 criterion bench up to 1000 systems + 2000 routes. `Arc<str>` IDs neutralize the ~2000 `.clone()` calls. Fx-vs-BTree discipline is correctly applied in the writer (verified lookup-only). No determinism invariant is violated in any path inspected.

- **Auto-save serializes the entire document and writes to disk synchronously on every command apply** ([state/undo.rs:50,75-95](builder/src/builder/state/undo.rs)) — when `auto_save_path` is `Some`, every `BuilderCommand` (e.g. each tick of a value drag) pretty-serializes the full `GeneratedSector` and does a blocking `fs::write` on the UI thread, with no debounce. This is the genuine per-keystroke cost — strictly larger than the ~0.5 ms `check_sector` that PERF1 already moved off this path. Bites only the auto-save-on subset (default `None`), hence **Medium.** Fix: debounce like validation already is, and/or hand the bytes to the background thread; guarantee a flush on close.
- **Every region edit deep-clones the entire `Vec<WarpRegion>`** ([command.rs:732,747,782](builder/src/builder/command.rs)) under the copy-on-write `Arc<Vec>` design; `EditRegion` is the paint-stroke path firing many commands per drag. **(Medium)** Fix: coalesce stroke commands to one per drag-release (preferred; no model change), and/or make `WarpRegion::hexes` an `Arc<Vec>`.
- Low: full `BuilderIndex` rebuild on every apply/undo/redo (gate on `dep_classes` — structural commands rebuild, field edits skip); no bench covers the analysis layer's biggest derivations at scale (personae/search/scoring are multiplied inside the seed-search loop); `analyze_with` makes ~6–8 independent passes (micro, only fuse if a bench flags it); the gui-core `cache: None` per-frame fallback (have the viewer build a cache once — ties to gui-core #7). Influence-field dense accumulator is a documented, benched, sound decision (Nit/informational).

---

## 13. Observability review

**Score: 8/10.** Strong for a local CLI+GUI tool. The CLI exposes stable sysexits exit codes; error types carry path/context; the standout decision is that **the engine emits no diagnostics directly** — all progress is injected via `on_progress: &mut dyn FnMut(…)` callbacks, so the library never owns stderr and stays embeddable. GUIs surface failures through per-subsystem `error` fields and modals. Absence of tracing-spans/metrics/health endpoints is correct here — not a finding. The two real gaps are catastrophic-failure diagnosability:

- **No panic hook: under `panic="abort"`, every GUI panic is a silent hard crash.** **(High — verified.)** Neither GUI `main` ([builder/src/main.rs:63](builder/src/main.rs), [viewer/src/main.rs:33](viewer/src/main.rs)) installs `std::panic::set_hook`; *verifier confirmed grep across all three GUI crates returns nothing.* A release panic aborts with only the default libstd stderr line — no dialog, no crash file, no breadcrumb — for a tool whose threat model explicitly includes panic-on-malformed-input. Fix: a shared `gui_core::diagnostics::install_panic_hook(app)` (~15 lines, no new dep) writing a timestamped crash note to the OS temp dir, chaining the default hook.
- **Background-job workers run with no `catch_unwind` — a worker panic aborts the whole app.** **(High — verified.)** [gui-core/src/jobs.rs:61-65](gui-core/src/jobs.rs) invokes the worker closure bare, so a panic skips `tx.send` (receiver only ever sees `Disconnected`) and under `panic="abort"` aborts the process rather than becoming a surfaced `JobResult::Failed`. *This is the same root cause Concurrency #1 raised at Medium ("worker panics abort"); Observability owns it at High because the async paths run the heaviest, most input-dependent work and lose unsaved builder state.* Fix: `catch_unwind(AssertUnwindSafe(…))` in `spawn_job`, posting the panic message into the existing `status` channel (note the ordering dependency on relaxing `panic="abort"` or the hook above capturing it).
- Medium: no logging facade — diagnostics are `eprintln!`/`println!` with no runtime verbosity control (add `log`+`env_logger`, route the *existing* `log_progress` through `log::info!`, keep result emission on stdout). Low: the lone library `eprintln!` ([html_export.rs:68](src/export/html_export.rs)) bypasses the otherwise-clean callback boundary (emit as an `ExportProgress::HtmlSizeWarning` variant); GUI launch-failure diagnostics are thin (launch into a blank workspace with the open error surfaced in-window rather than exiting).

---

## 14. Dependencies / build / CI review

**Score: 7/10.** Dependency and build config are, on their own merits, near-exemplary: every shared dep pinned once in root `[workspace.dependencies]` with `name.workspace = true`; disciplined default-features (`image` png-only, `eframe` drops `wgpu`, `criterion` drops html/plotting); `cargo tree -d --edges normal` reports **zero duplicate versions in the production graph** (the only dups are in the dev-only proptest/tempfile subtree); every declared dep is used; four thoughtfully-documented profiles (`quick` for ~1s relinks, `profiling`, `release` with `panic="abort"`+fat-LTO, `bench`). The score is held to 7 by one structural gap that dominates:

- **No CI pipeline exists for a 122k-LoC actively-developed codebase.** **(High — verified.)** *Verifier confirmed `.github/` is absent and was never in git history; no Makefile/justfile/pre-commit/hooks substitute exists.* Nothing automatically runs fmt/clippy/test/golden on push — every determinism guarantee depends on a human running the right command. A reordered `FxMap` iteration, an unstable sort, or a float-format drift can land on `main` and stay green for an author who skipped `cargo test --test it -- golden`. **This is the highest-leverage finding in the entire audit.** Fix: one minimal Ubuntu `ci.yml` — `clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test --test it -- golden`, plus a build-only `cargo +nightly fuzz build`; pin to 1.87 to make the MSRV claim real. (Skip `fmt --check` — the repo is intentionally not rustfmt-clean.)
- Low: `panic` setting asymmetric across release-derived profiles (`[profile.bench]` is standalone with no `panic` key, so benches measure `unwind` codegen vs the shipped `abort` — make it `inherits = "release"`); `version`/`edition`/`rust-version` repeated verbatim 4× (add `[workspace.package]`); the MSRV `1.87` is declared but unverified (fold into CI). Cleared: `src/analysis/history/build.rs` is a normal module, not a Cargo build script.

---

## 15. Documentation / DX review

**Score: 7/10.** Against the grain of the "simple app" assumption, this is an unusually well-documented large project: GUIDE.md is a thorough 4,157-line user guide (per-OS prerequisites, every CLI subcommand with copy-pasteable examples); [src/lib.rs](src/lib.rs) carries a first-rate crate-level architecture doc with a runnable quick-start; error-doc discipline is strong (69 `# Errors` sections); the cargo run-aliases each document their *why*. The score is held to 7 by front-door defects:

- **README.md is effectively empty — the canonical entry point documents nothing.** **(High — verified.)** *Verifier confirmed the file is 22 bytes, a single `# 40k-sector-generator` title line.* All onboarding content lives in GUIDE.md/BUILDER.md, which an arriving developer has no signposted reason to open. Fix: a ~30–40 line README lifting GUIDE §0–§1 (prerequisites + quick start), a Documentation links section, and the workspace-member table.
- **GUIDE.md claims example projects are "bundled into the binaries" — they are not.** **(High — verified.)** *Verifier confirmed zero `include_dir!`/`include_bytes!`/`RustEmbed` embedding of examples; they exist only as on-disk dirs loaded via CWD-relative paths.* A developer who trusts the claim and runs a relocated binary with `--project examples/m42_project` gets file-not-found. Confidently-wrong docs cost more trust than missing ones. Fix: delete the bundling sentence ([GUIDE.md:53-54](GUIDE.md)), replace with "run from the repository root."
- Medium: project/crate naming split ("40k-sector-generator" repo/README vs "sectorforge" everywhere else); `docs/` has no index and mixes durable specs with dated point-in-time process docs. Low/Nit: MAP.md coverage drifting to ~88% of `.rs` files; no `# Panics` rustdoc despite `panic="abort"`; two thin facade wrappers (`load_project`/`validate_project`) lack `# Errors`; stale `*_old.toml` files shipped in `presets/_base`.

---

## 16. Determinism & core-invariant compliance (crown-jewel section)

**Score: 6/10.** This is the section the project cares about most, and it splits cleanly: **the deterministic engine (`src/`) alone would score 9** — every RNG draw routes through `src/model/rng.rs` (the two `SystemTime` reads are isolated, blake3-folded seed *mints*, not in-stage RNG); every output-bearing accumulator uses `BTreeMap`/`BTreeSet` while Fx maps appear only as coordinate/key lookups iterated by a deterministic outer loop; sorts carry explicit tiebreaks; SVG/text float emission is fixed-precision and locale-independent. No `thread_rng`/`from_entropy`/`rand::random` anywhere; no Fx-iteration-for-output; no byte-stability violation in any writer inspected.

**The score is held to 6 because the violations live entirely in the GUI layer, and all three are confirmed (verified) shipping-code breaches of "do not violate" invariants:**

- **Viewer "Irregular dimensions" checkbox lets width and height diverge — direct geometry-invariant violation.** **(High — verified; reviewer floated Blocker, verifier held High.)** [viewer/src/editor/dialogs.rs:114-136](viewer/src/editor/dialogs.rs): the checkbox disables the `height=width` mirror and divergent dims flow unchecked into `empty_sector(…,width,height)` → `set_sector`, neither of which enforces squareness. *Verifier confirmed this is a shipping UI path with only a cosmetic warning label — not test code (carve-out c) or transient UI (carve-out b).* Writes a non-square `sector.json`/`sectorforge.toml` that can break segmentum composition and tiling. Fix: remove the escape hatch; make the mirror unconditional (matching the builder's locked behaviour).
- **Builder "New project" modal seeds non-square 8×10 defaults; lock only fires on edit.** **(High — verified.)** [builder/.../panels/project.rs:87-88](builder/src/builder/panels/project.rs): the modal is pre-seeded `width:8, height:10`, the mirror runs only inside `.changed()`, and an untouched Create passes the pair through `new_project`→`scaffold_blank`→`default_app_config(width,height)` (no `.max()`/square check). *Verifier confirmed the full chain and that sibling seeds correctly use 8×8 — only this entry point is poisoned.* A latent bug that defeats the lock specifically added to enforce squareness. Fix: change the default to `height:8`; add a `.max()` normalization in `new_project`/`scaffold_blank` for defense-in-depth.
- **Data-catalog editor family mutates `state.data_catalogs.*` directly, bypassing the command bus (not undoable, §R4).** **(High — verified.)** *Verifier confirmed the live handlers in economy.rs/regions.rs/hooks.rs/personae.rs (and ~6 more) write `data_catalogs.* = Some(...)` + set `state.dirty` by hand with no `state.run(BuilderCommand::...)`; no catalog-edit variant exists in `command.rs`; `data_catalogs` is enumerated as document state in the invariant, so carve-out b does not apply; the secondary `#[cfg(test)]` citations are correctly exempt but the finding stands on the live sites.* The headline feature (Ctrl-Z) silently does nothing for any data-table edit, and could undo *past* a catalog edit to resurrect a sector that no longer matches the live catalog. *(This is the same issue surfaced by Builder-panels #1 and noted by Builder-arch — stated once here.)* Fix, two honest options: (a) **code** — add one `BuilderCommand::EditCatalog { which, before, after }` snapshotting the `Option<Config>` and route every write through `state.run`; or (b) **invariant** — if catalog editing is intentionally non-undoable (it edits generation *inputs*, not the document), amend the CLAUDE.md carve-out to exempt `data_catalogs` and document why. Do **not** attempt a macro rewrite.

Low/Nit (defensive): `mint_seed`/viewer `random_seed_str` are sanctioned seed-mints — add a `// SEED MINT ONLY` guard comment so the `SystemTime` grep signature isn't mistaken for a violation; `power_projection::system_top_reach`'s `>` tiebreak is deterministic only because `by_faction` is a `BTreeMap` (make the `fid` tiebreak explicit to future-proof against an `FxMap` switch). The reviewer pre-ruled-out false positives (BTreeMap iterations, coordinate-lookup HashMaps, test-module writes, tiebroken `sort_unstable`) — confirmed not violations.

---

## 17. Prioritized findings table

| # | Priority | Severity | Category | Location | Finding | Recommended action |
|---|---|---|---|---|---|---|
| 1 | P0 | High (verified) | CI / safety-net | `.github/` (absent) | No CI; determinism goldens/proptests/fuzz run only on opt-in local runs | Add minimal `ci.yml`: clippy -D warnings, `test --workspace`, `test --test it -- golden`, `+nightly fuzz build`; pin 1.87 |
| 2 | P0 | High (verified) | Security / availability | [src/validate/validation.rs:58](src/validate/validation.rs), [placement.rs:14,27](src/gen/generation/placement.rs) | Unbounded sector dims → multi-GB `Vec` → OOM abort under `panic=abort` (disk-loaded configs uncapped) | Add `GEN_SECTOR_TOO_LARGE` validation rule (reuse `MAX_CUSTOM_DIM`); optional early-return guard in `place_systems` |
| 3 | P0 | High (verified) | Security / availability | [src/gen/faction_style.rs:162](src/gen/faction_style.rs) | `parse_hex_rgb` UTF-8 slice panic on 6-byte multibyte `style_fill` → builder abort | One-line `if !t.is_ascii() { return None; }` + regression test |
| 4 | P0 | High (verified) | Observability / robustness | [builder/src/main.rs:63](builder/src/main.rs), [viewer/src/main.rs:33](viewer/src/main.rs) | No GUI panic hook; release panic = silent hard crash, no breadcrumb | Shared `gui_core::diagnostics::install_panic_hook(app)` writing a crash note, chaining default |
| 5 | P0 | High (verified) | Observability / robustness | [gui-core/src/jobs.rs:61](gui-core/src/jobs.rs) | Background workers have no `catch_unwind`; a worker panic aborts the whole app (Disconnect recovery is dead under `panic=abort`) | `catch_unwind(AssertUnwindSafe(…))` in `spawn_job`; post panic into `status` channel |
| 6 | P1 | High (verified) | Determinism / geometry | [viewer/src/editor/dialogs.rs:114](viewer/src/editor/dialogs.rs) | "Irregular dimensions" checkbox lets width≠height (shipping UI) | Remove escape hatch; make `height=width` mirror unconditional |
| 7 | P1 | High (verified) | Determinism / geometry | [builder/.../panels/project.rs:88](builder/src/builder/panels/project.rs) | New-project modal seeds non-square 8×10; lock only fires on edit | Default `height:8`; add `.max()` square-normalization in `scaffold_blank` |
| 8 | P1 | High (verified) | Determinism / command-bus | economy/regions/hooks/personae panels + [command.rs](builder/src/builder/command.rs) | Data-catalog editor family bypasses the bus → catalog edits not undoable (§R4) | Add `BuilderCommand::EditCatalog`, OR amend the CLAUDE.md carve-out to exempt `data_catalogs` |
| 9 | P1 | High (verified) | Correctness / CLI | [src/cli/hooks.rs:16](src/cli/hooks.rs), [missions.rs:16](src/cli/missions.rs) | `hooks`/`missions` silently drop project-authored config (`manual` entries, caps) on `--project` | Switch both to `resolve_sector_with_cfg(...|input| input.catalogs.{hooks,missions}.clone())` |
| 10 | P1 | High (verified) | Correctness / API contract | [src/loading/config.rs:5](src/loading/config.rs) | No `deny_unknown_fields` anywhere → typo'd config keys silently dropped, default applied | Add to hand-authored config structs (not `sector.json` model); sweep presets first |
| 11 | P1 | High (verified) | Duplication / correctness | [viewer/app/sector_view.rs:464](viewer/src/app/sector_view.rs), [editor/map_panel.rs:151](viewer/src/editor/map_panel.rs) | Two parallel map-edit stacks; editor path omits faction-presence pruning the App path does | Lift mutations into `GeneratedSector` methods in `mutation.rs`; keep thin per-stack finalizers |
| 12 | P1 | High (verified) | Test coverage | viewer (9 tests) — [file_ops.rs:74](viewer/src/editor/file_ops.rs), [data_editor.rs:74](viewer/src/data_editor.rs) | Viewer edit/save/export write-paths untested (incl. path-traversal guard) | Unit module: save happy path + 4 forbidden-name `Err` + data-editor round-trip |
| 13 | P1 | High (verified) | Complexity / maintainability | [gui-core/src/sector_view/view.rs:123](gui-core/src/sector_view/view.rs) | `SectorView::show` 807-line monolith (cull/paint/label/hit-test) on the hottest render path | Extract phase blocks into `pub(super)` helpers behind map-snapshot goldens |
| 14 | P1 | High (verified) | Documentation / DX | [README.md:1](README.md) | README is a 22-byte title line — zero front-door onboarding | Write ~30–40 line README from GUIDE §0–§1 + doc links + crate table |
| 15 | P1 | High (verified) | Documentation accuracy | [GUIDE.md:53](GUIDE.md) | False claim examples are "bundled into the binaries" | Delete the sentence; document "run from repo root" |
| 16 | P2 | Medium | Architecture / encapsulation | [src/lib.rs:90](src/lib.rs) | GUIs bypass the facade and bind to `generation`/`world_pool` internals | Promote the real GUI entry points into the facade; migrate call sites |
| 17 | P2 | Medium | Determinism / test gap | [src/analysis/search.rs:1150](src/analysis/search.rs) | `run_search` near-miss *set* may be timing-dependent (untested) | Parallel-vs-single-thread byte-equality test; make near-miss selection guard-independent if it flakes |
| 18 | P2 | Medium | Correctness / single-source | [scaffold.rs:27](src/model/sector_model/scaffold.rs) vs [mod.rs:413](src/model/sector_model/mod.rs) | Two "empty sector" constructors bake different (one stale) generator metadata | Delegate `empty_sector` to `GeneratedSector::empty` |
| 19 | P2 | Medium | Correctness / dead config | [src/validate/diff.rs:707](src/validate/diff.rs), [builder/panels/diff.rs:257](builder/src/builder/panels/diff.rs) | `top_faction_deltas` is a wired, user-settable knob that does nothing | Apply `.take()`/`truncate` to the Markdown digest; add a row-count test |
| 20 | P2 | Medium | Cohesion / data quality | [src/analysis/personae.rs:557](src/analysis/personae.rs), [control.rs:210](src/analysis/control.rs) | Faction-kind dispatch duplicated across 5 tables with divergent vocabularies → placeholder personae for canonical kinds | `canonical_kind` normalizer + cross-table coverage test |
| 21 | P2 | Medium | Cache-coherence | [builder/.../project_io.rs:886](builder/src/builder/project_io.rs) | Watcher-driven catalog reload doesn't invalidate the derivation ledger | Invalidate the matching `DepClass` after each successful `reload_catalog` |
| 22 | P2 | Medium | Undo fidelity / persistence | [builder/state/undo.rs:98](builder/src/builder/state/undo.rs) | Undo doesn't restore off-bus derived state; economy auto-saves stale on non-Economy tab | `ensure_fresh(Economy)` after undo/redo, OR gate auto-save until the stale set drains |
| 23 | P2 | Medium | Correctness / fidelity | [gui-core/src/map_theme.rs:129](gui-core/src/map_theme.rs), [view.rs:193](gui-core/src/sector_view/view.rs) | `RenderMapTheme::from_map_theme` drops overlay colours + hardcodes `region_tint_strength=0.5` → live≠export on light themes | Thread `region_tint_strength`; map the carried colours; tighten the doc |
| 24 | P2 | Medium | Type design | [src/gen/factions.rs:59](src/gen/factions.rs) | Closed-set `style_border` stored as `Option<String>` despite an existing enum+parser | Retype to `Option<FactionBorder>` with lowercase serde + `Unknown` fallback, behind goldens |
| 25 | P2 | Medium | Robustness / error containment | [src/export/subsectors/mod.rs:652](src/export/subsectors/mod.rs) | Structurally-valid-but-inconsistent `sector.json` can trip an engine `expect` → GUI abort | `catch_unwind` at GUI ingestion (shares fix with #4), or demote post-load `expect`s to `Result` |
| 26 | P2 | Medium | Security / fuzz coverage | [fuzz/fuzz_targets/](fuzz/fuzz_targets) | No fuzz target for `GeneratedSector` JSON or ~10 TOML catalogs | Add `sector_json_parse` + `factions_toml_parse` targets |
| 27 | P2 | Medium | Test coverage | [src/validate/validation.rs](src/validate/validation.rs) | Negative validation shallow (~4/67 codes); `GEN_SECTOR_NOT_SQUARE` has no negative test | Table-driven `rule_fires(code, mutate_input)` over the GEN_ rules |
| 28 | P2 | Medium | Test coverage | [src/export/{heatmap,system_map,html_export,segmentum}.rs](src/export) | 4 export writers lack pinned byte-goldens (determinism class) | Add blake3 hash goldens following the SVG-test template |
| 29 | P2 | Medium | Observability | `src/cli/common.rs` (`log_progress`) | No logging facade / verbosity control (`eprintln!`/`println!` only) | Add `log`+`env_logger`; route existing progress through `log::info!`, keep result emission on stdout |
| 30 | P2 | Medium | Performance / responsiveness | [builder/state/undo.rs:50,75](builder/src/builder/state/undo.rs) | Auto-save full-document serialize+blocking write on every command apply | Debounce; optionally move `fs::write` off the UI thread; flush on close |
| 31 | P2 | Medium | Performance / allocation | [builder/command.rs:732,747,782](builder/src/builder/command.rs) | Every region edit deep-clones the whole `Vec<WarpRegion>`; paint-drag fires many | Coalesce stroke commands to one per drag-release; optionally `Arc<Vec>` the hex lists |
| 32 | P2 | Medium (↓ from High) | Test coverage | [builder/command.rs:1485](builder/src/builder/command.rs) | 4 conflict commands lack dedicated round-trip; redo-truncation untested | Add 4 round-trip tests + a redo-truncation-through-`run` test |
| 33 | P2 | Medium (↓ from High) | Duplication / correctness | [viewer/src/editor/enums.rs:22](viewer/src/editor/enums.rs) | `enums.rs` forks canonical `worlds.rs` tables → label inconsistency (latent drift, not active) | Make inspector dropdowns consume `VARIANTS`+`display_name` via `enum_combo`; delete arrays |
| 34 | P2 | Medium | API-contract evolution | [src/loading/config.rs:40](src/loading/config.rs) | No `schema_version` on any config / `sector.json` envelope | Additive `schema_version: Option<u32>` load-gate |
| 35 | P3 | Low | Maintainability | [builder/command.rs:487,938](builder/src/builder/command.rs) | Twin 39-arm apply/revert: compiler forces presence, not inverse-correctness | One parametric `apply_then_revert_is_identity` test over `all_variants()` |
| 36 | P3 | Low | Determinism-hygiene | [src/gen/random_sector.rs:285](src/gen/random_sector.rs) | `mint_seed` entropy weak (pid+stack-slot+coarse nanos can collide) | Fold an `AtomicU64` counter into the hash |
| 37 | P3 | Low | Robustness | [src/gen/generation/routes.rs:160](src/gen/generation/routes.rs) | Reserved-but-unused route-stage `rng` (`let _ = rng`) is a latent determinism trap | Remove the unused plumbing, or pin the invariant in a comment |
| 38 | P3 | Low | Correctness / config | [src/validate/diff.rs:377](src/validate/diff.rs) | `min_faction_delta` overloaded as the economy-resource threshold | Add a dedicated `min_economy_delta` with the same default |
| 39 | P3 | Low | Maintainability | [src/validate/validation.rs:658](src/validate/validation.rs) | Split-brain validation-code centralization (26 codes bypass the enum; no guard test) | Finish the migration or soften the doc + add a casing/uniqueness test |
| 40 | P3 | Low | Layering | [src/analysis/economy/derive.rs:7](src/analysis/economy/derive.rs) | `analysis` modules co-locate IO+render with "pure" derive, contradicting the mod doc | Fix the doc; optionally propagate the `economy/` io.rs/render.rs split |
| 41 | P3 | Low | Build config | [Cargo.toml:125](Cargo.toml) | `[profile.bench]` measures `unwind` codegen vs shipped `abort` | `[profile.bench] inherits = "release"` |
| 42 | P3 | Low | Maintainability | members' `[package]` | `version`/`edition`/`rust-version` duplicated 4× | Add `[workspace.package]`; members use `.workspace = true` |
| 43 | P3 | Low | Maintainability | [gui-core/src/info_panel/mod.rs:104](gui-core/src/info_panel/mod.rs) | Route-control category list triplicated + stringly-typed | `legend_control_row` takes `RouteControlKind`; add `::ALL` |
| 44 | P3 | Low | Type design / single-source | per-enum (`enum_slug!` vs `rename_all`) | Slug vs serde wire string have two unguarded sources | One per-enum `to_string == as_slug` test (fixes Model #3 + Data #4) |
| 45 | P3 | Low | Performance | [gui-core/src/sector_view/view.rs:154](gui-core/src/sector_view/view.rs) | Viewer `cache: None` rebuilds a per-frame region `HashMap` (O(regions·hexes)) | Build a `SectorMapCache` once on viewer sector load |
| 46 | P3 | Low | Performance | [builder/state/undo.rs:40](builder/src/builder/state/undo.rs) | Full `BuilderIndex` rebuild on every apply/undo/redo | Gate rebuild on `dep_classes` (structural vs field-only) |
| 47 | P3 | Low | Documentation | [docs/MAP.md](docs/MAP.md) | File coverage drifting to ~88% | One reconciliation pass; optional coverage assertion test |
| 48 | P3 | Various Low/Nit | — | (see dimension sections) | export float-sort tiebreaks; `add_faction` no-op; history-event clone; `id_lookup`/`RouteComponents.kind` strings; `define_id!` `pub` payload; swallowed `create_dir_all`; `*_old.toml`; naming split; docs/ index; `# Panics` rustdoc; CLI `--strict`/`--seed`/`--observer`; `cli_smoke` list | Address opportunistically when touching the relevant file |

---

## 18. Refactoring roadmap (staged)

**Stage 0 — Safety net (do first; unblocks everything else).**
- Stand up minimal CI (#1): clippy -D warnings, `test --workspace`, `test --test it -- golden`, `+nightly fuzz build`, pinned to 1.87.
- Add the missing byte-goldens (#28) and negative-validation tests (#27) so the net covers what CI will run.
- Add viewer write-path tests (#12) and the conflict-command/redo-truncation tests (#32).
- These are all additive, touch no production logic, and convert the determinism invariants from documented to enforced.

**Stage 1 — Close the invariant leaks (small, compiler-checked, high-value).**
- Geometry: remove the viewer "Irregular dimensions" hatch (#6); fix the builder 8×10 default + add `.max()` normalization (#7).
- Command bus: decide and apply the data-catalog fix (#8) — either the single `EditCatalog` variant or the invariant-text amendment.
- Availability hardening: dimension upper-bound rule (#2), `parse_hex_rgb` `is_ascii` guard (#3), `i32`→`usize` in placement (#10's sibling Security #3).

**Stage 2 — Robustness & correctness fixes.**
- Observability: panic hook (#4) + worker `catch_unwind` (#5) + GUI ingestion `catch_unwind` (#25); these compound and should land together.
- CLI project-config fix (#9); `deny_unknown_fields` sweep (#10); `top_faction_deltas` (#19); `empty_sector` unification (#18).
- Cache-coherence: watcher-reload invalidation (#21) and undo-restores-derived (#22).

**Stage 3 — Boundary & duplication cleanup (sequence per the `pub`-item rule, upstream-first).**
- Viewer dual map-edit stacks → shared `mutation.rs` methods (#11) — touches `src/` consumed by the viewer, so apply upstream, `cargo check --workspace`, then migrate.
- Facade encapsulation: promote GUI entry points (#16); narrow the flat alias block.
- Type design: `style_border` enum (#24); faction-kind vocabulary normalization (#20); the per-enum slug-vs-serde guard test (#44).

**Stage 4 — God-file decomposition (each behind its golden net; pure moves).**
- `SectorView::show` phase extraction (#13) behind map snapshots.
- `segmentum.rs` submodule split behind `segmentum_tests`.
- The `show_*` panel model/layout splits (history wizard, control editors) — opportunistic, when next touching each panel.

**Stage 5 — Performance (only after a bench confirms the magnitude).**
- Auto-save debounce (#30); region-edit stroke coalescing (#31); index-rebuild gating (#46); viewer cache (#45). Add the missing analysis bench first so the gains are measured, not assumed.

**Stage 6 — Documentation/DX & polish.**
- README (#14), GUIDE bundling claim (#15), naming (#42-adjacent), `docs/` index, MAP.md reconciliation (#47), logging facade (#29), `[workspace.package]` (#42), bench profile (#41), and the Low/Nit tail (#48).

---

## 19. Quick wins (each < 1 day, concrete, repo-specific)

1. **`parse_hex_rgb` `is_ascii` guard** (#3) — insert `if !t.is_ascii() { return None; }` after the `len()!=6` check in [faction_style.rs:162](src/gen/faction_style.rs); add `assert!(parse_hex_rgb("a😀b").is_none())`. One line, kills a process-abort.
2. **Dimension upper-bound rule** (#2) — add a `GEN_SECTOR_TOO_LARGE` push after the square check in [validation.rs](src/validate/validation.rs) reusing `MAX_CUSTOM_DIM`; test with `dim=100000`. Closes the OOM bomb on every load path.
3. **`i32` overflow in placement** (Security #3) — compute `total_cells` as `(g.sector_width as usize) * (g.sector_height as usize)` in [placement.rs:14](src/gen/generation/placement.rs).
4. **README** (#14) — ~30–40 lines lifted from GUIDE §0–§1 + a Documentation links section + the workspace table. Zero code risk.
5. **GUIDE bundling claim** (#15) — delete [GUIDE.md:53-54](GUIDE.md), add "run from repo root."
6. **Builder 8×10 default** (#7) — change [project.rs:88](builder/src/builder/panels/project.rs) `height: 10` → `height: 8`.
7. **`top_faction_deltas` truncate** (#19) — add `.take(cfg.top_faction_deltas as usize)` to the diff Markdown faction loop; the knob users set in the builder finally does something.
8. **CLI hooks/missions config** (#9) — replace `load_or_regenerate`+`default()` with `resolve_sector_with_cfg(...|input| input.catalogs.{hooks,missions}.clone())` in [hooks.rs](src/cli/hooks.rs)/[missions.rs](src/cli/missions.rs), mirroring `sites.rs`.
9. **`empty_sector` unification** (#18) — have [scaffold.rs](src/model/sector_model/scaffold.rs) `empty_sector` delegate to `GeneratedSector::empty` (or swap the two literals for the crate constants). Stops persisting a stale `generator_version`.
10. **Per-enum slug guard test** (#44) — one `#[cfg(test)]` loop asserting `to_string().trim_matches('"') == v.as_slug()` over each slugged enum; fixes the Model + Data dual-source items together.
11. **`define_id!` payload visibility** (idioms #4) — `pub struct $name(pub Arc<str>)` → `$name(Arc<str>)`; grep confirms no external `.0` access, so `cargo check` proves it.
12. **`[profile.bench] inherits = "release"`** (#41) — three-line block replacement; benches then measure ship codegen.
13. **`[workspace.package]`** (#42) — add the block to root; members switch to `.workspace = true` for version/edition/rust-version.
14. **`mint_seed` counter** (#36) — add a module `static AtomicU64` and `hasher.update(&counter.fetch_add(1, Relaxed).to_le_bytes())`; drop the inaccurate ASLR comment.
15. **Seed-mint guard comments** (Determinism Low) — one-line `// determinism: SEED MINT ONLY — never call inside a generation stage` on `mint_seed` + viewer `random_seed_str`.

---

## 20. Validation checklist

Run before/after any change in this roadmap:
- [ ] `cargo build --workspace` — all four crates compile.
- [ ] `cargo check --workspace --all-targets` — including tests/benches.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — the intended CI gate.
- [ ] `cargo test --workspace` — full suite (~784 tests).
- [ ] `cargo test --test it -- golden` — **byte-stability gate**; any change to `render`/`svg_export`/`bitmap`/`html_export`/generation must keep this green (bless via `UPDATE_GOLDEN_{MD,JSON,SVG}=1` only when the change is intended).
- [ ] `UPDATE_MAP_SNAPSHOTS=0 cargo test -p sectorforge-gui-core` — live-render snapshot gate for any `SectorView`/`RenderMapTheme` change.
- [ ] `cargo test --test it segmentum -- --ignored` — when touching `segmentum.rs`.
- [ ] `cargo +nightly fuzz build` — when adding/altering deserializers or fuzz targets.
- [ ] For determinism-sensitive changes: confirm same-seed output is byte-identical and different-seed differs (the existing `golden_generation`/`invariants_proptest` cases).
- [ ] For any `pub`-item change in `lib.rs` or a re-exported type: enumerate downstream callers across all four trees, apply upstream, `cargo check --workspace`, then fix downstream.
- [ ] For command-bus changes: a round-trip test per affected variant (`apply`→`revert` restores a deep-cloned sector).

---

## 21. Codebase health scorecard

| Area | Score (1–10) | Rationale | Highest-impact improvement |
|---|---|---|---|
| Architecture & boundaries | 8 | Clean four-crate split, correct dependency arrow, honest thin parent modules; only weakness is the bifurcated facade vs flat-alias public surface | Promote GUI entry points into the facade so there's a real engine API boundary (#16) |
| File & module organization | 7 | Two scariest files are cohesive non-targets; real issues localized (one monolith + a `show_*` tail); `map/mod.rs` is a test-ratio artifact | Extract `SectorView::show` phases behind snapshots (#13) |
| Model & type design | 8 | Exemplary typed-IDs, controlled mutation API, correct RNG module; weaknesses are enforcement gaps not rot | Unify the two `empty_sector` constructors (#18) |
| Generation engine | 9 | Airtight RNG stage-keying + zero Fx-for-output + total-order sorts; only Low/Nit robustness items | Harden `mint_seed` entropy (#36) |
| Analysis subsystem | 8 | Strong determinism discipline (explicit tiebreaks, BTree outputs, per-anchor RNG); one untested parallel path | Byte-equality test for `run_search` near-misses (#17) |
| Export & render writers | 9 | Unusually disciplined byte-stability; Fx-for-lookup-only honored; well-covered goldens | Add explicit float-sort tiebreaks + a heatmap-on golden (Export, #28) |
| Validation & diff | 8 | Pure, well-organized; `GEN_SECTOR_NOT_SQUARE` correctly canonical; provably-deterministic diff | Wire up the dead `top_faction_deltas` knob (#19) |
| CLI structure & UX | 8 | Thin runners, sysexits codes, deterministic stdout, all 24 smoke-tested; one real config-drop bug | Fix hooks/missions `--project` config drop (#9) |
| Data layer & schema | 7 | Above-average serde discipline (aliases, DTO shim, no flatten); one systemic permissiveness gap | Add `deny_unknown_fields` to config structs (#10) |
| Builder architecture | 9 | Compiler-enforced command completeness + LD-ledger + off-thread re-derivation; gaps only at the off-bus derived seam | Invalidate ledger on watcher catalog reload (#21) |
| Builder panels | 8 | Structural edits all on-bus; map context menu textbook; the one broad gap is intentional/documented | Resolve the data-catalog bus bypass (#8) |
| Viewer (limited-edit) | 7 | Deliberate sound architecture + correct cascade cleanup; undercut by dual map-edit stacks + thin tests | Lift map mutations into shared `mutation.rs` methods (#11) |
| Shared GUI core | 8 | Enforced paint boundary, self-blessing snapshot suite, real benched caching; theme-fidelity + monolith gaps | Thread `region_tint_strength` into `RenderMapTheme` (#23) |
| Rust idioms | 9 | Newtypes, exhaustive enums, forbid-unsafe, exemplary Option/Result/panic discipline; localized stringly-typed seams | Retype `style_border` to its enum (#24) |
| Error handling | 9 | thiserror with source chaining + path context, sysexits mapping, GUIs surface failures; only gap is the panic boundary | Add the GUI panic boundary (shared with #4/#5) |
| Security (local model) | 7 | forbid-unsafe + path-traversal guard + fuzzed parsers; two reachable availability aborts on malformed input | Cap sector dimensions in validation (#2) |
| Testing & testability | 8 | Sophisticated golden/proptest/fuzz infra + CLI parity; thinness concentrated in viewer + negative paths | Test viewer write-paths (#12) |
| Performance | 9 | Hot paths demonstrably profiled (Arc-shared search, digest-gated render cache, honest benches) | Debounce per-command auto-save (#30) |
| Observability | 8 | Diagnostics-free engine via callbacks, stable exit codes, context-rich errors; catastrophic-failure diagnosability is the gap | Install a GUI panic hook (#4) |
| Dependencies / build / CI | 7 | Near-exemplary dep hygiene + thoughtful profiles, dragged down by total absence of CI | Add minimal CI pinned to 1.87 (#1) |
| Documentation / DX | 7 | Excellent GUIDE + crate doc + run-aliases, undercut by an empty README and one false claim | Write the README (#14) |
| **Determinism & invariants** | **6** | **Engine alone is 9 (airtight RNG/ordering/byte-stability); held to 6 by three confirmed GUI-layer invariant breaches** | **Close the geometry + command-bus leaks (#6/#7/#8)** |

**Workspace median: 8/10.** No Blockers; 15 distinct High findings, all localized and verified, none threatening engine output integrity.

---

## 22. Open questions

1. **Data-catalog undo (#8) — product decision required.** Is editing generation-input catalogs (relations/economy/factions/…) *intended* to be undoable, or is it deliberately outside the document-undo model because it edits inputs rather than the generated document? The fix differs entirely: a new `EditCatalog` command vs. an amendment to the CLAUDE.md command-bus carve-out. The reviewer and verifier both flagged this as a decision, not a defect to fix unilaterally.
2. **Viewer non-square sectors (#6).** Was the "Irregular dimensions" checkbox added for a real experimental workflow? If so, it needs to move behind the sanctioned test-only carve-out rather than being removed outright; if it was an oversight, plain removal is correct.
3. **`StarColour` richness (viewer #4).** Does `StarColour` carry any state finer than its 7 `code()` letters? If yes, the world-inspector save silently degrades it (Medium); if it's exactly 7 values, the round-trip is benign (Nit). One enum-definition check resolves the severity.
