//! ITERATIVE_GENERATION.md Phase P — the stage-by-stage random-generation panel.
//!
//! Renders the transient [`IterativeGenSession`] (held on
//! [`BuilderState::iterative_gen`], never document state — invariant #5 / §2.4
//! carve-out) as a wizard:
//!
//! - a left **step rail** ([`GenStep::ALL`]): current highlighted, accepted
//!   steps checked, downstream steps disabled until reached;
//! - a central **knob form** for the current step (§5 maps each step to its
//!   config fields) whose widgets read/write `session.config.*` **directly**
//!   (transient) — every edit is followed by [`BuilderState::note_config_edit`]
//!   (or [`BuilderState::set_grid_dim`] for the square grid field) so the DAG is
//!   invalidated and the preview re-runs;
//! - a central **live preview** via the shared [`SectorView`] widget, fed from
//!   `session.preview` (the partial [`GeneratedSector`] produced by
//!   [`sectorforge::generate_prefix`]); and
//! - a bottom **action bar**: 🎲 re-roll · ◀ back · ▶ next · ✓ commit · ✕ cancel.
//!
//! # Where the randomness comes from (invariant #1)
//!
//! Nothing in this panel draws entropy. The *only* generator is the engine: a
//! knob edit / re-roll / step change re-runs [`sectorforge::generate_prefix`]
//! off-thread (via [`BuilderState::spawn_prefix`] underneath the session ops),
//! and re-rolls flow through the per-stage [`sectorforge::RerollNonces`]. There
//! is no `rand::thread_rng()` here; the sole fresh-entropy point is the
//! `Size`-step re-roll, which mints a new root seed inside
//! [`BuilderState::reroll_step`].
//!
//! # Square sectors (invariant #4)
//!
//! The `Size` step exposes a **single** grid-dimension field bound to both width
//! and height through [`BuilderState::set_grid_dim`] — never a path that lets the
//! two diverge, so `GEN_SECTOR_NOT_SQUARE` can never trip.
//!
//! # Preview cutoff nuance (verified, by design)
//!
//! A literal `Stage::Placement` cutoff builds no [`sectorforge::sector_model::GeneratedSystem`]s
//! (systems are constructed at `Stage::Systems`), so it would render an
//! empty-of-systems grid. For a *useful* preview the early structural steps
//! (`Placement` / `Regions`) render the prefix through `Stage::Systems` so placed
//! systems are visible, while a **re-roll** of that step still bumps the step's
//! *own* stage nonce — see [`GenStep::preview_through`]. The panel surfaces this
//! to the user in the per-step hint.
//!
//! # Regions (editable via a transient session patch — §5)
//!
//! `RegionsConfig` lives in `data_catalogs.regions` (an `Option<RegionsConfig>`),
//! sourced from a *separate* TOML (`data/routes/regions.toml`), **not** in
//! [`sectorforge::AppConfig`]. ITERATIVE_GENERATION.md §5 says to edit it "via an
//! in-memory patch applied at load, preserving the split". The `Regions` step
//! therefore exposes the four scalar knobs (`enabled` / `count` / `mean_size` /
//! `apply_to_routes`) bound to [`IterativeGenSession::regions_override`] — a
//! **transient** `Option<RegionsConfig>` lazily seeded from the effective catalog
//! config on the first edit, written **directly** (no `BuilderCommand`; invariant
//! #5 carve-out). The override is substituted for the regions catalog **only when
//! assembling the `ProjectInput`** for a prefix run (via
//! [`BuilderState::synthesize_project_input_with`]) — it is never written into
//! `data_catalogs` or `sector.json` during preview, so `override == None` stays
//! byte-identical to the legacy path (invariants #2/#6). Only the new-project
//! **commit** folds the patch into the persisted regions catalog, at the
//! `open_project` session boundary. The per-condition weights list is heavier and
//! shown read-only for v1; the four scalars are the editable surface. Re-rolling
//! the step still re-rolls region placement (`Stage::Regions` nonce).

use egui::{RichText, Sense, Ui};

use sectorforge::config::{PlacementMode, WorldSelectionMode};
use sectorforge::SectorProgress;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::sector_view::{SectorGeom, SectorView};
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::state::GenStep;
use crate::builder::BuilderState;

