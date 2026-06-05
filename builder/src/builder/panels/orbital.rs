//! §O1 / §O2 (BUILDER_REQS §31) — per-system orbital-asset + blockade editor.
//!
//! Rendered from the SYSTEM tab via [`show_orbital_section`]. Mutations
//! route through [`BuilderCommand::SetOrbitalAssets`] and
//! [`BuilderCommand::SetBlockadeReport`] so every edit is undoable. The
//! "Auto-fill from system" button calls
//! [`sectorforge::orbital_assets::derive_orbital_assets`] and writes both
//! the assets list and the blockade report in a single batched pair of
//! commands.

use egui::{RichText, Ui};

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::orbital_assets::{
    derive_orbital_assets, BlockadeReport, OrbitalAsset, OrbitalAssetKind, ShipStock,
};
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

/// The four orbital-asset kinds, in the order the picker shows them. Kept as a
/// fixed slice (rather than a `match` over the `#[non_exhaustive]` enum) so a new
/// variant added upstream can't silently break this file at compile time.
const ASSET_KINDS: &[OrbitalAssetKind] = &[
    OrbitalAssetKind::Station,
    OrbitalAssetKind::Shipyard,
    OrbitalAssetKind::DefensePlatform,
    OrbitalAssetKind::BlockadeFleet,
];

/// Human-readable name for an asset kind. The enum only exposes a snake-case
/// `as_slug()` / `Display`, so we map the known variants to friendly words for
/// the UI and keep the raw slug in a hover. `_` keeps us forward-compatible with
/// the enum's `#[non_exhaustive]` marker.
fn kind_label(kind: OrbitalAssetKind) -> &'static str {
    match kind {
        OrbitalAssetKind::Station => "Station",
        OrbitalAssetKind::Shipyard => "Shipyard",
        OrbitalAssetKind::DefensePlatform => "Defense platform",
        OrbitalAssetKind::BlockadeFleet => "Blockade fleet",
        _ => "Other",
    }
}

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

