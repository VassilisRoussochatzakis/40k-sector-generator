# File-by-file map

Detailed reference for the workspace source tree. Externalized from
[CLAUDE.md](../CLAUDE.md) so the top-level doc stays small enough to load
on every turn. Reach for this file when you need the exact home of a
specific function/type, not for routine exploration — `rust-explorer` is
faster.

## Parent-module layout (REFACTOR_PART2.md Task 2)

The library crate splits into six parent modules:

- [src/model/](../src/model/) — data DTOs + IDs + RNG + taxonomy + errors
- [src/loading/](../src/loading/) — project / config / presets / sector_save
- [src/gen/](../src/gen/) — generation pipeline + supporting tables
- [src/analysis/](../src/analysis/) — pure derivations over a built sector
- [src/export/](../src/export/) — output writers + render backends
- [src/validate/](../src/validate/) — pre/post-generation validation + diff
- [src/cli/](../src/cli/) — binary command dispatcher

[src/worlds.rs](../src/worlds.rs) and [src/worlds_toml.rs](../src/worlds_toml.rs)
stay at the crate root as the foundational world taxonomy. [src/lib.rs](../src/lib.rs)
re-aliases every moved module back to its old short path (`pub use parent::foo;`)
so downstream crates and existing `crate::foo::Item` paths see no change.

## sectorforge library

| File | Purpose |
|---|---|
| [src/worlds.rs](../src/worlds.rs) | Canonical world enums (incl. `VARIANTS`/`display_name`) |
| [src/worlds_toml.rs](../src/worlds_toml.rs) | §45 native typed `worlds.toml` config (sole world-data format) |
| [src/lib.rs](../src/lib.rs) | Public API surface (doc-tests + `# Errors`); re-aliases moved modules back to root |
| [src/main.rs](../src/main.rs) | `sectorforge` binary entry: parses `cli::Cli`, dispatches to `cli::run`, maps errors to exit 2 |

### model/

| File | Purpose |
|---|---|
| [src/model/sector_model/mod.rs](../src/model/sector_model/mod.rs) | Output DTOs with Serialize/Deserialize |
| [src/model/sector_model/mutation.rs](../src/model/sector_model/mutation.rs) | `swap_systems` + in-place mutators used by the builder command bus |
| [src/model/ids.rs](../src/model/ids.rs) | Typed `SystemId` / `WorldId` / `RouteId` / `FactionId` |
| [src/model/errors.rs](../src/model/errors.rs) | Top-level `SectorError` (thiserror) |
| [src/model/rng.rs](../src/model/rng.rs) | Deterministic stage RNG (blake3-keyed ChaCha8Rng) |
| [src/model/taxonomy.rs](../src/model/taxonomy.rs) | Variant-name ↔ enum bridge |

### loading/

| File | Purpose |
|---|---|
| [src/loading/input.rs](../src/loading/input.rs) | `ProjectInput`, `load_project` |
| [src/loading/config.rs](../src/loading/config.rs) | `sectorforge.toml` schema |
| [src/loading/presets.rs](../src/loading/presets.rs) | Preset library resolution |
| [src/loading/sector_save.rs](../src/loading/sector_save.rs) | §13 IDs-only runtime save format |

### gen/

