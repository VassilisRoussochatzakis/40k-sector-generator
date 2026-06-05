//! CONTROL tab — §CL1 / §CL2 per-world claims list (filter + chip rows +
//! add-claim picker). Carved out of `control.rs` (§E10); the §CL3 contested
//! summary and §CL4 bulk-convert editors stay in the parent `mod.rs`.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, WorldId};
use sectorforge::sector_model::{ClaimType, FactionClaim};

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::panels::presence_widgets::claim_chip_colours;
use crate::builder::state::{EntityRef, ModalKind};
use crate::builder::BuilderState;

use super::{claim_label, CLAIM_TYPES};

/// §E10: a persistent single-line text filter. Reads the stored value for
/// `salt`, renders the edit box, writes it back on change, and returns the
/// current value — identical to the old store-then-re-read-from-temp dance
/// (the returned string is the value that was just written this frame).
fn filter_bar(ui: &mut Ui, salt: &str, hint: &str) -> String {
    let id = egui::Id::new(salt);
    let mut filter: String = ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
    let r = ui.add(
        egui::TextEdit::singleline(&mut filter)
            .hint_text(hint)
            .desired_width(180.0),
    );
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(id, filter.clone()));
    }
    filter
}

pub(super) fn show_world_list(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "ctrl_per_world_claims",
        "Per-world claims",
        false,
        |ui| {
            let mut filter = String::new();
            ui.horizontal(|ui| {
                ui.label("Filter:");
                filter = filter_bar(ui, "cl_world_filter", "world name…");
                let only_contested_id = egui::Id::new("cl_only_contested");
                let mut only_contested: bool =
                    ui.data_mut(|d| d.get_temp::<bool>(only_contested_id).unwrap_or(false));
                if ui
                    .checkbox(&mut only_contested, "Contested only")
                    .on_hover_text("Show only worlds claimed by more than one faction")
                    .changed()
                {
                    ui.data_mut(|d| d.insert_temp(only_contested_id, only_contested));
                }
            });

            let only_contested: bool = ui.data_mut(|d| {
                d.get_temp::<bool>(egui::Id::new("cl_only_contested"))
                    .unwrap_or(false)
            });
            let needle = filter.trim().to_lowercase();

            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();

            let mut rows: Vec<(usize, usize)> = Vec::new();
            for (si, sys) in state.sector.systems.iter().enumerate() {
                for (wi, w) in sys.worlds.iter().enumerate() {
                    if !needle.is_empty() && !w.name.to_lowercase().contains(&needle) {
                        continue;
                    }
                    let distinct: BTreeSet<&FactionId> =
                        w.claims.iter().map(|c| &c.faction_id).collect();
                    let contested = distinct.len() > 1;
                    if only_contested && !contested {
                        continue;
                    }
                    rows.push((si, wi));
                }
            }

            egui::ScrollArea::vertical()
                .id_salt("cl_world_scroll")
                .max_height(360.0)
                .show(ui, |ui| {
                    if rows.is_empty() {
                        ui_kit::placeholder(
                            ui,
                            "No worlds match the current filter. Clear the search box or the contested-only toggle.",
                        );
                        return;
                    }
                    for (si, wi) in rows {
                        show_world_row(ui, state, si, wi, &factions);
                        ui.add_space(2.0);
                    }
                });
        },
    );
}

