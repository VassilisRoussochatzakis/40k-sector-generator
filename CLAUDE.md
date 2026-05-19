# CLAUDE.md
Do not ever look in, or modify, anything in the "old" directory.

When making changes, update GUIDE.md accordingly.
## Commands

```bash
cargo build            # build all targets (sectorforge binary + sectorforge-gui)
cargo test             # all tests
cargo fmt              # format code
cargo check            # compile check
cargo run --bin sectorforge --help   # CLI help
cargo run --bin sectorforge-gui --help   # GUI help
```

## Source layout

| File | Purpose |
|---|---|
| [src/worlds.rs](src/worlds.rs) | Canonical world enums + CSV parser |
| [src/world_pool.rs](src/world_pool.rs) | GenerationRow → weighted candidate pool |
| [src/generation.rs](src/generation.rs) | Sector generation: placement, systems, worlds, factions, routes |
| [src/sector_model.rs](src/sector_model.rs) | Output DTOs with Serialize/Deserialize |
| [src/control.rs](src/control.rs) | Multi-dim presence / claim / control / power derivation |
| [src/validation.rs](src/validation.rs) | Pre-generation validation |
| [src/invariants.rs](src/invariants.rs) | Post-generation invariants (spec §11.11) |
| [src/render.rs](src/render.rs) | Markdown rendering (sector + standalone system) |
| [src/export.rs](src/export.rs) | JSON / Markdown / CSV / manifest / bitmap writers |
| [src/html_export.rs](src/html_export.rs) | §11 self-contained interactive HTML map (inlined JSON + JS canvas renderer + theme CSS) |
| [src/search.rs](src/search.rs) | §2 seed search: declarative wishes → deterministic seed enumeration |
| [src/diff.rs](src/diff.rs) | §10 sector diff: model-aware before/after report |
| [src/history.rs](src/history.rs) | §1 chronicle: dated in-universe event derivation |
| [src/personae.rs](src/personae.rs) | §3 dramatis personae: named characters per faction presence |
| [src/hooks.rs](src/hooks.rs) | §7 plot-hook generator: condition→template over model state |
| [src/prose.rs](src/prose.rs) | §6 gazetteer prose: deterministic template grammar |
| [src/relations.rs](src/relations.rs) | §4 inter-faction diplomacy: stance matrix + tension scalar |
| [src/regions.rs](src/regions.rs) | §5 regional warp phenomena overlay: seeded blob growth + route effects |
| [src/economy.rs](src/economy.rs) | §12 trade & resource economy: production/consumption + route volume |
| [src/main.rs](src/main.rs) | CLI (sectorforge binary) |
| [src/config.rs](src/config.rs) | sectorforge.toml schema |
| [src/rng.rs](src/rng.rs) | Deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name ↔ enum bridge |
| [src/lib.rs](src/lib.rs) | Public API surface (doc-tests + `# Errors`) |
| [src/bitmap/mod.rs](src/bitmap/mod.rs) | Sector PNG rendering |
| [src/bitmap/primitives.rs](src/bitmap/primitives.rs) | Pixel primitives + 5×7 font (shared w/ system_map) |
| [src/subsectors/mod.rs](src/subsectors/mod.rs) | Subsector clustering + public API |
| [src/subsectors/summary.rs](src/subsectors/summary.rs) | Ownership, faction control, capital selection |
| [src/gui/app/mod.rs](src/gui/app/mod.rs) | Top-level eframe app + navigation |
| [src/gui/app/export_ui.rs](src/gui/app/export_ui.rs) | PNG/JSON export UI |
| [src/gui/sector_view.rs](src/gui/sector_view.rs) | Hex map render |
| [src/gui/system_view.rs](src/gui/system_view.rs) | System detail panel |
| [src/gui/data_editor.rs](src/gui/data_editor.rs) | CSV data editor |
| [src/gui/info_panel.rs](src/gui/info_panel.rs) | Text formatting widgets |
| [src/gui/editor/](src/gui/editor/) | Sector/world editing UI |
| [src/gui/palette.rs](src/gui/palette.rs) | Color palette |
