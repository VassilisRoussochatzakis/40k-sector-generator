# CLAUDE.md
Do not ever look in, or modify, anything in the "old" directory.
OBEY ALL INSTRUCTIONS IN INPUT.md
Spec/requirement files live under [docs/](docs/): [docs/BUILDER_REQS.txt](docs/BUILDER_REQS.txt), [docs/IMPROVEMENT.txt](docs/IMPROVEMENT.txt), [docs/OPTIMIZE.txt](docs/OPTIMIZE.txt), [docs/REFACTOR.txt](docs/REFACTOR.txt), [docs/GUIBUILDER.txt](docs/GUIBUILDER.txt).
When making changes, update GUIDE.md accordingly.
## Commands

```bash
cargo build            # build all targets (sectorforge + sectorforge-viewer + sectorforge-builder)
cargo test             # all tests
cargo fmt              # format code
cargo check            # compile check
cargo run --bin sectorforge --help   # CLI help
cargo run -p sectorforge-viewer -- --help   # Viewer help
cargo run -p sectorforge-builder -- --help   # Builder help
```

## Source layout

| File | Purpose |
|---|---|
| [src/worlds.rs](src/worlds.rs) | Canonical world enums (incl. `VARIANTS`/`display_name`) |
| [src/worlds_toml.rs](src/worlds_toml.rs) | §45 native typed `worlds.toml` config (sole world-data format) |
| [src/world_pool.rs](src/world_pool.rs) | GenerationRow → weighted candidate pool (+ authored-feature overlay) |
| [src/generation/mod.rs](src/generation/mod.rs) | Sector generation facade: `SectorProgress`, `generate*` orchestrator, manifest build |
| [src/generation/placement.rs](src/generation/placement.rs) | Hex grid placement with min-distance relaxation |
| [src/generation/systems.rs](src/generation/systems.rs) | Per-system build: `build_system*`, star colour, system naming, spectral fallback |
| [src/generation/world_placement.rs](src/generation/world_placement.rs) | Per-world build: candidate pick, features, naming, tags, `regenerate_world_payload` |
| [src/generation/factions.rs](src/generation/factions.rs) | Faction assignment + `aggregate_factions` roll-up + `assign_factions_for_systems` |
| [src/generation/routes.rs](src/generation/routes.rs) | Public route graph + `classify_route` + union-find connector |
| [src/sector_model.rs](src/sector_model.rs) | Output DTOs with Serialize/Deserialize |
| [src/control.rs](src/control.rs) | Multi-dim presence / claim / control / power derivation |
| [src/validation.rs](src/validation.rs) | Pre-generation validation |
| [src/invariants.rs](src/invariants.rs) | Post-generation invariants (spec §11.11) |
| [src/render.rs](src/render.rs) | Markdown rendering (sector + standalone system) |
| [src/export.rs](src/export.rs) | JSON / Markdown / manifest / bitmap writers |
| [src/html_export.rs](src/html_export.rs) | §11 self-contained interactive HTML map (inlined JSON + JS canvas renderer + theme CSS) |
| [src/svg_export/mod.rs](src/svg_export/mod.rs) | SVG export facade: `render_sector_svg`, `write_sector_svg_to*`, top-level orchestrator + shared `HEX_SIZE` / `star_radius_ratio` |
| [src/svg_export/primitives.rs](src/svg_export/primitives.rs) | Low-level SVG emitters: `<rect>`/`<circle>`/`<polygon>`/`<line>`/`<text>` + XML escape |
| [src/svg_export/colors.rs](src/svg_export/colors.rs) | Star/route/tint/darken/dim/short helpers |
| [src/svg_export/geom.rs](src/svg_export/geom.rs) | `MapBounds`, `map_bounds`, `hex_center`, `hex_vertices` |
| [src/svg_export/grid.rs](src/svg_export/grid.rs) | Hex grid fill + subsector borders + per-system/region tints |
| [src/svg_export/routes.rs](src/svg_export/routes.rs) | Route line patterns + route-control glyphs (exports `draw_route_pattern`, `ControlKind` for legend) |
| [src/svg_export/regions.rs](src/svg_export/regions.rs) | §5 warp-region label overlay |
| [src/svg_export/systems.rs](src/svg_export/systems.rs) | Star disks, capital markers, world-count pips |
| [src/svg_export/labels.rs](src/svg_export/labels.rs) | System name labels + collision-aware subsector titles |
| [src/svg_export/legend.rs](src/svg_export/legend.rs) | Right-hand legend (title, route key, factions, heatmap chip), full + compact variants |
| [src/search.rs](src/search.rs) | §2 seed search: declarative wishes → deterministic seed enumeration |
| [src/diff.rs](src/diff.rs) | §10 sector diff: model-aware before/after report |
| [src/history/mod.rs](src/history/mod.rs) | §1 chronicle facade: `derive*`, orchestration, `anchor_key`, `pub use` surface |
| [src/history/config.rs](src/history/config.rs) | `HistoryConfig`/`HistoryFile`/`HistoryEra`/`HistoryEventRule` + defaults |
| [src/history/model.rs](src/history/model.rs) | Output DTOs: `HistoryReport`, `HistoryEvent`, `HistoryAnchor`, `EventKind` (+ topo/weight), entity/consequence types |
| [src/history/context.rs](src/history/context.rs) | `EmitContext` borrowed state threaded into emit families |
| [src/history/progress.rs](src/history/progress.rs) | `HistoryProgress` enum + progress-stride helpers |
| [src/history/build.rs](src/history/build.rs) | Event construction: `build_event`, date/era synthesis, entity refs, consequences |
| [src/history/worlds.rs](src/history/worlds.rs) | Per-world emission: foundation, claims, contested control, hidden cults, purges |
| [src/history/systems.rs](src/history/systems.rs) | Per-system emission: system state + archetype activations (necron/tyranid/ork/GSC/tau/aeldari/chaos) |
| [src/history/routes.rs](src/history/routes.rs) | Per-route emission: warp hazards, concealed passages, pirate/interdictor control |
| [src/history/subsectors.rs](src/history/subsectors.rs) | Per-subsector emission: clustering vs deterministic sampling |
| [src/history/regions.rs](src/history/regions.rs) | Per-region emission: warp phenomena → chronicle entries |
| [src/history/rules.rs](src/history/rules.rs) | Declarative `[[event_rules]]` enforcement + `EventKind` aliases |
| [src/history/labels.rs](src/history/labels.rs) | Small string helpers: `kind_slug`, `article_phrase`, `gsc_stage_label`, `tau_band_label` |
| [src/history/markdown.rs](src/history/markdown.rs) | `render_markdown` + `write_report` (history.md/json) |
| [src/history/tests.rs](src/history/tests.rs) | Determinism + smoke tests for the chronicle pipeline |
| [src/personae.rs](src/personae.rs) | §3 dramatis personae: named characters per faction presence |
| [src/hooks.rs](src/hooks.rs) | §7 plot-hook generator: condition→template over model state |
| [src/prose.rs](src/prose.rs) | §6 gazetteer prose: deterministic template grammar |
| [src/relations.rs](src/relations.rs) | §4 inter-faction diplomacy: stance matrix + tension scalar |
| [src/regions.rs](src/regions.rs) | §5 regional warp phenomena overlay: seeded blob growth + route effects |
| [src/economy.rs](src/economy.rs) | §12 trade & resource economy: production/consumption + route volume |
| [src/segmentum.rs](src/segmentum.rs) | §14 multi-sector composition: child loader + deterministic stitch + super-manifest |
| [src/main.rs](src/main.rs) | `sectorforge` binary entry: parses `cli::Cli`, dispatches to `cli::run`, maps errors to exit 2 |
| [src/cli/mod.rs](src/cli/mod.rs) | Clap `Cli` + `Command` enum + per-variant `run` dispatcher |
| [src/cli/common.rs](src/cli/common.rs) | Shared CLI helpers: JSON printing, validation/invariant/workbook printers, `parse_heatmap`, `load_or_regenerate`, `log_*progress` |
| [src/cli/generate.rs](src/cli/generate.rs) | `generate` + `generate-system` runners (with §15 NEW2 constraint search wiring) |
| [src/cli/validate.rs](src/cli/validate.rs) | `validate`, `validate-sector`, `render-markdown`, `inspect-worlds` runners |
| [src/cli/analyze.rs](src/cli/analyze.rs) | `analyze` runner |
| [src/cli/presets.rs](src/cli/presets.rs) | `new` + `list-presets` runners |
| [src/cli/search.rs](src/cli/search.rs) | `search` runner |
| [src/cli/history.rs](src/cli/history.rs) | `history` runner |
| [src/cli/personae.rs](src/cli/personae.rs) | `personae` runner |
| [src/cli/hooks.rs](src/cli/hooks.rs) | `hooks` runner |
| [src/cli/prose.rs](src/cli/prose.rs) | `prose` runner |
| [src/cli/relations.rs](src/cli/relations.rs) | `relations` runner |
| [src/cli/regions.rs](src/cli/regions.rs) | `regions` runner |
| [src/cli/economy.rs](src/cli/economy.rs) | `economy` runner |
| [src/cli/compose.rs](src/cli/compose.rs) | `compose` runner |
| [src/cli/interestingness.rs](src/cli/interestingness.rs) | `interestingness` runner + profile parser |
| [src/cli/briefing.rs](src/cli/briefing.rs) | `briefing` runner |
| [src/cli/missions.rs](src/cli/missions.rs) | `missions` runner |
| [src/cli/sites.rs](src/cli/sites.rs) | `sites` runner |
| [src/cli/diff.rs](src/cli/diff.rs) | `diff` runner + `DiffArgs` |
| [src/config.rs](src/config.rs) | sectorforge.toml schema |
| [src/rng.rs](src/rng.rs) | Deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name ↔ enum bridge |
| [src/lib.rs](src/lib.rs) | Public API surface (doc-tests + `# Errors`) |
| [src/bitmap/mod.rs](src/bitmap/mod.rs) | Sector PNG facade: `write_bitmap*`, `render_sector_image`, `encode_png_bytes`, `RenderOptions`, top-level `render()` orchestrator |
| [src/bitmap/primitives.rs](src/bitmap/primitives.rs) | Pixel primitives + 5×7 font (shared w/ system_map) |
| [src/bitmap/geom.rs](src/bitmap/geom.rs) | `Geom`, `MapBounds`, hex centre/vertices, `Rect` collision type |
| [src/bitmap/colors.rs](src/bitmap/colors.rs) | Star/route/tint/darken/short color helpers |
| [src/bitmap/grid.rs](src/bitmap/grid.rs) | Hex grid fill + per-system/region tint computation |
| [src/bitmap/routes.rs](src/bitmap/routes.rs) | Route line drawing: stride/jagged/zigzag/disc/chevron/etc. patterns + route control glyphs |
| [src/bitmap/regions.rs](src/bitmap/regions.rs) | §5 warp region label overlay |
| [src/bitmap/systems.rs](src/bitmap/systems.rs) | Star disks, pip text, capital markers |
| [src/bitmap/labels.rs](src/bitmap/labels.rs) | System labels, subsector borders, subsector label placement |
| [src/bitmap/legend.rs](src/bitmap/legend.rs) | Right-hand legend (title, route key, factions, heatmap chip) |
| [src/bitmap/tests.rs](src/bitmap/tests.rs) | Unit tests for the bitmap facade |
| [src/subsectors/mod.rs](src/subsectors/mod.rs) | Subsector clustering + public API |
| [src/subsectors/summary.rs](src/subsectors/summary.rs) | Ownership, faction control, capital selection |
| [gui-core/src/lib.rs](gui-core/src/lib.rs) | Shared egui widgets/utilities used by GUI + builder |
| [gui-core/src/jobs.rs](gui-core/src/jobs.rs) | Background job helper |
| [gui-core/src/palette.rs](gui-core/src/palette.rs) | Color palette / faction glyph rendering |
| [gui-core/src/sector_view.rs](gui-core/src/sector_view.rs) | Read-only hex map render |
| [gui-core/src/system_view.rs](gui-core/src/system_view.rs) | System detail panel widget — also embedded under the SYSTEM tab by `panels/system.rs::show_system_map_section` (§CTX0) |
| [gui-core/src/info_panel.rs](gui-core/src/info_panel.rs) | Text formatting widgets |
| [gui-core/src/heatmap.rs](gui-core/src/heatmap.rs) | GUI heatmap color/cache wrapper |
| [gui-core/src/nav.rs](gui-core/src/nav.rs) | §LINK4 `entity_link` cross-tab link widget |
| [viewer/src/main.rs](viewer/src/main.rs) | Viewer/editor binary entry (`sectorforge-viewer`) |
| [viewer/src/app/mod.rs](viewer/src/app/mod.rs) | Top-level viewer/editor eframe app + navigation |
| [viewer/src/app/export_ui.rs](viewer/src/app/export_ui.rs) | PNG/SVG/HTML/JSON export UI |
| [viewer/src/data_editor.rs](viewer/src/data_editor.rs) | §45 typed `worlds.toml` editor (dropdowns + DragValue) |
| [viewer/src/editor/](viewer/src/editor/) | Sector/world editing UI |
| [builder/src/main.rs](builder/src/main.rs) | Builder binary entry (`sectorforge-builder`) |
| [builder/src/app.rs](builder/src/app.rs) | Thin builder eframe app host |
| [builder/src/builder/](builder/src/builder/) | Builder state, command bus, project I/O, panels |
| [builder/src/builder/state/mod.rs](builder/src/builder/state/mod.rs) | `BuilderState` struct (§D5) + `new_blank` constructor + `default_config` + slice facade. Carries §CTX0 `scroll_target: Option<&'static str>` consumed by `panels/system.rs::show` to scroll the Star section into view when the in-system map's central star is clicked, and §CTX1 `sector_context_menu: Option<SectorContextMenu>` consumed by `panels/map.rs::show_sector_context_menu` to render the right-click menu on the sector map. Phase 3 adds `pending_bulk_rename: Option<PendingBulkRename>` driven from the multi-selection menu's `BULK RENAME…` row and rendered by `panels/map.rs::show_bulk_rename_dialog`. |
| [builder/src/builder/state/types.rs](builder/src/builder/state/types.rs) | UI/dialog types: `BuilderTab`, `MapTool`, `ControlOverlay`, `ModalKind`, `HealthLevel`, `JobHandle`, `PartialRegenRect`, `Pending*` (incl. §CTX1 Phase 3 `PendingBulkRename`), `MapViewCache`, `HistoryWizardState`, `HistoryAnchorKind`, `TickLogEntry`, `TickLogScope`, §CTX1 `SectorContextMenu` (carries `bulk_delete_confirm` for the Phase 3 inline DELETE-ALL gate) + `SectorMenuTarget`, `DEFAULT_*` |
| [builder/src/builder/state/selection.rs](builder/src/builder/state/selection.rs) | §S1/§S4 selection helpers: `focus_system`, `toggle_system_selection` + §LINK2 `focus_entity` / `nav_back` / `nav_forward` |
| [builder/src/builder/state/nav.rs](builder/src/builder/state/nav.rs) | §LINK1 cross-tab navigation: `EntityRef` enum + `target_tab` |
| [builder/src/builder/state/undo.rs](builder/src/builder/state/undo.rs) | R4 command bus: `run`, `undo`, `redo`, ring-buffer trim, `snapshot`, `trigger_auto_save` |
| [builder/src/builder/state/derivations.rs](builder/src/builder/state/derivations.rs) | Economy / relations / chronicle re-derive, debounced validation pump, `synthesize_project_input`, `health_level`, §CF4/§CF5 `advance_conflict_ticks` driver + tick-log capture |
| [builder/src/builder/state/regions_ops.rs](builder/src/builder/state/regions_ops.rs) | §REG1..§REG3 warp-region helpers: add/remove/paint/erase/update/next id |
| [builder/src/builder/state/generation_ops.rs](builder/src/builder/state/generation_ops.rs) | §G2..§G5 + §S5 + §W4: `generate_system_here`, `regenerate_world`, `apply_preview`, `regenerate_partial`, `reroll_seed`, `find_world_indices` |
| [builder/src/builder/panels/conflict.rs](builder/src/builder/panels/conflict.rs) | §CF1..§CF6 conflict + stability editor: per-world / per-system `ConflictState` + `StabilityState`, advance-ticks button, tick log, conflict heatmap toggle |
| [builder/src/builder/panels/missions.rs](builder/src/builder/panels/missions.rs) | §M1..§M5 missions tab: cached `MissionsReport` list + detail card, manual mission editor over `MissionsConfig::manual`, auto-derive + player-edition toggles, click-to-highlight `primary_location` via `focus_entity` |
| [builder/src/builder/panels/prose.rs](builder/src/builder/panels/prose.rs) | §PR1..§PR4 prose tab: per-system + sector overview Override toggles backed by `ProseConfig::overrides`, tone preset combo (Gazetteer / Dispatch), "Regenerate prose" runs `BuilderState::recompute_prose`. Overrides survive every regenerate because `prose::derive_with` re-applies them after the deterministic derivation. |
| [builder/src/builder/panels/briefing.rs](builder/src/builder/panels/briefing.rs) | §BR1..§BR5 briefing tab: `AudiencePreset` picker + observer-faction `ComboBox` + 0..=100 min-confidence slider build a `BriefingProfile`; "Generate briefing" calls `sectorforge::apply_briefing` + `briefing::render_markdown` and caches both `BriefingPack` and redacted Markdown on `BuilderState`. "Export .md + .json" writes the cached pack through `sectorforge::write_briefing` into a folder picked via `rfd::FileDialog`. |
| [builder/src/builder/panels/interestingness.rs](builder/src/builder/panels/interestingness.rs) | §INT1..§INT4 interestingness tab: `ProfileId` picker (PoliticalSandbox/GrimCollapse/Mercantile/Villainous/Frontier), "Score sector" runs `sectorforge::derive_interestingness_with` and caches `InterestingnessReport` on `BuilderState`, per-metric band chart painted via `ui.painter_at` (target band shaded green, observed value ticked), and a per-profile threshold override editor backed by `BuilderState::interestingness_custom_overrides` (keyed by snake-case profile id, seeded from each profile's built-in band). |