pub(crate) fn show_orbital_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "orb_root", "Orbital assets & blockade", false, |ui| {
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
        ui.label(RichText::new("Blockade").strong());
        ui.label(
            RichText::new("Whether this system is being besieged, by whom, and how hard.")
                .color(palette::chrome_text_dim()),
        );

        let mut report_working = state.sector.systems[sys_idx].blockade.clone();
        let report_original = report_working.clone();
        show_report_editor(ui, &mut report_working, &factions);

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("✨  Auto-fill from system")
                .on_hover_text(
                    "Rebuild the asset list and blockade from the system's worlds and \
                     faction presence, replacing whatever is here now.",
                )
                .clicked()
            {
                let sys = &state.sector.systems[sys_idx];
                let (assets, report) = derive_orbital_assets(sys);
                assets_working = assets;
                report_working = report;
            }
            if ui
                .button("🗑  Clear assets")
                .on_hover_text("Remove every orbital asset from this system")
                .clicked()
            {
                assets_working.clear();
            }
            if ui
                .button("🗑  Clear blockade")
                .on_hover_text("Reset the blockade back to none")
                .clicked()
            {
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
        ui_kit::placeholder(
            ui,
            "No orbital assets yet — use \"Auto-fill from system\" below, or \"➕ Add orbital asset\".",
        );
    }
    let mut remove_at: Option<usize> = None;
    for (i, asset) in assets.iter_mut().enumerate() {
        let owner = factions
            .iter()
            .find(|(fid, _)| fid == &asset.faction_id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| asset.faction_id.to_string());
        let header_label = format!(
            "{n}. {kind} — {owner}  ·  strength {s}",
            n = i + 1,
            kind = kind_label(asset.kind),
            s = asset.strength,
        );
        ui_kit::collapsing_section(ui, ("orb_asset", sys_id, i), &header_label, false, |ui| {
            labeled(
                ui,
                "ID",
                "Stable identifier for this asset (schema: id). Generated automatically.",
                |ui| {
                    ui.monospace(&asset.id);
                },
            );
            labeled(
                ui,
                "Kind",
                "What this orbital structure is (schema: kind): a station, shipyard, defense platform, or blockade fleet.",
                |ui| {
                    ui_kit::combo(format!("orb_kind_{i}"), kind_label(asset.kind)).show_ui(
                        ui,
                        |ui| {
                            for &k in ASSET_KINDS {
                                ui.selectable_value(&mut asset.kind, k, kind_label(k))
                                    .on_hover_text(format!("key: {}", k.as_slug()));
                            }
                        },
                    );
                },
            );
            labeled(
                ui,
                "Owner",
                "Faction that controls this asset (schema: faction_id).",
                |ui| faction_combo(ui, &format!("orb_fac_{i}"), &mut asset.faction_id, factions),
            );
            labeled(
                ui,
                "Strength",
                "How powerful the asset is, 0–100 (schema: strength). Higher means harder to overcome.",
                |ui| {
                    ui.add(egui::Slider::new(&mut asset.strength, 0..=100).text("/ 100"));
                },
            );

            ui.add_space(2.0);
            ui.label(RichText::new("Ships stationed here").strong());
            let mut drop_ship: Option<usize> = None;
            if asset.ship_inventory.is_empty() {
                ui_kit::placeholder(ui, "none yet — use \"➕ Add ship class\" below.");
            }
            for (si, stock) in asset.ship_inventory.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}.", si + 1));
                    ui.label("Hull").on_hover_text(
                        "Ship class name, e.g. escort or cruiser (schema: hull_class).",
                    );
                    ui.text_edit_singleline(&mut stock.hull_class);
                    ui.label("Count")
                        .on_hover_text("How many of this hull class (schema: count).");
                    ui.add(egui::DragValue::new(&mut stock.count).range(0..=u16::MAX));
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove this ship class")
                        .clicked()
                    {
                        drop_ship = Some(si);
                    }
                });
            }
            if let Some(si) = drop_ship {
                asset.ship_inventory.remove(si);
            }
            if ui
                .button("➕  Add ship class")
                .on_hover_text("Add a hull class and count to this asset")
                .clicked()
            {
                asset.ship_inventory.push(ShipStock {
                    hull_class: "escort".into(),
                    count: 1,
                });
            }

            ui.add_space(4.0);
            if ui
                .button("🗑  Remove asset")
                .on_hover_text("Delete this orbital asset from the system")
                .clicked()
            {
                remove_at = Some(i);
            }
        });
    }
    if let Some(i) = remove_at {
        assets.remove(i);
    }
    ui.add_space(4.0);
    if ui
        .button("➕  Add orbital asset")
        .on_hover_text("Add a new station to this system")
        .clicked()
    {
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
    labeled(
        ui,
        "Under blockade",
        "Tick if this system is currently being blockaded (schema: under_blockade).",
        |ui| {
            ui.checkbox(&mut report.under_blockade, "");
        },
    );
    labeled(
        ui,
        "Blockading faction",
        "Who is enforcing the blockade (schema: blockader).",
        |ui| optional_faction_combo(ui, "blockade_blockader", &mut report.blockader, factions),
    );
    labeled(
        ui,
        "Besieged faction",
        "Who is being blockaded (schema: besieged).",
        |ui| optional_faction_combo(ui, "blockade_besieged", &mut report.besieged, factions),
    );
    labeled(
        ui,
        "Intensity",
        "How tight the blockade is, 0–100 (schema: intensity).",
        |ui| {
            ui.add(egui::Slider::new(&mut report.intensity, 0..=100).text("/ 100"));
        },
    );
}

/// Display name for a faction id, falling back to the raw id when the roster has
/// no match (e.g. a stale presence). Keeps the combo's selected-text readable.
fn faction_name(factions: &[(FactionId, String)], id: &FactionId) -> String {
    factions
        .iter()
        .find(|(fid, _)| fid == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn faction_combo(
    ui: &mut Ui,
    id_salt: &str,
    current: &mut FactionId,
    factions: &[(FactionId, String)],
) {
    if factions.is_empty() {
        ui_kit::placeholder(ui, "no factions in this sector yet");
        return;
    }
    let label = faction_name(factions, current);
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        for (fid, name) in factions {
            if ui
                .selectable_label(fid == current, name.as_str())
                .on_hover_text(format!("id: {fid}"))
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
        .map(|f| faction_name(factions, f))
        .unwrap_or_else(|| "(none)".into());
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "(none)").clicked() {
            *current = None;
        }
        for (fid, name) in factions {
            let sel = current.as_ref() == Some(fid);
            if ui
                .selectable_label(sel, name.as_str())
                .on_hover_text(format!("id: {fid}"))
                .clicked()
            {
                *current = Some(fid.clone());
            }
        }
    });
}
