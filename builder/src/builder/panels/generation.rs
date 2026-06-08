//! §6 Generation panel (G1..G6).
//!
//! Provides feature-parity controls over every `[generation]`,
//! `[generation.placement]`, `[generation.world_selection]`,
//! `[generation.routes]`, and `[generation.relations]` field (§G1), the
//! seed-lock toggle plus deterministic re-roll (§G2), the live debounced
//! preview pipeline (§G3, §G4), per-subsector partial regeneration (§G5),
//! and the "New sector from preset" launcher (§G6) that scaffolds a fresh
//! project on disk and pushes it onto a [`super::super::BuilderWorkspace`]
//! tab when one is provided.
//!
//! Hosted from [`super::project::show`] as a collapsing header so the §N1
//! tab list (locked at 24 tabs) stays stable.

use camino::Utf8PathBuf;
use egui::{RichText, Ui};

use sectorforge::config::{PlacementMode, WorldSelectionMode};
use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::{
    palette,
    ui_kit::{self, labeled},
    widgets,
};

use crate::builder::project_io::{new_project, NewProjectOptions};
use crate::builder::state::PartialRegenRect;
use crate::builder::{BuilderState, BuilderWorkspace, ModalKind};

const DEBOUNCE_SECONDS: f64 = 0.20;

/// Dimmed sub-header inside a section, used to group related field rows
/// (Basics / Placement / …). Replaces the old raw `ui.label("placement")` /
/// `ui.label("world_selection")` lines that surfaced schema keys as headings.
fn subhead(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).strong());
    ui.separator();
}

/// §6 entry point. Renders every G1..G6 surface.
///
/// `workspace` is optional — when supplied the §G6 button pushes the new
/// state onto it. Hosts that have not yet adopted [`BuilderWorkspace`] can
/// pass `None` and the §G6 button will scaffold but report that no host
/// workspace is available.
pub(crate) fn show(
    ui: &mut Ui,
    state: &mut BuilderState,
    workspace: Option<&mut BuilderWorkspace>,
) {
    show_g1_parameters(ui, state);
    ui.add_space(6.0);
    show_g2_seed_lock(ui, state);
    ui.add_space(6.0);
    show_g3_g4_preview(ui, state);
    ui.add_space(6.0);
    show_g5_partial_regen(ui, state);
    ui.add_space(6.0);
    show_g6_new_from_preset(ui, state, workspace);
}

// ── G1 ──────────────────────────────────────────────────────────────────────

