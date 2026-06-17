# sectorforge — Codebase Audit (REVIEW.md-driven, 2026-06-16 refresh)

## 1. Executive Summary

**Overall health: strong and improving — a mature, determinism-disciplined Rust workspace with all five prior-audit themes addressed and no Blocker/High structural defects; the residual risk is concentrated in two correctness bugs and a handful of off-bus document-state leaks introduced by the new iterative-generation feature.**

- **Median score:** 7.5 / 10 (18 dimensions; scores range 6–9)
- **Findings by severity:** 0 Blocker · 2 High · 17 Medium · 27 Low · 19 Nit (65 total)
- **Highest-priority recommendation:** Fix the silent metric-zeroing key-casing bug in `src/analysis/interestingness.rs:193,197,202` (queries `"Warzone"`/`"Blockaded"`/`"Infiltrated"` against lowercase-slug keys), which makes the GrimCollapse/PoliticalSandbox/Mercantile/Villainous profile bands score 0 regardless of actual sector state. It is the only High-severity *correctness* defect with no compensating mechanism.

The two High findings are `interestingness.rs` (silent 0-scoring metrics) and the builder `regenerate_world` off-bus mutation (non-undoable world re-roll that also skips derivation invalidation). Neither is a crash or determinism violation; both are confined, well-localized, and have clear fixes.

---

## 2. DELTA vs the 2026-06-06 audit

The prior audit found 5 themes (median 8/10, no Blockers). Status of each against current HEAD:

| # | Prior theme | Status | Evidence (verified in code, not commit messages) |
|---|---|---|---|
| 1 | **No CI** | **HELD** (closed) | `.github/workflows/ci.yml` exists (995f1d8/b1d4f91): clippy `-D warnings`, `cargo test --workspace`, golden gate, nightly fuzz-build smoke. Swatinem cache on both jobs, MSRV 1.87 pinned consistent with workspace. CI went from 0 → ~60% of an ideal pipeline. |
| 2 | **Malformed-input aborts under panic=abort** | **PARTIAL** (engine fixed; GUI inherent) | `parse_hex_rgb` UTF-8 slice panic fixed — rejects non-ASCII before slicing (`src/gen/faction_style.rs:192`, regression test :280). Oversized-sector OOM blocked by `MAX_SECTOR_DIM=1024` guard (`src/validate/validation.rs:94-104`) before allocation (`placement.rs:39`), gated by `validate_project` (`generate.rs:103`). GUI `install_panic_hook` present in both binaries (`builder/src/main.rs:67`, `viewer/src/main.rs:37`) writing a crash note; `catch_unwind` in `gui-core/src/jobs.rs:100`. **Residual (inherent):** `panic=abort` propagates to release/quick/bench profiles, so `catch_unwind` is a documented no-op — a GUI panic logs then still aborts. |
| 3 | **Geometry + command-bus leaks** | **PARTIAL** (geometry HELD fully; bus mostly held with 2 new leaks) | **Geometry HELD:** viewer Irregular-dims checkbox removed (no match in `viewer/src/`), builder default now 8×8 (`builder/src/app.rs:20-26`), both gen panels lock `width==height` per-edit, `SectorSize` presets all N×N (`src/gen/random_sector.rs:87-104`), `GEN_SECTOR_NOT_SQUARE` fires pre-gen (`src/validate/validation.rs:84`). **Command-bus:** catalog-editor off-bus gap (30b8c4f) closed and round-trip tested. **But two new off-bus leaks remain:** `regenerate_world` (`generation_ops.rs:203-210`) and `apply_preview`/`apply_search_seed` (`generation_ops.rs:263,357`), both introduced by new feature work. |
| 4 | **Viewer under-tested + duplicated map-edit stacks** | **PARTIAL** (substantially closed) | Write-path test gap closed: 57 unit tests across 12 files (file_ops save/load round-trip `viewer/src/editor/file_ops.rs:198`, editor state `state.rs:320`, lifecycle `lifecycle.rs:443`). Two map-edit stacks deduped via shared engine methods (F11). **Residual:** `routes_panel.rs:105` still bypasses `sector.remove_route()` (manifest `route_count` stale on auto-save); auto-save path re-implements encode instead of `sector_to_json_bytes`. |
| 5 | **Docs: README stub + false "examples bundled" claim** | **HELD** (fixed) | README.md now substantive (67 lines: prerequisites, quick start, workspace table, doc links). `GUIDE.md:54-56` explicitly states examples are "NOT embedded in the binaries." New minor gaps surfaced separately: `BUILDER.md` has no ITERATIVE tab coverage; `eprintln!` in `html_export.rs:68` bypasses log facade. |

