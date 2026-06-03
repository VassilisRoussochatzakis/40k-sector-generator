//! §SU1 / §SU2 (BUILDER_REQS §32) — per-world surface-region editor.
//!
//! Rendered from the WORLD tab via [`show_surface_regions_section`].
//! Mutations route through [`BuilderCommand::SetSurfaceRegions`] so every
//! edit is undoable. The `Auto-seed` button calls
//! [`sectorforge::surface_region::derive_regions`] (§SU2) and replaces the
//! list wholesale. Each row exposes name, kind, dominant faction, control
//! score, population weight, visibility, and free-form notes.

use egui::{Color32, Ui};

use sectorforge_gui_core::ui_kit;

use sectorforge::ids::FactionId;
use sectorforge::surface_region::{derive_regions, RegionKind, SurfaceRegion};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

const REGION_KINDS: [RegionKind; 12] = [
    RegionKind::Capital,
    RegionKind::Hive,
    RegionKind::Underhive,
    RegionKind::ForgeComplex,
    RegionKind::ShrineContinent,
    RegionKind::AgriBelt,
    RegionKind::CardinalSpire,
    RegionKind::KnightHousehold,
    RegionKind::Wilderness,
    RegionKind::TombComplex,
    RegionKind::Hideout,
    RegionKind::Other,
];

pub fn show_surface_regions_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "sreg_surface_regions", "Surface regions", false, |ui| {
        let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();

        let mut working = state.sector.systems[sys_idx].worlds[w_idx].regions.clone();
        let original = working.clone();
        show_regions_editor(ui, &world_id, &mut working, &factions);

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Auto-seed")
                .on_hover_text(
                    "Calls sectorforge::surface_region::derive_regions for this world \
                     and replaces the regions list with the derived split.",
                )
                .clicked()
            {
                let w = &state.sector.systems[sys_idx].worlds[w_idx];
                working = derive_regions(w);
            }
            if ui.button("Clear regions").clicked() {
                working.clear();
            }
        });

        if working != original {
            let cmd = BuilderCommand::SetSurfaceRegions {
                world: world_id,
                before: None,
                after: working,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!(
                    "Surface region update failed: {e}"
                )));
            }
        }
    });
}

fn show_regions_editor(
    ui: &mut Ui,
    world_id: &sectorforge::ids::WorldId,
    regions: &mut Vec<SurfaceRegion>,
    factions: &[(FactionId, String)],
) {
    if regions.is_empty() {
        ui.colored_label(Color32::GRAY, "no surface regions (use Auto-seed or + Add)");
    }
    let mut remove_at: Option<usize> = None;
    for (i, region) in regions.iter_mut().enumerate() {
        let dominant_label = region
            .dominant
            .as_ref()
            .map(|f| f.to_string())
            .unwrap_or_else(|| "(none)".into());
        let header_label = format!(
            "{n}. {kind:?} — {name}  · {fid}",
            n = i + 1,
            kind = region.kind,
            name = region.name,
            fid = dominant_label,
        );
        ui_kit::collapsing_section(
            ui,
            ("sreg_region", world_id, i),
            &header_label,
            false,
            |ui| {
                egui::Grid::new(format!("surface_region_grid_{i}"))
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("name");
                        ui.text_edit_singleline(&mut region.name);
                        ui.end_row();

                        ui.label("kind");
                        ui_kit::combo(format!("sr_kind_{i}"), format!("{}", region.kind)).show_ui(
                            ui,
                            |ui| {
                                for k in REGION_KINDS {
                                    ui.selectable_value(&mut region.kind, k, format!("{}", k));
                                }
                            },
                        );
                        ui.end_row();

                        ui.label("dominant faction");
                        optional_faction_combo(
                            ui,
                            &format!("sr_fac_{i}"),
                            &mut region.dominant,
                            factions,
                        );
                        ui.end_row();

                        ui.label("control_score");
                        ui.add(egui::Slider::new(&mut region.control_score, 0..=100).text("/100"));
                        ui.end_row();

                        ui.label("population_weight");
                        ui.add(
                            egui::Slider::new(&mut region.population_weight, 0..=100).text("/100"),
                        );
                        ui.end_row();

                        ui.label("visibility");
                        ui.add(egui::Slider::new(&mut region.visibility, 0..=100).text("/100"));
                        ui.end_row();

                        ui.label("notes");
                        ui.add(
                            egui::TextEdit::multiline(&mut region.notes)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                    });

                ui.add_space(4.0);
                if ui.button("× remove region").clicked() {
                    remove_at = Some(i);
                }
            },
        );
    }
    if let Some(i) = remove_at {
        regions.remove(i);
    }
    ui.add_space(4.0);
    let total: u32 = regions.iter().map(|r| u32::from(r.population_weight)).sum();
    if total > 100 {
        ui.colored_label(
            Color32::from_rgb(220, 170, 60),
            format!("population_weight sum = {total} (>100 — over-allocated)"),
        );
    }
    if ui.button("+ Add surface region").clicked() {
        let default_fid = factions.first().map(|(f, _)| f.clone());
        regions.push(SurfaceRegion {
            name: format!("Region {}", regions.len() + 1),
            kind: RegionKind::Other,
            dominant: default_fid,
            control_score: 50,
            population_weight: 10,
            visibility: 50,
            notes: String::new(),
        });
    }
}

fn optional_faction_combo(
    ui: &mut Ui,
    id_salt: &str,
    current: &mut Option<FactionId>,
    factions: &[(FactionId, String)],
) {
    let label = current
        .as_ref()
        .map(|f| f.to_string())
        .unwrap_or_else(|| "(none)".into());
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "(none)").clicked() {
            *current = None;
        }
        for (fid, name) in factions {
            let sel = current.as_ref() == Some(fid);
            if ui
                .selectable_label(sel, format!("{fid} ({name})"))
                .clicked()
            {
                *current = Some(fid.clone());
            }
        }
    });
}