fn show_g1_parameters(ui: &mut Ui, state: &mut BuilderState) {
    let mut changed = false;
    ui_kit::collapsing_section(
        ui,
        "gen_parameters",
        "How the sector is built",
        true,
        |ui| {
            ui.label(
                RichText::new(
                    "These settings drive the generator. Edit one and the live preview re-rolls.",
                )
                .color(palette::chrome_text_dim()),
            );
            ui.separator();
            // Square-sector invariant: highlight it right where the grid dims
            // are edited so it is unmissable in the generator UI.
            ui.colored_label(
                palette::warning(),
                "◧ Sectors are always square — width and height stay locked equal.",
            );
            ui.add_space(2.0);
            let gen = &mut state.config.generation;

            subhead(ui, "Basics");
            labeled(
            ui,
            "Seed",
            "Random seed for this sector (schema: seed). Same seed + same settings = the same sector every time.",
            |ui| {
                changed |= ui.text_edit_singleline(&mut gen.seed).changed();
            },
        );
            // Sectors must be square: mirror either edit into the other dimension
            // so they stay locked equal. Two rows, one shared invariant note.
            labeled(
            ui,
            "Sector width",
            "Sector width in hexes (schema: sector_width). Locked equal to height — sectors are always square.",
            |ui| {
                if ui
                    .add(egui::DragValue::new(&mut gen.sector_width).range(1..=200))
                    .changed()
                {
                    gen.sector_height = gen.sector_width;
                    changed = true;
                }
            },
        );
            labeled(
            ui,
            "Sector height",
            "Sector height in hexes (schema: sector_height). Locked equal to width — sectors are always square.",
            |ui| {
                if ui
                    .add(egui::DragValue::new(&mut gen.sector_height).range(1..=200))
                    .changed()
                {
                    gen.sector_width = gen.sector_height;
                    changed = true;
                }
            },
        );
            labeled(
            ui,
            "Subsector width",
            "Width of each subsector block in hexes (schema: subsector_width). 0 leaves it automatic.",
            |ui| {
                let mut sw = gen.subsector_width.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut sw).range(0..=64))
                    .changed()
                {
                    gen.subsector_width = (sw > 0).then_some(sw);
                    changed = true;
                }
            },
        );
            labeled(
            ui,
            "Subsector height",
            "Height of each subsector block in hexes (schema: subsector_height). 0 leaves it automatic.",
            |ui| {
                let mut sh = gen.subsector_height.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut sh).range(0..=64))
                    .changed()
                {
                    gen.subsector_height = (sh > 0).then_some(sh);
                    changed = true;
                }
            },
        );
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
            labeled(
                ui,
                "Min worlds / system",
                "Fewest worlds any system may hold (schema: min_worlds_per_system).",
                |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut gen.min_worlds_per_system).range(0..=20))
                        .changed();
                },
            );
            labeled(
                ui,
                "Max worlds / system",
                "Most worlds any system may hold (schema: max_worlds_per_system).",
                |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut gen.max_worlds_per_system).range(0..=20))
                        .changed();
                },
            );
            labeled(
                ui,
                "Features / world",
                "How many notable features each world may gain (schema: world_feature_count).",
                |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut gen.world_feature_count).range(0..=10))
                        .changed();
                },
            );
            labeled(
            ui,
            "Allow empty hexes",
            "Let some hexes stay starless instead of forcing a system everywhere (schema: allow_empty_hexes).",
            |ui| {
                changed |= ui.checkbox(&mut gen.allow_empty_hexes, "").changed();
            },
        );
            labeled(
            ui,
            "Strict world rows",
            "Require fully-filled world rows when building systems (schema: strict_world_rows).",
            |ui| {
                changed |= ui.checkbox(&mut gen.strict_world_rows, "").changed();
            },
        );

            subhead(ui, "Where systems go");
            labeled(
                ui,
                "Placement style",
                "How systems are spread across the grid (schema: placement.mode).",
                |ui| changed |= placement_mode_combo(ui, &mut gen.placement.mode),
            );
            labeled(
                ui,
                "Clustering",
                "Higher = systems clump into denser pockets (schema: placement.cluster_bias).",
                |ui| {
                    changed |= ui
                        .add(egui::Slider::new(
                            &mut gen.placement.cluster_bias,
                            0.0..=1.0,
                        ))
                        .changed();
                },
            );
            labeled(
            ui,
            "Min system spacing",
            "Smallest gap allowed between two systems, in hexes (schema: placement.minimum_system_distance).",
            |ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut gen.placement.minimum_system_distance)
                            .range(1..=16),
                    )
                    .changed();
            },
        );

            subhead(ui, "Worlds in each system");
            labeled(
                ui,
                "Selection style",
                "How worlds are chosen for each system (schema: world_selection.mode).",
                |ui| changed |= world_selection_mode_combo(ui, &gen.world_selection.mode),
            );
            labeled(
            ui,
            "Require full rows",
            "Only build a world row when it can be fully populated (schema: world_selection.require_complete_rows).",
            |ui| {
                changed |= ui
                    .checkbox(&mut gen.world_selection.require_complete_rows, "")
                    .changed();
            },
        );
            labeled(
                ui,
                "Allow partial rows",
                "Permit half-filled world rows (schema: world_selection.allow_partial_rows).",
                |ui| {
                    changed |= ui
                        .checkbox(&mut gen.world_selection.allow_partial_rows, "")
                        .changed();
                },
            );
            labeled(
            ui,
            "Same star-colour bias",
            "Higher = worlds in a system lean toward one star colour (schema: world_selection.same_star_colour_bias).",
            |ui| {
                changed |= ui
                    .add(egui::Slider::new(
                        &mut gen.world_selection.same_star_colour_bias,
                        0.0..=5.0,
                    ))
                    .changed();
            },
        );
            labeled(
            ui,
            "Strict star colour",
            "Force every world in a system to share one star colour (schema: world_selection.strict_same_star_colour).",
            |ui| {
                changed |= ui
                    .checkbox(&mut gen.world_selection.strict_same_star_colour, "")
                    .changed();
            },
        );
            labeled(
            ui,
            "Avoid duplicate types",
            "Try not to repeat a world type within the same system (schema: world_selection.avoid_duplicate_world_type_in_system).",
            |ui| {
                changed |= ui
                    .checkbox(
                        &mut gen.world_selection.avoid_duplicate_world_type_in_system,
                        "",
                    )
                    .changed();
            },
        );

            subhead(ui, "Warp routes");
            labeled(
                ui,
                "Generate routes",
                "Draw warp routes between systems (schema: routes.enabled).",
                |ui| {
                    changed |= ui.checkbox(&mut gen.routes.enabled, "").changed();
                },
            );
            labeled(
                ui,
                "Max route length",
                "Longest a single route may span, in hexes (schema: routes.max_route_distance).",
                |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut gen.routes.max_route_distance).range(1..=16))
                        .changed();
                },
            );
            labeled(
                ui,
                "Route density",
                "Higher = more routes drawn between systems (schema: routes.route_density).",
                |ui| {
                    changed |= ui
                        .add(egui::Slider::new(&mut gen.routes.route_density, 0.0..=1.0))
                        .changed();
                },
            );
            labeled(
            ui,
            "Connect everything",
            "Guarantee every system is reachable by warp route (schema: routes.ensure_connected_graph).",
            |ui| {
                changed |= ui
                    .checkbox(&mut gen.routes.ensure_connected_graph, "")
                    .changed();
            },
        );

            subhead(ui, "Faction relations");
            labeled(
            ui,
            "Min world presence",
            "Fewest worlds a faction needs before it counts as present (schema: relations.min_world_presence).",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.relations.min_world_presence).range(0..=64))
                    .changed();
            },
        );
        },
    );

    if changed {
        let now = ui.ctx().input(|i| i.time);
        state.generation.preview.schedule(now, DEBOUNCE_SECONDS);
    }
}