/// Phase P entry point. Renders the iterative-generation wizard when a session
/// is active; otherwise shows a short placeholder explaining how to start one.
pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    // NOTE: `pump_prefix` borrows `&mut state`, so drain the worker channel
    // *before* borrowing the session for rendering. A landed/failed result asks
    // for a repaint so the embedded `SectorView` picks up the new preview.
    if state.pump_prefix() {
        ui.ctx().request_repaint();
    }

    if state.iterative_gen.is_none() {
        show_no_session(ui);
        return;
    }

    ui.heading("Iterative sector");
    ui.label(
        RichText::new(
            "Walk the generator one step at a time — size, placement, regions, contents, \
             factions, routes, finalize. Tune the knobs and re-roll each step, then commit.",
        )
        .color(palette::chrome_text_dim()),
    );
    ui.add_space(4.0);

    // Bottom action bar first (egui lays bottom panels before the central fill),
    // so the rail + form + preview flow into the space that remains.
    show_action_bar(ui, state);

    egui::SidePanel::left("iterative_gen_rail")
        .resizable(true)
        .default_width(280.0)
        .width_range(220.0..=360.0)
        .show_inside(ui, |ui| show_step_rail(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| {
        // Surface a failed prefix run (recorded by `pump_prefix`).
        if let Some(err) = state.feedback.last_command_error.clone() {
            ui.colored_label(palette::danger(), format!("⚠ {err}"));
            ui.add_space(2.0);
        }
        show_step_form(ui, state);
        ui.separator();
        show_preview(ui, state);
    });
}

// ── no-session placeholder ────────────────────────────────────────────────────

fn show_no_session(ui: &mut Ui) {
    ui.heading("Iterative sector");
    ui.add_space(6.0);
    ui_kit::placeholder(
        ui,
        "No iterative session is open. Start one from PROJECT → “Iterative sector…”.",
    );
}

// ── step rail (left) ──────────────────────────────────────────────────────────

/// The 7-step rail. The current step is highlighted; steps at or before
/// `accepted_through` are checked; steps *after* the current one are disabled
/// until reached (you advance with ▶ Next). Clicking an enabled, already-visited
/// step jumps back to it (re-running its preview).
fn show_step_rail(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("STEPS").small().color(palette::chrome_text_dim()));
    ui.separator();

    let Some(session) = state.iterative_gen.as_ref() else {
        return;
    };
    let current = session.current_step;
    let accepted = session.accepted_through;
    let running = session.is_running();

    // The furthest step the user may click: the current step, or one past the
    // accepted-through frontier (whichever is later). Steps beyond that are
    // disabled until Next walks the prefix forward to them.
    let frontier = accepted
        .map(|a| a.next().unwrap_or(a))
        .unwrap_or(GenStep::Size)
        .max(current);

    let mut jump_to: Option<GenStep> = None;
    for step in GenStep::ALL {
        let is_current = step == current;
        let is_done = accepted.is_some_and(|a| step <= a);
        let reachable = step <= frontier;

        let mark = if is_current {
            "▶"
        } else if is_done {
            "✓"
        } else {
            "○"
        };
        let mut text = RichText::new(format!("{mark}  {}", step.label()));
        if is_current {
            text = text.strong().color(palette::info());
        } else if is_done {
            text = text.color(palette::success());
        } else if !reachable {
            text = text.color(palette::chrome_text_dim());
        }

        // Clicking the current step is a no-op; jumping forward of the frontier
        // is disabled; a running prefix freezes navigation to avoid stacking
        // dispatches.
        let clickable = reachable && !is_current && !running;
        let resp = ui.add_enabled(clickable, egui::Button::new(text).frame(is_current));
        let resp = resp.on_hover_text(step_hint(step));
        if resp.clicked() {
            jump_to = Some(step);
        }
    }

    ui.add_space(8.0);
    ui.separator();
    let step_no = current.index() + 1;
    ui.label(
        RichText::new(format!("Step {step_no} of {}", GenStep::ALL.len()))
            .small()
            .color(palette::chrome_text_dim()),
    );

    // Jump back to an enabled, previously-visited step. We only walk via
    // `step_back` (one at a time) so `current_step`'s preview is always re-run by
    // the session op; this keeps the rail honest without a bespoke "set_step".
    if let Some(target) = jump_to {
        if target < current {
            // Step backward until we land on the target (each hop re-runs the
            // prefix through the new current step).
            while state
                .iterative_gen
                .as_ref()
                .is_some_and(|s| s.current_step > target)
            {
                state.step_back(ui.ctx());
            }
        }
    }
}

/// Short per-step hint shown on the rail hover and above the form, including the
/// documented preview-cutoff nuance for the early structural steps.
fn step_hint(step: GenStep) -> &'static str {
    match step {
        GenStep::Size => {
            "Sector size (square) and root seed. No engine run — the grid is empty until \
             you place systems."
        }
        GenStep::Placement => {
            "Where star systems sit. Preview renders through System contents so the placed \
             systems are visible; re-roll moves only placement (and everything downstream)."
        }
        GenStep::Regions => {
            "Warp regions seed anomaly hexes that bias world selection, so they run before \
             System contents. Tune them here (an in-memory patch over data/routes/regions.toml); \
             preview renders through System contents; re-roll re-rolls placement (and downstream)."
        }
        GenStep::Systems => "Worlds per system, features, and star-colour bias.",
        GenStep::Factions => "Faction roster and assignment (edited in the FACTIONS tab).",
        GenStep::Routes => "Warp routes: density, distance, and connectivity.",
        GenStep::Finalize => {
            "Overlays through the chronicle (relations, economy, history). Run the full prefix \
             before committing."
        }
    }
}

