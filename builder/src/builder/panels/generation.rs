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
use egui::Ui;

use sectorforge::config::{PlacementMode, WorldSelectionMode};
use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::ui_kit;

use crate::builder::project_io::{new_project, NewProjectOptions};
use crate::builder::state::PartialRegenRect;
use crate::builder::{BuilderState, BuilderWorkspace, ModalKind};

const DEBOUNCE_SECONDS: f64 = 0.20;

/// §6 entry point. Renders every G1..G6 surface.
///
/// `workspace` is optional — when supplied the §G6 button pushes the new
/// state onto it. Hosts that have not yet adopted [`BuilderWorkspace`] can
/// pass `None` and the §G6 button will scaffold but report that no host
/// workspace is available.
pub fn show(ui: &mut Ui, state: &mut BuilderState, workspace: Option<&mut BuilderWorkspace>) {
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
    ui_kit::collapsing_section(ui, "gen_parameters", "Generation parameters", true, |ui| {
        ui.label("`[generation]` parity");
        ui.separator();
        // Square-sector invariant: highlight it right where the grid dims
        // are edited so it is unmissable in the generator UI.
        ui.colored_label(
            egui::Color32::from_rgb(0xE0, 0xA0, 0x30),
            "◧ Sectors must be square — sector_width and sector_height are locked equal.",
        );
        ui.add_space(2.0);
        let gen = &mut state.config.generation;
        egui::Grid::new("gen_basic_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("seed");
                changed |= ui.text_edit_singleline(&mut gen.seed).changed();
                ui.end_row();
                // Sectors must be square: mirror either edit into the
                // other dimension so they stay locked equal.
                ui.label("sector_width");
                if ui
                    .add(egui::DragValue::new(&mut gen.sector_width).range(1..=200))
                    .changed()
                {
                    gen.sector_height = gen.sector_width;
                    changed = true;
                }
                ui.end_row();
                ui.label("sector_height");
                if ui
                    .add(egui::DragValue::new(&mut gen.sector_height).range(1..=200))
                    .changed()
                {
                    gen.sector_width = gen.sector_height;
                    changed = true;
                }
                ui.end_row();
                ui.label("subsector_width");
                let mut sw = gen.subsector_width.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut sw).range(0..=64))
                    .changed()
                {
                    gen.subsector_width = (sw > 0).then_some(sw);
                    changed = true;
                }
                ui.end_row();
                ui.label("subsector_height");
                let mut sh = gen.subsector_height.unwrap_or(0);
                if ui
                    .add(egui::DragValue::new(&mut sh).range(0..=64))
                    .changed()
                {
                    gen.subsector_height = (sh > 0).then_some(sh);
                    changed = true;
                }
                ui.end_row();
                ui.label("system_count");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.system_count).range(0..=2000))
                    .changed();
                ui.end_row();
                ui.label("min_worlds_per_system");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.min_worlds_per_system).range(0..=20))
                    .changed();
                ui.end_row();
                ui.label("max_worlds_per_system");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.max_worlds_per_system).range(0..=20))
                    .changed();
                ui.end_row();
                ui.label("world_feature_count");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.world_feature_count).range(0..=10))
                    .changed();
                ui.end_row();
                ui.label("allow_empty_hexes");
                changed |= ui.checkbox(&mut gen.allow_empty_hexes, "").changed();
                ui.end_row();
                ui.label("strict_world_rows");
                changed |= ui.checkbox(&mut gen.strict_world_rows, "").changed();
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.label("placement");
        ui.separator();
        egui::Grid::new("gen_placement_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("mode");
                changed |= placement_mode_combo(ui, &mut gen.placement.mode);
                ui.end_row();
                ui.label("cluster_bias");
                changed |= ui
                    .add(egui::Slider::new(
                        &mut gen.placement.cluster_bias,
                        0.0..=1.0,
                    ))
                    .changed();
                ui.end_row();
                ui.label("minimum_system_distance");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut gen.placement.minimum_system_distance)
                            .range(1..=16),
                    )
                    .changed();
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.label("world_selection");
        ui.separator();
        egui::Grid::new("gen_ws_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("mode");
                changed |= world_selection_mode_combo(ui, &gen.world_selection.mode);
                ui.end_row();
                ui.label("require_complete_rows");
                changed |= ui
                    .checkbox(&mut gen.world_selection.require_complete_rows, "")
                    .changed();
                ui.end_row();
                ui.label("allow_partial_rows");
                changed |= ui
                    .checkbox(&mut gen.world_selection.allow_partial_rows, "")
                    .changed();
                ui.end_row();
                ui.label("same_star_colour_bias");
                changed |= ui
                    .add(egui::Slider::new(
                        &mut gen.world_selection.same_star_colour_bias,
                        0.0..=5.0,
                    ))
                    .changed();
                ui.end_row();
                ui.label("strict_same_star_colour");
                changed |= ui
                    .checkbox(&mut gen.world_selection.strict_same_star_colour, "")
                    .changed();
                ui.end_row();
                ui.label("avoid_duplicate_world_type_in_system");
                changed |= ui
                    .checkbox(
                        &mut gen.world_selection.avoid_duplicate_world_type_in_system,
                        "",
                    )
                    .changed();
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.label("routes");
        ui.separator();
        egui::Grid::new("gen_routes_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("enabled");
                changed |= ui.checkbox(&mut gen.routes.enabled, "").changed();
                ui.end_row();
                ui.label("max_route_distance");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.routes.max_route_distance).range(1..=16))
                    .changed();
                ui.end_row();
                ui.label("route_density");
                changed |= ui
                    .add(egui::Slider::new(&mut gen.routes.route_density, 0.0..=1.0))
                    .changed();
                ui.end_row();
                ui.label("ensure_connected_graph");
                changed |= ui
                    .checkbox(&mut gen.routes.ensure_connected_graph, "")
                    .changed();
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.label("relations");
        ui.separator();
        egui::Grid::new("gen_rel_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("min_world_presence");
                changed |= ui
                    .add(egui::DragValue::new(&mut gen.relations.min_world_presence).range(0..=64))
                    .changed();
                ui.end_row();
            });
    });

    if changed {
        let now = ui.ctx().input(|i| i.time);
        state.preview.schedule(now, DEBOUNCE_SECONDS);
    }
}