/// Human label for a placement mode (the raw slug is shown on hover so the
/// schema mapping stays discoverable). Presentational only — the enum keeps its
/// snake_case `Display`/`as_slug`.
fn placement_mode_label(mode: PlacementMode) -> &'static str {
    match mode {
        PlacementMode::UniformGrid => "Even grid",
        PlacementMode::WeightedGrid => "Weighted grid",
        PlacementMode::Clustered => "Clustered",
        _ => "Other",
    }
}

fn placement_mode_combo(ui: &mut Ui, mode: &mut PlacementMode) -> bool {
    let mut changed = false;
    ui_kit::combo("placement_mode_combo", placement_mode_label(*mode)).show_ui(ui, |ui| {
        for option in [
            PlacementMode::UniformGrid,
            PlacementMode::WeightedGrid,
            PlacementMode::Clustered,
        ] {
            if ui
                .selectable_label(*mode == option, placement_mode_label(option))
                .on_hover_text(format!("key: {}", option.as_slug()))
                .clicked()
            {
                *mode = option;
                changed = true;
            }
        }
    });
    changed
}

/// GEN-1: `WorldSelectionMode` has a single variant (`WeightedRows`) today, so a
/// combo could only ever offer one option. Render a static label instead of a
/// no-choice dropdown. Kept as a function (taking `mode` by ref so it can read
/// the display name) so a real combo drops back in unchanged once a second
/// variant exists. The return type stays `bool` (`false` — nothing to change)
/// to keep the caller's `changed |=` accumulation intact.
fn world_selection_mode_combo(ui: &mut Ui, mode: &WorldSelectionMode) -> bool {
    // Only `WeightedRows` exists today; `WorldSelectionMode` is `#[non_exhaustive]`,
    // so a single label is correct until a second variant lands (then this becomes
    // a real combo again).
    let label = "Weighted rows";
    ui.label(label).on_hover_text(format!(
        "Only one world-selection style exists today (key: {}).",
        mode.as_slug()
    ));
    false
}

