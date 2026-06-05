# File-by-file map

Detailed reference for the workspace source tree. Externalized from
[CLAUDE.md](../CLAUDE.md) so the top-level doc stays small enough to load
on every turn. Reach for this file when you need the exact home of a
specific function/type, not for routine exploration — `rust-explorer` is
faster.

> Coverage note: this map aims to list every `.rs` file in the workspace.
> Per-module `mod.rs` files that only re-export children are noted at the
> head of their section rather than given their own row. If you add a file,
> add its row here (CLAUDE.md: "update GUIDE.md / docs on non-trivial
> changes").

## Parent-module layout (REFACTOR.txt Task 2)

The library crate splits into six parent modules:

- [src/model/](../src/model/) — data DTOs + IDs + RNG + taxonomy + errors
- [src/loading/](../src/loading/) — project / config / presets / sector_save
- [src/gen/](../src/gen/) — generation pipeline + supporting tables
- [src/analysis/](../src/analysis/) — pure derivations over a built sector
- [src/export/](../src/export/) — output writers + render backends
- [src/validate/](../src/validate/) — pre/post-generation validation + diff
- [src/cli/](../src/cli/) — binary command dispatcher

Each parent has a `mod.rs` that just declares + re-exports its children
(`src/model/mod.rs`, `src/loading/mod.rs`, `src/gen/mod.rs`,
`src/analysis/mod.rs`, `src/export/mod.rs`, `src/validate/mod.rs`) — not
listed individually below.

[src/worlds.rs](../src/worlds.rs) and [src/worlds_toml.rs](../src/worlds_toml.rs)
stay at the crate root as the foundational world taxonomy. [src/lib.rs](../src/lib.rs)
re-aliases every moved module back to its old short path (`pub use parent::foo;`)
so downstream crates and existing `crate::foo::Item` paths see no change.

## sectorforge library

| File | Purpose |
|---|---|
| [src/worlds.rs](../src/worlds.rs) | Canonical world enums (incl. `VARIANTS`/`display_name`); re-exports the worlds-data loader from `worlds_toml` |
| [src/worlds_toml.rs](../src/worlds_toml.rs) | §45 native typed `worlds.toml` config (sole world-data format) + its loader (`WorldError`/`WorldsLoad`/`load_worlds_data`, C6) |
| [src/lib.rs](../src/lib.rs) | Public API surface (doc-tests + `# Errors`); re-aliases moved modules back to root |
| [src/macros.rs](../src/macros.rs) | Crate-internal `macro_rules!` (`#[macro_use]`): `enum_slug!` — `as_slug` for a fieldless enum from verbatim variant→slug pairs (B-S3) |
| [src/main.rs](../src/main.rs) | `sectorforge` binary entry: parses `cli::Cli`, dispatches to `cli::run`, maps errors to exit 2 |
| [src/bin/dhat_profile.rs](../src/bin/dhat_profile.rs) | docs/OPTIMIZE.txt §G4 heap-profiling harness binary (dhat) |

### model/

| File | Purpose |
|---|---|
| [src/model/sector_model/mod.rs](../src/model/sector_model/mod.rs) | Output DTOs with Serialize/Deserialize |
| [src/model/sector_model/routes_view.rs](../src/model/sector_model/routes_view.rs) | Route render vocabulary split out of the DTO file (§A1): `RoutePattern`/`strides`, `RouteViewMode`, `stable_pattern_hash`, render impl-methods of `RouteType`/`RouteKind`/`GeneratedRoute` (re-exported at the `sector_model` root) |
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
| [src/gen/random_sector.rs](../src/gen/random_sector.rs) | RANDOM.md size-only → fully-complete sector: `SectorSize`, `mint_seed`, `build_random_config`, `generate_random_sector`, `RandomReport` |
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
| [src/analysis/scores.rs](../src/analysis/scores.rs) | TF-NT-2 score newtypes: thin `f32` wrappers for comparable scores |
| [src/analysis/intel.rs](../src/analysis/intel.rs) | §7 NEXT fog-of-war record |
| [src/analysis/missions.rs](../src/analysis/missions.rs) | §3 NEW2 mission seeds |
| [src/analysis/briefing.rs](../src/analysis/briefing.rs) | §9 NEW2 redacted briefing packs |
| [src/analysis/prose.rs](../src/analysis/prose.rs) | §6 gazetteer prose: deterministic template grammar |
| [src/analysis/personae.rs](../src/analysis/personae.rs) | §3 dramatis personae: named characters per faction presence |
| [src/analysis/hooks.rs](../src/analysis/hooks.rs) | §7 plot-hook generator: condition→template over model state |
| [src/analysis/relations/](../src/analysis/relations/) | §4 inter-faction diplomacy: stance matrix + tension scalar — dir module (`config`/`tables`/`tension`/`derive`/`render`) |
| [src/analysis/economy/](../src/analysis/economy/) | §12 trade & resource economy: production/consumption + route volume — dir module (`config`/`tables`/`derive`/`risk`/`render`) |
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

`render_core/` Pass C/D is **done** (was deferred in an earlier revision of
this map): the `Canvas` trait + shared `grid`/`routes` draw functions now
exist and both backends import them.

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
| [src/export/render_core/mod.rs](../src/export/render_core/mod.rs) | Backend-shared render layer: `RenderOptions`, colour helpers, `Canvas` trait, shared `grid`/`routes` draws |
| [src/export/render_core/colors.rs](../src/export/render_core/colors.rs) | `star_color`, `stability_color`, `tint_against`, `darken`, `dim`, `short`, `rgba`, `route_thickness_f32` — single source of truth, both backends import from here |
| [src/export/render_core/options.rs](../src/export/render_core/options.rs) | Cross-backend `RenderOptions` (formerly in bitmap; re-exported there + at lib.rs for compat) |
| [src/export/render_core/canvas.rs](../src/export/render_core/canvas.rs) | Backend-neutral `Canvas` drawing surface shared by `bitmap` and `svg_export` |
| [src/export/render_core/grid.rs](../src/export/render_core/grid.rs) | Shared hex-grid fill + subsector-border drawing (both backends) |
| [src/export/render_core/routes.rs](../src/export/render_core/routes.rs) | Shared route-line drawing for both PNG and SVG backends |
| [src/export/bitmap/mod.rs](../src/export/bitmap/mod.rs) | Sector PNG facade: `write_bitmap*`, `render_sector_image`, `encode_png_bytes`, top-level `render()` orchestrator. `RenderOptions` is now a re-export of `super::render_core::RenderOptions`. |
| [src/export/bitmap/primitives.rs](../src/export/bitmap/primitives.rs) | Pixel primitives + 5×7 font (shared w/ system_map) |
| [src/export/bitmap/geom.rs](../src/export/bitmap/geom.rs) | `Geom`, `MapBounds`, hex centre/vertices, `Rect` collision type |
| [src/export/bitmap/colors.rs](../src/export/bitmap/colors.rs) | `i32`-quantised wrappers (`route_thickness`, `stroke_px`); re-exports the shared helpers from `render_core::colors` |
| [src/export/bitmap/canvas.rs](../src/export/bitmap/canvas.rs) | `Canvas` impl for the PNG backend: quantises `f32` world coords to pixels |
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
| [src/export/svg_export/canvas.rs](../src/export/svg_export/canvas.rs) | `Canvas` impl for the SVG backend: emits XML primitives |
| [src/export/svg_export/geom.rs](../src/export/svg_export/geom.rs) | `MapBounds`, `map_bounds`, `hex_center`, `hex_vertices` |
| [src/export/svg_export/grid.rs](../src/export/svg_export/grid.rs) | Hex grid fill + subsector borders + per-system/region tints |
| [src/export/svg_export/routes.rs](../src/export/svg_export/routes.rs) | Route line patterns + route-control glyphs (exports `draw_route_pattern`, `ControlKind` for legend) |
| [src/export/svg_export/regions.rs](../src/export/svg_export/regions.rs) | §5 warp-region label overlay |
| [src/export/svg_export/systems.rs](../src/export/svg_export/systems.rs) | Star disks, capital markers, world-count pips |
| [src/export/svg_export/labels.rs](../src/export/svg_export/labels.rs) | System name labels + collision-aware subsector titles |
| [src/export/svg_export/legend.rs](../src/export/svg_export/legend.rs) | Right-hand legend (title, route key, factions, heatmap chip), full + compact variants |
| [src/export/svg_export/tests.rs](../src/export/svg_export/tests.rs) | Unit tests for the SVG backend |

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
| [src/cli/exit_code.rs](../src/cli/exit_code.rs) | Maps `SectorError` variants to stable `ExitCode` values |
| [src/cli/generate.rs](../src/cli/generate.rs) | `generate` + `generate-system` runners (with §15 NEW2 constraint search wiring) |
| [src/cli/validate.rs](../src/cli/validate.rs) | `validate`, `validate-sector`, `render-markdown`, `inspect-worlds` runners |
| [src/cli/analyze.rs](../src/cli/analyze.rs) | `analyze` runner |
| [src/cli/presets.rs](../src/cli/presets.rs) | `new` + `list-presets` runners |
| [src/cli/random.rs](../src/cli/random.rs) | `random` runner — RANDOM.md (synthesise + generate + export bundle & five reports) |
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
| [gui-core/src/sector_view/](../gui-core/src/sector_view/mod.rs) | Read-only hex map render. Split (F3, verbatim) into `view.rs` (the `SectorView` widget + `show()`), `render.rs` (`SectorGeom`, hex math, paint helpers, geometry tests), and `cache.rs` (`SectorMapCache`); `mod.rs` re-exports the public surface so `sector_view::*` paths are unchanged. `SectorGeom::hit_route` and `SectorMapCache::region_for_hex` back the right-click route + region-hex schemas in `builder::panels::map::context_menu::resolve_sector_context`. **§BEAUTY** live-only void flourishes (`paint_star_dust` / `paint_vignette` / `paint_chart_frame`, gated by `is_dark` + canvas size, in `render.rs`) paint on the egui path only — the golden PNG/SVG exporters are untouched; the gui-core `map_snapshots` goldens cover them. |
| [gui-core/src/system_view.rs](../gui-core/src/system_view.rs) | System detail panel widget — also embedded under the SYSTEM tab by `panels/system/preview.rs::show_system_map_section` (§CTX0). §CTX1 Phase 6 exports `SystemGeom`, `SystemPick`, and `pick_world` so `builder::panels::system_map::arm_system_context_menu` can hit-test star / planet / orbit-ring / background. |
| [gui-core/src/info_panel.rs](../gui-core/src/info_panel.rs) | Text formatting widgets |
| [gui-core/src/heatmap.rs](../gui-core/src/heatmap.rs) | GUI heatmap color/cache wrapper |
| [gui-core/src/map_theme.rs](../gui-core/src/map_theme.rs) | GUI-side map theme wrapper over `sectorforge` themes |
| [gui-core/src/visual_tokens.rs](../gui-core/src/visual_tokens.rs) | Semantic map tokens bridging sector data and egui painting |
| [gui-core/src/app_icon.rs](../gui-core/src/app_icon.rs) | Shared window icon for the GUI binaries |
| [gui-core/src/nav.rs](../gui-core/src/nav.rs) | §LINK4 `entity_link` cross-tab link widget |
| [gui-core/src/theme.rs](../gui-core/src/theme.rs) | Global chrome theming — 8 `Theme` presets → `Visuals` (routes through `design` tokens); `Heading` requests `design::display_family()` |
| [gui-core/src/ui_kit.rs](../gui-core/src/ui_kit.rs) | §UO shared chrome widgets — `section`/`collapsing_section`/`field`/`combo`/`kv` (kv is the §BEAUTY aligned ledger), text helpers, responsive columns |
| [gui-core/src/design.rs](../gui-core/src/design.rs) | §DESIGN form tokens — spacing/radius/elevation/motion/type scale, accent ramp, `vertical_gradient`, `FONT_DISPLAY`/`display_family` |
| [gui-core/src/card.rs](../gui-core/src/card.rs) | §BEAUTY `selectable_plate` — hand-painted hover/selection-animated roster + nav row |
| [gui-core/src/widgets.rs](../gui-core/src/widgets.rs) | §BEAUTY bespoke painted controls — `primary_button` (brass) + `toggle`/`toggle_with_label` (sliding) |
| [gui-core/src/modal.rs](../gui-core/src/modal.rs) | §BEAUTY `scrim(ctx, open)` — fading modal backdrop (dims + inerts page) returning the eased entrance factor |
| [gui-core/src/fonts.rs](../gui-core/src/fonts.rs) | §BEAUTY §5.5 custom-font registration (`install`); opt-in `bundled-fonts` feature, `FontData::from_static` from `assets/fonts/` |
| [gui-core/tests/map_snapshots.rs](../gui-core/tests/map_snapshots.rs) | Snapshot tests for shared map rendering |

## sectorforge-viewer

Viewer/editor binary. `viewer/src/app/` is the eframe app split into
per-view modules; `viewer/src/editor/` is the in-place sector/world editor.

| File | Purpose |
|---|---|
| [viewer/src/main.rs](../viewer/src/main.rs) | Viewer/editor binary entry (`sectorforge-viewer`) |
| [viewer/src/lib.rs](../viewer/src/lib.rs) | Viewer crate root: module declarations, modular-by-entity layout |
| [viewer/src/dashboard.rs](../viewer/src/dashboard.rs) | §8 sector analytics dashboard (lazy `SectorAnalysis`) |
| [viewer/src/factions_overview.rs](../viewer/src/factions_overview.rs) | High-level faction overview/edit surface |
| [viewer/src/preset_gallery.rs](../viewer/src/preset_gallery.rs) | §9 "New from preset" gallery window |
| [viewer/src/route_planner.rs](../viewer/src/route_planner.rs) | Route planner: pathfind between two systems over the route graph |
| [viewer/src/segmentum_view.rs](../viewer/src/segmentum_view.rs) | Segmentum overview widgets + on-disk bundle loading |
| [viewer/src/data_editor.rs](../viewer/src/data_editor.rs) | §45 typed `worlds.toml` editor (dropdowns + DragValue) |

### viewer app/ (per-view modules)

| File | Purpose |
|---|---|
| [viewer/src/app/mod.rs](../viewer/src/app/mod.rs) | Top-level viewer/editor eframe app + navigation |
| [viewer/src/app/export_ui.rs](../viewer/src/app/export_ui.rs) | PNG/SVG/HTML/JSON export UI |
| [viewer/src/app/lifecycle.rs](../viewer/src/app/lifecycle.rs) | App lifecycle: init / load / save wiring |
| [viewer/src/app/layout.rs](../viewer/src/app/layout.rs) | App panel layout scaffolding |
| [viewer/src/app/types.rs](../viewer/src/app/types.rs) | App-level types (tab/state enums) |
| [viewer/src/app/ui_helpers.rs](../viewer/src/app/ui_helpers.rs) | Shared egui helpers across views |
| [viewer/src/app/sector_view.rs](../viewer/src/app/sector_view.rs) | Sector hex-map view |
| [viewer/src/app/system_view.rs](../viewer/src/app/system_view.rs) | System detail view |
| [viewer/src/app/factions_view.rs](../viewer/src/app/factions_view.rs) | Factions view |
| [viewer/src/app/relations_view.rs](../viewer/src/app/relations_view.rs) | Inter-faction relations view |
| [viewer/src/app/regions_view.rs](../viewer/src/app/regions_view.rs) | Warp-regions view |
| [viewer/src/app/trade_view.rs](../viewer/src/app/trade_view.rs) | Economy/trade view |
| [viewer/src/app/planner_view.rs](../viewer/src/app/planner_view.rs) | Route-planner view host |
| [viewer/src/app/analytics_views.rs](../viewer/src/app/analytics_views.rs) | Analytics dashboard views |
| [viewer/src/app/editor_views.rs](../viewer/src/app/editor_views.rs) | Hosts the in-place editor panels |
| [viewer/src/app/segmentum.rs](../viewer/src/app/segmentum.rs) | Segmentum overview view host |

### viewer editor/ (in-place sector/world editing)

| File | Purpose |
|---|---|
| [viewer/src/editor/mod.rs](../viewer/src/editor/mod.rs) | Sector editor: load/create/edit/save a `GeneratedSector` via GUI |
| [viewer/src/editor/state.rs](../viewer/src/editor/state.rs) | Editor state machine: working sector + selection + pending dialogs |
| [viewer/src/editor/enums.rs](../viewer/src/editor/enums.rs) | Fixed dropdown option lists (match string forms) |
| [viewer/src/editor/dialogs.rs](../viewer/src/editor/dialogs.rs) | Modal dialogs; read/mutate state in place |
| [viewer/src/editor/file_ops.rs](../viewer/src/editor/file_ops.rs) | Disk ops: list/load/save projects under `examples/` |
| [viewer/src/editor/toolbar.rs](../viewer/src/editor/toolbar.rs) | Top toolbar: file ops + tab switcher |
| [viewer/src/editor/ui_helpers.rs](../viewer/src/editor/ui_helpers.rs) | Small egui helpers shared across editor panels |
| [viewer/src/editor/map_panel.rs](../viewer/src/editor/map_panel.rs) | Editable hex-grid map panel |
| [viewer/src/editor/system_panel.rs](../viewer/src/editor/system_panel.rs) | Selected-system inspector (name/coord/star/worlds) |
| [viewer/src/editor/world_panel.rs](../viewer/src/editor/world_panel.rs) | World inspector; all fields editable via dropdown except name/orbit |
| [viewer/src/editor/factions_panel.rs](../viewer/src/editor/factions_panel.rs) | Factions list editor (id/name/kind/disposition + system assignment) |
| [viewer/src/editor/routes_panel.rs](../viewer/src/editor/routes_panel.rs) | Routes list editor (from/to/type/stability/distance) |
| [viewer/src/editor/settings_panel.rs](../viewer/src/editor/settings_panel.rs) | Sector settings: id/title/seed/size/manifest hints |
| [viewer/src/editor/generation_panel.rs](../viewer/src/editor/generation_panel.rs) | Generation settings for real-time building |
| [viewer/src/editor/wishes_panel.rs](../viewer/src/editor/wishes_panel.rs) | `wishes.toml` editor + search integration |

## sectorforge-builder

`builder/src/builder/` holds builder state, the command bus, project I/O,
and panels. `builder/src/builder/mod.rs` is the Phase-A foundation facade.

| File | Purpose |
|---|---|
| [builder/src/main.rs](../builder/src/main.rs) | Builder binary entry (`sectorforge-builder`) |
| [builder/src/lib.rs](../builder/src/lib.rs) | Builder crate root |
| [builder/src/app.rs](../builder/src/app.rs) | Thin builder eframe app host |

### Builder core (builder/src/builder/)

| File | Purpose |
|---|---|
| [builder/src/builder/mod.rs](../builder/src/builder/mod.rs) | GUI builder foundation facade (Phase A of docs/BUILDER_REQS.txt) |
| [builder/src/builder/command.rs](../builder/src/builder/command.rs) | §U1 command bus: every structural mutation flows through `BuilderCommand` (undo/redo). Includes the coarse §R4 detail-editor commands `EditWorld` / `EditSystem` / `EditChronicle` / `BulkEditWorlds` / `DeriveBaselineIntel` (snapshot-replace; `apply` captures `before`) used by the WORLD/SYSTEM/CONTROL/INTEL/HISTORY inspectors |
| [builder/src/builder/session.rs](../builder/src/builder/session.rs) | §D6 `.sgforge` session file (load/save round-trip) |
| [builder/src/builder/snapshot.rs](../builder/src/builder/snapshot.rs) | §U3/§U4 named save points (freeze sector + cursor position) |
| [builder/src/builder/project_io.rs](../builder/src/builder/project_io.rs) | §P1–§P3 project file I/O for the GUI builder |
| [builder/src/builder/preview.rs](../builder/src/builder/preview.rs) | §G3 live preview pipeline |
| [builder/src/builder/index.rs](../builder/src/builder/index.rs) | Deterministic lookup index over a `GeneratedSector`, rebuilt after mutations |
| [builder/src/builder/derivation_cache.rs](../builder/src/builder/derivation_cache.rs) | §LD1/§R5 BLAKE3-keyed derivation cache per overlay |
| [builder/src/builder/data_catalogs.rs](../builder/src/builder/data_catalogs.rs) | In-memory mirrors of config files feeding the generator |
| [builder/src/builder/file_watcher.rs](../builder/src/builder/file_watcher.rs) | §P5 background watcher for the project directory |
| [builder/src/builder/workspace.rs](../builder/src/builder/workspace.rs) | §G6 workspace: ring of open `BuilderState` sessions |
| [builder/src/builder/preferences.rs](../builder/src/builder/preferences.rs) | §P6 user preferences (`~/.config/sectorforge/preferences.toml`) |
| [builder/src/builder/errors.rs](../builder/src/builder/errors.rs) | Builder-local error types |
| [builder/src/builder/analytics_run.rs](../builder/src/builder/analytics_run.rs) | §A1..§A4 ANALYTICS runtime: editable `AnalyzeConfig` + strict mode |
| [builder/src/builder/diff_run.rs](../builder/src/builder/diff_run.rs) | §DF1..§DF5 DIFF runtime: two scratch sector slots + diff/tick |
| [builder/src/builder/search_run.rs](../builder/src/builder/search_run.rs) | §SR1..§SR5 SEARCH runtime: editable wishes doc + search driver |
| [builder/src/builder/segmentum_run.rs](../builder/src/builder/segmentum_run.rs) | §SG1..§SG5 SEGMENTUM runtime: editable `segmentum.toml` + off-thread compose job + composed result |

### Builder state (builder/src/builder/state/)

| File | Purpose |
|---|---|
| [builder/src/builder/state/mod.rs](../builder/src/builder/state/mod.rs) | `BuilderState` struct (§D5) + `new_blank` + `default_config` + slice facade. Carries the transient context-menu fields (`sector_context_menu`, `partial_regen_anchor`, `pending_bulk_rename`, `pending_region_rename`, `system_context_menu`, `pending_world_rename`, `last_menu_action`) — all in-memory only, dropped by `session::SessionFile` round-trip. |
| [builder/src/builder/state/types.rs](../builder/src/builder/state/types.rs) | UI/dialog types: `BuilderTab`, `MapTool`, `ControlOverlay`, `ModalKind`, `HealthLevel`, `JobHandle`, `PartialRegenRect`, `Pending*`, `MapViewCache`, `HistoryWizardState`, `HistoryAnchorKind`, `TickLogEntry`, `TickLogScope`, `SectorContextMenu` + `SectorMenuTarget`, `SystemContextMenu` + `SystemMenuTarget`, `DEFAULT_*` |
| [builder/src/builder/state/selection.rs](../builder/src/builder/state/selection.rs) | §S1/§S4 selection helpers: `focus_system`, `toggle_system_selection` + §LINK2 `focus_entity` / `nav_back` / `nav_forward` |
| [builder/src/builder/state/nav.rs](../builder/src/builder/state/nav.rs) | §LINK1 cross-tab navigation: `EntityRef` enum + `target_tab` |
| [builder/src/builder/state/undo.rs](../builder/src/builder/state/undo.rs) | R4 command bus: `run`, `undo`, `redo`, ring-buffer trim, `snapshot`, `trigger_auto_save` |
| [builder/src/builder/state/derivations.rs](../builder/src/builder/state/derivations.rs) | Economy / relations / chronicle re-derive, debounced validation pump, `synthesize_project_input`, `health_level`, §CF4/§CF5 `advance_conflict_ticks` driver + tick-log capture |
| [builder/src/builder/state/regions_ops.rs](../builder/src/builder/state/regions_ops.rs) | §REG1..§REG3 warp-region helpers: add/remove/paint/erase/update/next id |
| [builder/src/builder/state/generation_ops.rs](../builder/src/builder/state/generation_ops.rs) | §G2..§G5 + §S5 + §W4: `generate_system_here`, `regenerate_world`, `apply_preview`, `regenerate_partial`, `reroll_seed`, `find_world_indices` |
| [builder/src/builder/state/tests.rs](../builder/src/builder/state/tests.rs) | Unit tests for builder state + command bus |

### Builder panels — MAP tab (split per REFACTOR.txt Task 1)

| File | Purpose |
|---|---|
| [builder/src/builder/panels/map/mod.rs](../builder/src/builder/panels/map/mod.rs) | §S1 + §R2 MAP tab facade. `pub fn show`, `pub fn show_toolbox`, child-module declarations, full unit-test suite. Re-exports `menu_anchor_pivot` at `pub(super)` so `panels/system_map.rs` can share the viewport-pivot helper. |
| [builder/src/builder/panels/map/interactions.rs](../builder/src/builder/panels/map/interactions.rs) | Hex render dispatcher + tool routing: `show_hex_map`, `handle_click`, `handle_drag_drop`, `apply_rect_select`, `apply_partial_regen_anchor_click`, `paint_region_at`, `add_route_between`. |
| [builder/src/builder/panels/map/context_menu.rs](../builder/src/builder/panels/map/context_menu.rs) | §CTX1 right-click surface: `SectorMenuAction` + `OpenInTarget` enums, `resolve_sector_context`, `apply_sector_menu_action`, `render_empty_hex_menu` / `render_system_menu` / `render_multi_selection_menu` / `render_route_menu` / `render_region_hex_menu`, `show_sector_context_menu`, `should_dismiss_sector_context_menu`, `sector_menu_target_is_stale`, `sector_menu_action_label`, `menu_anchor_pivot`. |
| [builder/src/builder/panels/map/dialogs.rs](../builder/src/builder/panels/map/dialogs.rs) | Transient modal dialogs surfaced from the MAP panel: `show_place_dialog`, `show_rename_dialog`, `show_bulk_rename_dialog`, `show_region_rename_dialog`, `show_collision_dialog`. |
| [builder/src/builder/panels/map/cache.rs](../builder/src/builder/panels/map/cache.rs) | `refresh_map_cache` + `sector_view_digest` — rebuilds `MapViewCache` (subsector clustering + hex→system lookup + region tints) when the sector slice digest changes. |

### Builder panels — other tabs

`panels/mod.rs` declares the modules (R10, §41/§N2); `panels/nav.rs` is the
top-level tab router; `panels/placeholder.rs` is the shared stub-panel
helper.

| File | Purpose |
|---|---|
| [builder/src/builder/panels/mod.rs](../builder/src/builder/panels/mod.rs) | Panel module declarations (R10, §41/§N2) |
| [builder/src/builder/panels/nav.rs](../builder/src/builder/panels/nav.rs) | Top-level tab router (§N1/§N2). Dispatches the two right-most diagnostics tabs `BuilderTab::Validation` / `Invariants` to `validation::show` / `invariants::show` (XC-1) |
| [builder/src/builder/panels/placeholder.rs](../builder/src/builder/panels/placeholder.rs) | Shared stub-panel helper (§N2) |
| [builder/src/builder/panels/text_buf.rs](../builder/src/builder/panels/text_buf.rs) | Persistent-buffer wrappers around `text_edit_singleline`/multiline |
| [builder/src/builder/panels/shortcuts.rs](../builder/src/builder/panels/shortcuts.rs) | Global keyboard shortcuts (§U2 + §LINK3) |
| [builder/src/builder/panels/project.rs](../builder/src/builder/panels/project.rs) | PROJECT tab (§N1/§N2): composes Phase-A project I/O surfaces |
| [builder/src/builder/panels/project_tree.rs](../builder/src/builder/panels/project_tree.rs) | §P4 PROJECT tree panel |
| [builder/src/builder/panels/new_project.rs](../builder/src/builder/panels/new_project.rs) | New-project wizard (§P1) |
| [builder/src/builder/panels/generate_random.rs](../builder/src/builder/panels/generate_random.rs) | Random-sector wizard (RANDOM.md) — `ModalKind::GenerateRandom` → `generate_random_sector` → `open_project` |
| [builder/src/builder/panels/open_project.rs](../builder/src/builder/panels/open_project.rs) | Open-project picker (§P2) |
| [builder/src/builder/panels/save_project.rs](../builder/src/builder/panels/save_project.rs) | Save-project actions (§P3) |
| [builder/src/builder/panels/preferences.rs](../builder/src/builder/panels/preferences.rs) | §P6 preferences panel: recent-projects MRU |
| [builder/src/builder/panels/conflict_resolver.rs](../builder/src/builder/panels/conflict_resolver.rs) | §P5 dialog when the file watcher detects an external change |
| [builder/src/builder/panels/generation.rs](../builder/src/builder/panels/generation.rs) | §6 generation panel (G1..G6) |
| [builder/src/builder/panels/world/](../builder/src/builder/panels/world/) | WORLD tab §W1..§W7 inspector, split (E4) into `mod.rs` (orchestration + `EnumPicker`/`combo_enum` pickers + tests), `identity.rs`, `environment.rs`, `features.rs` (§W5 weighted features), `factions.rs` (presence), `claims.rs` (§W7), `overlays.rs` (control/overlays/chronicle/regen) |
| [builder/src/builder/panels/routes.rs](../builder/src/builder/panels/routes.rs) | ROUTES tab — Phase B §R1..§R7 route editor |
| [builder/src/builder/panels/factions.rs](../builder/src/builder/panels/factions.rs) | FACTIONS tab — §F1..§F7 faction roster editor |
| [builder/src/builder/panels/control.rs](../builder/src/builder/panels/control.rs) | CONTROL tab — Phase C §C1..§C8 presence/dominance/control-state |
| [builder/src/builder/panels/economy.rs](../builder/src/builder/panels/economy.rs) | ECONOMY tab — Phase C §E1..§E7 |
| [builder/src/builder/panels/relations.rs](../builder/src/builder/panels/relations.rs) | RELATIONS tab — Phase C §REL1..§REL9 |
| [builder/src/builder/panels/regions.rs](../builder/src/builder/panels/regions.rs) | REGIONS tab — Phase C §REG1..§REG7 |
| [builder/src/builder/panels/subsectors.rs](../builder/src/builder/panels/subsectors.rs) | SUBSECTORS tab — Phase C §SUB1..§SUB5 |
| [builder/src/builder/panels/history.rs](../builder/src/builder/panels/history.rs) | HISTORY tab — Phase C §H1..§H8 |
| [builder/src/builder/panels/hooks.rs](../builder/src/builder/panels/hooks.rs) | HOOKS tab — Phase D §HK1..§HK6 |
| [builder/src/builder/panels/personae.rs](../builder/src/builder/panels/personae.rs) | PERSONAE tab — Phase D §PER1..§PER5 |
| [builder/src/builder/panels/sites.rs](../builder/src/builder/panels/sites.rs) | SITES tab — Phase D §ST1..§ST4 |
| [builder/src/builder/panels/intel.rs](../builder/src/builder/panels/intel.rs) | §I1..§I5 (BUILDER_REQS §29) intel / fog-of-war editor |
| [builder/src/builder/panels/surface_regions.rs](../builder/src/builder/panels/surface_regions.rs) | §SU1/§SU2 (BUILDER_REQS §32) per-world surface-region editor |
| [builder/src/builder/panels/search.rs](../builder/src/builder/panels/search.rs) | SEARCH tab — constraint-directed seed search |
| [builder/src/builder/panels/diff.rs](../builder/src/builder/panels/diff.rs) | DIFF tab — Phase E §DF1..§DF5 two-sector + tick diff editor |
| [builder/src/builder/panels/analytics.rs](../builder/src/builder/panels/analytics.rs) | ANALYTICS tab — Phase E §A1..§A4 read-only analytics |
| [builder/src/builder/panels/segmentum.rs](../builder/src/builder/panels/segmentum.rs) | SEGMENTUM tab — Phase E §SG1..§SG5 compose editor |
| [builder/src/builder/panels/export.rs](../builder/src/builder/panels/export.rs) | EXPORT tab — Phase E §EX1..§EX8 export-bundle editor |
| [builder/src/builder/panels/validation.rs](../builder/src/builder/panels/validation.rs) | Validation panel (§V1): pre-generation report |
| [builder/src/builder/panels/invariants.rs](../builder/src/builder/panels/invariants.rs) | Invariants panel (§V2): post-generation report |
| [builder/src/builder/panels/conflict.rs](../builder/src/builder/panels/conflict.rs) | §CF1..§CF6 conflict + stability editor: per-world / per-system `ConflictState` + `StabilityState`, advance-ticks button, tick log, conflict heatmap toggle |
| [builder/src/builder/panels/missions.rs](../builder/src/builder/panels/missions.rs) | §M1..§M5 missions tab: cached `MissionsReport` list + detail card, manual mission editor over `MissionsConfig::manual`, auto-derive + player-edition toggles, click-to-highlight `primary_location` via `focus_entity` |
| [builder/src/builder/panels/prose.rs](../builder/src/builder/panels/prose.rs) | §PR1..§PR4 prose tab: per-system + sector overview Override toggles backed by `ProseConfig::overrides`, tone preset combo (Gazetteer / Dispatch), "Regenerate prose" runs `BuilderState::recompute_prose`. Overrides survive every regenerate because `prose::derive_with` re-applies them after the deterministic derivation. |
| [builder/src/builder/panels/briefing.rs](../builder/src/builder/panels/briefing.rs) | §BR1..§BR5 briefing tab: `AudiencePreset` picker + observer-faction `ComboBox` + 0..=100 min-confidence slider build a `BriefingProfile`; "Generate briefing" calls `sectorforge::apply_briefing` + `briefing::render_markdown` and caches both `BriefingPack` and redacted Markdown on `BuilderState`. "Export .md + .json" writes the cached pack through `sectorforge::write_briefing` into a folder picked via `rfd::FileDialog`. |
| [builder/src/builder/panels/interestingness.rs](../builder/src/builder/panels/interestingness.rs) | §INT1..§INT4 interestingness tab: `ProfileId` picker (PoliticalSandbox/GrimCollapse/Mercantile/Villainous/Frontier), "Score sector" runs `sectorforge::derive_interestingness_with` and caches `InterestingnessReport` on `BuilderState`, per-metric band chart painted via `ui.painter_at` (target band shaded green, observed value ticked), and a per-profile threshold override editor backed by `BuilderState::interestingness_custom_overrides` (keyed by snake-case profile id, seeded from each profile's built-in band). |
| [builder/src/builder/panels/system/](../builder/src/builder/panels/system/) | §S2 SYSTEM-tab inspector, split (E7) into `mod.rs` (orchestration: `show` + roster/inspector/header + read-only deep-link sections + `apply_bulk_*` re-exports + tests), `identity.rs` (identity/coord/star/tags), `archetype.rs` (§AR1..§AR3), `preview.rs` (§CTX0 `show_system_map_section` embedding `gui_core::system_view::SystemView` + §T5 bitmap preview; routes `secondary_clicked` through `panels/system_map::arm_system_context_menu`), `regen.rs` (§S5), `bulk_ops.rs` (the `apply_bulk_*` `pub(crate)` helpers reused by the multi-selection right-click menu in `panels/map`). |
| [builder/src/builder/panels/status.rs](../builder/src/builder/panels/status.rs) | §N4 status bar: project label, dirty flag, tri-coloured §V3 health pip, command-cursor position, derivation-cache count, pending-job spinner. §CTX1 Phase 7 tails `BuilderState::last_menu_action` as `ctx_menu: <schema> :: <item>`. |
| [builder/src/builder/panels/system_map.rs](../builder/src/builder/panels/system_map.rs) | §CTX1 Phase 6 in-system right-click menu (`§6.6`..`§6.9`). Owns `SystemMenuAction` + `resolve_system_context` + `apply_system_menu_action` + `arm_system_context_menu` + `show_system_context_menu` + `show_world_rename_dialog`. Reuses `super::map::menu_anchor_pivot` to flip the menu's anchor pivot when the cursor sits on the right/bottom half of the viewport. |
| [builder/src/builder/panels/orbital.rs](../builder/src/builder/panels/orbital.rs) | §O1/§O2 orbital + blockade editor. Exposes `pub(crate) derive_and_apply_orbital_assets(state, system)` (§CTX1 Phase 6) so the in-system right-click `DERIVE ORBITAL ASSETS` row can reuse the same `SetOrbitalAssets` + `SetBlockadeReport` dispatch path. |

## Integration tests (tests/it/)

Single-binary integration suite. [tests/it.rs](../tests/it.rs) is the entry
that declares the modules below.

| File | Purpose |
|---|---|
| [tests/it/shared.rs](../tests/it/shared.rs) | Shared test fixtures across the suite |
| [tests/it/golden_generation.rs](../tests/it/golden_generation.rs) | Full project → generate → export → reload golden tests |
| [tests/it/golden_png.rs](../tests/it/golden_png.rs) | docs/OPTIMIZE.txt §G3 golden PNG byte-stability |
| [tests/it/svg_export_tests.rs](../tests/it/svg_export_tests.rs) | SVG exporter smoke test against the bundled m42 fixture |
| [tests/it/cli_smoke.rs](../tests/it/cli_smoke.rs) | TF-T-4 per-subcommand CLI smoke coverage |
| [tests/it/cli_gui_parity.rs](../tests/it/cli_gui_parity.rs) | docs/OPTIMIZE.txt §G2 CLI/GUI parity |
| [tests/it/analytics_and_presets.rs](../tests/it/analytics_and_presets.rs) | §8/§9 analytics + presets integration |
| [tests/it/economy_tests.rs](../tests/it/economy_tests.rs) | `sectorforge::economy` coverage (TEST-001) |
| [tests/it/hooks_tests.rs](../tests/it/hooks_tests.rs) | `sectorforge::hooks` coverage (TEST-001) |
| [tests/it/personae_tests.rs](../tests/it/personae_tests.rs) | `sectorforge::personae` coverage (TEST-001) |
| [tests/it/relations_tests.rs](../tests/it/relations_tests.rs) | `sectorforge::relations` coverage (TEST-001) |
| [tests/it/search_and_diff.rs](../tests/it/search_and_diff.rs) | §2/§10 seed search + sector diff integration |
| [tests/it/segmentum_tests.rs](../tests/it/segmentum_tests.rs) | §14 segmentum composition (slow; `#[ignore]`) |
| [tests/it/invariants_tests.rs](../tests/it/invariants_tests.rs) | Post-generation invariant checks (spec §11.11) + standalone APIs |
| [tests/it/invariants_proptest.rs](../tests/it/invariants_proptest.rs) | Property-based fuzz over spec §11.11 invariants |
| [tests/it/validation_tests.rs](../tests/it/validation_tests.rs) | Validation behaviour in adverse cases |
| [tests/it/imports_test.rs](../tests/it/imports_test.rs) | Import/round-trip coverage |
