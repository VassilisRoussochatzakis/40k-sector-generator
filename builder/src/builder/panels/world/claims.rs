//! WORLD tab — claims chip-row (§W7).

use egui::{RichText, Ui};

use sectorforge::sector_model::{ClaimType, FactionClaim};
use sectorforge_gui_core::ui_kit;
use crate::builder::state::{EntityRef, ModalKind};
use crate::builder::BuilderState;

// ── claims chip-row (W7) ───────────────────────────────────────────────────

pub(super) fn show_claims_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let claim_count = state.sector.systems[sys_idx].worlds[w_idx].claims.len();
    ui_kit::collapsing_section(
        ui,
        "world_claims",
        &format!("Claims ({claim_count})"),
        true,
        |ui| {
            let claims = state.sector.systems[sys_idx].worlds[w_idx].claims.clone();
            let mut remove: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                for (i, c) in claims.iter().enumerate() {
                    let (bg, fg) = crate::builder::panels::presence_widgets::claim_chip_colours(
                        c.claim_type,
                    );
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, fg))
                        .rounding(4.0)
                        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let label =
                                    format!("{}  {}  {}", c.faction_id, c.claim_type, c.strength);
                                let resp = ui.label(RichText::new(label).color(fg).monospace());
                                if resp.clicked() {
                                    state.focus_entity(EntityRef::Faction(c.faction_id.clone()));
                                }
                                if ui.small_button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                }
                if claims.is_empty() {
                    ui_kit::placeholder(ui, "No claims on this world yet — add one below.");
                }
            });
            if let Some(i) = remove {
                // §R4: remove the claim via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                if let Err(e) = state.edit_world(wid, |w| {
                    w.claims.remove(i);
                }) {
                    state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
            ui.add_space(4.0);
            show_add_claim_row(ui, state, sys_idx, w_idx);
        },
    );
}

pub(super) fn show_add_claim_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let factions: Vec<_> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    if factions.is_empty() {
        return;
    }
    let row_id = egui::Id::new(("w_add_claim", sys_idx, w_idx));
    #[derive(Clone, Default)]
    struct AddBuf {
        faction: Option<sectorforge::ids::FactionId>,
        claim_type: Option<ClaimType>,
        strength: u8,
    }
    let mut buf: AddBuf = ui.data_mut(|d| d.get_temp::<AddBuf>(row_id).unwrap_or_default());
    if buf.faction.is_none() {
        buf.faction = Some(factions[0].0.clone());
    }
    if buf.claim_type.is_none() {
        buf.claim_type = Some(ClaimType::LegalSovereignty);
    }
    if buf.strength == 0 {
        buf.strength = 50;
    }

    ui.horizontal(|ui| {
        ui.label("Add claim:")
            .on_hover_text("Record a faction's claim over this world, with a type and strength.");
        let selected_label = buf
            .faction
            .as_ref()
            .and_then(|fid| {
                factions
                    .iter()
                    .find(|(id, _)| id == fid)
                    .map(|(_, n)| n.clone())
            })
            .unwrap_or_else(|| "(none)".into());
        ui_kit::combo(("w_add_claim_fac", sys_idx, w_idx), selected_label).show_ui(ui, |ui| {
            for (fid, name) in &factions {
                if ui
                    .selectable_label(buf.faction.as_ref() == Some(fid), name)
                    .clicked()
                {
                    buf.faction = Some(fid.clone());
                }
            }
        });
        ui_kit::combo(
            ("w_add_claim_kind", sys_idx, w_idx),
            format!("{}", buf.claim_type.unwrap_or(ClaimType::LegalSovereignty)),
        )
        .show_ui(ui, |ui| {
            for c in CLAIM_TYPES {
                ui.selectable_value(&mut buf.claim_type, Some(*c), format!("{c}"));
            }
        });
        ui.add(egui::DragValue::new(&mut buf.strength).range(0..=100))
            .on_hover_text("Claim strength 0–100 — how forcefully the faction presses this claim.");
        if ui
            .button("➕ Add claim")
            .on_hover_text("Add this claim to the world")
            .clicked()
        {
            if let (Some(fid), Some(kind)) = (buf.faction.clone(), buf.claim_type) {
                // §R4: add the claim via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                if let Err(e) = state.edit_world(wid, |w| {
                    w.claims.push(FactionClaim {
                        faction_id: fid,
                        claim_type: kind,
                        strength: buf.strength,
                    });
                }) {
                    state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(row_id, buf));
}

const CLAIM_TYPES: &[ClaimType] = &[
    ClaimType::LegalSovereignty,
    ClaimType::ImperialMandate,
    ClaimType::TreatyRight,
    ClaimType::ReligiousMandate,
    ClaimType::DynasticRight,
    ClaimType::CommercialCharter,
    ClaimType::MilitaryOccupation,
    ClaimType::AncientDomain,
    ClaimType::HuntingGround,
    ClaimType::CovertWrit,
    ClaimType::Rebellion,
];