// ── G2 ──────────────────────────────────────────────────────────────────────

fn show_g2_seed_lock(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "gen_seed", "Seed control", true, |ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.generation.seed_locked, "Lock seed")
                .on_hover_text(
                    "When locked, re-rolling keeps the same seed so the sector stays put.",
                );
            if state.generation.seed_locked {
                ui_kit::placeholder(ui, "locked — re-roll keeps this seed");
            } else {
                ui.label(
                    RichText::new(format!(
                        "re-rolls so far: {}",
                        state.generation.seed_reroll_counter
                    ))
                    .color(palette::chrome_text_dim()),
                );
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("🎲  Re-roll")
                .on_hover_text("Draw a fresh seed (or keep it when locked) and rebuild the preview")
                .clicked()
            {
                let new_seed = state.reroll_seed();
                let now = ui.ctx().input(|i| i.time);
                state.generation.preview.schedule(now, DEBOUNCE_SECONDS);
                let preview = if state.generation.seed_locked {
                    "(locked)"
                } else {
                    &new_seed
                };
                ui.label(format!("seed: {preview}"));
            }
            ui.label(
                RichText::new(format!("active seed: {}", &state.config.generation.seed))
                    .color(palette::chrome_text_dim()),
            );
        });
    });
}

// ── G3 / G4 ─────────────────────────────────────────────────────────────────

fn show_g3_g4_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "gen_live_preview", "Live preview", true, |ui| {
        if state.data_catalogs.worlds.is_none() {
            ui.colored_label(
                palette::warning(),
                "Open a project with a worlds catalog to enable the preview.",
            );
        }
        let now = ui.ctx().input(|i| i.time);
        // Build the project input up front so the closure handed to
        // `pump` does not borrow `state` while `state.generation.preview` is held.
        let pending_input = state.synthesize_project_input();
        let new_result = state
            .generation
            .preview
            .pump(ui.ctx(), now, move || pending_input);
        if new_result {
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            if ui
                .button("🔄  Regenerate preview")
                .on_hover_text("Rebuild the preview now from the current settings")
                .clicked()
            {
                let now = ui.ctx().input(|i| i.time);
                state.generation.preview.schedule(now, 0.0);
            }
            if ui
                .button("✖  Cancel preview")
                .on_hover_text("Discard the in-progress or finished preview")
                .clicked()
            {
                state.generation.preview.clear();
            }
        });
        if let Some(preview_sector) = state.generation.preview.sector.as_ref() {
            ui.colored_label(
                palette::success(),
                format!(
                    "PREVIEW READY — {} system(s), {} route(s)",
                    preview_sector.systems.len(),
                    preview_sector.routes.len()
                ),
            );
            // Applying the preview replaces the sector and clears undo history —
            // a deliberate session boundary, so the note spells it out.
            // §BEAUTY §6.6: the marquee "apply" action gets the bespoke brass
            // primary button (was a flat green fill).
            if widgets::primary_button(ui, "✅  Apply preview")
                .on_hover_text(
                    "Replace the current sector with this preview. This starts a fresh session and clears undo history; pinned systems are kept.",
                )
                .clicked()
            {
                let pinned_count = state.pinned_systems.len();
                if state.apply_preview() {
                    ui.label(format!(
                        "Preview applied. Pinned systems kept: {pinned_count}."
                    ));
                }
            }
        } else if state.generation.preview.job.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Generating preview…");
            });
        } else if let Some(err) = state.generation.preview.error.as_ref() {
            ui.colored_label(palette::danger(), err);
        } else {
            ui_kit::placeholder(ui, "No preview yet — edit any setting above to build one.");
        }
    });
}

// ── G5 ──────────────────────────────────────────────────────────────────────

