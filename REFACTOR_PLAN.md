# Refactoring Plan — sectorforge workspace

Three independent refactors. They can be done in any order, but the ordering below is from smallest blast radius to largest. Each one ships on its own and behavior must be preserved — these are mechanical/structural changes, not rewrites. After each task: `cargo fmt --all`, `cargo check --workspace --all-targets`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets` should all be green (or no worse than baseline).

---

## Task 1 — Split `builder/src/builder/panels/map.rs`

**Why.** Largest single file in the workspace at 3,341 lines / 23 functions. The file's purpose is coherent ("MAP tab UI") but it has accreted context menus, modal dialogs, brush logic, drag/select interaction, and the partial-regen anchor flow into one module. Splitting is a pure mechanical refactor with zero semantic change.

**Current state.** Single file at `builder/src/builder/panels/map.rs`. Imported from `builder/src/builder/panels/mod.rs`.

**Target state.** Convert to a module directory:

```
builder/src/builder/panels/map/
    mod.rs              // pub fn show(...), show_toolbox, the top-level orchestration
    interactions.rs     // handle_click, handle_drag_drop, apply_rect_select,
                        //   add_route_between, paint/erase brush helpers
    context_menu.rs     // resolve_sector_context, render_empty_hex_menu,
                        //   render_system_menu, render_multi_selection_menu,
                        //   render_route_menu, render_region_hex_menu,
                        //   show_sector_context_menu, should_dismiss_*,
                        //   pivot helpers, stability_label
    dialogs.rs          // show_place_dialog, show_rename_dialog,
                        //   show_bulk_rename_dialog, show_region_rename_dialog,
                        //   show_collision_dialog
    cache.rs            // refresh_map_cache, sector_view_digest
```

**Approach.**
1. Create the directory and the four new files alongside the existing `map.rs`.
2. Move functions by category. Keep visibility (`pub` / `pub(super)` / private) the same — bump to `pub(super)` only where cross-module calls now require it.
3. In `mod.rs`, add `mod interactions; mod context_menu; mod dialogs; mod cache;` and `use` whatever the orchestration code needs.
4. Delete the old `map.rs`.
5. No call sites outside `panels/map.rs` should need to change — the public surface (`show`, `show_toolbox`) stays in `panels/map/mod.rs` and `panels::map::show` resolves identically.

**Acceptance.**
- All builder unit tests pass.
- `cargo clippy -p sectorforge-builder` is no worse than current baseline.
- `git log --follow builder/src/builder/panels/map/mod.rs` should preserve history; use `git mv` for the rename portion if practical.

---

## Task 2 — Reorganize `src/lib.rs` namespace

**Why.** `src/lib.rs` currently declares 56 `pub mod` items at the top level. The crate has clear architectural layers (model → loading → generation → analysis → export → validation → CLI) but `lib.rs` reads as a flat alphabetical wall. Grouping these into a handful of parent modules makes the layering visible to a new reader and to the IDE outline. The public API stays identical via `pub use` re-exports.

**Current state.** ~56 top-level `pub mod` declarations in `src/lib.rs`, followed by a long block of `pub use` re-exports. No internal grouping.

**Target state.** Reorganize into the following parent modules. Existing `src/foo.rs` files become `src/<parent>/foo.rs`; existing `src/foo/` directories become `src/<parent>/foo/`. The exact grouping below is a proposal — if you disagree with where a module lands, document the alternative briefly inside the relevant parent's `mod.rs` doc-comment rather than silently re-shuffling.

```
src/
    model/      // sector_model, ids, errors, rng, taxonomy
    loading/    // input, config, presets, worlds_toml, sector_save
    gen/        // generation, archetypes, world_pool, world_ecs, names,
                //   factions, routes, regions, sites, faction_style,
                //   hidden_routes, orbital_assets, surface_region
    analysis/   // analytics, conflict, control, influence_field,
                //   importance, interestingness, power_projection,
                //   route_control, stability, intel, missions, briefing,
                //   prose, personae, history, hooks, search
    export/     // bitmap, svg_export, html_export, render, export,
                //   segmentum, subsectors, system_map, map_theme,
                //   heatmap
    validate/   // validation, invariants, diff
    cli/        // (already a directory — leaves as-is)
    worlds.rs   // stays at root: it's the canonical world taxonomy
                //   the lib doc-comment singles out as foundational
    worlds_toml.rs  // companion to worlds.rs — keep adjacent
    lib.rs
    main.rs
    bin/