**Plain-language delta:** CI now exists and gates regressions. The two malformed-input panics are gone at the engine layer (the GUI-abort tradeoff is inherent to `panic=abort` and documented). Square geometry is fully locked everywhere. The viewer is now meaningfully tested. README/GUIDE are honest. The new regressions are all in the *newly added* iterative-gen + preview-apply surface — not re-openings of the old findings.

---

## 3. NEW since prior audit

Findings specific to the three feature waves added after 2026-06-06.

### Iterative generation (`afca9e4`, `cf64b94` — panels/iterative_gen.rs 1691 LoC + state/iterative_gen.rs 1118 LoC)
- **`commit_new_project` corrupts BuilderState on post-generation failure** (Medium, `state/iterative_gen.rs:828-860`): destructively overwrites `self.config`/`self.sector`/`self.data_catalogs` before `save_project_as`/`open_project`; if either `?`-fails the state is left with wizard document state installed, no `project_path`, session still present. No test covers the failure branch.
- **DAG step-navigation logic has zero direct unit tests** (Medium, `state/iterative_gen.rs:941/961/909/975/873`): `step_next`/`step_back`/`note_config_edit`/`invalidate_from`/`reroll_step` — the §2.3 DAG contract — exercised only transitively by the happy-path commit test.
- **Rail multi-step-back spawns+cancels up to 5 prefix jobs per frame** (Low, `panels/iterative_gen.rs:218-224`): loop calls `step_back()`→`rerun_preview()`→`spawn_prefix()` per hop; only the final spawn matters.
- **`last_command_error` shared with Conflict panel** (Low, `panels/iterative_gen.rs:119`): a conflict-panel error can surface inside the wizard as a fake preview failure.
- **Regions hint misidentifies its re-roll** (Nit, `panels/iterative_gen.rs:242`): says "re-rolls placement" but bumps `Stage::Regions`.
- **Positives verified:** RNG nonce threading maintains byte-identity with one-shot path at nonce-0; `precheck_generatable` MAX_CUSTOM_DIM=80 guard + inverted-worlds-range check prevent oversize/`gen_range` panics; correctly classified as transient (non-undoable) UI state.

### Viewer `--project` resolution (`7a01325`, `cb4ff17`)
- **Strong, no findings of concern** (positive, `viewer/src/main.rs:122-148`): handles all three input forms (project dir / out dir / sector.json), structured error listing all tried paths, hard exit code 2 on miss, 9 dedicated edge-case tests. Prior theme-5 "silent empty editor" fully fixed.

### Convex per-node warp-route density (`f75e5e9`)
- **Mathematically correct and fully deterministic** (positive, `src/gen/generation/routes.rs`): pure math on config inputs (`ROUTE_DENSITY_CURVE=2.0`, `ROUTE_DENSITY_EXTRA_PER_NODE=2.0`) with stable tiebreaks; short-is-safer invariant maintained by `cap_perilous_routes` + `rebalance_public_stability`. One adjacent latent bug (CalmCorridor floor mismatch) is config-divergence-only — see §6.

---

## 4. Codebase Health Scorecard

