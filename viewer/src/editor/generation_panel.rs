//! Generation settings panel for real-time sector building.

use egui::Ui;

use super::state::EditorState;
use super::ui_helpers::{dim, label, section};

fn random_seed_str() -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed = sectorforge::rng::derive_stage_seed(&nanos.to_string(), "viewer_reroll", "session");
    let v = u64::from_le_bytes(seed[..8].try_into().expect("blake3 returns 32 bytes"));
    format!("{v:016x}")
}

pub fn show_generation_settings(ui: &mut Ui, state: &mut EditorState) {
    let Some(input) = state.project_input.as_mut() else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            dim(
                ui,
                "Generation requires project context (sectorforge.toml + data/).",
            );
            dim(
                ui,
                "Manual edits to an isolated sector.json do not support re-generation.",
            );
        });
        return;
    };

    let mut changed = false;

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.auto_generate, "AUTO-GENERATE");
        ui.add_space(10.0);
        if ui.button("🎲").on_hover_text("Randomize seed").clicked() {
            input.config.generation.seed = random_seed_str();
            changed = true;
        }
    });
    ui.add_space(8.0);

    section(ui, "BASIC PARAMETERS");

    ui.horizontal(|ui| {
        label(ui, "SEED");
        if ui
            .text_edit_singleline(&mut input.config.generation.seed)
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        label(ui, "WIDTH");
        if ui
            .add(egui::DragValue::new(&mut input.config.generation.sector_width).range(1..=100))
            .changed()
        {
            changed = true;
        }
        label(ui, "HEIGHT");
        if ui
            .add(egui::DragValue::new(&mut input.config.generation.sector_height).range(1..=100))
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        label(ui, "SYSTEM COUNT");
        if ui
            .add(egui::DragValue::new(&mut input.config.generation.system_count).range(1..=1000))
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        label(ui, "WORLDS PER SYS");
        if ui
            .add(
                egui::DragValue::new(&mut input.config.generation.min_worlds_per_system)
                    .range(0..=10),
            )
            .changed()
        {
            changed = true;
        }
        ui.label("-");
        if ui
            .add(
                egui::DragValue::new(&mut input.config.generation.max_worlds_per_system)
                    .range(0..=10),
            )
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(8.0);
    section(ui, "PLACEMENT");
    ui.horizontal(|ui| {
        label(ui, "MODE");
        if ui
            .selectable_label(
                input.config.generation.placement.mode
                    == sectorforge::config::PlacementMode::UniformGrid,
                "UNIFORM",
            )
            .clicked()
        {
            input.config.generation.placement.mode =
                sectorforge::config::PlacementMode::UniformGrid;
            changed = true;
        }
        if ui
            .selectable_label(
                input.config.generation.placement.mode
                    == sectorforge::config::PlacementMode::Clustered,
                "CLUSTERED",
            )
            .clicked()
        {
            input.config.generation.placement.mode = sectorforge::config::PlacementMode::Clustered;
            changed = true;
        }
    });
    if input.config.generation.placement.mode == sectorforge::config::PlacementMode::Clustered {
        ui.horizontal(|ui| {
            label(ui, "CLUSTER BIAS");
            if ui
                .add(egui::Slider::new(
                    &mut input.config.generation.placement.cluster_bias,
                    0.0..=1.0,
                ))
                .changed()
            {
                changed = true;
            }
        });
    }
    ui.horizontal(|ui| {
        label(ui, "MIN DISTANCE");
        if ui
            .add(
                egui::DragValue::new(
                    &mut input.config.generation.placement.minimum_system_distance,
                )
                .range(1..=10),
            )
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(8.0);
    section(ui, "WORLD SELECTION");
    if ui
        .checkbox(
            &mut input
                .config
                .generation
                .world_selection
                .avoid_duplicate_world_type_in_system,
            "AVOID DUPLICATE TYPES IN SYS",
        )
        .changed()
    {
        changed = true;
    }
    ui.horizontal(|ui| {
        label(ui, "STAR COLOUR BIAS");
        if ui
            .add(egui::Slider::new(
                &mut input
                    .config
                    .generation
                    .world_selection
                    .same_star_colour_bias,
                0.0..=5.0,
            ))
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(8.0);
    section(ui, "ROUTES");
    if ui
        .checkbox(&mut input.config.generation.routes.enabled, "ENABLED")
        .changed()
    {
        changed = true;
    }
    if input.config.generation.routes.enabled {
        ui.horizontal(|ui| {
            label(ui, "DENSITY");
            if ui
                .add(egui::Slider::new(
                    &mut input.config.generation.routes.route_density,
                    0.0..=1.0,
                ))
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            label(ui, "MAX DIST");
            if ui
                .add(
                    egui::DragValue::new(&mut input.config.generation.routes.max_route_distance)
                        .range(1..=10),
                )
                .changed()
            {
                changed = true;
            }
        });
    }
    let mut input_seed = input.config.generation.seed.clone();
    let mut input_ensure_connected = input.config.generation.routes.ensure_connected_graph;

    ui.horizontal(|ui| {
        ui.label("SEED");
        if ui.text_edit_singleline(&mut input_seed).changed() {
            changed = true;
        }
        if ui
            .checkbox(&mut input_ensure_connected, "ENSURE CONNECTED")
            .changed()
        {
            changed = true;
        }
    });

    // Update back to state
    if changed {
        if let Some(input) = state.project_input.as_mut() {
            input.config.generation.seed = input_seed;
            input.config.generation.routes.ensure_connected_graph = input_ensure_connected;
        }
    }

    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("RE-GENERATE (SAME SEED)").min_size(egui::vec2(160.0, 40.0)))
                .clicked()
            {
                changed = true;
            }
            if ui
                .add(egui::Button::new("RE-ROLL (NEW SEED)").min_size(egui::vec2(160.0, 40.0)))
                .clicked()
            {
                if let Some(input) = state.project_input.as_mut() {
                    input.config.generation.seed = random_seed_str();
                }
                changed = true;
            }
        });

        if state.preview_sector.is_some() {
            ui.add_space(10.0);
            if ui
                .add(
                    egui::Button::new("APPLY PREVIEW")
                        .min_size(egui::vec2(330.0, 40.0))
                        .fill(egui::Color32::from_rgb(0, 100, 0)),
                )
                .clicked()
            {
                if let Some(preview) = state.preview_sector.take() {
                    state.sector = Some(preview);
                    state.mark_dirty();
                }
            }
        } else if state.preview_job.is_some() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Generating preview...");
            });
        } else if let Some(error) = &state.preview_error {
            ui.add_space(10.0);
            ui.colored_label(egui::Color32::from_rgb(180, 80, 80), error);
        }
    });
    if changed && state.auto_generate {
        state.schedule_preview(ui.ctx().input(|i| i.time) + 0.2);
    } else if changed {
        state.preview_timer = None;
        state.invalidate_preview();
        state.preview_job = None;
    }
}
