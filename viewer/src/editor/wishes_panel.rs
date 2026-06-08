//! Visual editor for wishes.toml + search integration.

use egui::{RichText, Ui};

use super::state::EditorState;
use super::ui_helpers::{dim, label, section};

pub(crate) fn show_wishes(ui: &mut Ui, state: &mut EditorState) {
    let Some(input) = state.project_input.as_ref() else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            dim(ui, "Wishes require project context.");
        });
        return;
    };

    if state.wishes.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            if ui.button("+ CREATE wishes.toml").clicked() {
                state.wishes = Some(sectorforge::search::WishesFile {
                    search: Default::default(),
                    constraints: Vec::new(),
                });
            }
        });
        return;
    }

    let wishes = state.wishes.as_mut().unwrap();

    section(ui, "SEARCH CONFIG");
    ui.horizontal(|ui| {
        label(ui, "BUDGET");
        ui.add(egui::DragValue::new(&mut wishes.search.budget).range(1..=1000));
    });

    ui.add_space(8.0);
    section(ui, "CONSTRAINTS");

    let mut remove_idx: Option<usize> = None;
    for (i, c) in wishes.constraints.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:?}", c)).size(10.0));
            if ui.small_button("x").clicked() {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = remove_idx {
        wishes.constraints.remove(i);
    }

    if ui.button("+ ADD CONSTRAINT").clicked() {
        wishes
            .constraints
            .push(sectorforge::search::Constraint::RouteGraphConnected);
    }

    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        if ui
            .add(egui::Button::new("RUN SEARCH").min_size(egui::vec2(200.0, 40.0)))
            .clicked()
        {
            match sectorforge::search::run_search(input, wishes) {
                Ok(outcome) => {
                    state.search_outcome = Some(outcome);
                }
                Err(e) => {
                    state.dialog = super::state::Dialog::Message(format!("Search failed: {e}"));
                }
            }
        }
    });

    let mut apply_seed: Option<String> = None;
    let mut preview_seed: Option<String> = None;

    if let Some(outcome) = &state.search_outcome {
        ui.add_space(12.0);
        section(ui, "SEARCH OUTCOME");

        if let Some(win) = &outcome.winning {
            ui.colored_label(
                crate::palette::success(),
                format!("WINNER: candidate #{} (seed {})", win.n, win.seed),
            );
            if ui.button("APPLY WINNING SEED").clicked() {
                apply_seed = Some(win.seed.clone());
            }
        } else {
            ui.colored_label(crate::palette::warning(), "No perfect match found.");
        }

        ui.add_space(8.0);
        ui.label("NEAR MISSES:");
        for cand in &outcome.near_misses {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!(
                    "#{} miss={:.3} ({})",
                    cand.n, cand.total_miss, cand.seed
                )));
                if ui.button("PREVIEW").clicked() {
                    preview_seed = Some(cand.seed.clone());
                }
            });
        }
    }

    if let Some(seed) = apply_seed {
        if let Some(input) = state.project_input.as_mut() {
            input.config.generation.seed = seed.clone();
            if let Ok(sec) = sectorforge::generate_sector(input.clone()) {
                state.sector = Some(sec);
                state.mark_dirty();
            }
        }
    }

    if let Some(seed) = preview_seed {
        if let Some(input) = state.project_input.as_mut() {
            let mut preview_input = input.clone();
            preview_input.config.generation.seed = seed;
            if let Ok(sec) = sectorforge::generate_sector(preview_input) {
                state.sector = Some(sec);
                // VED-4: applying a near-miss preview replaces the working sector, so mark dirty
                // consistently with the APPLY WINNING SEED path above.
                state.mark_dirty();
            }
        }
    }
}