// ── step knob form (central, top) ─────────────────────────────────────────────

fn show_step_form(ui: &mut Ui, state: &mut BuilderState) {
    let Some(step) = state.iterative_gen.as_ref().map(|s| s.current_step) else {
        return;
    };

    ui.heading(step.label());
    ui.label(RichText::new(step_hint(step)).color(palette::chrome_text_dim()));
    ui.add_space(4.0);

    match step {
        GenStep::Size => show_size_form(ui, state),
        GenStep::Placement => show_placement_form(ui, state),
        GenStep::Regions => show_regions_form(ui, state),
        GenStep::Systems => show_systems_form(ui, state),
        GenStep::Factions => show_factions_form(ui, state),
        GenStep::Routes => show_routes_form(ui, state),
        GenStep::Finalize => show_finalize_form(ui, state),
    }
}

/// Size & seed. **Invariant #4:** one grid-dimension field bound to *both* width
/// and height via [`BuilderState::set_grid_dim`]; never write width/height apart.
fn show_size_form(ui: &mut Ui, state: &mut BuilderState) {
    ui.colored_label(
        palette::warning(),
        "◧ Sectors are always square — one dimension drives both width and height.",
    );
    ui.add_space(2.0);

    // Read the current dim out, edit a local, then route the change through
    // `set_grid_dim` (which writes both sides + re-runs the preview).
    let mut dim = state
        .iterative_gen
        .as_ref()
        .map(|s| s.config.generation.sector_width)
        .unwrap_or(16);
    let mut dim_changed = false;
    labeled(
        ui,
        "Grid dimension",
        "Sector size in hexes per side (schema: sector_width = sector_height). Sectors are \
         square, so this single field sets both.",
        |ui| {
            dim_changed = ui
                .add(egui::DragValue::new(&mut dim).range(1..=sectorforge::random_sector::MAX_CUSTOM_DIM))
                .changed();
        },
    );
    if dim_changed {
        state.set_grid_dim(ui.ctx(), dim);
    }

    // Seed: write directly to the session (transient), keep `root_seed` in sync,
    // then note the edit so the Size step + everything downstream re-derives.
    let mut seed = state
        .iterative_gen
        .as_ref()
        .map(|s| s.config.generation.seed.clone())
        .unwrap_or_default();
    let mut seed_changed = false;
    labeled(
        ui,
        "Seed",
        "Root seed every stage is keyed from (schema: seed). Same seed + same knobs = the \
         same sector. Use 🎲 below to mint a fresh one.",
        |ui| {
            seed_changed = ui
                .add(egui::TextEdit::singleline(&mut seed).hint_text("root seed"))
                .changed();
        },
    );
    if seed_changed {
        if let Some(session) = state.iterative_gen.as_mut() {
            session.config.generation.seed = seed.clone();
            session.root_seed = seed;
        }
        state.note_config_edit(ui.ctx(), GenStep::Size);
    }

    ui.add_space(4.0);
    ui.small(
        "Tip: 🎲 Re-roll this step mints a brand-new seed and re-rolls every knob; editing the \
         seed by hand keeps the rest of your knobs.",
    );
}

/// System placement — `system_count` + [`sectorforge::config::PlacementConfig`].
fn show_placement_form(ui: &mut Ui, state: &mut BuilderState) {
    let mut changed = false;
    if let Some(session) = state.iterative_gen.as_mut() {
        let gen = &mut session.config.generation;
        labeled(
            ui,
            "Star systems",
            "How many star systems to place (schema: system_count).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.system_count).range(0..=2000))
                    .changed();
            },
        );

        ui.add_space(4.0);
        ui.label(RichText::new("Placement").strong());
        ui.separator();

        let placement = &mut gen.placement;
        labeled(
            ui,
            "Mode",
            "How systems are scattered across the grid (schema: placement.mode).",
            |ui| {
                changed |= placement_mode_combo(ui, &mut placement.mode);
            },
        );
        labeled(
            ui,
            "Cluster bias",
            "How strongly systems clump together (schema: placement.cluster_bias). 0 = even.",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut placement.cluster_bias)
                            .speed(0.01)
                            .range(0.0..=1.0),
                    )
                    .changed();
            },
        );
        labeled(
            ui,
            "Minimum distance",
            "Smallest gap (in hexes) allowed between two systems (schema: \
             placement.minimum_system_distance).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut placement.minimum_system_distance).range(1..=20))
                    .changed();
            },
        );
    }
    if changed {
        state.note_config_edit(ui.ctx(), GenStep::Placement);
    }
}

