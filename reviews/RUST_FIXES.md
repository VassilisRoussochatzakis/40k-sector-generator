# RUST_FIXES.md — Prioritised Action Backlog

> Companion to `FINDINGS.MD`. Every entry is an action item, ordered for sequencing.
> Each carries the source-finding ID(s) (cross-ref `reviews/**/*.review.md`), a
> concrete fix sketch, effort (S/M/L/XL), and risk (Low/Medium/High).
>
> **Definitions.** Effort: S ≤ 1 hr per site, M ≤ 1 day, L ≤ 1 week, XL > 1 week.
> Risk: chance the change introduces a regression or requires golden-test refresh.

## How to read

1. **§1 Quick wins** — do these first. Small, safe, high-leverage. Most are S/Low.
2. **§2 Targeted fixes** — medium scope, scope to one logical area at a time.
3. **§3 Structural refactors** — large rewrites; sequence carefully, golden-stable plan included for each.
4. **§4 Follow-ups** — preventive / observability work (not findings, but recommended).

All fixes are read-only-derived from the agent reports. **Verify each site before applying** — the review was a single pass and the codebase may have moved since.

---

## §1 Quick wins (do first)

### §1.1 Determinism hard-rule fixes (CRITICAL priority; 3 changes)

| # | Source | File:Line | Fix | E/R |
|---|---|---|---|---|
| QW-D-1 | F-011-001 | `src/analysis/importance.rs:139-179` | Replace `std::collections::HashMap<KindGroup, AggregateAcc>` with `BTreeMap`. Derive `PartialOrd, Ord` on `KindGroup`. Add tertiary tie-break in the final sort: `.then(group_label_cmp(...))`. | S/Low |
| QW-D-2 | F-017-006 | `builder/src/builder/panels/history.rs:1409` | Replace `DefaultHasher` with `blake3` via `sectorforge::rng::derive_stage_seed("chronicle", &event_payload)` — first 8 bytes truncated to a hex string. | S/Low |
| QW-D-3 | F-021-002 / F-X06-RNG | `viewer/src/editor/generation_panel.rs:30,259` | Replace `rand::random::<f64>()` with `blake3::Hasher` keyed by `std::time::SystemTime::now()` mod a session salt; render as `u64` (8 hex bytes). Then drop `rand = "0.8"` from `viewer/Cargo.toml`. | S/Low |

### §1.2 Reachable panic fixes (HIGH priority; 7 changes)

| # | Source | File:Line | Fix | E/R |
|---|---|---|---|---|
| QW-P-1 | F-X07-001, F-017-012 | `builder/src/builder/panels/control.rs:1231,1240` | Replace `partial_cmp(...).unwrap()` with `f32::total_cmp` (no NaN risk) or `.unwrap_or(Ordering::Equal)`. | S/Low |
| QW-P-2 | F-X07-002 | `viewer/src/app/mod.rs:237` | Replace `Utf8PathBuf::from_path_buf(p).unwrap()` with `match Utf8PathBuf::from_path_buf(p) { Ok(p) => p, Err(orig) => { error_dialog!("path is not UTF-8: {}", orig.display()); return; } }`. | S/Low |
| QW-P-3 | F-X07-003 | `gui-core/src/sector_view.rs:719`, `src/export/svg_export/labels.rs:240`, `src/export/bitmap/labels.rs:247` | Replace `expect("non-empty")` with `let Some(first) = sub.hex_cells.first() else { continue; };`. Apply the identical fix in all three sites. | S/Low |
| QW-P-4 | F-009-002 | `src/analysis/search.rs:843-855` | Guard `n - 1`: `let miss = if passed { 0.0 } else if n == 0 { 1.0 } else { (n - 1) as f32 };` (or `n.saturating_sub(1) as f32` if 0 is a valid "max miss" sentinel). | S/Low |
| QW-P-5 | F-013-001 | `src/export/map_theme.rs:514-536` | `parse_color`: `if !hex.is_ascii() { return Err(...); }` at the top. Or rewrite with `hex.as_bytes().chunks_exact(2).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16))`. | S/Low |
| QW-P-6 | F-014-001 | `src/export/bitmap/geom.rs:20-37` | `Geom::new`: `let scale = scale.try_into().map_err(...)?;` for `i32`; clamp `total_w/h` to `i32::MAX as u32` before passing to `RgbaImage::from_pixel`. | S/Low |
| QW-P-7 | F-006-001 | `src/validate/diff.rs:454,549,705` | Replace `unreachable!()` in `(None, None)` arms with documented no-op `continue;` or return an empty diff. Add `#[cold]` annotation. | S/Low |

