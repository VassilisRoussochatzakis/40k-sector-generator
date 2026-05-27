# Refactor Report — Part 2

Implementation report for the three tasks in [REFACTOR_PLAN.md](REFACTOR_PLAN.md).

## Task 1 — Split `builder/src/builder/panels/map.rs` ✅

3,341-line file → module dir `builder/src/builder/panels/map/`:

- [mod.rs](builder/src/builder/panels/map/mod.rs) — `show`, `show_toolbox`, full test suite (1,338 lines, ~100 prod + tests)
- [interactions.rs](builder/src/builder/panels/map/interactions.rs) — `show_hex_map`, `handle_click`, `handle_drag_drop`, `apply_rect_select`, `paint_region_at`, `add_route_between`, `apply_partial_regen_anchor_click` (460 lines)
- [context_menu.rs](builder/src/builder/panels/map/context_menu.rs) — §CTX1 right-click surface, all `render_*_menu` + `apply_sector_menu_action` + `SectorMenuAction`/`OpenInTarget` enums + `menu_anchor_pivot`/`sector_menu_action_label`/`sector_menu_target_is_stale` (1,113 lines)
- [dialogs.rs](builder/src/builder/panels/map/dialogs.rs) — Place / Rename / Bulk-Rename / Region-Rename / Collision (247 lines)
- [cache.rs](builder/src/builder/panels/map/cache.rs) — `refresh_map_cache` + `sector_view_digest` (98 lines)

`menu_anchor_pivot` re-exported `pub(super)` from `mod.rs` so `panels/system_map.rs` still calls `super::map::menu_anchor_pivot`. Public surface unchanged.

## Task 2 — Reorganize `src/lib.rs` namespace ✅

56 flat `pub mod` decls → six parent modules:

- `model/` — sector_model, ids, errors, rng, taxonomy
- `loading/` — input, config, presets, sector_save
- `gen/` — generation, archetypes, world_pool, world_ecs, names, factions, routes, regions, sites, faction_style, hidden_routes, orbital_assets, surface_region
- `analysis/` — analytics, conflict, control, influence_field, importance, interestingness, power_projection, route_control, stability, intel, missions, briefing, prose, personae, history, hooks, search, economy, relations (last two added here per the plan's "document the alternative" clause)
- `export/` — bitmap, svg_export, html_export, render, segmentum, subsectors, system_map, map_theme, heatmap, writers (the old `src/export.rs`, renamed to free up the parent name — `pub use writers::*` hoist keeps `crate::export::export_all` etc resolving)
- `validate/` — validation, invariants, diff
- `worlds.rs`, `worlds_toml.rs` stay at root per plan
- `cli/` stays at root per plan

All file moves used `git mv` (history preserved). [src/lib.rs](src/lib.rs) re-aliases every moved module at the root (`pub use parent::foo;`) so every existing `crate::foo::Item` and `sectorforge::foo::Item` path resolves unchanged — **zero changes to downstream crates** (builder/viewer/gui-core/tests didn't touch their imports). All 519 tests pass.

## Task 3 — Dedup bitmap/svg_export ⚠️ Pass B only

### Delivered (Pass B)

- New [src/export/render_core/](src/export/render_core/) hosting the genuinely-identical helpers: `star_color`, `stability_color`, `tint_against`, `darken`, `dim`, `short`, `rgba`, `route_thickness_f32`. Both backends now import from here. Single source of truth for theme/route colour math.
- `RenderOptions` lifted out of `bitmap/` into [src/export/render_core/options.rs](src/export/render_core/options.rs); `bitmap::RenderOptions` is now a re-export. Fixes the leaky abstraction (svg_export no longer reaches into bitmap).
- Backend `colors.rs` files shrunk from 95+98 = 193 lines to 30+24 = 54 lines.

### Deferred (Pass C + D)

The `Canvas` trait + shared high-level drawing functions (`draw_routes`, `draw_systems`, etc.) are **not** implemented. Reason: doing it safely needs per-function golden-test verification at each migration step (the bitmap PNGs pin specific `i32` rounding behaviour that the SVG's `f32` math doesn't). One bad quantization choice and `golden_png.rs` starts failing in subtle ways. The risk profile didn't fit in this session.

**Net lines saved by Pass B alone: ~5 lines.** That falls well short of the plan's 800-line floor. The plan's exact wording — "if you're below it, the abstraction is leaking and worth a re-think before merging" — applies. **The architectural value is real** (single colour source, RenderOptions decoupled) but the LOC dedup target requires the deferred passes. [src/export/render_core/mod.rs](src/export/render_core/mod.rs) documents the deferred work for the next pass.

## Documentation

[CLAUDE.md](CLAUDE.md) and [GUIDE.md](GUIDE.md) updated:

- All `src/<file>.rs` markdown links repointed to their new parent-module homes (sed-driven, 207 references).
- CLAUDE.md gained a top-of-section overview of the new parent-module layout.
- The `map.rs` row in CLAUDE.md replaced with five rows (one per submodule).
- New render_core rows added.
- `panels/map.rs` references → `panels/map/mod.rs`.

## Final state

- `cargo check --workspace --all-targets`: clean
- `cargo test --workspace`: **all 13 binaries green, 519 tests passing, 0 failures**
- `cargo clippy --workspace --all-targets`: **26 warnings, same as baseline** (no new lints introduced)
- `cargo fmt --all`: applied

## Recommendation

Land Tasks 1 & 2 confidently. Treat Task 3 as a checkpoint — the render_core scaffolding is in place and one TODO comment documents what Passes C/D need. The 800-line dedup floor is a real follow-up.