| File | Purpose |
|---|---|
| [src/gen/generation/mod.rs](../src/gen/generation/mod.rs) | Sector generation facade: `SectorProgress`, `generate*` orchestrator, manifest build |
| [src/gen/generation/placement.rs](../src/gen/generation/placement.rs) | Hex grid placement with min-distance relaxation |
| [src/gen/generation/systems.rs](../src/gen/generation/systems.rs) | Per-system build: `build_system*`, star colour, system naming, spectral fallback |
| [src/gen/generation/world_placement.rs](../src/gen/generation/world_placement.rs) | Per-world build: candidate pick, features, naming, tags, `regenerate_world_payload` |
| [src/gen/generation/factions.rs](../src/gen/generation/factions.rs) | Faction assignment + `aggregate_factions` + `assign_factions_for_systems` |
| [src/gen/generation/routes.rs](../src/gen/generation/routes.rs) | Public route graph + `classify_route` + union-find connector |
| [src/gen/world_pool.rs](../src/gen/world_pool.rs) | GenerationRow → weighted candidate pool (+ authored-feature overlay) |
| [src/gen/world_ecs.rs](../src/gen/world_ecs.rs) | Lazy ECS projection of a built sector |
| [src/gen/archetypes.rs](../src/gen/archetypes.rs) | Archetype activations (necron/tyranid/ork/GSC/tau/aeldari/chaos) |
| [src/gen/faction_style.rs](../src/gen/faction_style.rs) | Per-faction glyph + colour palette |
| [src/gen/factions.rs](../src/gen/factions.rs) | Faction config + presence math |
| [src/gen/routes.rs](../src/gen/routes.rs) | Route generation primitives |
| [src/gen/regions.rs](../src/gen/regions.rs) | §5 regional warp phenomena overlay: seeded blob growth + route effects |
| [src/gen/sites.rs](../src/gen/sites.rs) | Authored planetary sites |
| [src/gen/hidden_routes.rs](../src/gen/hidden_routes.rs) | §11 concealed-route catalogue + reveal rules |
| [src/gen/orbital_assets.rs](../src/gen/orbital_assets.rs) | §2 NEXT orbital + blockade structures |
| [src/gen/surface_region.rs](../src/gen/surface_region.rs) | §1 NEXT named on-world surface regions |
| [src/gen/names.rs](../src/gen/names.rs) | Name dictionaries |

### analysis/

| File | Purpose |
|---|---|
| [src/analysis/analytics.rs](../src/analysis/analytics.rs) | §8 read-only sector analytics dashboard |
| [src/analysis/conflict.rs](../src/analysis/conflict.rs) | §5 NEXT conflict + tick advance |
| [src/analysis/control.rs](../src/analysis/control.rs) | Multi-dim presence / claim / control / power derivation |
| [src/analysis/influence_field.rs](../src/analysis/influence_field.rs) | Per-faction influence diffusion |
| [src/analysis/importance.rs](../src/analysis/importance.rs) | Per-system/per-world importance scoring |
| [src/analysis/interestingness.rs](../src/analysis/interestingness.rs) | §18 NEW2 interestingness scorecard |
| [src/analysis/power_projection.rs](../src/analysis/power_projection.rs) | Faction power-projection model |
| [src/analysis/route_control.rs](../src/analysis/route_control.rs) | Per-route control derivation |
| [src/analysis/stability.rs](../src/analysis/stability.rs) | System stability state machine |
| [src/analysis/intel.rs](../src/analysis/intel.rs) | §7 NEXT fog-of-war record |
| [src/analysis/missions.rs](../src/analysis/missions.rs) | §3 NEW2 mission seeds |
| [src/analysis/briefing.rs](../src/analysis/briefing.rs) | §9 NEW2 redacted briefing packs |
| [src/analysis/prose.rs](../src/analysis/prose.rs) | §6 gazetteer prose: deterministic template grammar |
| [src/analysis/personae.rs](../src/analysis/personae.rs) | §3 dramatis personae: named characters per faction presence |
| [src/analysis/hooks.rs](../src/analysis/hooks.rs) | §7 plot-hook generator: condition→template over model state |
| [src/analysis/relations.rs](../src/analysis/relations.rs) | §4 inter-faction diplomacy: stance matrix + tension scalar |
| [src/analysis/economy.rs](../src/analysis/economy.rs) | §12 trade & resource economy: production/consumption + route volume |
| [src/analysis/search.rs](../src/analysis/search.rs) | §2 seed search: declarative wishes → deterministic seed enumeration |
| [src/analysis/history/mod.rs](../src/analysis/history/mod.rs) | §1 chronicle facade: `derive*`, orchestration, `anchor_key`, `pub use` surface |
| [src/analysis/history/config.rs](../src/analysis/history/config.rs) | `HistoryConfig`/`HistoryFile`/`HistoryEra`/`HistoryEventRule` + defaults |
| [src/analysis/history/model.rs](../src/analysis/history/model.rs) | DTOs: `HistoryReport`, `HistoryEvent`, `HistoryAnchor`, `EventKind`, entity/consequence types |
| [src/analysis/history/context.rs](../src/analysis/history/context.rs) | `EmitContext` borrowed state threaded into emit families |
| [src/analysis/history/progress.rs](../src/analysis/history/progress.rs) | `HistoryProgress` enum + progress-stride helpers |
| [src/analysis/history/build.rs](../src/analysis/history/build.rs) | Event construction: `build_event`, date/era synthesis, entity refs, consequences |
| [src/analysis/history/worlds.rs](../src/analysis/history/worlds.rs) | Per-world emission: foundation, claims, contested control, hidden cults, purges |
| [src/analysis/history/systems.rs](../src/analysis/history/systems.rs) | Per-system emission: system state + archetype activations |
| [src/analysis/history/routes.rs](../src/analysis/history/routes.rs) | Per-route emission: warp hazards, concealed passages, pirate/interdictor control |
| [src/analysis/history/subsectors.rs](../src/analysis/history/subsectors.rs) | Per-subsector emission: clustering vs deterministic sampling |
| [src/analysis/history/regions.rs](../src/analysis/history/regions.rs) | Per-region emission: warp phenomena → chronicle entries |
| [src/analysis/history/rules.rs](../src/analysis/history/rules.rs) | Declarative `[[event_rules]]` enforcement + `EventKind` aliases |
| [src/analysis/history/labels.rs](../src/analysis/history/labels.rs) | Small string helpers: `kind_slug`, `article_phrase`, `gsc_stage_label`, `tau_band_label` |
| [src/analysis/history/markdown.rs](../src/analysis/history/markdown.rs) | `render_markdown` + `write_report` (history.md/json) |
| [src/analysis/history/tests.rs](../src/analysis/history/tests.rs) | Determinism + smoke tests for the chronicle pipeline |