/// Warp regions — editable via the transient session **override patch**
/// (ITERATIVE_GENERATION.md §5). The four scalar `RegionsConfig` knobs
/// (`enabled` / `count` / `mean_size` / `apply_to_routes`) are bound to
/// [`IterativeGenSession::regions_override`], lazily seeded from the effective
/// catalog config on the first edit. Edits are written **directly** to the
/// session (no `BuilderCommand`; invariant #5 carve-out) and followed by
/// [`BuilderState::note_config_edit`] so Regions + downstream re-derive. The
/// override never touches `data_catalogs` during preview — it is substituted into
/// the `ProjectInput` only at run-assembly (`synthesize_project_input_with`). The
/// per-condition weights list is shown read-only for v1.
fn show_regions_form(ui: &mut Ui, state: &mut BuilderState) {
    ui.colored_label(
        palette::warning(),
        "Warp regions come from a separate file (data/routes/regions.toml). Edits here are an \
         in-memory patch for this wizard only — applied to the preview now and persisted into the \
         new project on Commit; the original file is untouched until then.",
    );
    ui.add_space(4.0);

    // The *effective* config the preview is using: the session override patch if
    // the user has touched it, otherwise the project catalog, otherwise defaults.
    // Edits below operate on a local copy of this, then commit it wholesale into
    // the session override (so the catalog's `conditions` list is preserved into
    // the override + the eventual commit).
    let mut cfg = state
        .iterative_gen
        .as_ref()
        .and_then(|s| s.regions_override.clone())
        .or_else(|| state.data_catalogs.regions.clone())
        .unwrap_or_default();

    let mut changed = false;
    labeled(
        ui,
        "Enabled",
        "Whether the regions stage runs at all (schema: regions.enabled). Off = no warp overlays.",
        |ui| {
            changed |= ui.checkbox(&mut cfg.enabled, "").changed();
        },
    );
    labeled(
        ui,
        "Count",
        "Approximate number of regions to grow (schema: regions.count). Clamped to fit the grid.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.count).range(0..=64))
                .changed();
        },
    );
    labeled(
        ui,
        "Mean size",
        "Mean footprint in hexes per region (schema: regions.mean_size). Clamped to >= 1.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.mean_size).range(1..=200))
                .changed();
        },
    );
    labeled(
        ui,
        "Apply to routes",
        "Whether region effects hard-rewrite route stability (schema: regions.apply_to_routes). \
         Off = advisory tags only.",
        |ui| {
            changed |= ui.checkbox(&mut cfg.apply_to_routes, "").changed();
        },
    );

    if changed {
        // Write the patched config directly to the session (transient — no
        // command), then re-run the prefix so the preview reflects the new
        // regions and every downstream stage (invariant #5; §2.3 DAG rule).
        if let Some(session) = state.iterative_gen.as_mut() {
            session.regions_override = Some(cfg.clone());
        }
        state.note_config_edit(ui.ctx(), GenStep::Regions);
    }

    // Per-condition weights are heavier to edit; show a labeled read-only summary
    // for v1 (the list still flows through to the preview + commit verbatim).
    ui.add_space(4.0);
    ui.label(RichText::new("Conditions").strong());
    ui.separator();
    if cfg.conditions.is_empty() {
        ui.small(
            "Using the built-in condition pool (warp storm / turbulence / calm corridor / \
             blackout / anomaly). Edit the per-condition weights in the REGIONS tab.",
        );
    } else {
        for entry in &cfg.conditions {
            let name = entry
                .label
                .clone()
                .unwrap_or_else(|| entry.kind.label().to_string());
            ui.small(format!("• {name} — weight {:.2}", entry.weight));
        }
        ui.add_space(2.0);
        ui.small("Per-condition weights are read-only here; edit them in the REGIONS tab.");
    }

    if !cfg.enabled {
        ui.add_space(4.0);
        ui.small("Regions are disabled — enable them above to grow warp overlays in the preview.");
    }
}

/// System contents — world counts + [`sectorforge::config::WorldSelectionConfig`].
fn show_systems_form(ui: &mut Ui, state: &mut BuilderState) {
    let mut changed = false;
    if let Some(session) = state.iterative_gen.as_mut() {
        let gen = &mut session.config.generation;
        labeled(
            ui,
            "Min worlds / system",
            "Fewest worlds a system may have (schema: min_worlds_per_system).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.min_worlds_per_system).range(0..=20))
                    .changed();
            },
        );
        labeled(
            ui,
            "Max worlds / system",
            "Most worlds a system may have (schema: max_worlds_per_system).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.max_worlds_per_system).range(0..=20))
                    .changed();
            },
        );
        // Keep min <= max so the generator never sees an empty range.
        if gen.min_worlds_per_system > gen.max_worlds_per_system {
            gen.max_worlds_per_system = gen.min_worlds_per_system;
        }
        labeled(
            ui,
            "Features / world",
            "How many notable features to roll per world (schema: world_feature_count).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.world_feature_count).range(0..=20))
                    .changed();
            },
        );

        ui.add_space(4.0);
        ui.label(RichText::new("World selection").strong());
        ui.separator();

        let ws = &mut gen.world_selection;
        labeled(
            ui,
            "Mode",
            "How world rows are sampled (schema: world_selection.mode).",
            |ui| {
                changed |= world_selection_mode_combo(ui, &mut ws.mode);
            },
        );
        labeled(
            ui,
            "Same star-colour bias",
            "How strongly worlds prefer their system's star colour (schema: \
             world_selection.same_star_colour_bias).",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut ws.same_star_colour_bias)
                            .speed(0.05)
                            .range(0.0..=5.0),
                    )
                    .changed();
            },
        );
        labeled(
            ui,
            "Strict star colour",
            "Require worlds to match the star colour exactly (schema: \
             world_selection.strict_same_star_colour).",
            |ui| {
                changed |= ui
                    .checkbox(&mut ws.strict_same_star_colour, "")
                    .changed();
            },
        );
        labeled(
            ui,
            "Avoid duplicate types",
            "Avoid repeating a world type within one system (schema: \
             world_selection.avoid_duplicate_world_type_in_system).",
            |ui| {
                changed |= ui
                    .checkbox(&mut ws.avoid_duplicate_world_type_in_system, "")
                    .changed();
            },
        );
    }
    if changed {
        state.note_config_edit(ui.ctx(), GenStep::Systems);
    }
}

