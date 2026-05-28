//! §I1..§I5 (BUILDER_REQS §29) — intel / fog-of-war editor surface.
//!
//! This module provides the shared UI primitives used by:
//!
//! * §I1 — per-system intel editor (rendered from the SYSTEM tab via
//!   [`show_system_intel_section`]).
//! * §I2 — per-world intel editor (rendered from the WORLD tab via
//!   [`show_world_intel_section`]).
//! * §I3 — "Generate baseline intel" button (calls
//!   [`sectorforge::intel::derive_intel`] over the live sector with the
//!   current faction roster as observer ids). Available from both editors.
//! * §I4 — observer-faction lens combo + redaction overlay on the MAP tab
//!   (see [`show_map_intel_controls`]).
//! * §I5 — `player_min_confidence` cutoff slider, applied to the redaction
//!   pass.
//!
//! Backing storage is [`sectorforge::intel::SystemIntel`] on both
//! [`sectorforge::sector_model::GeneratedSystem::intel`] and the new
//! [`sectorforge::sector_model::GeneratedWorld::intel`] field. Per-observer
//! views live under `intel.by_observer`, keyed by `FactionId.as_str()`.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::FactionId;
use sectorforge::intel::{
    ClassifiedState, IntelSource, ObserverView, PropagandaState, SuspectedPresence, SystemIntel,
};

use crate::builder::BuilderState;

/// §I4 — observer-faction lens combo + §I5 cutoff slider + §I3 baseline button.
/// Rendered above the hex map. Mutates `BuilderState::intel_observer` and
/// `BuilderState::intel_player_min_confidence`. The baseline button walks the
/// full sector and writes both layers.
pub fn show_map_intel_controls(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("observer:");
        let current = state.intel_observer.clone();
        let label = current
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(omniscient)".into());
        egui::ComboBox::from_id_salt("intel_observer_picker")
            .selected_text(label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.intel_observer, None, "(omniscient)");
                for f in &state.sector.factions {
                    let value = Some(f.id.clone());
                    let sel = state.intel_observer == value;
                    let name = format!("{} — {}", f.id, f.name);
                    if ui.selectable_label(sel, name).clicked() {
                        state.intel_observer = value;
                    }
                }
            });
        ui.separator();
        ui.label("cutoff:");
        ui.add(egui::Slider::new(&mut state.intel_player_min_confidence, 0..=100).text("min conf"));
        ui.separator();
        if ui
            .button("Generate baseline intel")
            .on_hover_text("Walks every system + world and overwrites their intel records with derive_intel(observer_ids = sector factions).")
            .clicked()
        {
            run_baseline_intel(state);
        }
        if state.intel_observer.is_some() && ui.button("clear lens").clicked() {
            state.intel_observer = None;
        }
    });
}

/// §I3 — runs `derive_intel` over the entire sector using every distinct
/// faction id in `sector.factions` as an observer. Marks the project dirty and
/// re-arms validation.
pub fn run_baseline_intel(state: &mut BuilderState) {
    let observer_ids: Vec<String> = state
        .sector
        .factions
        .iter()
        .map(|f| f.id.as_str().to_string())
        .collect();
    let observer_refs: Vec<&str> = observer_ids.iter().map(|s| s.as_str()).collect();
    sectorforge::intel::derive_intel(&mut state.sector, &observer_refs);
    state.dirty = true;
    state.mark_validation_dirty();
}

/// §I1 — per-system intel editor, hosted under a `CollapsingHeader` in the
/// SYSTEM panel. Reads the system's `intel.by_observer` map and lets the user
/// add / edit / remove observer views, plus their nested
/// suspected-presence rows.
pub fn show_system_intel_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Intel / fog of war")
        .default_open(false)
        .show(ui, |ui| {
            show_baseline_row(ui, state);
            ui.separator();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            let intel = &mut state.sector.systems[sys_idx].intel;
            let mut dirty = false;
            dirty |= show_observer_editor(ui, intel, "sys_intel", &factions);
            if dirty {
                state.dirty = true;
                state.mark_validation_dirty();
            }
        });
}