```

**Approach.**
1. Create parent module directories with empty `mod.rs` files (containing only `//! <one-line purpose>` and the child `pub mod` lines).
2. Move files one parent at a time. Use `git mv` so history is preserved.
3. Update intra-crate `use` paths. Most of these are `use crate::foo::Bar` → `use crate::<parent>::foo::Bar`. A bulk find-and-replace per parent is feasible but verify after each.
4. In `lib.rs`, keep the **entire existing `pub use ...` block intact**. Update the paths inside it (`pub use economy::...` → `pub use analysis::economy::...`) so the external API surface is byte-identical. This is the contract that protects the `builder`, `viewer`, and `gui-core` crates from any breakage.
5. After each parent is moved and the workspace builds, commit before starting the next parent. Small commits, not one giant one.

**Acceptance.**
- `cargo doc --no-deps -p sectorforge` produces the same public items (check `target/doc/sectorforge/index.html` before and after — same `pub fn` / `pub struct` count, same names).
- `builder`, `viewer`, and `gui-core` build with **no changes to their `use sectorforge::...` lines**. If any downstream crate had to change, the `pub use` block in `lib.rs` is incomplete — fix the re-exports, not the downstream code.
- Tests pass.

**Do not do.** Don't take this opportunity to also rename modules, change visibility, or extract types. The point is to make the existing structure visible, not to redesign it.

---

## Task 3 — Deduplicate `src/bitmap/` and `src/svg_export/` via a `Canvas` trait

**Why.** This is the one place where the line count is genuinely the codebase's fault, not the domain's. The two modules have the same eleven filenames (`colors.rs`, `geom.rs`, `grid.rs`, `labels.rs`, `legend.rs`, `primitives.rs`, `regions.rs`, `routes.rs`, `systems.rs`, `mod.rs`, `tests.rs`) and shadow each other's logic. Concrete examples:

- `bitmap/colors.rs::star_color` and `svg_export/colors.rs::star_color` are byte-identical match arms.
- `bitmap/colors.rs::stability_color` and `svg_export/colors.rs::stability_color` are identical.
- `bitmap/colors.rs::route_thickness` and `svg_export/colors.rs::route_thickness` differ only in return type (`i32` vs `f32`) and one stray import path.
- `bitmap/routes.rs::draw_routes` and `svg_export/routes.rs::draw_routes` have identical iteration structure, identical `shorten_to_star` math, identical `pattern_with_salt` plumbing — the only real difference is which primitive functions they call at the leaves.
- `svg_export/routes.rs` already imports `crate::bitmap::RenderOptions`, so the abstraction boundary is already leaky in one direction.

Combined size: ~3,700 lines. Conservative estimate of savings after deduplication: ~1,000–1,200 lines, and (more importantly) one place to fix a rendering bug instead of two.

**Current state.** Two parallel module trees under `src/`. Each has its own primitives (`draw_line_thick`, `fill_circle`, `draw_rect_outline` for bitmap; `line`, `circle`, `rect` for SVG) and its own high-level drawing functions that both end up duplicating routing/geometry logic.

**Target state.** A single `src/render_core/` module (or `src/export/render_core/` if Task 2 has landed) containing:

- A `Canvas` trait with the small set of primitive operations both backends need. Sketch:
  ```rust
  pub trait Canvas {
      fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba<u8>, thickness: f32);
      fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgba<u8>);
      fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgba<u8>, thickness: f32);
      fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba<u8>);
      fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba<u8>, thickness: f32);
      fn draw_text(&mut self, x: f32, y: f32, text: &str, color: Rgba<u8>, size: f32, align: TextAlign);
      // ...whatever else the audit below surfaces
  }
  ```
- Shared high-level drawing functions (`draw_routes`, `draw_systems`, `draw_regions`, `draw_grid`, `draw_legend`, `draw_labels`) generic over `C: Canvas` and operating in `f32` world coordinates.
- Shared color/theme helpers (the current `colors.rs` content, deduplicated).
- Shared geometry (`hex_center`, `shorten_to_star`, etc.) in `f32`.

