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
| [src/validation.rs](src/validation.rs) | Pre-generation validation |
| [src/invariants.rs](src/invariants.rs) | Post-generation invariants (spec §11.11) |
| [src/render.rs](src/render.rs) | Markdown rendering (sector + standalone system) |
| [src/export.rs](src/export.rs) | JSON / Markdown / CSV / manifest / bitmap writers |
| [src/main.rs](src/main.rs) | CLI (sectorforge binary) |
| [src/config.rs](src/config.rs) | sectorforge.toml schema |
| [src/rng.rs](src/rng.rs) | Deterministic RNG |
| [src/taxonomy.rs](src/taxonomy.rs) | Variant-name ↔ enum bridge |
| [src/lib.rs](src/lib.rs) | Public API surface |
| [src/bitmap.rs](src/bitmap.rs) | PNG rendering (via image crate) |
| [src/gui/app.rs](src/gui/app.rs) | Top-level eframe app + navigation |
| [src/gui/sector_view.rs](src/gui/sector_view.rs) | Hex map render |
| [src/gui/system_view.rs](src/gui/system_view.rs) | System detail panel |
| [src/gui/data_editor.rs](src/gui/data_editor.rs) | CSV data editor |
| [src/gui/info_panel.rs](src/gui/info_panel.rs) | Text formatting widgets |
| [src/gui/editor/](src/gui/editor/) | Sector/world editing UI |
| [src/gui/palette.rs](src/gui/palette.rs) | Color palette |