fn placement_mode_combo(ui: &mut Ui, mode: &mut PlacementMode) -> bool {
    let mut changed = false;
    ui_kit::combo("placement_mode_combo", format!("{mode}")).show_ui(ui, |ui| {
        for option in [
            PlacementMode::UniformGrid,
            PlacementMode::WeightedGrid,
            PlacementMode::Clustered,
        ] {
            if ui
                .selectable_label(*mode == option, format!("{option}"))
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
    ui.label(format!("{mode}"))
        .on_hover_text("Only one world-selection mode exists today (weighted rows).");
    false
}

// ── G2 ──────────────────────────────────────────────────────────────────────

fn show_g2_seed_lock(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "gen_seed", "Seed", true, |ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut state.seed_locked, "Lock seed");
            if state.seed_locked {
                ui.colored_label(egui::Color32::GRAY, "Re-roll preserves seed");
            } else {
                ui.label(format!("re-roll counter: {}", state.seed_reroll_counter));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Re-roll").clicked() {
                let new_seed = state.reroll_seed();
                let now = ui.ctx().input(|i| i.time);
                state.preview.schedule(now, DEBOUNCE_SECONDS);
                let preview = if state.seed_locked {
                    "(locked)"
                } else {
                    &new_seed
                };
                ui.label(format!("seed: {preview}"));
            }
            ui.label(format!("active seed: {}", &state.config.generation.seed));
        });
    });
}

// ── G3 / G4 ─────────────────────────────────────────────────────────────────

fn show_g3_g4_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "gen_live_preview", "Live preview + Apply", true, |ui| {
        if state.data_catalogs.worlds.is_none() {
            ui.colored_label(
                egui::Color32::from_rgb(180, 120, 80),
                "Open a project with a worlds catalog to enable preview.",
            );
        }
        let now = ui.ctx().input(|i| i.time);
        // Build the project input up front so the closure handed to
        // `pump` does not borrow `state` while `state.preview` is held.
        let pending_input = state.synthesize_project_input();
        let new_result = state.preview.pump(ui.ctx(), now, move || pending_input);
        if new_result {
            ui.ctx().request_repaint();
        }
        ui.horizontal(|ui| {
            if ui.button("Regenerate preview now").clicked() {
                let now = ui.ctx().input(|i| i.time);
                state.preview.schedule(now, 0.0);
            }
            if ui.button("Cancel preview").clicked() {
                state.preview.clear();
            }
        });
        if let Some(preview_sector) = state.preview.sector.as_ref() {
            ui.colored_label(
                egui::Color32::from_rgb(120, 200, 120),
                format!(
                    "PREVIEW READY — {} system(s), {} route(s)",
                    preview_sector.systems.len(),
                    preview_sector.routes.len()
                ),
            );
            if ui
                .add(egui::Button::new("Apply preview").fill(egui::Color32::from_rgb(0, 100, 0)))
                .clicked()
            {
                let pinned_count = state.pinned_systems.len();
                if state.apply_preview() {
                    ui.label(format!(
                        "Preview applied. Pinned systems preserved: {pinned_count}."
                    ));
                }
            }
        } else if state.preview.job.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Generating preview…");
            });
        } else if let Some(err) = state.preview.error.as_ref() {
            ui.colored_label(egui::Color32::from_rgb(200, 80, 80), err);
        } else {
            ui.colored_label(
                egui::Color32::GRAY,
                "No preview yet — edit any field above.",
            );
        }
    });
}