/// §I2 — per-world intel editor (same pattern as §I1, scoped to one world's
/// `intel.by_observer`).
pub fn show_world_intel_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    egui::CollapsingHeader::new("Intel / fog of war")
        .default_open(false)
        .show(ui, |ui| {
            show_baseline_row(ui, state);
            ui.separator();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            let intel = &mut state.sector.systems[sys_idx].worlds[w_idx].intel;
            let mut dirty = false;
            dirty |= show_observer_editor(ui, intel, "world_intel", &factions);
            if dirty {
                state.dirty = true;
                state.mark_validation_dirty();
            }
            if state.intel_player_min_confidence > 0 || state.intel_observer.is_some() {
                ui.separator();
                show_world_redaction_preview(
                    ui,
                    &state.sector.systems[sys_idx].worlds[w_idx],
                    state.intel_observer.as_ref(),
                    state.intel_player_min_confidence,
                );
            }
        });
}

fn show_baseline_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Generate baseline intel")
            .on_hover_text("Overwrites every system + world intel record from the live sector.")
            .clicked()
        {
            run_baseline_intel(state);
        }
        if let Some(obs) = state.intel_observer.clone() {
            ui.colored_label(
                Color32::from_rgb(150, 200, 255),
                format!("observer lens: {obs}"),
            );
        } else {
            ui.colored_label(Color32::GRAY, "lens: (omniscient)");
        }
        ui.colored_label(
            Color32::GRAY,
            format!("cutoff: {}", state.intel_player_min_confidence),
        );
    });
}

fn show_observer_editor(
    ui: &mut Ui,
    intel: &mut SystemIntel,
    id_salt: &str,
    factions: &[(FactionId, String)],
) -> bool {
    let mut dirty = false;
    if intel.by_observer.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "No observer records — run above or add one below.",
        );
    }
    let observers: Vec<String> = intel.by_observer.keys().cloned().collect();
    let mut remove_observer: Option<String> = None;
    for observer in observers {
        let header_label = format!("observer: {observer}");
        let response = egui::CollapsingHeader::new(header_label)
            .id_salt(format!("{id_salt}_{observer}"))
            .default_open(false)
            .show(ui, |ui| {
                let Some(view) = intel.by_observer.get_mut(&observer) else {
                    return false;
                };
                let mut local_dirty = false;
                local_dirty |= show_view_editor(ui, view, id_salt, &observer, factions);
                ui.horizontal(|ui| {
                    if ui.button("× remove observer").clicked() {
                        remove_observer = Some(observer.clone());
                    }
                });
                local_dirty
            });
        if let Some(inner) = response.body_returned {
            dirty |= inner;
        }
    }
    if let Some(key) = remove_observer {
        intel.by_observer.remove(&key);
        dirty = true;
    }

    ui.separator();
    show_add_observer_row(ui, intel, id_salt, factions, &mut dirty);
    dirty
}

fn show_view_editor(
    ui: &mut Ui,
    view: &mut ObserverView,
    id_salt: &str,
    observer: &str,
    factions: &[(FactionId, String)],
) -> bool {
    let mut dirty = false;
    egui::Grid::new(format!("{id_salt}_{observer}_meta"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("last_verified_tick");
            dirty |= ui
                .add(egui::DragValue::new(&mut view.last_verified_tick).range(0..=u32::MAX))
                .changed();
            ui.end_row();
            ui.label("confidence");
            dirty |= ui
                .add(egui::Slider::new(&mut view.confidence, 0..=100))
                .changed();
            ui.end_row();
            ui.label("propaganda_state");
            dirty |= propaganda_combo(
                ui,
                &format!("{id_salt}_{observer}_prop"),
                &mut view.propaganda_state,
            );
            ui.end_row();
            ui.label("classified_state");
            dirty |= classified_combo(
                ui,
                &format!("{id_salt}_{observer}_cls"),
                &mut view.classified_state,
            );
            ui.end_row();
        });

    ui.add_space(2.0);
    ui.label(RichText::new("suspected presences").strong());
    let mut remove_idx: Option<usize> = None;
    for (i, sus) in view.suspected_presences.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).monospace());
            ui.label(format!("faction: {}", sus.faction_id));
            ui.separator();
            ui.label("source:");
            dirty |= source_combo(
                ui,
                &format!("{id_salt}_{observer}_src_{i}"),
                &mut sus.source,
            );
            ui.separator();
            ui.label("conf:");
            dirty |= ui
                .add(egui::Slider::new(&mut sus.confidence, 0..=100))
                .changed();
            if ui.small_button("×").clicked() {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = remove_idx {
        view.suspected_presences.remove(i);
        dirty = true;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("+ suspected:");
        let existing: std::collections::BTreeSet<_> = view
            .suspected_presences
            .iter()
            .map(|s| s.faction_id.clone())
            .collect();
        for (fid, name) in factions {
            if existing.contains(fid) {
                continue;
            }
            if fid.as_str() == observer {
                continue;
            }
            if ui
                .small_button(format!("+{name}"))
                .on_hover_text(fid.as_str())
                .clicked()
            {
                view.suspected_presences.push(SuspectedPresence {
                    faction_id: fid.clone(),
                    source: IntelSource::Rumor,
                    confidence: 25,
                });
                dirty = true;
            }
        }
    });
    dirty
}

