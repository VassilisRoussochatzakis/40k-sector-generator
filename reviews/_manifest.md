# Review Manifest (Phase 1)

Primary review units. Each owned by exactly one agent. Cross-cutting sweeps are in `_xcut/`.

## Primary units

- [x] U001  gui-core/src/{sector_view.rs, system_view.rs, heatmap.rs, map_theme.rs, visual_tokens.rs, nav.rs}  (≈2.5k LOC)  → reviews/gui-core/render.review.md
- [x] U002  gui-core/src/{info_panel.rs, palette.rs, app_icon.rs, jobs.rs, lib.rs}  (≈2.5k LOC)  → reviews/gui-core/ui.review.md
- [x] U003  src/{lib.rs, worlds.rs, worlds_toml.rs}  (≈2.5k LOC)  → reviews/src/lib-core.review.md
- [x] U004  src/model/  (≈2.5k LOC)  → reviews/src/model.review.md
- [x] U005  src/loading/ + src/cli/  (≈2.5k LOC)  → reviews/src/loading-cli.review.md
- [x] U006  src/validate/  (≈2.5k LOC)  → reviews/src/validate.review.md
- [x] U007  src/gen/ (part A: generation/, regions.rs, sites.rs, archetypes.rs, hidden_routes.rs)  (≈3.5k LOC)  → reviews/src/gen-a.review.md
- [x] U008  src/gen/ (part B: orbital_assets.rs, surface_region.rs, world_pool.rs, world_ecs.rs, faction_style.rs, factions.rs, other)  (≈2k LOC)  → reviews/src/gen-b.review.md
- [x] U009  src/analysis/ (part A: economy.rs, relations.rs, search.rs)  (≈4.7k LOC)  → reviews/src/analysis-a.review.md
- [x] U010  src/analysis/ (part B: personae.rs, analytics.rs, hooks.rs, missions.rs, briefing.rs, prose.rs)  (≈4.6k LOC)  → reviews/src/analysis-b.review.md
- [x] U011  src/analysis/ (part C: control.rs, route_control.rs, stability.rs, intel.rs, influence_field.rs, power_projection.rs, interestingness.rs, conflict.rs, importance.rs)  (≈4k LOC)  → reviews/src/analysis-c.review.md
- [x] U012  src/export/ (part A: segmentum.rs, subsectors/, system_map.rs)  (≈3.5k LOC)  → reviews/src/export-a.review.md
- [x] U013  src/export/ (part B: render.rs, render_core/, map_theme.rs, html_export.rs, heatmap.rs, writers.rs)  (≈3k LOC)  → reviews/src/export-b.review.md
- [x] U014  src/export/ (part C: bitmap/, svg_export/)  (≈2k LOC)  → reviews/src/export-c.review.md
- [x] U015  builder/src/builder/{state/, command.rs, project_io.rs, session.rs, workspace.rs, preview.rs}, builder/src/{app.rs, lib.rs, main.rs}  (≈4.5k LOC)  → reviews/builder/core.review.md
- [x] U016  builder/src/builder/panels/ (part A: map/, system.rs, system_map.rs, world.rs, routes.rs)  (≈6.7k LOC)  → reviews/builder/panels-map.review.md
- [x] U017  builder/src/builder/panels/ (part B: history.rs, control.rs, relations.rs, factions.rs, economy.rs)  (≈6.4k LOC)  → reviews/builder/panels-econ.review.md
- [x] U018  builder/src/builder/panels/ (part C: missions.rs, hooks.rs, subsectors.rs, regions.rs, generation.rs, interestingness.rs, intel.rs, personae.rs, prose.rs, conflict.rs, briefing.rs, sites.rs, orbital.rs, invariants.rs, validation.rs, surface_regions.rs)  (≈6.8k LOC)  → reviews/builder/panels-misc.review.md
- [x] U019  viewer/src/{factions_overview.rs, segmentum_view.rs, route_planner.rs, dashboard.rs, data_editor.rs, preset_gallery.rs}  (≈3.6k LOC)  → reviews/viewer/views.review.md
- [x] U020  viewer/src/app/  (≈2.5k LOC)  → reviews/viewer/app.review.md
- [x] U021  viewer/src/editor/ + viewer/src/{lib.rs, main.rs}  (≈2.5k LOC)  → reviews/viewer/editor.review.md
- [x] U022  tests/it/ + benches/generation.rs + gui-core/tests/  (≈2.5k LOC)  → reviews/tests/integration.review.md

## Cross-cutting sweeps

- [x] X01  unsafe-audit       → reviews/_xcut/unsafe-audit.review.md  (NOTE: 0 unsafe blocks — should be a one-line confirmation)
- [x] X02  public-api         → reviews/_xcut/public-api.review.md
- [x] X03  error-model        → reviews/_xcut/error-model.review.md
- [x] X04  concurrency        → reviews/_xcut/concurrency.review.md  (NOTE: no async; rayon-only)
- [x] X05  perf-hotpath       → reviews/_xcut/perf-hotpath.review.md
- [x] X06  dependencies       → reviews/_xcut/dependencies.review.md
- [x] X07  panic-surface      → reviews/_xcut/panic-surface.review.md
- [x] X08  testing            → reviews/_xcut/testing.review.md

## Final

- [x] Phase 4 aggregation → FINDINGS.MD + RUST_FIXES.md