### export/

| File | Purpose |
|---|---|
| [src/export/writers.rs](../src/export/writers.rs) | JSON / Markdown / manifest / bitmap writers (was `src/export.rs` pre-Task 2; renamed to free the parent name, re-hoisted via `pub use writers::*`) |
| [src/export/render.rs](../src/export/render.rs) | Markdown rendering (sector + standalone system) |
| [src/export/heatmap.rs](../src/export/heatmap.rs) | Heatmap colour scale |
| [src/export/map_theme.rs](../src/export/map_theme.rs) | `MapTheme` + `MapThemeConfig` + built-in palettes |
| [src/export/system_map.rs](../src/export/system_map.rs) | Per-system bitmap render |
| [src/export/segmentum.rs](../src/export/segmentum.rs) | §14 multi-sector composition: child loader + deterministic stitch + super-manifest |
| [src/export/html_export.rs](../src/export/html_export.rs) | §11 self-contained interactive HTML map (inlined JSON + JS canvas renderer + theme CSS) |
| [src/export/subsectors/mod.rs](../src/export/subsectors/mod.rs) | Subsector clustering + public API |
| [src/export/subsectors/summary.rs](../src/export/subsectors/summary.rs) | Ownership, faction control, capital selection |
| [src/export/render_core/mod.rs](../src/export/render_core/mod.rs) | Pass B of REFACTOR_PART2 Task 3: shared color helpers + `RenderOptions` used by both PNG + SVG backends. Pass C/D (Canvas trait + shared draw_* functions) deferred. |
| [src/export/render_core/colors.rs](../src/export/render_core/colors.rs) | `star_color`, `stability_color`, `tint_against`, `darken`, `dim`, `short`, `rgba`, `route_thickness_f32` — single source of truth, both backends import from here |
| [src/export/render_core/options.rs](../src/export/render_core/options.rs) | Cross-backend `RenderOptions` (formerly in bitmap; re-exported there + at lib.rs for compat) |
| [src/export/bitmap/mod.rs](../src/export/bitmap/mod.rs) | Sector PNG facade: `write_bitmap*`, `render_sector_image`, `encode_png_bytes`, top-level `render()` orchestrator. `RenderOptions` is now a re-export of `super::render_core::RenderOptions`. |
| [src/export/bitmap/primitives.rs](../src/export/bitmap/primitives.rs) | Pixel primitives + 5×7 font (shared w/ system_map) |
| [src/export/bitmap/geom.rs](../src/export/bitmap/geom.rs) | `Geom`, `MapBounds`, hex centre/vertices, `Rect` collision type |
| [src/export/bitmap/colors.rs](../src/export/bitmap/colors.rs) | `i32`-quantised wrappers (`route_thickness`, `stroke_px`); re-exports the shared helpers from `render_core::colors` |
| [src/export/bitmap/grid.rs](../src/export/bitmap/grid.rs) | Hex grid fill + per-system/region tint computation |
| [src/export/bitmap/routes.rs](../src/export/bitmap/routes.rs) | Route line drawing: stride/jagged/zigzag/disc/chevron/etc. patterns + route control glyphs |
| [src/export/bitmap/regions.rs](../src/export/bitmap/regions.rs) | §5 warp region label overlay |
| [src/export/bitmap/systems.rs](../src/export/bitmap/systems.rs) | Star disks, pip text, capital markers |
| [src/export/bitmap/labels.rs](../src/export/bitmap/labels.rs) | System labels, subsector borders, subsector label placement |
| [src/export/bitmap/legend.rs](../src/export/bitmap/legend.rs) | Right-hand legend (title, route key, factions, heatmap chip) |
| [src/export/bitmap/tests.rs](../src/export/bitmap/tests.rs) | Unit tests for the bitmap facade |
| [src/export/svg_export/mod.rs](../src/export/svg_export/mod.rs) | SVG export facade: `render_sector_svg`, `write_sector_svg_to*`, top-level orchestrator + shared `HEX_SIZE` / `star_radius_ratio` |
| [src/export/svg_export/primitives.rs](../src/export/svg_export/primitives.rs) | Low-level SVG emitters: `<rect>`/`<circle>`/`<polygon>`/`<line>`/`<text>` + XML escape |
| [src/export/svg_export/colors.rs](../src/export/svg_export/colors.rs) | `f32`-native wrappers (`route_thickness`, `stroke_px`); re-exports the shared helpers from `render_core::colors` |
| [src/export/svg_export/geom.rs](../src/export/svg_export/geom.rs) | `MapBounds`, `map_bounds`, `hex_center`, `hex_vertices` |
| [src/export/svg_export/grid.rs](../src/export/svg_export/grid.rs) | Hex grid fill + subsector borders + per-system/region tints |
| [src/export/svg_export/routes.rs](../src/export/svg_export/routes.rs) | Route line patterns + route-control glyphs (exports `draw_route_pattern`, `ControlKind` for legend) |
| [src/export/svg_export/regions.rs](../src/export/svg_export/regions.rs) | §5 warp-region label overlay |
| [src/export/svg_export/systems.rs](../src/export/svg_export/systems.rs) | Star disks, capital markers, world-count pips |
| [src/export/svg_export/labels.rs](../src/export/svg_export/labels.rs) | System name labels + collision-aware subsector titles |
| [src/export/svg_export/legend.rs](../src/export/svg_export/legend.rs) | Right-hand legend (title, route key, factions, heatmap chip), full + compact variants |

