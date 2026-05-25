# CLAUDE.md
Do not ever look in, or modify, anything in the "old" directory.
OBEY ALL INSTRUCTIONS IN INPUT.md
When making changes, update GUIDE.md accordingly.
## Commands

```bash
cargo build            # build all targets (sectorforge + sectorforge-gui + sectorforge-builder)
cargo test             # all tests
cargo fmt              # format code
cargo check            # compile check
cargo run --bin sectorforge --help   # CLI help
cargo run -p sectorforge-gui -- --help   # GUI help
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
| [src/svg_export.rs](src/svg_export.rs) | Vector SVG sector map mirroring `bitmap` |
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
| [src/main.rs](src/main.rs) | CLI (sectorforge binary) |
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
| [gui-core/src/system_view.rs](gui-core/src/system_view.rs) | System detail panel widget |
| [gui-core/src/info_panel.rs](gui-core/src/info_panel.rs) | Text formatting widgets |
| [gui-core/src/heatmap.rs](gui-core/src/heatmap.rs) | GUI heatmap color/cache wrapper |
| [gui/src/main.rs](gui/src/main.rs) | Viewer/editor binary entry (`sectorforge-gui`) |
| [gui/src/app/mod.rs](gui/src/app/mod.rs) | Top-level viewer/editor eframe app + navigation |
| [gui/src/app/export_ui.rs](gui/src/app/export_ui.rs) | PNG/SVG/HTML/JSON export UI |
| [gui/src/data_editor.rs](gui/src/data_editor.rs) | §45 typed `worlds.toml` editor (dropdowns + DragValue) |
| [gui/src/editor/](gui/src/editor/) | Sector/world editing UI |
| [builder/src/main.rs](builder/src/main.rs) | Builder binary entry (`sectorforge-builder`) |
| [builder/src/app.rs](builder/src/app.rs) | Thin builder eframe app host |
| [builder/src/builder/](builder/src/builder/) | Builder state, command bus, project I/O, panels |