### §1.3 Security / injection (HIGH priority; 3 changes)

| # | Source | File:Line | Fix | E/R |
|---|---|---|---|---|
| QW-S-1 | F-005-001 | `src/loading/presets.rs::rewrite_seed` | Use `toml::Value::String(seed).to_string()` to escape, or reject seeds containing `"`/`\`/control chars with `Err(SectorError::config_parse(...))`. | S/Low |
| QW-S-2 | F-005-003 | `src/loading/input.rs::read_relative` | Reject `Path::is_absolute() || rel.components().any(|c| matches!(c, Component::ParentDir))`. Use `Utf8Path::normalize()` (vendored if not in `camino`) for the digest key. | S/Low |
| QW-S-3 | F-014-002 | `src/export/svg_export/primitives.rs::escape_xml_into` | Add `c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => { /* drop or replace with U+FFFD */ }` to the match. | S/Low |

### §1.4 `#[non_exhaustive]` blanket pass (HIGH priority; one PR per crate)

> Per-finding sources: F-X02-001, F-001-API1, F-002-002, F-003-001, F-004-005, F-009-003, F-008 (`FactionDef`, `FactionsFile`), F-013/014 enum mentions.

| # | Crate | Target | Notes |
|---|---|---|---|
| QW-NE-1 | `sectorforge` | All `pub enum` in `src/{worlds,validate,model,analysis,export,loading}.rs` and all sub-modules | One-line attribute. Confirm no downstream `match` block in this workspace requires a wildcard arm (compile will fail loud if so). |
| QW-NE-2 | `sectorforge-gui-core` | `MapTheme`, `MapRegionOverlay`, `RouteControlKind`, 7 token enums, `SectorView` (struct), `JobHandle` (struct) | Combine with QW-API-1 below to move `SectorView` to a builder pattern. |
| QW-NE-3 | `sectorforge-builder` | `BuilderError`, `BuilderCommand` (32 variants), `BuilderTab`, every panel-action enum | Sequence after the command-bus refactor (see §3) so no churn. |
| QW-NE-4 | `sectorforge-viewer` | `ViewerError`, `EditorState` (struct, if it stays), `View` enum (`layout.rs`) | Resolve T-06 (viewer scope) before applying so removed types don't get attributes. |

Effort each: S. Risk: Low (compile-time confirmation; consumers in-tree).

### §1.5 Cargo hygiene (MEDIUM priority; one PR)

| # | Source | Change | E/R |
|---|---|---|---|
| QW-C-1 | F-X06-Unused-1 | Remove `image = ...` from `viewer/Cargo.toml` (verified unused by grep). | S/Low |
| QW-C-2 | F-X06-Unused-2 | Remove `tempfile = "3"` from `viewer/Cargo.toml`'s `dev-dependencies` (verified unused by grep). | S/Low |
| QW-C-3 | F-X06-Unused-3 | Remove `eframe = ...` from `gui-core/Cargo.toml` if `gui-core/src/lib.rs` doesn't re-export `eframe::*` (verify; the crate may use it for type aliases). | S/Low |
| QW-C-4 | F-X06-Lints | Move `[lints.clippy] disallowed_types/methods = "deny"` from `builder/Cargo.toml` + `viewer/Cargo.toml` into a workspace `[workspace.lints]` block at the root. Delete the duplicate `builder/clippy.toml` and `viewer/clippy.toml` (they are byte-identical). | S/Low |
| QW-C-5 | F-X05-015 | Tighten `[profile.bench]` to `lto = "fat"` so bench numbers match prod release. | S/Low |
| QW-C-6 | F-X06-MSRV | Add `rust-version = "1.78"` (or whatever the workspace builds against) to each `[package]` section. | S/Low |
| QW-C-7 | (preventive) | Add `#![forbid(unsafe_code)]` at each crate root (`src/lib.rs`, `builder/src/lib.rs`, `viewer/src/lib.rs`, `gui-core/src/lib.rs`). Locks the existing 0-unsafe property in. | S/Low |