/// Factions — read-only. The roster is document state in the data catalogs (not
/// a scalar config block), and ITERATIVE_GENERATION.md §5 frames it that way with
/// no session-override recipe (unlike the Regions patch). So the wizard inherits
/// the project roster and only *assigns* from it; editing the roster itself
/// happens in the FACTIONS tab, through the command bus.
fn show_factions_form(ui: &mut Ui, state: &mut BuilderState) {
    ui.colored_label(
        palette::warning(),
        "The faction roster is project data, edited in the FACTIONS tab — not in this wizard. The \
         wizard assigns factions from that roster; re-roll re-assigns without changing it.",
    );
    ui.add_space(2.0);
    let n = state
        .data_catalogs
        .factions
        .as_ref()
        .map(|f| f.factions.len())
        .unwrap_or(0);
    labeled(
        ui,
        "Roster size",
        "How many factions are available to assign.",
        |ui| {
            ui.label(RichText::new(n.to_string()).monospace());
        },
    );
    ui.add_space(4.0);
    ui.small("Re-roll this step to re-assign factions to worlds without changing the roster.");
}

/// Routes — [`sectorforge::config::RouteGenerationConfig`].
fn show_routes_form(ui: &mut Ui, state: &mut BuilderState) {
    let mut changed = false;
    if let Some(session) = state.iterative_gen.as_mut() {
        let routes = &mut session.config.generation.routes;
        labeled(
            ui,
            "Enabled",
            "Whether warp routes are generated at all (schema: routes.enabled).",
            |ui| {
                changed |= ui.checkbox(&mut routes.enabled, "").changed();
            },
        );
        labeled(
            ui,
            "Route density",
            "How many routes to draw, as a fraction of the candidates (schema: \
             routes.route_density).",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut routes.route_density)
                            .speed(0.01)
                            .range(0.0..=1.0),
                    )
                    .changed();
            },
        );
        labeled(
            ui,
            "Max distance",
            "Longest route allowed, in hexes (schema: routes.max_route_distance).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut routes.max_route_distance).range(1..=64))
                    .changed();
            },
        );
        labeled(
            ui,
            "Ensure connected",
            "Add bridging routes so every system is reachable (schema: \
             routes.ensure_connected_graph).",
            |ui| {
                changed |= ui.checkbox(&mut routes.ensure_connected_graph, "").changed();
            },
        );
    }
    if changed {
        state.note_config_edit(ui.ctx(), GenStep::Routes);
    }
    ui.add_space(4.0);
    ui.small(
        "Note: route selection is currently deterministic given the systems + these knobs, so \
         re-roll only changes routes once the knobs change.",
    );
}

/// Finalize & overlays — advanced knobs
/// ([`sectorforge::config::RelationsGenerationConfig`]).
fn show_finalize_form(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(
        RichText::new(
            "These run after the structural sector: relations, economy, and the chronicle. \
             Commit runs the full prefix.",
        )
        .color(palette::chrome_text_dim()),
    );
    ui.add_space(4.0);

    let mut changed = false;
    if let Some(session) = state.iterative_gen.as_mut() {
        let relations = &mut session.config.generation.relations;
        labeled(
            ui,
            "Min world presence",
            "Drop factions with fewer than this many world presences from the relations matrix \
             (schema: relations.min_world_presence).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut relations.min_world_presence).range(1..=50))
                    .changed();
            },
        );
    }
    if changed {
        state.note_config_edit(ui.ctx(), GenStep::Finalize);
    }
}

// ── combos ────────────────────────────────────────────────────────────────────

const PLACEMENT_MODES: &[(PlacementMode, &str)] = &[
    (PlacementMode::UniformGrid, "Uniform grid"),
    (PlacementMode::WeightedGrid, "Weighted grid"),
    (PlacementMode::Clustered, "Clustered"),
];