fn show_g5_partial_regen(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "gen_partial_regen",
        "Rebuild part of the map",
        false,
        |ui| {
            // §CTX1 Phase 4 — hint shown when the MAP tab's right-click
            // `START PARTIAL REGEN HERE` item armed an anchor. The next
            // primary click on the map completes the rect anchor→click.
            if let Some(anchor) = state.map_view.partial_regen_anchor {
                ui.colored_label(
                    palette::info(),
                    format!(
                        "Corner set at ({},{}) — click the opposite corner on the map to finish the box.",
                        anchor.q, anchor.r
                    ),
                );
                if ui
                    .button("Cancel corner")
                    .on_hover_text("Forget the armed corner and start over")
                    .clicked()
                {
                    state.map_view.partial_regen_anchor = None;
                }
                ui.separator();
            }
            let mut rect = state
                .map_view
                .partial_regen_rect
                .unwrap_or(PartialRegenRect {
                    min_q: 0,
                    min_r: 0,
                    max_q: 0,
                    max_r: 0,
                });
            ui.label(
                RichText::new("Box to rebuild (hex coordinates):")
                    .color(palette::chrome_text_dim()),
            );
            labeled(
                ui,
                "Left edge (q)",
                "Smallest column in the box (schema: min_q).",
                |ui| {
                    ui.add(egui::DragValue::new(&mut rect.min_q));
                },
            );
            labeled(
                ui,
                "Top edge (r)",
                "Smallest row in the box (schema: min_r).",
                |ui| {
                    ui.add(egui::DragValue::new(&mut rect.min_r));
                },
            );
            labeled(
                ui,
                "Right edge (q)",
                "Largest column in the box (schema: max_q).",
                |ui| {
                    ui.add(egui::DragValue::new(&mut rect.max_q));
                },
            );
            labeled(
                ui,
                "Bottom edge (r)",
                "Largest row in the box (schema: max_r).",
                |ui| {
                    ui.add(egui::DragValue::new(&mut rect.max_r));
                },
            );
            // Normalise the corners so the helper doesn't see inverted rects.
            let normalised = PartialRegenRect::from_corners(
                HexCoord {
                    q: rect.min_q,
                    r: rect.min_r,
                },
                HexCoord {
                    q: rect.max_q,
                    r: rect.max_r,
                },
            );
            state.map_view.partial_regen_rect = Some(normalised);

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Clear box")
                    .on_hover_text("Forget the selected box")
                    .clicked()
                {
                    state.map_view.partial_regen_rect = None;
                }
                if ui
                    .button("🔄  Rebuild these hexes")
                    .on_hover_text(
                        "Regenerate only the systems inside the box; pinned systems are skipped",
                    )
                    .clicked()
                {
                    match state.regenerate_partial() {
                        Ok(n) => {
                            ui.label(format!("Rebuilt {n} system(s) (pinned skipped)."));
                        }
                        Err(e) => {
                            ui.colored_label(palette::danger(), format!("Couldn't rebuild: {e}"));
                        }
                    }
                }
            });
            ui.label(
                RichText::new("Hexes outside the box and pinned systems stay untouched.")
                    .color(palette::chrome_text_dim()),
            );
        },
    );
}

// ── G6 ──────────────────────────────────────────────────────────────────────