### validate/

| File | Purpose |
|---|---|
| [src/validate/validation.rs](../src/validate/validation.rs) | Pre-generation validation |
| [src/validate/invariants.rs](../src/validate/invariants.rs) | Post-generation invariants (spec §11.11) |
| [src/validate/diff.rs](../src/validate/diff.rs) | §10 sector diff: model-aware before/after report |

### cli/

| File | Purpose |
|---|---|
| [src/cli/mod.rs](../src/cli/mod.rs) | Clap `Cli` + `Command` enum + per-variant `run` dispatcher |
| [src/cli/common.rs](../src/cli/common.rs) | Shared CLI helpers: JSON printing, validation/invariant/workbook printers, `parse_heatmap`, `load_or_regenerate`, `log_*progress` |
| [src/cli/generate.rs](../src/cli/generate.rs) | `generate` + `generate-system` runners (with §15 NEW2 constraint search wiring) |
| [src/cli/validate.rs](../src/cli/validate.rs) | `validate`, `validate-sector`, `render-markdown`, `inspect-worlds` runners |
| [src/cli/analyze.rs](../src/cli/analyze.rs) | `analyze` runner |
| [src/cli/presets.rs](../src/cli/presets.rs) | `new` + `list-presets` runners |
| [src/cli/search.rs](../src/cli/search.rs) | `search` runner |
| [src/cli/history.rs](../src/cli/history.rs) | `history` runner |
| [src/cli/personae.rs](../src/cli/personae.rs) | `personae` runner |
| [src/cli/hooks.rs](../src/cli/hooks.rs) | `hooks` runner |
| [src/cli/prose.rs](../src/cli/prose.rs) | `prose` runner |
| [src/cli/relations.rs](../src/cli/relations.rs) | `relations` runner |
| [src/cli/regions.rs](../src/cli/regions.rs) | `regions` runner |
| [src/cli/economy.rs](../src/cli/economy.rs) | `economy` runner |
| [src/cli/compose.rs](../src/cli/compose.rs) | `compose` runner |
| [src/cli/interestingness.rs](../src/cli/interestingness.rs) | `interestingness` runner + profile parser |
| [src/cli/briefing.rs](../src/cli/briefing.rs) | `briefing` runner |
| [src/cli/missions.rs](../src/cli/missions.rs) | `missions` runner |
| [src/cli/sites.rs](../src/cli/sites.rs) | `sites` runner |
| [src/cli/diff.rs](../src/cli/diff.rs) | `diff` runner + `DiffArgs` |