### §1.6 Small bug fixes (HIGH/MEDIUM by reachability; ~12 changes)

| # | Source | File:Line | Fix | E/R |
|---|---|---|---|---|
| QW-B-1 | F-004-001 | `src/model/errors.rs:41-52` | Delete the dead duplicate `MutationError` enum (no callers). | S/Low |
| QW-B-2 | F-011-003 | `src/analysis/route_control.rs:142` | Expand `pirate` matcher to include `chaos_space_marine`, `chaos_knight`, `traitor_guard`, `traitor_titan_legion`, `daemon`, `cult`. Add an inline test that exercises each. | S/Low (gold refresh) |
| QW-B-3 | F-011-004 | `src/analysis/control.rs:326-331` | Remove dead `disposition == "lawful"` branch in `claim_for::imperial` (both arms return `ImperialMandate`). | XS/Low |
| QW-B-4 | F-011-002 | `src/analysis/control.rs:368-372, 473-481, 493-497` | Unify tie-break direction across `derive_world_control` and `derive_system_control` via a shared `score_then_id(...)` helper. | S/Low (gold refresh on tied data) |
| QW-B-5 | F-011-009 | `src/analysis/conflict.rs:74-90` | Replace `.fold(...)` second-place pick with sorted top-2 (rank by `local_control_score` desc, id asc). | S/Low |
| QW-B-6 | F-011-010 | `src/analysis/intel.rs:212-226` | Add `if !raw_conf.is_finite() || raw_conf < 5.0 { continue; }`. | XS/Low |
| QW-B-7 | F-012-003 | `src/export/subsectors/mod.rs:518-525 vs 594-606` | Make seeding and Lloyd-refinement tie-break directions agree (pick one, apply both). | S/Low (gold refresh) |
| QW-B-8 | F-006-007 | `src/validate/diff.rs` (`diff_relations`) | Don't drop pairs present in `before` but absent in `after`; emit a `Removed` variant. | S/Low |
| QW-B-9 | F-006-005 | `src/validate/diff.rs` (faction-delta sort) | Replace `partial_cmp(...).unwrap_or(Equal)` with `f32::total_cmp` for fully-defined NaN behavior. | S/Low |
| QW-B-10 | F-009-011 | `src/analysis/economy.rs:913-917` | Replace `is_none()` + `unwrap()` with `let Some(sys) = ... else { continue };`. | S/Low |
| QW-B-11 | F-009-013 | `src/analysis/economy.rs:1117` | `Vec::with_capacity(systems.len() * 4)` in `derive_dependency_edges`. | S/Low |
| QW-B-12 | F-018-010 | `builder/src/builder/panels/conflict.rs:167-180` | Stop dispatching `SetSystemConflict` every frame in aggregate mode — guard with a dirty flag. Also propagate the dispatch error rather than swallow it. | S/Low |
| QW-B-13 | F-020-001 | `viewer/src/app/mod.rs:211` | Replace `serde_json::to_string_pretty(...).unwrap()` with `?`; propagate write error from auto-save to the UI status bar. | S/Low |

---

## §2 Targeted fixes (M effort, one logical area each)

### §2.1 Public API surface narrowing

| # | Source | Change | E/R |
|---|---|---|---|
| TF-API-1 | F-X02-003 | Audit every `pub` in `builder/src/builder/panels/`, `builder/src/builder/session.rs`, `viewer/src/**/*.rs`. Downgrade ~350 internal items to `pub(crate)`. Run `cargo doc --workspace --no-deps` before/after to verify the public docs surface shrinks. | M/Low |
| TF-API-2 | F-X02-004, F-001-API2 | Convert `SectorView` (19 pub fields) from struct-literal construction to a builder: `SectorView::new(model_ref).with_theme(t).with_overlay(o).build()`. Keep `pub` only on fields explicitly intended for external mutation. Move `SectorMapCache`'s 4 raw HashMap pub fields behind accessor methods. Replace `JobHandle`'s `pub Arc<Mutex<f32>>` and `pub Receiver<T>` with private fields + getter methods. | M/Medium |
| TF-API-3 | F-X02-002 | Resolve the two-crate naming collisions:<br>(a) `sectorforge::map_theme::MapTheme` (data) vs `sectorforge_gui_core::map_theme::MapTheme` (rendering) → rename one (suggest `RenderMapTheme` for gui-core).<br>(b) `sectorforge::heatmap::HeatCellRgb` vs `sectorforge_gui_core::heatmap::HeatCell` → consolidate into one type with a `From` impl. | M/Medium |
| TF-API-4 | F-003-002, F-010-004, F-010-017 | Introduce `pub fn as_slug(&self) -> &'static str` on every taxonomy/enum that's used as a map key or in user-visible text. Matches `#[serde(rename_all = "snake_case")]`. Replace every `format!("{:?}", enum)` site with `as_slug()`. Bonus: `Display` falls through to `as_slug`. | M/Low |

