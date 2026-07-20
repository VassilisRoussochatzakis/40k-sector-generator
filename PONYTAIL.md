# PONYTAIL.md — over-engineering audit (suggestions, nothing applied)

Repo-wide ponytail audit, 2026-07-19. Method: 8 parallel area auditors (src core, src cli/export, builder panels, builder rest, viewer, gui-core, tests/benches/fuzz, manifests/CI) + 4 independent verifier agents re-checking every load-bearing "dead / never constructed / byte-equal" claim.

**Legend:** `delete` dead code · `stdlib` std ships it · `native` an existing dep/crate layer ships it · `yagni` unused flexibility · `shrink` same logic, fewer lines.
✓ = claim independently re-verified against the baseline commit. ⚠g = golden/map-snapshot-sensitive (see instructions).

## How to apply (instructions for a subsequent run)

- **Baseline:** line numbers were correct at the baseline commit (see git history for when this file was added); they drift — re-locate by symbol, not line.
- Work top-down or by file cluster; tick checkboxes as items land. Dispatch one subagent per independent area (CLAUDE.md routing); after each batch: `cargo check --workspace`, then tests via `test-runner`.
- Re-verify a finding's premise before cutting (grep the symbol across `src builder viewer gui-core tests benches fuzz`). ✓ items were double-checked by a second agent; unmarked items were verified once by the finding agent.
- **⚠g items:** after applying, run `cargo test --test it -- golden` **without** any `UPDATE_*` env vars and require zero diffs — these cuts must be byte-stable, not re-pinned. #12 additionally needs the gui-core map-snapshot suite run unblessed.
- **CLAUDE.md determinism invariants apply to every edit:** BTree ordering for anything reaching output, all RNG through the stage-keyed RNG, builder document mutations through the command bus.
- #19: keep the `"seg-golden"`/`"stitch-golden"` strings byte-exact — fixture-content changes churn the m42 goldens via `input_digests`.
- #59 (`log`+`env_logger`): do **not** bundle silently — a prior review kept these deliberately; cutting loses `RUST_LOG` filtering. Needs an explicit decision.
- Update GUIDE.md for any cut that removes something it documents. Commit per batch directly on main (project convention for review execution).
- **Second-pass hunting grounds** (passed mechanical scans only, not line-read): detail bodies of the missions/sites/personae/hooks/prose panels, `map/interactions.rs`, world/system sub-editors; the ×7 per-catalog `show_save_row`/`on_catalog_edited`/`after_catalog_commit` family and `faction_combo` ×5 (collapsing them needs a *new* shared abstraction — decide whether it's wanted first). Re-run `/ponytail:ponytail-audit` after applying to confirm the tree comes back lean.

## Findings (ranked, biggest cut first)

- [ ] **1.** `shrink` 9 hand-written `FromStr` tables (StarColour…NotableFeature), exact inverses of `display_name()`/`code()` ✓. Replace with `Self::VARIANTS.iter().find(|v| v.display_name() == s)`. `src/worlds.rs:388-605` (~225)
- [ ] **2.** `shrink` 4 parse tables duplicating variant names ✓ (`parse_tables_cover_all_variants` proves equivalence). `VARIANTS.iter().find()` on `{v:?}`/`as_ref()`. `src/model/taxonomy.rs:35-114` (~150)
- [ ] **3.** `delete` `BuilderCommand::AddFaction`/`RemoveFaction` — only tests construct them ✓; factions edit via `EditCatalog`. Cut variants + apply/revert/dep_classes arms + their tests. `builder/src/builder/command.rs:160` (~35 + ~50 test)
- [ ] **4.** `yagni` permanently-disabled preset-gallery cluster: `CreationTarget`, `target`/`add_to_existing`/`width`/`height`, `add_enabled_ui(false)` selector ✓, never-set `presets_dir` ✓, single-variant `PresetGalleryError`→`String` ✓. Delete all; button label becomes `"CREATE PROJECT FROM THIS PRESET"`. `viewer/src/preset_gallery.rs:21-134` (~80)
- [ ] **5.** `shrink` `distance_to_segment` twin of `point_segment_distance` + drift-guard tests comparing the two ✓. Use `point_segment_distance` at the one caller (view.rs:1111). `gui-core/src/sector_view/render.rs:478` (~78)
- [ ] **6.** `shrink` `SessionFile::into_state` lists ~70 defaults duplicating `new_blank`. Build via `new_blank(...)`, overwrite the 9 loaded fields. `builder/src/builder/session.rs:85` (~70)
- [ ] **7.** `shrink` `default_app_config` byte-dup of `state/mod.rs::default_config` except `version`. Share one fn, set version at caller. `builder/src/builder/project_io.rs:44` (~60)
- [ ] **8.** `delete` `MapRegionOverlay` — 1:1 identity mirror of `RegionConditionKind`, zero uses outside gui-core ✓. Match `RegionConditionKind` directly, `_ =>` fallback in `region_color`. `gui-core/src/visual_tokens.rs:18` (~60)
- [ ] **9.** `native` 3× hand-rolled `Option<FactionId>` dropdowns. gui-core `widgets::enum_combo` (sentinel `—`). `builder/src/builder/panels/conflict.rs:557`, `surface_regions.rs:262`, `orbital.rs:376` (~55)
- [ ] **10.** `yagni` `scaffold::empty_sector`/`empty_system` duplicate `GeneratedSector::empty`/`new_at` field-for-field — world defaults have already drifted (Yellow/Standard vs White/Low: decide which is correct). Delegate one to the other. `src/model/sector_model/scaffold.rs:21` (~50)
- [ ] **11.** `yagni` `EnumPicker` trait + 7 pure-delegation impls. Pass `E::VARIANTS` + `display_name` as args like `enum_combo` does. `builder/src/builder/panels/world/mod.rs:209` (~50)
- [ ] **12.** `shrink` 3 truncate-with-dot impls (`short`, `short_upper`, `region_label_text`) + 3 test suites. One `short()`, uppercase at call sites. `gui-core/src/info_panel/mod.rs:145`, `system_view.rs:361`, `sector_view/render.rs:655` (~50) ⚠g (map snapshots)
- [ ] **13.** `stdlib` 3 `debug_name` tables + 2 mirror-enum tuple-matches. Derived `{:?}` and `as_slug()` comparison. `src/analysis/search.rs:235-407` (~45)
- [ ] **14.** `delete` `EditorState::next_system_index` + 3 tests — test-only helper tested by its own tests ✓. `viewer/src/editor/state.rs:259` (~45)
- [ ] **15.** `yagni` `SubsectorConfig` dead knobs — `ControlDenominator::AllSystems` never constructed ✓, `tracked_faction_ids` always empty ✓; the zero-row append + re-sort blocks are unreachable. Delete fields + enum + blocks. `src/export/subsectors/mod.rs:112`, `summary.rs:443` (~40)
- [ ] **16.** `yagni` 6 pure-delegation `compute_*` wrappers. Call `sectorforge::X::derive_with` directly at both call sites; keep `compute_chronicle`. `builder/src/builder/derivation_jobs.rs:118` (~40)
- [ ] **17.** `shrink` 13× identical catalog-save block. One `save_toml<T: Serialize>(root, rel, &val, digests)` beside `write_and_digest`. `builder/src/builder/project_io.rs:484` (~40)
- [ ] **18.** `yagni` `DimEdit` enum + `mirror_square` wrapping one assignment ✓. Inline `sector.height = sector.width` as dialogs.rs:123 already does. `viewer/src/editor/settings_panel.rs:8` (~35)
- [ ] **19.** `shrink` duplicate 38-line `SegmentumFile` literal. Parameterize existing `segmentum_tests::base_file` with the 3 differing strings — keep strings byte-exact. `tests/it/export_byte_goldens.rs:133` (~33)
- [ ] **20.** `shrink` `recompute_economy` hand-sums 6 ResourceVector + 10 StrategicOutput fields twice. Export `ResourceVector::fields`/`StrategicOutput::add_assign` from `src/analysis/economy/config.rs` and loop. `builder/src/builder/state/derivations.rs:593` (~30)
- [ ] **21.** `delete` `propaganda_slug`/`classified_slug`/`source_slug` re-matching model slugs (byte-equal). `.as_slug()` from the `enum_slug!` impls at `src/analysis/intel.rs:67,93,118`. `builder/src/builder/panels/intel.rs:558` (~30)
- [ ] **22.** `shrink` 5× identical sort→take→dim count blocks (world types/population/tech/government/features). One `top_counts(ui, title, map, n)`. `gui-core/src/info_panel/subsector.rs:718` (~30)
- [ ] **23.** `native` `SerializableSnapshot` field-identical mirror + 2 `From` impls. Derive Serialize/Deserialize on `Snapshot`, use it in `SessionFile`. `builder/src/builder/session.rs:38` (~26)
- [ ] **24.** `native` `world_type_color` 26-arm table duplicated in gui-core. Keep one (diff tables first); convert Rgba→Color32 from it. `src/export/system_map.rs:355` + `gui-core/src/palette.rs:809` (~25) ⚠g
- [ ] **25.** `delete` `build_world_index` — production-dead, only its own unit test calls it ✓. `src/model/sector_model/mod.rs:479` (~25)
- [ ] **26.** `shrink` `system_state_label` ×3 identical. Keep the `pub(super)` copy at `system/mod.rs:89`. `builder/src/builder/panels/control/mod.rs:125`, `history.rs:1617` (~23)
- [ ] **27.** `delete` `mono_title/section/body/dim` — zero external callers ✓ (only info_panel wrappers + ui_kit showcase). Inline into the wrappers, update showcase. `gui-core/src/ui_kit.rs:164` (~22)
- [ ] **28.** `shrink` `commit_new_project` 4× `iterative_gen.as_ref().expect().X.clone()` blocks. One destructuring borrow. `builder/src/builder/state/iterative_gen.rs:771` (~20)
- [ ] **29.** `yagni` `OverrideEnum` trait + 2 impls forwarding inherent `label`/`as_slug`. Pass variants+fns to `override_combo`, or gui-core `enum_combo`. `builder/src/builder/panels/relations.rs:655` (~20)
- [ ] **30.** `delete` 7 dead `_sector`/`_systems` params + call-site args. `src/export/writers.rs:339`, `render.rs:756`, `subsectors/mod.rs:423`, `subsectors/summary.rs:319,516,560`, `render_core/labels.rs:25` (~20)
- [ ] **31.** `delete` CI `fuzz-build` gates nothing (`continue-on-error: true` ✓) yet pays an uncached `cargo install cargo-fuzz` every run. Drop the flag so it gates, or drop the job. `.github/workflows/ci.yml:71` (~19)
- [ ] **32.** `yagni` `DerivationJobResult::Failed` — `#[allow(dead_code)]`, "reserved for future fallible derivations", never constructed. Return `DerivationPayload` directly. `builder/src/builder/derivation_jobs.rs:52` (~18)
- [ ] **33.** `delete` `load_session` — zero callers ✓; no UI opens `.sgforge`. Keep `save_session` (smoke test). `builder/src/builder/session.rs:176` (~17)
- [ ] **34.** `native` `stability_label` ×2 emitting exactly `RouteStability::as_slug()`. Call it. `builder/src/builder/panels/routes.rs:536`, `map/context_menu/render.rs:356` (~17)
- [ ] **35.** `native` `population_rank`/`tech_rank` match on `to_string()` (alloc in Lloyd hot loop) + verbatim twins. Match variants directly, keep one copy. `src/export/subsectors/summary.rs:821` + `src/analysis/history/subsectors.rs:176` (~15) ⚠g
- [ ] **36.** `shrink` `world_history`/`system_history` near-identical bodies. One helper (filter, title, cap). `gui-core/src/info_panel/history.rs:800` (~15)
- [ ] **37.** `shrink` panic-downcast chain ×5 (diagnostics + 3 tests re-copy jobs.rs `panic_message`). One `pub(crate)` helper. `gui-core/src/diagnostics.rs:61`, `jobs.rs:133` (~15)
- [ ] **38.** `delete` `generate_random_sector_default` — zero callers ✓. `src/gen/random_sector.rs:707` (~14)
- [ ] **39.** `delete` `load_regions_file` + its lib.rs re-export — zero callers ✓. `src/gen/regions.rs:260`, `src/lib.rs:167` (~14)
- [ ] **40.** `delete` `PresenceStats.subfactions`/`forces` + their population block — written, never read ✓ (display reads `GeneratedFaction` fields; adjudicated by direct read). `viewer/src/factions_overview.rs:36,682-691` (~14)
- [ ] **41.** `delete` `write_interactive_html` lib wrapper — zero callers ✓ (CLI/builder use `export_all`/`html_export`). `src/lib.rs:565` (~13)
- [ ] **42.** `stdlib` `update_bounds` four-accumulator min/max scan. `hex_cells.iter().map(..).min()/.max()`. `src/export/subsectors/mod.rs:439` (~13)
- [ ] **43.** `delete` `hex_pick` self-described legacy shim, 2 callers ✓. `SectorGeom { origin, ..g }.pick_hex(...)`. `gui-core/src/sector_view/render.rs:345` (~13)
- [ ] **44.** `native` PlaceSystem kind picker round-trips `{kind:?}` through a string array + reverse match. `enum_combo` over a const `SystemKind` slice, as `world_panel.rs:240` does. `viewer/src/editor/dialogs.rs:246` (~13)
- [ ] **45.** `yagni` `route_type_str`/`route_type_from_str`/`route_stab_str` delegation wrappers + tests. Core `as_slug`/`from_key` directly (slugs byte-identical). `viewer/src/editor/routes_panel.rs:149` (~13)
- [ ] **46.** `shrink` 2 hand-rolled "no sector loaded" panels. Existing `require_sector` (5 views already use it). `viewer/src/app/analytics_views.rs:8,56` (~13)
- [ ] **47.** `stdlib` `parse_system_state` 17-line match. `SYSTEM_STATES.iter().find(|s| s.as_slug() == key)`. `builder/src/builder/panels/history.rs:1599` (~13)
- [ ] **48.** `shrink` `humanize_slug` dup of `system::pretty_slug` (pub(super), reachable). Call the shared one. `builder/src/builder/panels/map/theme.rs:53` (~13)
- [ ] **49.** `yagni` `load_generation_rows`→`into_legacy_tuple` one-caller chain. Have `world_pool::inspect_workbook` call `load_worlds_data`, take `(tables, rows)`. `src/worlds.rs:408`, `src/worlds_toml.rs:191` (~12)
- [ ] **50.** `shrink` `factions_visible` verbatim in both legend backends. Hoist to `render_core::labels` beside `system_label_visible`. `src/export/bitmap/legend.rs:66`, `svg_export/legend.rs:57` (~12) ⚠g
- [ ] **51.** `shrink` `TickLogEntry` 10-field literal ×2. Local `entry(idx, scope, before, after)` ctor. `builder/src/builder/state/derivations.rs:1105` (~12)
- [ ] **52.** `delete` `system_state_key` reimplementing `SystemState::as_slug` byte-equal. `.as_slug()` at history.rs:462,467. `builder/src/builder/panels/history.rs:1630` (~11)
- [ ] **53.** `native` local `kv` dup of exported `ui_kit::kv` (builder already uses it). `viewer/src/segmentum_view.rs:797` (~10)
- [ ] **54.** `native` `star_colour_name` — 7 strings byte-identical to `StarColour::short_name()` ✓. `parse::<StarColour>().map(short_name)`. `viewer/src/editor/enums.rs:11` (~10)
- [ ] **55.** `yagni` `text_edit<T: AsRef<str>+From<String>>` + per-frame String round-trip — all 14 callers pass `String`. Take `&mut String`. `viewer/src/factions_overview.rs:964` (~10)
- [ ] **56.** `stdlib` 5× `partial_cmp().unwrap_or(Equal)` → `total_cmp` (HeapNode's comment already claims it). `viewer/src/route_planner.rs:243`, `app/trade_view.rs:110`, `factions_overview.rs:664`, `editor/factions_panel.rs:126` (~10)
- [ ] **57.** `delete` 6 discarded `let _ = derive_*` bench setup calls (install nothing; the "fully enriched" comment is false). `benches/generation.rs:178` (~10)
- [ ] **58.** `shrink` 8 hand-rebuilt fixture/bin paths. Existing `shared::fixture_dir()`/`shared::bin()`. `tests/it/validation_tests.rs:105`, `cli_gui_parity.rs:24`, `golden_png.rs:23`, `cli_behavior.rs:27` (~10)
- [ ] **59.** `native` `log`+`env_logger` for 8 macro sites in 3 CLI files — **borderline: kept deliberately by a prior review ("Finding #29"); cutting loses RUST_LOG**. Verbosity flag + `eprintln!` if that's acceptable. `Cargo.toml:38`, `src/main.rs:37` (~10, −2 deps)
- [ ] **60.** `delete` `RouteStability::pattern_key` byte-dup of `as_slug()`, one caller ✓. `self.stability.as_slug()`. `src/model/sector_model/routes_view.rs:215` (~9) ⚠g (route-pattern hash)
- [ ] **61.** `delete` `export_json` — zero callers ✓. `src/export/writers.rs:360` (~9)
- [ ] **62.** `delete` `sector_overview` pub wrapper — zero external callers ✓ (confirm the internal overview.rs:77 call when applying; the same-named fn in `src/analysis/prose.rs` is unrelated and live). `gui-core/src/info_panel/overview.rs:71` (~9)
- [ ] **63.** `yagni` `draw_faction_chip_sized` — sole caller passes constants (20×20, 3.0, 13.0). Merge, drop params. `gui-core/src/palette.rs:917` (~9)
- [ ] **64.** `shrink` `ensure_connected` copy-out/write-back dance. Bind `ui.checkbox(&mut input.config.generation.routes.ensure_connected_graph, ..)` like the sibling checkboxes. `viewer/src/editor/generation_panel.rs:211` (~9)
- [ ] **65.** `delete` render-surface sweep: svg's dup `star_radius_ratio` (import `render_core/routes.rs:24`), bitmap's `#[allow(unused_imports)]` re-exports, `_unused(WorldDto)` stub. `src/export/svg_export/mod.rs:46`, `bitmap/mod.rs:39`, `subsectors/mod.rs:817` (~8)
- [ ] **66.** `yagni` `write_sector_png_to` — sole caller is its own parity test. Test calls `write_sector_png_to_with(.., RenderOptions::default())`. `src/export/bitmap/mod.rs:83` (~8)
- [ ] **67.** `shrink` `faction_name` byte-identical ×2. One `pub(crate)` copy. `builder/src/builder/panels/surface_regions.rs:293`, `orbital.rs:344` (~8)
- [ ] **68.** `yagni` `[profile.bench] inherits = "release"` restates the cargo default. Delete section. `Cargo.toml:141` (~7)
- [ ] **69.** `yagni` `reading_column` `max_w` — 5/5 callers pass 720.0. Hardcode, drop param. `gui-core/src/ui_kit.rs:141` (~6)
- [ ] **70.** `yagni` `[lib] path` / `[[bin]]` / `default = []` restate auto-discovery (bin name stays `sectorforge` via the package name). Delete. `Cargo.toml:51,89` (~6)
- [ ] **71.** `delete` `SPACE_XL` (zero uses ✓) + `SystemGeom` over-pub visibility. `gui-core/src/design.rs:46`, `system_view.rs:280` (~5)
- [ ] **72.** `delete` `define_id!`'s `into_string` — zero callers ✓; `From<$name> for String` exists. `src/model/ids.rs:47` (~5)
- [ ] **73.** `delete` `get_system_mut` — zero callers ✓; mutations go through mutation.rs. `src/model/sector_model/mod.rs:439` (~5)
- [ ] **74.** `delete` `BuilderIndex.factions` — rebuilt on every mutation, never read ✓. Drop field + rebuild loop. `builder/src/builder/index.rs:15` (~5)
- [ ] **75.** `delete` stale first-person TODO musings in the special-location diamond branch. Keep code, cut comments. `src/export/system_map.rs:153` (~5)
- [ ] **76.** `delete` builder dev-dependency `sectorforge` duplicating its `[dependencies]` entry. `builder/Cargo.toml:30` (~4)
- [ ] **77.** `yagni` empty `[profile.dev]` section. Move comment onto `[profile.dev.package."*"]`. `Cargo.toml:125` (~4)
- [ ] **78.** `delete` CI "Golden output tests" step — strict subset of the `--workspace` run two lines up (ignored segmentum goldens run in the `--ignored` job ✓). `.github/workflows/ci.yml:35` (~3)
- [ ] **79.** `yagni` single-member workspace pins (`rand`, `log`/`env_logger` if kept). Inline like `rand_chacha`. `Cargo.toml:31` (~1)
- [ ] **80.** `shrink` CI: gate `push` to `branches: [main]` (workflow runs twice per PR commit ✓) and `integration-ignored` → `-- --ignored` instead of `--include-ignored`. `.github/workflows/ci.yml:3,56` (~0, wall-clock only)

**net: -2,050 lines (~85 test code), -2 deps possible** (the 2 deps are borderline #59).