fn show_add_observer_row(
    ui: &mut Ui,
    intel: &mut SystemIntel,
    id_salt: &str,
    factions: &[(FactionId, String)],
    dirty: &mut bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label("+ observer:");
        for (fid, name) in factions {
            if intel.by_observer.contains_key(fid.as_str()) {
                continue;
            }
            if ui
                .small_button(format!("+{name}"))
                .on_hover_text(fid.as_str())
                .clicked()
            {
                intel
                    .by_observer
                    .insert(fid.as_str().to_string(), ObserverView::default());
                *dirty = true;
            }
        }
        // Free-text observer (covers external observers that are not in the
        // faction roster — useful for tests + imported sectors).
        let id = egui::Id::new(format!("{id_salt}_new_obs_text"));
        let mut text = ui
            .data_mut(|d| d.get_temp::<String>(id).clone())
            .unwrap_or_default();
        if ui.text_edit_singleline(&mut text).changed() {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        if ui.small_button("+ free").clicked() {
            let key = text.trim();
            if !key.is_empty() && !intel.by_observer.contains_key(key) {
                intel
                    .by_observer
                    .insert(key.to_string(), ObserverView::default());
                *dirty = true;
                ui.data_mut(|d| d.insert_temp::<String>(id, String::new()));
            }
        }
    });
}

fn propaganda_combo(ui: &mut Ui, id: &str, value: &mut PropagandaState) -> bool {
    let mut changed = false;
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .selected_text(propaganda_label(*value))
        .show_ui(ui, |ui| {
            for v in [
                PropagandaState::None,
                PropagandaState::OfficialPacified,
                PropagandaState::OfficialContested,
                PropagandaState::OfficialLost,
                PropagandaState::Counterfactual,
            ] {
                ui.selectable_value(value, v, propaganda_label(v));
            }
        });
    if *value != before {
        changed = true;
    }
    changed
}

fn classified_combo(ui: &mut Ui, id: &str, value: &mut ClassifiedState) -> bool {
    let mut changed = false;
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .selected_text(classified_label(*value))
        .show_ui(ui, |ui| {
            for v in [
                ClassifiedState::Public,
                ClassifiedState::CodexRedactus,
                ClassifiedState::PurgatusSigillum,
                ClassifiedState::ExterminatusFlag,
            ] {
                ui.selectable_value(value, v, classified_label(v));
            }
        });
    if *value != before {
        changed = true;
    }
    changed
}

fn source_combo(ui: &mut Ui, id: &str, value: &mut IntelSource) -> bool {
    let mut changed = false;
    let before = *value;
    egui::ComboBox::from_id_salt(id)
        .selected_text(source_label(*value))
        .show_ui(ui, |ui| {
            for v in [
                IntelSource::DirectObservation,
                IntelSource::AstropathicReport,
                IntelSource::InquisitorialAnalysis,
                IntelSource::Rumor,
                IntelSource::ImaginedDeduction,
            ] {
                ui.selectable_value(value, v, source_label(v));
            }
        });
    if *value != before {
        changed = true;
    }
    changed
}