### §2.2 Newtype discipline for narrative IDs

| # | Source | Change | E/R |
|---|---|---|---|
| TF-NT-1 | F-010-003 | Add `PersonaId`, `HookId`, `MissionId` via `define_id!` in `src/model/ids.rs`. Re-export from `crate::ids`. Switch `Persona.id`, `Hook.id`, `MissionSeed.id` bare `String`s to the newtypes. `#[serde(transparent)]` keeps disk format unchanged. Also: `BriefingProfile::observer_faction: Option<FactionId>` and `restrict_to_factions: Vec<FactionId>`. | M/Medium (golden test refresh + downstream consumers) |
| TF-NT-2 | F-011-017 | Introduce score newtypes: `ControlScore(f32)`, `DisplayImportance(f32)`, `ProjectedPower(f32)` with `Copy + PartialOrd + Display`. Replace bare `f32` returns in `control.rs:260,365,464-469`; `importance.rs:112-115`; `power_projection.rs:53,167`. | M/Low |
| TF-NT-3 | F-016-various | Cache `feature_weights_for_world(...)` result on `BuilderState` derivations rather than rebuilding per frame; add a typed `FeatureWeightsCache`. | M/Low |

### §2.3 Performance: rayon search + per-frame format pruning

| # | Source | Change | E/R |
|---|---|---|---|
| TF-P-1 | F-009-001 | Wrap immutable arms of `ProjectInput` in `Arc`:<br>`pub struct ProjectInput { pub root_dir: Utf8PathBuf, pub config: AppConfig, pub catalogs: Arc<ProjectCatalogs> }`. `clone_project_with_seed` then `Arc::clone(&self.catalogs)` (one atomic op vs deep clone). | M/Medium (public ProjectInput shape; downstream callers in CLI + viewer) |
| TF-P-2 | F-X05-002 | Replace `(0..budget).into_par_iter().map(...).collect::<Vec<Slot>>()` in `src/analysis/search.rs:1098-1123` with a streaming pattern that short-circuits on first acceptable winner. Either:<br>(a) keep `.collect()` but cap the iterator with an `AtomicBool` "winner found" guard inside the closure, or<br>(b) use `find_any` / `find_first` to locate the winner, then a second pass for `near_misses`. | M/Medium (perf gain claim needs criterion bench to validate) |
| TF-P-3 | F-X05-001, F-002-001 | Per-enum `as_slug()` pass (combined with TF-API-4) kills ~half of the `format!` density. Hoist label-uppercasing to `SectorMapCache::system_label_cache: BTreeMap<SystemId, Arc<str>>`. | M/Low |
| TF-P-4 | F-X05-003 | Add `SectorMapCache::faction_style_index: BTreeMap<FactionId, FactionStyle>` populated once per derivation cycle. Replace per-route + per-system linear `faction_style_by_id(...)` scans. | M/Low |
| TF-P-5 | F-X05-004 | Replace `pub fn color_hex(c: Color32) -> String` in svg writer with `pub fn write_color_hex(into: &mut String, c: Color32)` and update every call site. | S/Low |
| TF-P-6 | F-009-005, F-009-007, F-009-008, F-009-009, F-009-010 | Batch hoist in `src/analysis/{economy,relations}.rs`: precompute route adjacency once, build override BTreeMaps once per derive, pre-bucket deps by `(consumer, resource)` once, share `valid_routes_by_sys` between `stranded` check and `derive_dependency_edges`. | M/Low |
| TF-P-7 | F-010-001 | `briefing::apply`: change `BriefingPack::sector` to `Cow<'a, GeneratedSector>` so `GmFullTruth` profile borrows. Separately project a `Vec<FactionRelation>` rather than `Arc::make_mut`'ing the matrix. | M/Medium |
| TF-P-8 | F-010-002 | Pre-bucket non-perilous routes by `(endpoint, crit)` once before the triple-nested loop in `hooks::emit_economy_hooks`. Drops O(R²) to O(R). | S/Low |
| TF-P-9 | F-016-007/008/009 | Builder panels — replace per-frame `sector.routes.clone()`, world `factions`/`claims` clones, and `feature_weights_for_world()` rebuilds with cache-backed reads. | M/Low |
| TF-P-10 | F-019-001, F-019-002 | Viewer — `dashboard.rs:43` borrow the `SectorAnalysis` instead of cloning. `segmentum_view.rs::super_map` — hoist the `BTreeMap<String,Rect>` to `app::state` and invalidate via a sector-digest hash. | M/Low |
| TF-P-11 | F-X05-005 | Replace whole-document `serde_json::to_string_pretty` + `fs::write` with `serde_json::to_writer_pretty(BufWriter::new(File::create(p)?), &value)` across every export writer. | M/Low |
| TF-P-12 | F-016-various | Add `BuilderState::sector_lookup: BuilderIndex { systems_by_id: BTreeMap<SystemId, usize>, ... }`. Replace the 19 `iter().find(|s| s.id == target)` linear scans across the panels with `O(log n)` lookups. | M/Low |