| Dimension | Score | One-line rationale |
|---|---:|---|
| determinism | 9 | No FxMap iterated for output; all RNG via `stage_rng`; one cosmetic BFS-visited FxSet, output still byte-stable via commutative max. |
| geometry-invariant | 9 | Prior theme-3 geometry fully held; every non-square path closed; `GEN_SECTOR_NOT_SQUARE` pre-gen gate intact. |
| architecture | 8 | Clean engine←gui-core←(builder,viewer) layering, no egui in engine; facade hoists some pipeline internals as public. |
| rust-idioms | 8 | Excellent `define_id!` + closed-set enums + slug-parity tests; faction `kind`/`disposition` still open `Arc<str>`. |
| error-handling | 8 | thiserror non_exhaustive enum, structured diagnostics, clean exit-code map; 4 variants stringify their source. |
| export-bytestability | 8 | Golden net comprehensive, floats fixed-precision; `{:?}` Debug used for 11 fields with Display; `</script>` injection gap. |
| security-robustness | 8 | All three theme-2 items held; symlink-traversal not canonicalized (desktop-only, narrow). |
| dependencies-build | 8 | Strong workspace hygiene; single-consumer deps in workspace scope; `panic=abort` propagation noted. |
| viewer | 8 | 57 unit tests, map-stacks deduped; auto-save skips manifest refresh, `routes_panel` bypasses `remove_route`. |
| gui-core-docs-obs | 8 | SectorView decomposed, palette single-source; one `eprintln!` in library, BUILDER.md missing ITERATIVE. |
| file-size-complexity | 7 | Large files mostly data-density or cohesive; UI panels embed pure algorithms (union-find/MST in routes.rs). |
| gen-correctness | 7 | Sound + tested; EmpyricBleed/BeaconChain silent no-op; CalmCorridor floor uses wrong max under config divergence. |
| analysis-correctness | 7 | Strong determinism; interestingness key-casing zeroes 3 metrics; two algorithmic inefficiencies at upper scale. |
| testing | 7 | Large well-structured suite; DAG step-nav untested; gen pipeline not fuzzed. |
| builder-commandbus | 7 | 40-variant exhaustive bus held; `regenerate_world` + `apply_preview` off-bus; `ensure_*_catalog` skip `note_catalog_edit`. |
| iterative-gen-new | 7 | Architecturally sound, well-guarded; `commit_new_project` state-corruption window on save failure. |
| ci-release | 6 | CI exists (theme-1 closed); no fmt-check, no audit/deny, ignored tests never run, no release workflow. |

**Median: 7.5**

---

## 5. Prioritized Findings (Medium+ only)

Low/Nit findings excluded from table (aggregate: **27 Low, 19 Nit**). Highest-impact first.

| P | Severity | Category | Location | Finding | Action |
|---|---|---|---|---|---|
| 1 | High | Logic Error | `analysis/interestingness.rs:193,197,202` | PascalCase keys never match lowercase-slug `system_state_counts`; warzone/blockaded/infiltrated counts always 0 | Lowercase the 3 `.get()` keys; add a sector-state derive test |
| 2 | High | command-bus | `state/generation_ops.rs:203-210` | `regenerate_world` mutates world payload off-bus → non-undoable + skips LD2 invalidation | Route through `BuilderCommand::EditWorld` |
| 3 | Medium | Logic Bug | `gen/regions.rs:614-663` | EmpyricBleed/BeaconChain have precedence>0 + docs but fall to wildcard no-op arm | Add Turbulence/CalmCorridor branches + tests |
| 4 | Medium | Logic Bug | `gen/generation/mod.rs:521-525` | CalmCorridor floor uses `config.max_route_distance`, not rules-capped max → can degrade routes | Pass `config.max(rules).max_distance` at call site |
| 5 | Medium | command-bus | `state/generation_ops.rs:263,357` | `apply_preview`/`apply_search_seed` swap sector without clearing log → phantom no-op undos | Clear `command_log`/`command_cursor`/`snapshots` |
| 6 | Medium | command-bus | `panels/{personae,hooks,missions,sites,prose}.rs` | `ensure_*_catalog` writes `data_catalogs` without `note_catalog_edit` → catalog-create not undoable | Call `note_catalog_edit()` after init write |
| 7 | Medium | Correctness | `state/iterative_gen.rs:828-860` | `commit_new_project` overwrites state before save; failure leaves corrupt stuck state | Snapshot+restore, or build scratch state for save |
| 8 | Medium | Test gap | `state/iterative_gen.rs:941/961/909/975/873` | §2.3 DAG step-nav (`step_next`/`back`/`note_config_edit`/`invalidate_from`/`reroll_step`) untested | Add 4 unit tests in `iterative_gen_session` |
| 9 | Medium | type-design | `model/sector_model/mod.rs:756-757` | faction `kind`/`disposition` open `Arc<str>`, string-dispatched across 6+ modules with divergence | Introduce `FactionKind` enum w/ `Unknown(Arc<str>)` |
| 10 | Medium | type-design | `gen/routes.rs:54-63` | `RouteCondition` filter fields raw `Option<String>` → misspelled condition silent no-op | Typed `Option<NotableFeature/WorldType/...>` |
| 11 | Medium | byte-stability | `export/render.rs` (11 sites) | `{:?}` Debug for enums with Display → PascalCase in goldens, drifts on rename | Swap `{:?}`→`{}`, re-bless goldens |
| 12 | Medium | byte-stability | `export/segmentum.rs:864` | `{:?}` for RouteType/RouteStability in inter-sector links table (no committed MD golden) | Swap `{:?}`→`{}` |
| 13 | Medium | XSS/output | `export/html_export.rs:93-97,149` | Sector JSON embedded in `<script>` without `</script>` escaping → malformed HTML | `.replace("</","<\\/")` on JSON before embed |
| 14 | Medium | Boundary Leak | `lib.rs:98-108`, `gen/generation/mod.rs:25-29` | Compat aliases expose `build_system`/`assign_factions_for_systems` as public API | Downgrade internals to `pub(crate)` |
| 15 | Medium | sep-of-concerns | `panels/routes.rs:1331-1460` | 130-line union-find+MST graph algorithm in UI layer, partial dup of `regions.rs:775` | Move to `src/gen/routes.rs` |
| 16 | Medium | Performance | `analysis/search.rs:662-680` | `count_systems_matching_distance` BFS scans all routes per node → O(S·R) | Build adjacency map once, BFS O(S+R) |
| 17 | Medium | CI coverage | `.github/workflows/ci.yml` | No `cargo fmt --check`; repo documented not-fmt-clean → policy ambiguity | Bless format once + add gate, or document skip |
| 18 | Medium | Security/CI | `.github/workflows/ci.yml` | No `cargo audit`/`cargo deny` → supply-chain CVEs undetected | Add `rustsec/audit-check@v1` or `deny.toml` |

