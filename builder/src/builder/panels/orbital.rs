//! §O1 / §O2 (BUILDER_REQS §31) — per-system orbital-asset + blockade editor.
//!
//! Rendered from the SYSTEM tab via [`show_orbital_section`]. Mutations
//! route through [`BuilderCommand::SetOrbitalAssets`] and
//! [`BuilderCommand::SetBlockadeReport`] so every edit is undoable. The
//! `Derive now` button calls
//! [`sectorforge::orbital_assets::derive_orbital_assets`] and writes both
//! the assets list and the blockade report in a single batched pair of
//! commands.

use egui::{Color32, Ui};

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::orbital_assets::{
    derive_orbital_assets, BlockadeReport, OrbitalAsset, OrbitalAssetKind, ShipStock,
};
use sectorforge_gui_core::ui_kit;

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

/// §CTX1 Phase 6 (§6.9) — derive-and-dispatch helper extracted so the
/// SYSTEM-tab right-click `DERIVE ORBITAL ASSETS` row can reuse the same
/// command path the panel's "Derive now" button takes. Skips the dispatch
/// when the derived value equals the current one so the command log stays
/// clean; surfaces errors through [`ModalKind::Message`] like the panel does.
pub(crate) fn derive_and_apply_orbital_assets(state: &mut BuilderState, system: &SystemId) {
    let Some(sys) = state.sector.systems.iter().find(|s| &s.id == system) else {
        return;
    };
    let (assets, report) = derive_orbital_assets(sys);
    if assets != sys.orbital_assets {
        let cmd = BuilderCommand::SetOrbitalAssets {
            system: system.clone(),
            before: None,
            after: assets,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!(
                "Orbital asset derive failed: {e}"
            )));
            return;
        }
    }
    let prior = state
        .sector
        .systems
        .iter()
        .find(|s| &s.id == system)
        .map(|s| s.blockade.clone())
        .unwrap_or_default();
    if report != prior {
        let cmd = BuilderCommand::SetBlockadeReport {
            system: system.clone(),
            before: None,
            after: report,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Blockade derive failed: {e}")));
        }
    }
}

pub fn show_orbital_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "orb_root", "Orbital assets + blockade", false, |ui| {
        let sys_id = state.sector.systems[sys_idx].id.clone();
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();

        let mut assets_working = state.sector.systems[sys_idx].orbital_assets.clone();
        let assets_original = assets_working.clone();
        show_assets_editor(ui, &sys_id, &mut assets_working, &factions);

        ui.add_space(6.0);
        ui.separator();
        ui.label(egui::RichText::new("Blockade report").strong());

        let mut report_working = state.sector.systems[sys_idx].blockade.clone();
        let report_original = report_working.clone();
        show_report_editor(ui, &mut report_working, &factions);

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Derive now")
                .on_hover_text(
                    "Calls sectorforge::orbital_assets::derive_orbital_assets for this \
                         system and replaces both the asset list and the blockade report.",
                )
                .clicked()
            {
                let sys = &state.sector.systems[sys_idx];
                let (assets, report) = derive_orbital_assets(sys);
                assets_working = assets;
                report_working = report;
            }
            if ui.button("Clear assets").clicked() {
                assets_working.clear();
            }
            if ui.button("Clear blockade").clicked() {
                report_working = BlockadeReport::default();
            }
        });

        if assets_working != assets_original {
            let cmd = BuilderCommand::SetOrbitalAssets {
                system: sys_id.clone(),
                before: None,
                after: assets_working,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!(
                    "Orbital asset update failed: {e}"
                )));
            }
        }
        if report_working != report_original {
            let cmd = BuilderCommand::SetBlockadeReport {
                system: sys_id,
                before: None,
                after: report_working,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Blockade update failed: {e}")));
            }
        }
    });
}