fn placement_mode_combo(ui: &mut Ui, mode: &mut PlacementMode) -> bool {
    let mut changed = false;
    let label = PLACEMENT_MODES
        .iter()
        .find(|(m, _)| m == mode)
        .map_or("Uniform grid", |(_, l)| *l);
    ui_kit::combo("iter_placement_mode", label).show_ui(ui, |ui| {
        for (m, l) in PLACEMENT_MODES {
            changed |= ui.selectable_value(mode, *m, *l).changed();
        }
    });
    changed
}

const WORLD_SELECTION_MODES: &[(WorldSelectionMode, &str)] =
    &[(WorldSelectionMode::WeightedRows, "Weighted rows")];

fn world_selection_mode_combo(ui: &mut Ui, mode: &mut WorldSelectionMode) -> bool {
    let mut changed = false;
    let label = WORLD_SELECTION_MODES
        .iter()
        .find(|(m, _)| m == mode)
        .map_or("Weighted rows", |(_, l)| *l);
    ui_kit::combo("iter_world_selection_mode", label).show_ui(ui, |ui| {
        for (m, l) in WORLD_SELECTION_MODES {
            changed |= ui.selectable_value(mode, *m, *l).changed();
        }
    });
    changed
}

// ── live preview (central, bottom) ────────────────────────────────────────────

fn show_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Preview").strong());
        if let Some(session) = state.iterative_gen.as_ref() {
            if session.is_running() {
                // Real per-stage fill (was a constant 0.5, so it looked frozen)
                // plus an inline percentage so it's obvious the engine is working.
                ui.add(
                    egui::ProgressBar::new(session.fraction())
                        .desired_width(220.0)
                        .show_percentage()
                        .animate(true),
                );
                let detail = session
                    .progress_snapshot()
                    .map(progress_detail)
                    .unwrap_or_else(|| "Starting…".to_string());
                ui.label(RichText::new(detail).small().color(palette::chrome_text_dim()));
                // Keep the frame loop spinning so the bar animates + the result
                // is pumped next frame.
                ui.ctx().request_repaint();
            }
        }
    });
    ui.add_space(2.0);

    let hex_size = 14.0_f32;
    let preview_sector = state
        .iterative_gen
        .as_ref()
        .and_then(|s| s.preview.as_ref());

    let Some(sector) = preview_sector else {
        let session = state.iterative_gen.as_ref();
        // Three distinct empty states — don't show a static "Building…" when
        // nothing is actually building (the old text lied in the idle/failed case).
        if session.is_some_and(|s| s.is_running()) {
            // A run is genuinely in flight but hasn't produced a sector yet: a
            // live spinner in the central area (the header row carries the bar/%).
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Building preview…").color(palette::chrome_text_dim()));
            });
            ui.ctx().request_repaint();
            return;
        }
        let on_size_step = session.is_some_and(|s| s.current_step == GenStep::Size);
        let msg = if on_size_step {
            "No systems yet — pick a size and seed, then step to System placement to see the grid."
        } else {
            "No preview yet — press “▶  Next” or “🎲  Re-roll this step” to build it."
        };
        ui_kit::placeholder(ui, msg);
        return;
    };

    if sector.width == 0 || sector.height == 0 {
        ui_kit::placeholder(ui, "Sector has zero extent.");
        return;
    }

    // Mirror the `panels/map/interactions.rs` embed: size the canvas exactly,
    // confine the render to that rect, and disable internal click dispatch (the
    // preview is non-interactive). Wrapped in a scroll area so a big grid does
    // not overflow the central panel.
    egui::ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let geom = SectorGeom::new(hex_size, egui::Pos2::ZERO);
            let canvas_size = geom.map_size_px(sector.width, sector.height);
            let (rect, _response) = ui.allocate_exact_size(canvas_size, Sense::hover());
            let origin = rect.min;
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                SectorView {
                    hex_size,
                    origin,
                    route_view_mode: sectorforge::sector_model::RouteViewMode::Detailed,
                    disable_internal_click_dispatch: true,
                    show_hover_coord: false,
                    ..SectorView::new(sector)
                }
                .show(ui);
            });
        });
}

// ── action bar (bottom) ───────────────────────────────────────────────────────