## sectorforge-gui-core

| File | Purpose |
|---|---|
| [gui-core/src/lib.rs](../gui-core/src/lib.rs) | Shared egui widgets/utilities used by GUI + builder |
| [gui-core/src/jobs.rs](../gui-core/src/jobs.rs) | Background job helper |
| [gui-core/src/palette.rs](../gui-core/src/palette.rs) | Color palette / faction glyph rendering |
| [gui-core/src/sector_view.rs](../gui-core/src/sector_view.rs) | Read-only hex map render. `SectorGeom::hit_route` and `SectorMapCache::region_for_hex` back the right-click route + region-hex schemas in `builder::panels::map::context_menu::resolve_sector_context`. |
| [gui-core/src/system_view.rs](../gui-core/src/system_view.rs) | System detail panel widget — also embedded under the SYSTEM tab by `panels/system.rs::show_system_map_section` (§CTX0). §CTX1 Phase 6 exports `SystemGeom`, `SystemPick`, and `pick_world` so `builder::panels::system_map::arm_system_context_menu` can hit-test star / planet / orbit-ring / background. |
| [gui-core/src/info_panel.rs](../gui-core/src/info_panel.rs) | Text formatting widgets |
| [gui-core/src/heatmap.rs](../gui-core/src/heatmap.rs) | GUI heatmap color/cache wrapper |
| [gui-core/src/nav.rs](../gui-core/src/nav.rs) | §LINK4 `entity_link` cross-tab link widget |

## sectorforge-viewer

| File | Purpose |
|---|---|
| [viewer/src/main.rs](../viewer/src/main.rs) | Viewer/editor binary entry (`sectorforge-viewer`) |
| [viewer/src/app/mod.rs](../viewer/src/app/mod.rs) | Top-level viewer/editor eframe app + navigation |
| [viewer/src/app/export_ui.rs](../viewer/src/app/export_ui.rs) | PNG/SVG/HTML/JSON export UI |
| [viewer/src/data_editor.rs](../viewer/src/data_editor.rs) | §45 typed `worlds.toml` editor (dropdowns + DragValue) |
| [viewer/src/editor/](../viewer/src/editor/) | Sector/world editing UI |

## sectorforge-builder