fn propaganda_label(v: PropagandaState) -> &'static str {
    match v {
        PropagandaState::None => "none",
        PropagandaState::OfficialPacified => "official_pacified",
        PropagandaState::OfficialContested => "official_contested",
        PropagandaState::OfficialLost => "official_lost",
        PropagandaState::Counterfactual => "counterfactual",
        _ => "unknown",
    }
}

fn classified_label(v: ClassifiedState) -> &'static str {
    match v {
        ClassifiedState::Public => "public",
        ClassifiedState::CodexRedactus => "codex_redactus",
        ClassifiedState::PurgatusSigillum => "purgatus_sigillum",
        ClassifiedState::ExterminatusFlag => "exterminatus_flag",
        _ => "unknown",
    }
}

fn source_label(v: IntelSource) -> &'static str {
    match v {
        IntelSource::DirectObservation => "direct_observation",
        IntelSource::AstropathicReport => "astropathic_report",
        IntelSource::InquisitorialAnalysis => "inquisitorial_analysis",
        IntelSource::Rumor => "rumor",
        IntelSource::ImaginedDeduction => "imagined_deduction",
        _ => "unknown",
    }
}

/// §I4 / §I5 — read-only redacted view of a world's presences from the
/// observer's perspective. Hidden-tier presences below the cutoff are dropped
/// from the readout (so the user can see what the player edition would show).
fn show_world_redaction_preview(
    ui: &mut Ui,
    world: &sectorforge::sector_model::GeneratedWorld,
    observer: Option<&FactionId>,
    cutoff: u8,
) {
    let observer_str = observer.map(|f| f.as_str()).unwrap_or("");
    ui.label(RichText::new("redacted view").strong());
    if observer_str.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            format!("(omniscient — cutoff {cutoff} ignored)"),
        );
        return;
    }
    let kept = sectorforge::intel::redact_world_for_observer(world, observer_str, cutoff);
    if kept.is_empty() {
        ui.colored_label(Color32::DARK_GRAY, "redacted: nothing visible");
        return;
    }
    for p in kept {
        ui.label(format!(
            "{} ({:?}, vis {}, intel {})",
            p.faction_id, p.influence, p.dimensions.visibility as i32, p.intel_confidence
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::HexCoord;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 6, 6)
    }

    #[test]
    fn run_baseline_intel_writes_system_and_world_records() {
        let mut state = blank();
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.sector.add_world_to_system(&sys, "World One").unwrap();
        // Roster has zero factions, so observer_ids is empty and the records
        // stay empty — but we still expect the call not to panic and to mark
        // dirty.
        run_baseline_intel(&mut state);
        assert!(state.dirty);
    }

    #[test]
    fn run_baseline_intel_with_factions_populates_observers() {
        use sectorforge::sector_model::{
            DominanceState, FactionInfluence, GeneratedFaction, PresenceDimensions,
            WorldFactionPresence,
        };
        let mut state = blank();
        let sys = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let wid = state.sector.add_world_to_system(&sys, "World One").unwrap();
        state.sector.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "Imperial".into(),
            disposition: "Lawful".into(),
            subfactions: vec![],
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        });
        // Wire a presence so the per-world view has something to record.
        // Direct lookup — the builder index isn't rebuilt by raw sector mutation.
        let (sx, wx) = state
            .sector
            .systems
            .iter()
            .enumerate()
            .find_map(|(si, s)| s.worlds.iter().position(|w| w.id == wid).map(|wi| (si, wi)))
            .unwrap();
        state.sector.systems[sx].worlds[wx]
            .factions
            .push(WorldFactionPresence {
                faction_id: "imp".into(),
                subfaction_id: None,
                subfaction_name: None,
                force_id: None,
                force_name: None,
                influence: FactionInfluence::Dominant,
                relationship_to_government: "lawful".into(),
                dimensions: PresenceDimensions {
                    visibility: 80.0,
                    ..Default::default()
                },
                dominance: DominanceState::Controlled,
                intel_confidence: 90,
            });
        run_baseline_intel(&mut state);
        let sys0 = &state.sector.systems[0];
        assert!(sys0.intel.by_observer.contains_key("imp"));
        assert!(sys0.worlds[0].intel.by_observer.contains_key("imp"));
    }
}