fn show_action_bar(ui: &mut Ui, state: &mut BuilderState) {
    egui::TopBottomPanel::bottom("iterative_gen_actions").show_inside(ui, |ui| {
        ui.add_space(4.0);
        let (current, has_prev, has_next, running, has_dest) = {
            let Some(session) = state.iterative_gen.as_ref() else {
                return;
            };
            (
                session.current_step,
                session.current_step.prev().is_some(),
                session.current_step.next().is_some(),
                session.is_running(),
                session.dest.is_some(),
            )
        };

        ui.horizontal(|ui| {
            // 🎲 Re-roll this step. Disabled while a prefix run is in flight.
            let reroll_label = if current == GenStep::Size {
                "🎲  New seed"
            } else {
                "🎲  Re-roll this step"
            };
            if ui
                .add_enabled(!running, egui::Button::new(reroll_label))
                .on_hover_text(reroll_hint(current))
                .clicked()
            {
                state.reroll_step(ui.ctx(), current);
            }

            ui.separator();

            if ui
                .add_enabled(has_prev && !running, egui::Button::new("◀  Back"))
                .on_hover_text("Go to the previous step")
                .clicked()
            {
                state.step_back(ui.ctx());
            }
            if ui
                .add_enabled(has_next && !running, egui::Button::new("▶  Next"))
                .on_hover_text("Accept this step and move to the next")
                .clicked()
            {
                state.step_next(ui.ctx());
            }

            ui.separator();

            // ✓ Commit — Phase C1 new-project commit: runs the full sector,
            // writes a new project, and reopens it (a session boundary).
            let commit_hint = if has_dest {
                "Run the full sector and save it as a new project"
            } else {
                "Choose a destination folder, run the full sector, and save it as a new project"
            };
            if ui
                .add_enabled(!running, egui::Button::new("✓  Commit…"))
                .on_hover_text(commit_hint)
                .clicked()
            {
                commit(state, ui);
            }

            if ui
                .button("✕  Cancel")
                .on_hover_text("Discard the in-progress wizard")
                .clicked()
            {
                cancel(state);
            }
        });
        ui.add_space(4.0);
    });
}

fn reroll_hint(step: GenStep) -> &'static str {
    match step {
        GenStep::Size => "Mint a fresh root seed and re-roll every knob",
        _ => "Re-roll just this step (and everything downstream), keeping earlier steps frozen",
    }
}

/// Cancel the wizard: detach any in-flight prefix run and clear the transient
/// session (no command — it is not document state).
fn cancel(state: &mut BuilderState) {
    state.cancel_prefix();
    state.iterative_gen = None;
    // Clear any lingering preview-error so it does not leak onto the next panel.
    state.feedback.last_command_error = None;
}

/// ✓ Commit — **Phase C1 new-project commit.**
///
/// On commit the wizard materialises a brand-new project:
/// 1. ensure a destination folder is chosen (rfd picker, mirroring the random
///    wizard) — required to write into;
/// 2. hand off to [`BuilderState::commit_new_project`], which runs the **full**
///    pipeline synchronously through `Stage::LAST` with the session nonces,
///    writes the project tree (config + catalogs + `out/sector.json`) to `dest`,
///    then `open_project(&dest)` → `*state = new_state` — a **session boundary**,
///    so **no `BuilderCommand`** (invariant #5 carve-out), exactly like
///    `generate_random.rs::apply_result`;
/// 3. the reopened state has `iterative_gen == None`, so the wizard is cleared by
///    the replacement itself.
///
/// The synchronous full run can take a moment on large grids; that is acceptable
/// for the explicit Commit gesture (the spec permits a synchronous
/// `generate_prefix(Stage::LAST)` here), and the button is disabled while a
/// preview run is in flight. On failure the session is left intact and the error
/// is surfaced so the user can retry.
fn commit(state: &mut BuilderState, ui: &mut Ui) {
    // Ensure a destination is chosen — the commit needs a folder to write into.
    let needs_dest = state
        .iterative_gen
        .as_ref()
        .is_some_and(|s| s.dest.is_none());
    if needs_dest {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Iterative sector parent folder")
            .pick_folder()
        {
            if let Ok(parent) = camino::Utf8PathBuf::from_path_buf(folder) {
                if let Some(session) = state.iterative_gen.as_mut() {
                    let slug = dir_slug(&session.root_seed);
                    session.dest = Some(parent.join(format!("iterative-{slug}")));
                }
            }
        } else {
            // User dismissed the picker — nothing to do.
            return;
        }
    }

    // Full-run → write → reopen. This is a session boundary (`*state =
    // new_state`); on success `state.iterative_gen` is `None` and the panel
    // falls back to its no-session placeholder next frame.
    match state.commit_new_project() {
        Ok(()) => {
            // Clear any lingering preview-error so it does not leak onto the
            // freshly opened project.
            state.feedback.last_command_error = None;
        }
        Err(message) => {
            state.feedback.last_command_error = Some(message);
        }
    }
    ui.ctx().request_repaint();
}

/// Folder-name slug from a seed (same shape as the random wizard's `dir_slug`).
fn dir_slug(seed: &str) -> String {
    let s: String = seed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(16)
        .collect();
    if s.is_empty() {
        "sector".to_string()
    } else {
        s
    }
}