fn show_world_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
    factions: &[(FactionId, String)],
) {
    let (name, wid, sys_id, claims_snapshot, contested) = {
        let sys = &state.sector.systems[sys_idx];
        let w = &sys.worlds[w_idx];
        let distinct: BTreeSet<FactionId> = w.claims.iter().map(|c| c.faction_id.clone()).collect();
        (
            w.name.to_string(),
            w.id.clone(),
            sys.id.clone(),
            w.claims.clone(),
            distinct.len() > 1,
        )
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).strong());
            if contested {
                ui.label(
                    RichText::new("CONTESTED")
                        .color(palette::warning())
                        .monospace(),
                );
            }
            ui.label(
                RichText::new(format!("{} claim(s)", claims_snapshot.len()))
                    .color(Color32::DARK_GRAY),
            );
            if ui
                .small_button("🌐 Open world")
                .on_hover_text("Jump to this world in the WORLD tab")
                .clicked()
            {
                state.focus_entity(EntityRef::World {
                    system: sys_id.clone(),
                    world: wid.clone(),
                });
            }
        });

        let mut remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, c) in claims_snapshot.iter().enumerate() {
                let (bg, fg) = claim_chip_colours(c.claim_type);
                egui::Frame::none()
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0, fg))
                    .rounding(4.0)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = format!(
                                "{}  {}  {}",
                                c.faction_id,
                                claim_label(c.claim_type),
                                c.strength
                            );
                            ui.label(RichText::new(label).color(fg))
                                .on_hover_text(format!(
                                    "{} · strength {} (schema: {})",
                                    c.faction_id,
                                    c.strength,
                                    c.claim_type.as_slug()
                                ));
                            if ui
                                .small_button("×")
                                .on_hover_text("Remove this claim")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
            }
            if claims_snapshot.is_empty() {
                ui_kit::placeholder(ui, "No claims yet — add one below.");
            }
        });
        if let Some(i) = remove {
            // §R4 (CTL-5): claim removal routes through EditWorld so it is
            // undoable.
            if i < state.sector.systems[sys_idx].worlds[w_idx].claims.len() {
                let id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                if let Err(e) = state.edit_world(id, |w| {
                    w.claims.remove(i);
                }) {
                    state.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
                }
            }
        }

        show_add_claim_row(ui, state, sys_idx, w_idx, &wid, factions);
    });
}

fn show_add_claim_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
    wid: &WorldId,
    factions: &[(FactionId, String)],
) {
    if factions.is_empty() {
        ui_kit::placeholder(
            ui,
            "No factions in this sector yet. Add some in the FACTIONS tab to record claims.",
        );
        return;
    }
    let row_id = egui::Id::new(("cl_add_claim", wid.as_str()));
    #[derive(Clone)]
    struct AddBuf {
        faction: FactionId,
        claim_type: ClaimType,
        strength: u8,
    }
    let default = AddBuf {
        faction: factions[0].0.clone(),
        claim_type: ClaimType::LegalSovereignty,
        strength: 50,
    };
    let mut buf: AddBuf = ui.data_mut(|d| d.get_temp::<AddBuf>(row_id).unwrap_or(default));

    ui.horizontal(|ui| {
        ui.label("Add claim:");
        let label = factions
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "(none)".into());
        ui_kit::combo(("cl_add_fac", wid.as_str()), label).show_ui(ui, |ui| {
            for (fid, n) in factions {
                if ui
                    .selectable_label(&buf.faction == fid, n)
                    .on_hover_text(format!("id: {fid}"))
                    .clicked()
                {
                    buf.faction = fid.clone();
                }
            }
        });
        ui_kit::combo(("cl_add_kind", wid.as_str()), claim_label(buf.claim_type)).show_ui(
            ui,
            |ui| {
                for k in CLAIM_TYPES {
                    ui.selectable_value(&mut buf.claim_type, *k, claim_label(*k))
                        .on_hover_text(format!("schema: {}", k.as_slug()));
                }
            },
        );
        ui.add(
            egui::DragValue::new(&mut buf.strength)
                .range(0..=100)
                .prefix("strength "),
        );
        if ui
            .button("➕ Add claim")
            .on_hover_text("Record this faction's claim on the world")
            .clicked()
        {
            // §R4 (CTL-5): claim add routes through EditWorld so it is undoable.
            let id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(id, |w| {
                w.claims.push(FactionClaim {
                    faction_id: buf.faction.clone(),
                    claim_type: buf.claim_type,
                    strength: buf.strength,
                });
            }) {
                state.modal = Some(ModalKind::Message(format!("Edit failed: {e}")));
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(row_id, buf));
}