---

## 6. Top Findings Detailed (all Blocker/High)

### [HIGH] interestingness.rs reads system_state_counts with wrong PascalCase keys — always returns 0
- **Severity / Category:** High / Logic Error (adversarially-confirmed)
- **Evidence:** `src/analysis/interestingness.rs:193,197,202` (`.get("Warzone")`, `.get("Blockaded")`, `.get("Infiltrated")`); keys inserted via `Arc::from(state.as_slug())` at `src/analysis/analytics.rs:353` produce lowercase `"warzone"`/`"blockaded"`/`"infiltrated"` per `src/model/sector_model/mod.rs:1164-1172`.
- **What:** `observed_metrics` queries `a.system_state_counts` with PascalCase literals, but `compute_system_state_counts` inserts lowercase slug keys. The keys never match, so `warzone_count`, `blockaded_count`, `infiltrated_count` are permanently 0 in every scorecard.
- **Why:** Profile bands for GrimCollapse (`warzone_count` w1.0, `infiltrated_count` w0.7), PoliticalSandbox (w0.6), Mercantile (w0.8), Villainous (w0.5/w0.4) silently treat these as zero. GrimCollapse requires `warzone_count >= 4` but scores 0 fit regardless of actual sector state — the profile's score is meaningless. No test exercises `derive_with` against a sector that actually has these states, so it went undetected.
- **Fix:** Lowercase the three `.get()` keys at lines 193/197/202. Add a test that builds a sector with a known system state, derives the scorecard, and asserts `metric_scores` for these names have `observed > 0`.

### [HIGH] regenerate_world mutates world payload off-bus (non-undoable world re-roll)
- **Severity / Category:** High / command-bus (adversarially-confirmed)
- **Evidence:** `builder/src/builder/state/generation_ops.rs:203-210`; caller `builder/src/builder/panels/world/overlays.rs:262`.
- **What:** `BuilderState::regenerate_world` directly assigns `w.world = dto`, `w.source_row_index = source_row`, `w.tags = tags` on `self.sector_mut().systems[sys_idx].worlds[w_idx]` — three document-state field writes that never enter the command log.
- **Why:** These fields are serialized into `sector.json` (world type, source catalog row, tag list). Bypassing the bus makes the re-roll non-undoable (§R4 violation), and LD2 per-class invalidation (`SystemsWorlds`) is skipped — only `dirty=true` and `mark_validation_dirty()` fire, so derived data (economy/relations/personae/etc. keyed off worlds) is left stale.
- **Fix:** Clone the world, set the three fields on the clone, then dispatch `self.run(BuilderCommand::EditWorld { world: id.clone(), before: None, after: Box::new(clone) })`. Remove the direct `sector_mut()` writes and the manual `dirty`/`mark_validation_dirty`/`trigger_auto_save` calls — the bus handles all three.