/// One-line detail for the progress bar, derived from the latest
/// [`SectorProgress`] event. The enum is `#[non_exhaustive]`, so the catch-all
/// arm keeps this compiling as the engine grows new events.
fn progress_detail(p: SectorProgress) -> String {
    match p {
        SectorProgress::WorldPoolBuilt { candidates, .. } => {
            format!("World pool — {candidates} candidates")
        }
        SectorProgress::SystemsPlaced { total, .. } => format!("Placed {total} systems"),
        SectorProgress::RegionsBuilt { count } => format!("Grew {count} warp regions"),
        SectorProgress::SystemBuilt {
            current,
            total,
            ..
        } => format!("Building systems — {current}/{total}"),
        SectorProgress::FactionsAssigned { catalog_rows } => {
            format!("Assigning factions — {catalog_rows} rows")
        }
        SectorProgress::FactionsAggregated { factions } => {
            format!("Aggregated {factions} factions")
        }
        SectorProgress::RoutesGenerated { routes } => format!("Generated {routes} routes"),
        SectorProgress::ManifestBuilt {
            systems,
            worlds,
            routes,
        } => format!("Manifest — {systems} systems, {worlds} worlds, {routes} routes"),
        SectorProgress::ChronicleComplete { events } => format!("Chronicle — {events} events"),
        SectorProgress::Complete {
            systems,
            worlds,
            routes,
        } => format!("Complete — {systems} systems, {worlds} worlds, {routes} routes"),
        SectorProgress::StageStarted { name } | SectorProgress::OverlayDerived { name } => {
            name.to_string()
        }
        _ => "Generating…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::state::IterativeGenSession;
    use sectorforge::random_sector::SectorSize;

    /// Drive `show` headless on both the no-session and active-session paths to
    /// prove the Phase P panel paints (rail + form + preview + action bar) without
    /// panicking — the embedded `SidePanel`/`CentralPanel`/`TopBottomPanel`
    /// nesting and the `SectorView` placeholder branch in particular.
    #[test]
    fn panel_paints_headless_in_both_states() {
        let run = |state: &mut BuilderState| {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, state));
            });
        };

        // No session → placeholder branch.
        let mut blank = BuilderState::new_blank("t", "T", "seed", 8, 8);
        run(&mut blank);
        assert!(blank.iterative_gen.is_none());

        // Active session on the Size step (no worlds catalog ⇒ no preview run;
        // the panel still renders the rail + size knobs + "no systems yet").
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(IterativeGenSession::new(
            state.config.clone(),
            "iter-seed".to_string(),
            SectorSize::Custom { dim: 8 },
        ));
        state.set_active_tab(crate::builder::state::BuilderTab::IterativeGen);
        run(&mut state);
        // Session survives a paint frame and stays on the Size step.
        assert!(state
            .iterative_gen
            .as_ref()
            .is_some_and(|s| s.current_step == GenStep::Size));
    }

    /// Invariant #4 — the size form's single grid field drives both width and
    /// height through `set_grid_dim`; they can never diverge.
    #[test]
    fn set_grid_dim_keeps_square() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.iterative_gen = Some(IterativeGenSession::new(
            state.config.clone(),
            "iter-seed".to_string(),
            SectorSize::Custom { dim: 8 },
        ));
        let ctx = egui::Context::default();
        state.set_grid_dim(&ctx, 21);
        let gen = &state.iterative_gen.as_ref().unwrap().config.generation;
        assert_eq!(gen.sector_width, 21);
        assert_eq!(gen.sector_height, 21);
        assert_eq!(gen.sector_width, gen.sector_height);
    }

    /// §5 — the editable Regions form paints on both branches (lazy-from-catalog
    /// when the override is `None`, and from the patch when it is `Some`) without
    /// panicking, and merely *painting* it does not spuriously create an override
    /// (the lazy init only fires on a real edit, so override-absent stays
    /// byte-identical — invariants #2/#6).
    #[test]
    fn regions_form_paints_on_both_override_branches() {
        use sectorforge::regions::RegionsConfig;

        let paint = |state: &mut BuilderState| {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show_regions_form(ui, state));
            });
        };

        // Override = None, with a project regions catalog to seed the read-out.
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        state.data_catalogs.regions = Some(RegionsConfig::default());
        state.iterative_gen = Some(IterativeGenSession::new(
            state.config.clone(),
            "iter-seed".to_string(),
            SectorSize::Custom { dim: 8 },
        ));
        paint(&mut state);
        assert!(
            state
                .iterative_gen
                .as_ref()
                .is_some_and(|s| s.regions_override.is_none()),
            "painting the Regions form must not create an override without an edit",
        );

        // Override = Some(patch) with a populated conditions list (read-only
        // summary branch).
        if let Some(session) = state.iterative_gen.as_mut() {
            session.regions_override = Some(RegionsConfig {
                enabled: true,
                count: 3,
                conditions: vec![sectorforge::regions::ConditionEntry {
                    kind: sectorforge::regions::RegionConditionKind::Anomaly,
                    weight: 2.0,
                    label: None,
                }],
                ..RegionsConfig::default()
            });
        }
        paint(&mut state);
        assert!(
            state
                .iterative_gen
                .as_ref()
                .is_some_and(|s| s.regions_override.is_some()),
            "the override survives a paint frame",
        );
    }
}