// ── G5 ──────────────────────────────────────────────────────────────────────

fn show_g5_partial_regen(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "gen_partial_regen",
        "Partial regeneration",
        false,
        |ui| {
            // §CTX1 Phase 4 — hint shown when the MAP tab's right-click
            // `START PARTIAL REGEN HERE` item armed an anchor. The next
            // primary click on the map completes the rect anchor→click.
            if let Some(anchor) = state.partial_regen_anchor {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 200, 240),
                    format!(
                        "Anchor armed at ({},{}) — click an opposite corner on the map to complete the rect.",
                        anchor.q, anchor.r
                    ),
                );
                if ui.button("Cancel anchor").clicked() {
                    state.partial_regen_anchor = None;
                }
                ui.separator();
            }
            let mut rect = state.partial_regen_rect.unwrap_or(PartialRegenRect {
                min_q: 0,
                min_r: 0,
                max_q: 0,
                max_r: 0,
            });
            egui::Grid::new("gen_partial_rect_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("min_q");
                    ui.add(egui::DragValue::new(&mut rect.min_q));
                    ui.end_row();
                    ui.label("min_r");
                    ui.add(egui::DragValue::new(&mut rect.min_r));
                    ui.end_row();
                    ui.label("max_q");
                    ui.add(egui::DragValue::new(&mut rect.max_q));
                    ui.end_row();
                    ui.label("max_r");
                    ui.add(egui::DragValue::new(&mut rect.max_r));
                    ui.end_row();
                });
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
            state.partial_regen_rect = Some(normalised);

            ui.horizontal(|ui| {
                if ui.button("Clear rect").clicked() {
                    state.partial_regen_rect = None;
                }
                if ui.button("Regenerate selected hexes").clicked() {
                    match state.regenerate_partial() {
                        Ok(n) => {
                            ui.label(format!("Regenerated {n} system(s) (pinned skipped)."));
                        }
                        Err(e) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(200, 80, 80),
                                format!("partial regen failed: {e}"),
                            );
                        }
                    }
                }
            });
            ui.colored_label(
                egui::Color32::GRAY,
                "Hexes outside the rect and pinned systems are untouched.",
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
        "New sector from preset",
        false,
        |ui| {
            if workspace.is_none() {
                ui.colored_label(
                    egui::Color32::from_rgb(180, 120, 80),
                    "No workspace host — the new state will replace the current one.",
                );
            }
            let mut modal = match state.modal.clone() {
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
            if ui.button("Open preset launcher…").clicked() {
                modal.3 = true;
            }
            if modal.3 {
                egui::Grid::new("g6_preset_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("preset id");
                        ui.text_edit_singleline(&mut modal.0);
                        ui.end_row();
                        ui.label("destination");
                        let mut dest_str = modal.1.to_string();
                        if ui.text_edit_singleline(&mut dest_str).changed() {
                            modal.1 = Utf8PathBuf::from(dest_str);
                        }
                        ui.end_row();
                        ui.label("seed");
                        ui.text_edit_singleline(&mut modal.2);
                        ui.end_row();
                    });
                let mut persist_modal = true;
                ui.horizontal(|ui| {
                    if ui.button("Create + open in new tab").clicked() {
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
                                state.modal = None;
                                persist_modal = false;
                                if let Some(ws) = workspace.as_mut() {
                                    ws.push(new_state);
                                } else {
                                    *state = new_state;
                                }
                            }
                            Err(e) => {
                                state.modal = Some(ModalKind::Message(format!(
                                    "preset scaffold failed: {e}"
                                )));
                                persist_modal = false;
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        state.modal = None;
                        persist_modal = false;
                    }
                });
                // Persist edits back to the modal state for the next frame.
                if persist_modal
                    && (matches!(state.modal, Some(ModalKind::NewFromPreset { .. })) || modal.3)
                {
                    state.modal = Some(ModalKind::NewFromPreset {
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
        state.seed_locked = true;
        let out = state.reroll_seed();
        assert_eq!(out, "seed-a");
        assert_eq!(state.config.generation.seed, "seed-a");
        assert_eq!(state.seed_reroll_counter, 0);
    }

    #[test]
    fn reroll_unlocked_advances_seed_and_counter() {
        let mut state = BuilderState::new_blank("t", "T", "seed-a", 4, 4);
        let s1 = state.reroll_seed();
        let s2 = state.reroll_seed();
        assert_ne!(s1, s2);
        assert_eq!(state.seed_reroll_counter, 2);
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
        state.partial_regen_rect = Some(PartialRegenRect {
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