fn show_g6_new_from_preset(
    ui: &mut Ui,
    state: &mut BuilderState,
    mut workspace: Option<&mut BuilderWorkspace>,
) {
    ui_kit::collapsing_section(
        ui,
        "gen_new_from_preset",
        "Start a new sector from a preset",
        false,
        |ui| {
            if workspace.is_none() {
                ui.colored_label(
                    palette::warning(),
                    "No second tab available — the new sector will replace this one.",
                );
            }
            let mut modal = match state.feedback.modal.clone() {
                Some(ModalKind::NewFromPreset {
                    preset_id,
                    dest,
                    seed,
                }) => (preset_id, dest, seed, true),
                _ => (
                    "embattled-frontier".to_string(),
                    Utf8PathBuf::from("./new-sector"),
                    state.config.generation.seed.clone(),
                    false,
                ),
            };
            if ui
                .button("🚀  Open preset launcher")
                .on_hover_text("Pick a preset, a folder, and a seed for a brand-new sector")
                .clicked()
            {
                modal.3 = true;
            }
            if modal.3 {
                labeled(
                    ui,
                    "Preset",
                    "Which starting preset to scaffold from (schema: preset id).",
                    |ui| {
                        ui.text_edit_singleline(&mut modal.0);
                    },
                );
                labeled(
                    ui,
                    "Save to folder",
                    "Where the new project files will be written (schema: destination path).",
                    |ui| {
                        let mut dest_str = modal.1.to_string();
                        if ui.text_edit_singleline(&mut dest_str).changed() {
                            modal.1 = Utf8PathBuf::from(dest_str);
                        }
                    },
                );
                labeled(
                    ui,
                    "Seed",
                    "Starting seed for the new sector (schema: seed).",
                    |ui| {
                        ui.text_edit_singleline(&mut modal.2);
                    },
                );
                let mut persist_modal = true;
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("🚀  Create and open")
                        .on_hover_text("Scaffold the project on disk and open it in a new tab")
                        .clicked()
                    {
                        let opts = NewProjectOptions {
                            dest: modal.1.clone(),
                            id: modal.1.file_name().unwrap_or("new-sector").to_string(),
                            title: modal.1.file_name().unwrap_or("New Sector").to_string(),
                            seed: modal.2.clone(),
                            width: state.config.generation.sector_width,
                            height: state.config.generation.sector_height,
                            preset: Some(modal.0.clone()),
                        };
                        match new_project(opts) {
                            Ok(new_state) => {
                                state.feedback.modal = None;
                                persist_modal = false;
                                if let Some(ws) = workspace.as_mut() {
                                    ws.push(new_state);
                                } else {
                                    *state = new_state;
                                }
                            }
                            Err(e) => {
                                state.feedback.modal = Some(ModalKind::Message(format!(
                                    "Couldn't create the new sector: {e}"
                                )));
                                persist_modal = false;
                            }
                        }
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Close the launcher without creating anything")
                        .clicked()
                    {
                        state.feedback.modal = None;
                        persist_modal = false;
                    }
                });
                // Persist edits back to the modal state for the next frame.
                if persist_modal
                    && (matches!(state.feedback.modal, Some(ModalKind::NewFromPreset { .. }))
                        || modal.3)
                {
                    state.feedback.modal = Some(ModalKind::NewFromPreset {
                        preset_id: modal.0,
                        dest: modal.1,
                        seed: modal.2,
                    });
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;
    use sectorforge::worlds_toml::WorldsConfig;

    #[test]
    fn partial_regen_rect_contains_normalises_corners() {
        let r = PartialRegenRect::from_corners(HexCoord { q: 3, r: 4 }, HexCoord { q: 1, r: 2 });
        assert_eq!(r.min_q, 1);
        assert_eq!(r.max_q, 3);
        assert!(r.contains(HexCoord { q: 2, r: 3 }));
        assert!(!r.contains(HexCoord { q: 0, r: 0 }));
    }

    #[test]
    fn reroll_locked_keeps_seed() {
        let mut state = BuilderState::new_blank("t", "T", "seed-a", 4, 4);
        state.generation.seed_locked = true;
        let out = state.reroll_seed();
        assert_eq!(out, "seed-a");
        assert_eq!(state.config.generation.seed, "seed-a");
        assert_eq!(state.generation.seed_reroll_counter, 0);
    }

    #[test]
    fn reroll_unlocked_advances_seed_and_counter() {
        let mut state = BuilderState::new_blank("t", "T", "seed-a", 4, 4);
        let s1 = state.reroll_seed();
        let s2 = state.reroll_seed();
        assert_ne!(s1, s2);
        assert_eq!(state.generation.seed_reroll_counter, 2);
        assert_eq!(state.config.generation.seed, s2);
    }

    #[test]
    fn apply_preview_with_no_scratch_returns_false() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 4, 4);
        assert!(!state.apply_preview());
    }

    #[test]
    fn partial_regen_without_input_errors() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 4, 4);
        state.map_view.partial_regen_rect = Some(PartialRegenRect {
            min_q: 0,
            min_r: 0,
            max_q: 1,
            max_r: 1,
        });
        // No worlds catalog -> synthesize returns None.
        assert!(state.regenerate_partial().is_err());
    }

    #[test]
    fn partial_regen_skips_when_no_rect() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 4, 4);
        state.data_catalogs.worlds = Some(WorldsConfig::default());
        assert!(state.regenerate_partial().is_err());
    }
}