fn show_assets_editor(
    ui: &mut Ui,
    sys_id: &sectorforge::ids::SystemId,
    assets: &mut Vec<OrbitalAsset>,
    factions: &[(FactionId, String)],
) {
    if assets.is_empty() {
        ui.colored_label(Color32::GRAY, "no orbital assets (use Derive now or + Add)");
    }
    let mut remove_at: Option<usize> = None;
    for (i, asset) in assets.iter_mut().enumerate() {
        let header_label = format!(
            "{i}. {kind:?} — {fid}  str={s}",
            i = i + 1,
            kind = asset.kind,
            fid = asset.faction_id,
            s = asset.strength
        );
        ui_kit::collapsing_section(ui, ("orb_asset", sys_id, i), &header_label, false, |ui| {
            egui::Grid::new(format!("orbital_asset_grid_{i}"))
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("id");
                    ui.monospace(&asset.id);
                    ui.end_row();

                    ui.label("kind");
                    ui_kit::combo(format!("orb_kind_{i}"), format!("{}", asset.kind)).show_ui(
                        ui,
                        |ui| {
                            for k in [
                                OrbitalAssetKind::Station,
                                OrbitalAssetKind::Shipyard,
                                OrbitalAssetKind::DefensePlatform,
                                OrbitalAssetKind::BlockadeFleet,
                            ] {
                                ui.selectable_value(&mut asset.kind, k, format!("{}", k));
                            }
                        },
                    );
                    ui.end_row();

                    ui.label("faction");
                    faction_combo(ui, &format!("orb_fac_{i}"), &mut asset.faction_id, factions);
                    ui.end_row();

                    ui.label("strength");
                    ui.add(egui::Slider::new(&mut asset.strength, 0..=100).text("/100"));
                    ui.end_row();
                });

            ui.add_space(2.0);
            ui.label("ship inventory");
            let mut drop_ship: Option<usize> = None;
            for (si, stock) in asset.ship_inventory.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", si + 1));
                    ui.label("hull");
                    ui.text_edit_singleline(&mut stock.hull_class);
                    ui.label("count");
                    ui.add(egui::DragValue::new(&mut stock.count).range(0..=u16::MAX));
                    if ui.small_button("×").clicked() {
                        drop_ship = Some(si);
                    }
                });
            }
            if let Some(si) = drop_ship {
                asset.ship_inventory.remove(si);
            }
            if ui.button("+ add ship stock").clicked() {
                asset.ship_inventory.push(ShipStock {
                    hull_class: "escort".into(),
                    count: 1,
                });
            }

            ui.add_space(4.0);
            if ui.button("× remove asset").clicked() {
                remove_at = Some(i);
            }
        });
    }
    if let Some(i) = remove_at {
        assets.remove(i);
    }
    ui.add_space(4.0);
    if ui.button("+ Add orbital asset").clicked() {
        let default_fid = factions
            .first()
            .map(|(f, _)| f.clone())
            .unwrap_or_else(|| FactionId::new("unknown"));
        let id_seq = assets.len() + 1;
        assets.push(OrbitalAsset {
            id: format!("{sys_id}-manual-{id_seq}"),
            kind: OrbitalAssetKind::Station,
            faction_id: default_fid,
            strength: 50,
            ship_inventory: Vec::new(),
        });
    }
}

fn show_report_editor(ui: &mut Ui, report: &mut BlockadeReport, factions: &[(FactionId, String)]) {
    egui::Grid::new("blockade_report_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("under_blockade");
            ui.checkbox(&mut report.under_blockade, "");
            ui.end_row();

            ui.label("blockader");
            optional_faction_combo(ui, "blockade_blockader", &mut report.blockader, factions);
            ui.end_row();

            ui.label("besieged");
            optional_faction_combo(ui, "blockade_besieged", &mut report.besieged, factions);
            ui.end_row();

            ui.label("intensity");
            ui.add(egui::Slider::new(&mut report.intensity, 0..=100).text("/100"));
            ui.end_row();
        });
}

fn faction_combo(
    ui: &mut Ui,
    id_salt: &str,
    current: &mut FactionId,
    factions: &[(FactionId, String)],
) {
    let label = current.to_string();
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        for (fid, name) in factions {
            if ui
                .selectable_label(fid == current, format!("{fid} ({name})"))
                .clicked()
            {
                *current = fid.clone();
            }
        }
    });
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
