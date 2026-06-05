//! WORLD tab — faction presence section (§W presence rows).

use egui::{Color32, Ui};

use sectorforge::ids::FactionId;
use sectorforge::sector_model::{
    DominanceState, FactionInfluence, PresenceDimensions, WorldFactionPresence,
};
use sectorforge_gui_core::ui_kit;
use crate::builder::state::{EntityRef, ModalKind};
use crate::builder::BuilderState;

// ── factions ──────────────────────────────────────────────────────────────

pub(super) fn show_factions_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "world_presence", "Faction presence", false, |ui| {
        let presences = state.sector.systems[sys_idx].worlds[w_idx].factions.clone();
        if presences.is_empty() {
            ui_kit::placeholder(
                ui,
                "No factions present here yet — add one with the picker below.",
            );
        }
        let mut remove_idx: Option<usize> = None;
        for (i, p) in presences.iter().enumerate() {
            ui.horizontal(|ui| {
                if sectorforge_gui_core::entity_link(ui, p.faction_id.to_string(), true).clicked() {
                    state.focus_entity(EntityRef::Faction(p.faction_id.clone()));
                }
                ui.colored_label(
                    Color32::DARK_GRAY,
                    format!(
                        "{} · {} · {}",
                        p.influence, p.relationship_to_government, p.dominance
                    ),
                );
                if ui
                    .small_button("×")
                    .on_hover_text("Remove this presence")
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }
        if let Some(i) = remove_idx {
            // §R4: remove the presence via EditWorld on a world clone.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(wid, |w| {
                w.factions.remove(i);
            }) {
                state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
        ui.separator();
        show_add_presence_row(ui, state, sys_idx, w_idx);
    });
}

const INFLUENCE_TIERS: &[FactionInfluence] = &[
    FactionInfluence::Dominant,
    FactionInfluence::Significant,
    FactionInfluence::Minor,
    FactionInfluence::Hidden,
];

const DOMINANCE_STATES: &[DominanceState] = &[
    DominanceState::Rumored,
    DominanceState::Presence,
    DominanceState::Influence,
    DominanceState::Contested,
    DominanceState::Controlled,
    DominanceState::Stronghold,
];

pub(super) fn show_add_presence_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let factions: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    // §E5: shared candidate computation; the picker widgets below stay
    // WORLD-specific (extra dominance combo, Display tier labels, wrapped layout)
    // — intentional divergence from the CONTROL row.
    use crate::builder::panels::presence_widgets::{presence_candidates, PresenceCandidates};
    let candidates = match presence_candidates(
        &state.sector.systems[sys_idx].worlds[w_idx],
        &factions,
    ) {
        PresenceCandidates::NoFactions => {
            ui_kit::placeholder(
                ui,
                "No factions in the sector roster yet — add some on the Factions tab first.",
            );
            return;
        }
        PresenceCandidates::AllPresent => {
            ui_kit::placeholder(
                ui,
                "Every faction in the roster already has a presence row on this world.",
            );
            return;
        }
        PresenceCandidates::Available(c) => c,
    };

    let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
    let buf_id = egui::Id::new(("w_add_presence", world_id.as_str()));
    #[derive(Clone)]
    struct Buf {
        faction: FactionId,
        tier: FactionInfluence,
        dominance: DominanceState,
    }
    let default = Buf {
        faction: candidates[0].0.clone(),
        tier: FactionInfluence::Minor,
        dominance: DominanceState::Presence,
    };
    let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(buf_id).unwrap_or(default));
    if !candidates.iter().any(|(fid, _)| fid == &buf.faction) {
        buf.faction = candidates[0].0.clone();
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("Add presence:")
            .on_hover_text("Place a faction on this world with a chosen influence and dominance.");
        let label = candidates
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| candidates[0].1.clone());
        ui_kit::combo(("w_add_fac", world_id.as_str()), label).show_ui(ui, |ui| {
            for (fid, n) in &candidates {
                if ui.selectable_label(&buf.faction == fid, n).clicked() {
                    buf.faction = (*fid).clone();
                }
            }
        });
        ui_kit::combo(("w_add_tier", world_id.as_str()), format!("{}", buf.tier)).show_ui(
            ui,
            |ui| {
                for t in INFLUENCE_TIERS {
                    ui.selectable_value(&mut buf.tier, *t, format!("{t}"));
                }
            },
        );
        ui_kit::combo(
            ("w_add_dom", world_id.as_str()),
            format!("{}", buf.dominance),
        )
        .show_ui(ui, |ui| {
            for d in DOMINANCE_STATES {
                ui.selectable_value(&mut buf.dominance, *d, format!("{d}"));
            }
        });
        if ui
            .button("➕ Add presence")
            .on_hover_text("Add this faction's presence to the world")
            .clicked()
        {
            // §R4: add the presence via EditWorld on a world clone.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if let Err(e) = state.edit_world(wid, |w| {
                w.factions.push(WorldFactionPresence {
                    faction_id: buf.faction.clone(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: buf.tier,
                    relationship_to_government: "neutral".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: buf.dominance,
                    intel_confidence: 100,
                });
            }) {
                state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(buf_id, buf));
}