*(No Blocker findings.)*

---

## 7. Refactoring Roadmap & Quick Wins

### Roadmap (staged; each stage gated by the prior one's safety net)

**Stage 0 — Safety net (do first; unblocks everything).**
- Add `cargo fmt --check` (bless the ~20 pre-existing files in one commit) + `cargo audit`/`deny.toml` + `--locked` to CI cargo commands (P17, P18; `ci.yml`).
- Add a scheduled job running `cargo test --test it -- --include-ignored` so the segmentum/export-byte determinism goldens actually run (`segmentum_tests.rs`, `export_byte_goldens.rs`).
- Add the missing DAG step-nav unit tests (P8) before touching iterative-gen internals.

**Stage 1 — Correctness bugs (highest user impact, all localized).**
- Fix interestingness key-casing (P1) and `regenerate_world` off-bus (P2).
- Fix EmpyricBleed/BeaconChain no-op (P3) and CalmCorridor floor mismatch (P4) with the new unit tests.
- Fix the three command-bus leaks: `apply_preview` log-clear (P5), `ensure_*_catalog` `note_catalog_edit` (P6), `commit_new_project` state-corruption window (P7).

**Stage 2 — Output-contract hardening (golden-gated).**
- Swap `{:?}`→`{}` in `render.rs` (11 sites) + `segmentum.rs:864`, re-bless goldens (P11, P12).
- `</script>` escaping in `html_export.rs` (P13); add a `segmentum.md` golden to catch future `{:?}` drift.

**Stage 3 — Type-design tightening (compiler-checked, semver-aware).**
- Introduce `FactionKind` enum (P9) — unifies the divergent `IMPERIAL_KINDS`/`KindGroup` taxonomies and makes adding a kind a compiler-flagged change across all 6+ sites.
- Typed `RouteCondition` filter fields (P10) and `FactionDef::preferred_*` vecs (Low) — serde catches misspellings at load.

**Stage 4 — Boundary & structure (proportionate, after goldens are green).**
- `pub(crate)` the pipeline internals leaked by `lib.rs` (P14); add `MutationError` to the facade.
- Extract the union-find+MST out of `panels/routes.rs` into `src/gen/` (P15).
- Optional: split `diff.rs` renderer into a submodule; `factions_overview.rs` presets/designer split — only when these files cross their next size threshold.

**Stage N — Performance (only if upper-scale profiling confirms).**
- Adjacency-map BFS in `search.rs` (P16) and double-sweep `graph_diameter` in `analytics.rs` — tolerable today, will bite at 900-system sectors.

### Quick wins (<1 day each)
- Lowercase 3 keys in `interestingness.rs:193,197,202` (P1) — minutes + 1 test.
- Clear `command_log`/`cursor`/`snapshots` on `apply_preview`/`apply_search_seed` (P5) — 3 lines, makes tooltip honest.
- `{:?}`→`{}` mechanical swap + re-bless goldens (P11, P12).
- `note_catalog_edit()` after each `ensure_*_catalog` init (P6) — 5 one-line additions.
- Fix Regions hint text `iterative_gen.rs:242` (Nit) and `let _ = rng;`→`_route_rng` doc (Nit).
- Swap `eprintln!`→`log::warn!` in `html_export.rs:68` (Low).
- Add `#[must_use]` to `all_worlds`/`hex_distance`/`FactionInfluence::weight` (Nit).
- Remove redundant `rayon` `[dev-dependencies]` entry (`Cargo.toml:101`) (Nit).
- Replace `routes_panel.rs:105` `sector.routes.remove(i)` with `sector.remove_route(&id)` (Low).