| File | Purpose |
|---|---|
| [builder/src/main.rs](../builder/src/main.rs) | Builder binary entry (`sectorforge-builder`) |
| [builder/src/app.rs](../builder/src/app.rs) | Thin builder eframe app host |
| [builder/src/builder/](../builder/src/builder/) | Builder state, command bus, project I/O, panels |
| [builder/src/builder/state/mod.rs](../builder/src/builder/state/mod.rs) | `BuilderState` struct (§D5) + `new_blank` + `default_config` + slice facade. Carries the transient context-menu fields (`sector_context_menu`, `partial_regen_anchor`, `pending_bulk_rename`, `pending_region_rename`, `system_context_menu`, `pending_world_rename`, `last_menu_action`) — all in-memory only, dropped by `session::SessionFile` round-trip. |
| [builder/src/builder/state/types.rs](../builder/src/builder/state/types.rs) | UI/dialog types: `BuilderTab`, `MapTool`, `ControlOverlay`, `ModalKind`, `HealthLevel`, `JobHandle`, `PartialRegenRect`, `Pending*`, `MapViewCache`, `HistoryWizardState`, `HistoryAnchorKind`, `TickLogEntry`, `TickLogScope`, `SectorContextMenu` + `SectorMenuTarget`, `SystemContextMenu` + `SystemMenuTarget`, `DEFAULT_*` |
| [builder/src/builder/state/selection.rs](../builder/src/builder/state/selection.rs) | §S1/§S4 selection helpers: `focus_system`, `toggle_system_selection` + §LINK2 `focus_entity` / `nav_back` / `nav_forward` |
| [builder/src/builder/state/nav.rs](../builder/src/builder/state/nav.rs) | §LINK1 cross-tab navigation: `EntityRef` enum + `target_tab` |
| [builder/src/builder/state/undo.rs](../builder/src/builder/state/undo.rs) | R4 command bus: `run`, `undo`, `redo`, ring-buffer trim, `snapshot`, `trigger_auto_save` |
| [builder/src/builder/state/derivations.rs](../builder/src/builder/state/derivations.rs) | Economy / relations / chronicle re-derive, debounced validation pump, `synthesize_project_input`, `health_level`, §CF4/§CF5 `advance_conflict_ticks` driver + tick-log capture |
| [builder/src/builder/state/regions_ops.rs](../builder/src/builder/state/regions_ops.rs) | §REG1..§REG3 warp-region helpers: add/remove/paint/erase/update/next id |
| [builder/src/builder/state/generation_ops.rs](../builder/src/builder/state/generation_ops.rs) | §G2..§G5 + §S5 + §W4: `generate_system_here`, `regenerate_world`, `apply_preview`, `regenerate_partial`, `reroll_seed`, `find_world_indices` |

### Builder panels — MAP tab (split per REFACTOR_PART2.md Task 1)

| File | Purpose |
|---|---|
| [builder/src/builder/panels/map/mod.rs](../builder/src/builder/panels/map/mod.rs) | §S1 + §R2 MAP tab facade. `pub fn show`, `pub fn show_toolbox`, child-module declarations, full unit-test suite. Re-exports `menu_anchor_pivot` at `pub(super)` so `panels/system_map.rs` can share the viewport-pivot helper. |
| [builder/src/builder/panels/map/interactions.rs](../builder/src/builder/panels/map/interactions.rs) | Hex render dispatcher + tool routing: `show_hex_map`, `handle_click`, `handle_drag_drop`, `apply_rect_select`, `apply_partial_regen_anchor_click`, `paint_region_at`, `add_route_between`. |
| [builder/src/builder/panels/map/context_menu.rs](../builder/src/builder/panels/map/context_menu.rs) | §CTX1 right-click surface: `SectorMenuAction` + `OpenInTarget` enums, `resolve_sector_context`, `apply_sector_menu_action`, `render_empty_hex_menu` / `render_system_menu` / `render_multi_selection_menu` / `render_route_menu` / `render_region_hex_menu`, `show_sector_context_menu`, `should_dismiss_sector_context_menu`, `sector_menu_target_is_stale`, `sector_menu_action_label`, `menu_anchor_pivot`. |
| [builder/src/builder/panels/map/dialogs.rs](../builder/src/builder/panels/map/dialogs.rs) | Transient modal dialogs surfaced from the MAP panel: `show_place_dialog`, `show_rename_dialog`, `show_bulk_rename_dialog`, `show_region_rename_dialog`, `show_collision_dialog`. |
| [builder/src/builder/panels/map/cache.rs](../builder/src/builder/panels/map/cache.rs) | `refresh_map_cache` + `sector_view_digest` — rebuilds `MapViewCache` (subsector clustering + hex→system lookup + region tints) when the sector slice digest changes. |