### §2.4 Error model coherence

| # | Source | Change | E/R |
|---|---|---|---|
| TF-E-1 | F-X03-002 | `builder/src/builder/project_io.rs:832-924` `reload_catalog` — replace 15 `if let Ok(cfg) = toml::from_str(...)` with `?` and a typed `CatalogReloadError` enum (per-file variant). Surface to UI status bar on failure. | M/Medium |
| TF-E-2 | F-X03-003 | `build_subsectors` 8 dropped-error sites — surface to UI in builder/viewer; in analytics, lift to a `HealthFlag { code: "SUBSECTOR_DERIVE_FAILED", ... }`. | M/Low |
| TF-E-3 | F-X03-004 | `src/main.rs` and every CLI runner — adopt a `cli::ExitCode::from(SectorError)` matcher that maps:<br>- `ValidationFailed` → 1<br>- `Cancelled` → 130<br>- `Io(...)` → 74<br>- `Config(...)` → 78<br>- `WorldDataLoad`/parse → 65<br>- default → 70. Document in `--help`. | M/Medium |
| TF-E-4 | F-005-002 | `src/loading/input.rs::load_project` — don't collapse `WorldError` into `SectorError::WorldDataLoad(String)`. Add `#[from] WorldError` and let the source chain flow. | M/Low |
| TF-E-5 | F-006-003 | `src/validate/validation.rs::render_markdown` — remove the `let _ = writeln!(s, ...)` pattern (80+ sites) by adding a local `wln!(s, ...)` macro that asserts `Ok(())` (writeln to `String` is infallible). | S/Low |
| TF-E-6 | F-006-004 | `src/validate/validation.rs` — replace stringly-typed `code: String` with `pub enum ValidationCode { ... } impl ValidationCode { pub fn as_slug(&self) -> &'static str }`. | M/Medium |
| TF-E-7 | F-015-005 | `builder/src/builder/state/undo.rs:68-78::trigger_auto_save` — propagate IO errors to `state.last_save_error` and render in a status bar. | S/Low |
| TF-E-8 | F-005-004 | `src/cli/generate.rs` — unify failure paths: every error returns `Err(SectorError::...)`. Let the new `cli::ExitCode` matcher decide the exit code. | M/Low |

### §2.5 Test-suite improvements