Then `bitmap/` and `svg_export/` each shrink to a `Canvas` implementation plus their respective entry points (the function that returns `RgbaImage` and the function that returns `String`). Bitmap's `Canvas` impl quantizes `f32` to `i32` at the primitive boundary; SVG's impl writes path strings.

**Approach.** Do this in **four small passes**, not one big rewrite. The goal is to keep tests green at every commit.

1. **Pass A — Inventory.** Read both modules end-to-end and write a side-by-side mapping of every drawing function and every primitive. Save it as a comment block at the top of the new `render_core/mod.rs` (or scratch file). This is the design step; don't skip it. Identify the **minimum** primitive set that satisfies all current call sites. Resist adding "future-proofing" primitives.

2. **Pass B — Move shared helpers, no trait yet.** Pull the genuinely-identical helpers (`star_color`, `stability_color`, `RouteStability` → color mapping, `shorten_to_star`, hex geometry constants) into `render_core/` and have both modules import them. This alone should remove a few hundred lines. Tests still pass; no behavior change.

3. **Pass C — Introduce the `Canvas` trait and one backend.** Define the trait. Implement it for bitmap first (because bitmap is the larger module and the more complicated coordinate-quantization story). Rewrite **one** high-level function (recommend starting with `draw_systems` — smallest and least branchy) to be generic over `Canvas`. Have `bitmap/systems.rs` call into it via the trait. Confirm golden tests (`tests/it/golden_png.rs`) still pass.

4. **Pass D — Migrate the rest.** Implement `Canvas` for SVG. Migrate `draw_routes`, `draw_regions`, `draw_grid`, `draw_labels`, `draw_legend` one at a time. Each migration: move the function to `render_core/`, switch both backends to call it, run `cargo test`, commit. When all the high-level functions are shared, delete the now-empty per-backend files.

**Constraints and gotchas.**
- **Determinism.** Output must be byte-identical for the bitmap golden tests. The `f32` → `i32` quantization in the bitmap `Canvas` impl needs to match the existing rounding behavior exactly (the existing `bitmap/primitives.rs` rounds at specific spots). If golden PNGs change, you've got a quantization mismatch — don't update the golden, find the discrepancy.
- **SVG output is text.** SVG tests (`tests/it/svg_export_tests.rs`) may compare strings. If string output diverges (e.g. `12` vs `12.0`, or attribute ordering), match the existing format rather than "improving" it. Format changes belong in a separate PR.
- **`RenderOptions` and `MapTheme`.** Both modules already share `MapTheme`. `RenderOptions` currently lives in `bitmap/` and is imported by `svg_export/`. Move it to `render_core/` as part of Pass B.
- **Do not unify `tests.rs`.** Keep backend-specific tests in their respective modules; they're testing the backend, not the shared logic. Add new tests for the shared logic under `render_core/`.

**Acceptance.**
- `tests/it/golden_png.rs` passes without regenerating the golden files.
- `tests/it/svg_export_tests.rs` passes without changing expected SVG output.
- `wc -l src/bitmap/ src/svg_export/ src/render_core/ -r` shows total reduction of at least 800 lines vs. baseline. (Target ~1,000+, but 800 is a hard floor — if you're below it, the abstraction is leaking and worth a re-think before merging.)
- No new `#[allow(...)]` attributes added to suppress clippy warnings introduced by the refactor.

---

## Order and dependencies

- Task 1 is independent and the safest. Land first.
- Task 2 is independent of Task 1 but touches more files; land second.
- Task 3 is largest. If Task 2 has landed, the new shared module goes under `src/export/render_core/`; if not, it goes at `src/render_core/`. Either is fine — don't block Task 3 on Task 2.

## What's explicitly out of scope

- Renaming public APIs.
- Changing rendering output (pixel-for-pixel and byte-for-byte SVG identical).
- Adding new features.
- "While you're in there" cleanups beyond `cargo fmt` and obvious dead-code removal that the compiler flags after a move.

If any of the above feel tempting during the refactor, leave a `// TODO(refactor):` comment and surface it as a follow-up rather than expanding scope.