### Builder panels — other tabs

| File | Purpose |
|---|---|
| [builder/src/builder/panels/conflict.rs](../builder/src/builder/panels/conflict.rs) | §CF1..§CF6 conflict + stability editor: per-world / per-system `ConflictState` + `StabilityState`, advance-ticks button, tick log, conflict heatmap toggle |
| [builder/src/builder/panels/missions.rs](../builder/src/builder/panels/missions.rs) | §M1..§M5 missions tab: cached `MissionsReport` list + detail card, manual mission editor over `MissionsConfig::manual`, auto-derive + player-edition toggles, click-to-highlight `primary_location` via `focus_entity` |
| [builder/src/builder/panels/prose.rs](../builder/src/builder/panels/prose.rs) | §PR1..§PR4 prose tab: per-system + sector overview Override toggles backed by `ProseConfig::overrides`, tone preset combo (Gazetteer / Dispatch), "Regenerate prose" runs `BuilderState::recompute_prose`. Overrides survive every regenerate because `prose::derive_with` re-applies them after the deterministic derivation. |
| [builder/src/builder/panels/briefing.rs](../builder/src/builder/panels/briefing.rs) | §BR1..§BR5 briefing tab: `AudiencePreset` picker + observer-faction `ComboBox` + 0..=100 min-confidence slider build a `BriefingProfile`; "Generate briefing" calls `sectorforge::apply_briefing` + `briefing::render_markdown` and caches both `BriefingPack` and redacted Markdown on `BuilderState`. "Export .md + .json" writes the cached pack through `sectorforge::write_briefing` into a folder picked via `rfd::FileDialog`. |
| [builder/src/builder/panels/interestingness.rs](../builder/src/builder/panels/interestingness.rs) | §INT1..§INT4 interestingness tab: `ProfileId` picker (PoliticalSandbox/GrimCollapse/Mercantile/Villainous/Frontier), "Score sector" runs `sectorforge::derive_interestingness_with` and caches `InterestingnessReport` on `BuilderState`, per-metric band chart painted via `ui.painter_at` (target band shaded green, observed value ticked), and a per-profile threshold override editor backed by `BuilderState::interestingness_custom_overrides` (keyed by snake-case profile id, seeded from each profile's built-in band). |
| [builder/src/builder/panels/system.rs](../builder/src/builder/panels/system.rs) | §S2 inspector + §CTX0 `show_system_map_section` that embeds `gui_core::system_view::SystemView` at the top of the SYSTEM tab and routes its `secondary_clicked` response through `panels/system_map::arm_system_context_menu` for the §CTX2 in-system right-click menu. Hosts the `apply_bulk_*` `pub(crate)` helpers reused by the multi-selection right-click menu in `panels/map`. |
| [builder/src/builder/panels/status.rs](../builder/src/builder/panels/status.rs) | §N4 status bar: project label, dirty flag, tri-coloured §V3 health pip, command-cursor position, derivation-cache count, pending-job spinner. §CTX1 Phase 7 tails `BuilderState::last_menu_action` as `ctx_menu: <schema> :: <item>`. |
| [builder/src/builder/panels/system_map.rs](../builder/src/builder/panels/system_map.rs) | §CTX1 Phase 6 in-system right-click menu (`§6.6`..`§6.9`). Owns `SystemMenuAction` + `resolve_system_context` + `apply_system_menu_action` + `arm_system_context_menu` + `show_system_context_menu` + `show_world_rename_dialog`. Reuses `super::map::menu_anchor_pivot` to flip the menu's anchor pivot when the cursor sits on the right/bottom half of the viewport. |
| [builder/src/builder/panels/orbital.rs](../builder/src/builder/panels/orbital.rs) | §O1/§O2 orbital + blockade editor. Exposes `pub(crate) derive_and_apply_orbital_assets(state, system)` (§CTX1 Phase 6) so the in-system right-click `DERIVE ORBITAL ASSETS` row can reuse the same `SetOrbitalAssets` + `SetBlockadeReport` dispatch path. |