| # | Source | Change | E/R |
|---|---|---|---|
| TF-T-1 | F-022-001 | Memoise the test fixture `OnceLock<GeneratedSector>` in `tests/it/mod.rs`. Switch the 5 files that re-generate per-test (`invariants_tests.rs`, `search_and_diff.rs`, `analytics_and_presets.rs`, `svg_export_tests.rs`, `validation_tests.rs`) to read from the shared lock. ~25 redundant generations saved per run. | M/Low |
| TF-T-2 | F-022-002, F-X08-001 | Replace `builder/src/builder/file_watcher.rs:134-170` test's `thread::sleep(1.2s)` + polling with `filetime::set_file_mtime` + `recv_timeout`. Extract a pure `scan_once()` for unit testing. | M/Medium |
| TF-T-3 | F-022-003 | Triage `tests/it/segmentum_tests.rs` (5 `#[ignore]` tests). For each: either delete it (with a comment in `segmentum.rs` that it's untested by integration), or re-enable and fix what's broken. | M/Medium |
| TF-T-4 | F-022-004 | Add CLI integration coverage for the 17 untested subcommands (`analyze`, `validate`, `relations`, `economy`, `personae`, ...). Use `assert_cmd` + `predicates`. One-test-per-subcommand starter. | L/Low |
| TF-T-5 | F-022-006 | Make `tests/it/golden_png.rs` pin a `blake3` hash of the rendered bytes (template: `gui-core/tests/map_snapshots.rs`). | S/Low (regenerate hash once) |
| TF-T-6 | F-022-007 | Delete the 4 duplicate `determinism_holds_across_random_seeds` proptests in `personae`/`economy`/`hooks`/`relations` — the sector-level proptest in `invariants_proptest.rs` already covers them. ~80 `generate_sector` calls saved per run. | S/Low |
| TF-T-7 | F-X08-002 | Delete the `elapsed() < 500ms` assertion in `gui-core/src/jobs.rs:117`. The adjacent `try_recv == Empty` already proves non-blocking dispatch. | S/Low |
| TF-T-8 | F-X08-003 | Make `gui-core/src/visual_tokens.rs:156-170` actually assert: e.g. `MapRegionOverlay::from_condition` returns the expected variant for each input. | S/Low |
| TF-T-9 | F-X08-004 | Add `proptest` tests for `src/loading/config.rs` (round-trip `to_string` ∘ `from_str` is identity for any valid `ProjectInput`), `src/worlds_toml.rs` (same), `src/loading/presets.rs::rewrite_seed` (escapes correctly for any string seed). | M/Low |
| TF-T-10 | F-X08-005 | Add `cargo-fuzz` setup. First targets: `loading::config::parse`, `loading::presets::load`, `worlds_toml::parse`, `export::map_theme::parse_color`. | M/Low |
| TF-T-11 | F-022-008 (cli_gui_parity) | The CLI parity test currently covers only `generate`. Extend to `validate` and `analyze` first (most impact). | M/Low |
| TF-T-12 | F-X08-006 | Add inline tests for `viewer/src/app/lifecycle.rs::preview_progress` and `fraction` (pure helpers, easy wins). | S/Low |
| TF-T-13 | F-015-015 | Extend `builder/src/builder/state/tests.rs` to round-trip every `BuilderCommand` variant through `state.run`. Currently only `AddSystem` is end-to-end. | M/Low |

---

## §3 Structural refactors (sequence carefully)

### TF-S-1 — Builder command-bus retrofit (L; the single biggest piece of debt)

**Source findings**: F-015-001/002/003, F-016-001..006, F-017-001..005, F-018-010 (~50 sites).

**Goal**: Every mutation to `BuilderState.sector` / `data_catalogs` / chronicle / presence / claim / faction-roster / relations-override / economy-override goes through `state.run(BuilderCommand::...)`. The bus already has clean apply/revert/undo.

**Plan (in this order)**:

1. **Visibility crackdown.** Change `BuilderState`'s ~30 `pub` fields to `pub(crate)`. Add typed read accessors (`pub fn sector(&self) -> &SectorModel`) and forbid `&mut` access from outside the bus. Compile will break loudly across every panel — surface the call sites in one PR.
2. **Mint missing `BuilderCommand` variants.**
   - `EditChronicleEvent { event_id, before, after }`
   - `EditPresence { system_id, world_id, faction_id, before, after }`
   - `EditClaim { world_id, claim_idx, before, after }`
   - `EditFactionRoster { faction_id, before, after }`
   - `EditRelationOverride { faction_a, faction_b, before, after }`
   - `EditEconomyOverride { system_id, resource, before, after }`
   - Variants for the 19 inspector-panel field edits (system identity / star / tags / notes / control).
3. **Rewire panels.** Each direct write becomes a `state.run(BuilderCommand::EditXyz { ... })`. Tooling: a one-shot grep for `state.sector.` / `state.data_catalogs.` over `builder/src/builder/panels/` will surface every offender.
4. **Fix `ReplaceRoutes` undo.** Every site (`routes.rs:401,503,902,910,962`) currently passes `before: Vec::new()`. Capture the previous routes from `state.sector.routes.clone()` before dispatch.
5. **Fix post-dispatch patches.** `context_menu.rs:321-329`, `system_map.rs:300-315, 347-353`, `system.rs:656-668` dispatch then `iter_mut()` to patch. Each patch becomes its own dispatched command (or, where it's a follow-up edit, a second command in the same frame).
6. **Carve out non-undoable transient state.** Document explicitly that fields like `drag_system`, `rect_select`, `scroll_target`, `partial_regen_anchor`, `sector_context_menu` are *intentionally* direct-write because they aren't undoable. Add a `pub(crate) struct TransientUiState { ... }` with `pub` fields, and document the carve-out in CLAUDE.md as an exception to R4.
7. **Tests.** Round-trip every new command variant through `state.run` with revert (TF-T-13).

Sequencing across PRs: do steps 1–2 in one PR (foundation), step 3 in one PR per panel (~6 PRs), step 4–5 in one PR each, step 6 in its own PR (documentation + struct extraction), step 7 ongoing.

**Effort: L. Risk: Medium** (the bus itself is well-tested; risk is in plumbing edits to ~50 sites and missed callers).

### TF-S-2 — Viewer/editor scope decision (M; product decision)

**Source findings**: F-021-001, F-019-003, F-021-002.

`CLAUDE.md` documents viewer as read-only, but the codebase contains a fully-featured mutator. Pick one:

**Option A — Remove the editor.** Delete `viewer/src/editor/` entirely. Delete the 400-line mutator surface in `factions_overview.rs`. Restore viewer to its documented contract. Effort: M. Risk: Low (it's dead-from-the-bus already, so no live downstream depends on it).

**Option B — Promote the editor.** Document the viewer's editor as a legitimate write surface. Either:
- Make viewer depend on `sectorforge-builder` and route through the builder's command bus.
- Extract the command bus into `sectorforge-gui-core` so both crates can use it.

Update CLAUDE.md to reflect the new contract. Effort: M-L. Risk: Medium.

Either way, also fix F-021-002 (RNG bypass) — the `rand::random()` call has no place even under option B.

### TF-S-3 — Cargo dependency hoist to `[workspace.dependencies]` (M; sequencing-sensitive)

**Source findings**: F-X06 dep hygiene cluster.

**Plan**:
1. Add `[workspace.dependencies]` section to root `Cargo.toml` with all 10 shared deps (`eframe`, `egui`, `image`, `camino`, `clap`, `serde`, `serde_json`, `toml`, `thiserror`, `rfd`, `rand`, `tempfile`) at their current versions and feature flags.
2. Per-crate `Cargo.toml` — replace each `name = { version=... }` with `name.workspace = true`. Per-crate feature additions can use `name = { workspace = true, features = [...] }`.
3. `cargo build --all-targets` after each crate (do not bulk).
4. Hoist `[workspace.lints]` similarly (per QW-C-4).

Effort: M. Risk: Low (mechanical).

### TF-S-4 — Decouple bin-crate panel `pub` surfaces (L; depends on TF-S-1)

**Source findings**: F-X02-003.

After the command-bus retrofit lands, `pub fn show(ui, state)` in panels can become `pub(crate)` because external consumers (the bin's `app.rs`) live in the same crate. Same logic for ~350 over-exposed items.

This is mechanical once the command-bus PR is settled. Effort: M. Risk: Low.

### TF-S-5 — Decide influence-field storage shape (M; perf-driven)

**Source finding**: F-011-005.

`influence_field::build` keeps a dense `cell_scores: Vec<f32>` of size `total × faction_count`. On a 200×200 × 30 factions sector that's ~19 MB mostly zero. Two paths:

- **Sparse**: `BTreeMap<(usize, usize), f32>` keyed by `(cell_index, faction_index)`. Wins on memory; lookup slower.
- **Hybrid**: keep dense per `(cell, faction)` slice but only allocate slices for factions that produced any non-zero anchor.

Pick one after profiling with a representative dataset. Effort: M. Risk: Medium (renderer golden tests gate it).

---

## §4 Follow-ups (preventive / observability — not findings, but recommended)

| # | Item | Why | Effort |
|---|---|---|---|
| FU-1 | Install `cargo-nextest` and `cargo-llvm-cov` in CI. Adopt `cargo nextest run --workspace --no-fail-fast` as the test command. | Per-test timing surfaces F-022's slowest-tests table without manual estimation. Coverage report seeds future test-gap findings. | S |
| FU-2 | Add `cargo audit` to CI with `.cargo/audit.toml` ignoring RUSTSEC-2024-0436 (unmaintained `paste` via wgpu-hal). | Catch future CVEs early. | S |
| FU-3 | Add a `tests/it/cli_subcommand_help.rs` that snapshots `--help` for every subcommand. | Surfaces accidental clap drift cheaply. | S |
| FU-4 | Add `cargo deny` for license + version policy. | Even though deps are all MIT/Apache-2.0 today, this is cheap to maintain. | S |
| FU-5 | Add a `tools/check-pubapi.sh` running `cargo public-api --simplified` against `main` to catch SemVer breaks in PRs. | Pairs with the `#[non_exhaustive]` push. | S |
| FU-6 | Add a `clippy.toml` at workspace root with `disallowed-types = [{path = "std::collections::HashMap", reason = "Use FxHashMap or BTreeMap; HashMap iteration is non-deterministic"}]` (refined to allow lookup-only sites). | Mechanical determinism enforcement; would catch F-011-001 statically. | S |
| FU-7 | Extend `CLAUDE.md` with the carve-outs for transient UI state (per TF-S-1 step 6) and the editor-scope decision (per TF-S-2). | Prevents the next reviewer from re-flagging these as violations. | S |
| FU-8 | Build a small CI job that runs `cargo run -p sectorforge -- generate --seed 'malicious"\"\\' ...` to catch the F-005-001-class TOML injection regressions. | Cheap fuzz-of-one for the highest-value path. | S |
| FU-9 | Add criterion benches for: `briefing::apply` (TF-P-7), `run_search` (TF-P-2), `influence_field::build` (TF-S-5), full sector render PNG path. | Validates the perf claims in §2.3 before they're called done. | M |

---

## Sequencing summary (recommended order)

1. **Week 1**: §1.1 (determinism), §1.2 (panics), §1.3 (security), §1.4 (`#[non_exhaustive]` blanket). All quick wins, very high value-per-effort. Optionally also QW-C-7 (`#![forbid(unsafe_code)]`).
2. **Week 2**: §1.5 (cargo hygiene), §1.6 (small bug fixes), TF-API-4 (`as_slug` rollout), TF-E-1 + TF-E-2 (load-boundary errors).
3. **Week 3-4**: TF-S-1 stages 1-2 (command-bus foundation), TF-S-2 (viewer/editor scope decision), TF-P-2 (rayon short-circuit), TF-P-1 (Arc-hoist ProjectInput).
4. **Week 5-6**: TF-S-1 stages 3-5 (command-bus rewire across panels), TF-S-3 (Cargo workspace deps).
5. **Week 7-8**: TF-P-3..12 (perf cleanups), TF-T-1..13 (test-suite improvements), TF-E-3..8 (error-model finish), TF-NT-1..3 (newtype rollout).
6. **Backlog**: TF-S-4 (panel visibility post-bus), TF-S-5 (influence-field storage), TF-API-1..3 (full API narrowing).
7. **Continuous**: FU-1..9 (CI / observability).

Once §1 is applied, the workspace's health-score should rise from **3.1** to ~**3.6**. After §3 lands, ~**4.2**.

---

*End of RUST_FIXES.md. Cross-ref `FINDINGS.MD` for theme rationale and `reviews/**/*.review.md` for per-finding evidence.*
