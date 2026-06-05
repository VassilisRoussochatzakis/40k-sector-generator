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
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

/// §I4 — observer-faction lens combo + §I5 cutoff slider + §I3 baseline button.
/// Rendered above the hex map. Mutates `BuilderState::intel_observer` and
/// `BuilderState::intel_player_min_confidence`. The baseline button walks the
/// full sector and writes both layers.
pub(crate) fn show_map_intel_controls(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Seen through:")
            .on_hover_text("Which faction's knowledge to view the map through. (omniscient) shows everything.");
        let current = state.intel_observer.clone();
        let label = current
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(omniscient)".into());
        ui_kit::combo("intel_observer_picker", label)
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
        ui.label("Hide below:")
            .on_hover_text("Confidence cutoff. Presences this faction is less sure of than the cutoff are hidden from the map.");
        ui.add(egui::Slider::new(&mut state.intel_player_min_confidence, 0..=100).text("min confidence"));
        ui.separator();
        if ui
            .button("↺  Re-derive baseline intel")
            .on_hover_text("Recomputes what every faction would know about each system and world from the current map. Replaces existing intel records.")
            .clicked()
        {
            run_baseline_intel(state);
        }
        if state.intel_observer.is_some()
            && ui
                .button("🚫  Clear lens")
                .on_hover_text("Stop viewing through a faction — show the omniscient map.")
                .clicked()
        {
            state.intel_observer = None;
        }
    });
}

/// §I3 — runs `derive_intel` over the entire sector using every distinct
/// faction id in `sector.factions` as an observer. Marks the project dirty and
/// re-arms validation.
pub(crate) fn run_baseline_intel(state: &mut BuilderState) {
    let observer_ids: Vec<String> = state
        .sector
        .factions
        .iter()
        .map(|f| f.id.as_str().to_string())
        .collect();
    // §R4 (INT-2): route the whole-sector derive through the command bus so it is
    // undoable. The command's apply() runs `intel::derive_intel`, snapshots the
    // prior per-entity intel for revert, and sets dirty + re-arms validation — so
    // the manual `state.dirty = true; state.mark_validation_dirty();` is gone.
    if let Err(e) = state.run(BuilderCommand::DeriveBaselineIntel {
        observers: observer_ids,
        before_systems: Vec::new(),
        before_worlds: Vec::new(),
    }) {
        state.modal = Some(ModalKind::Message(format!("Baseline intel failed: {e}")));
    }
}

/// §I1 — per-system intel editor, hosted under a `CollapsingHeader` in the
/// SYSTEM panel. Reads the system's `intel.by_observer` map and lets the user
/// add / edit / remove observer views, plus their nested
/// suspected-presence rows.
pub(crate) fn show_system_intel_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "intel_system_fog", "Intel / fog of war", false, |ui| {
        show_baseline_row(ui, state);
        ui.separator();
        // Gather the faction roster into an owned Vec *before* cloning the
        // system, so there is no outstanding `state` borrow when we clone.
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();
        // §R4 (INT-1): edit a clone of the system, then dispatch EditSystem so
        // the change is undoable. `state.run` sets dirty + re-runs invariants +
        // re-arms validation, replacing the old manual flags.
        let mut draft = state.sector.systems[sys_idx].clone();
        let dirty = show_observer_editor(ui, &mut draft.intel, "sys_intel", &factions);
        if dirty {
            if let Err(e) = state.run(BuilderCommand::EditSystem {
                system: draft.id.clone(),
                before: None,
                after: Box::new(draft),
            }) {
                state.modal = Some(ModalKind::Message(format!("Intel edit failed: {e}")));
            }
        }
    });
}

/// §I2 — per-world intel editor (same pattern as §I1, scoped to one world's
/// `intel.by_observer`).
pub(crate) fn show_world_intel_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "intel_world_fog", "Intel / fog of war", false, |ui| {
        show_baseline_row(ui, state);
        ui.separator();
        // Gather the faction roster into an owned Vec *before* cloning the
        // world, so there is no outstanding `state` borrow when we clone.
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();
        // §R4 (INT-1): edit a clone of the world, then dispatch EditWorld so
        // the change is undoable. `state.run` sets dirty + re-runs invariants +
        // re-arms validation, replacing the old manual flags.
        let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
        let dirty = show_observer_editor(ui, &mut draft.intel, "world_intel", &factions);
        if dirty {
            if let Err(e) = state.run(BuilderCommand::EditWorld {
                world: draft.id.clone(),
                before: None,
                after: Box::new(draft),
            }) {
                state.modal = Some(ModalKind::Message(format!("Intel edit failed: {e}")));
            }
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
            .button("↺  Re-derive baseline intel")
            .on_hover_text("Recomputes what every faction would know about each system and world from the current map. Replaces existing intel records.")
            .clicked()
        {
            run_baseline_intel(state);
        }
        if let Some(obs) = state.intel_observer.clone() {
            ui.colored_label(
                palette::info(),
                format!("Viewing as: {obs}"),
            );
        } else {
            ui.colored_label(Color32::DARK_GRAY, "Viewing as: everyone (omniscient)");
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            format!("Hide below confidence: {}", state.intel_player_min_confidence),
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
        ui_kit::placeholder(
            ui,
            "Nobody is tracking this yet. Re-derive baseline intel above, or add a faction's view below.",
        );
    }
    let observers: Vec<String> = intel.by_observer.keys().cloned().collect();
    let mut remove_observer: Option<String> = None;
    for observer in observers {
        let header_label = format!("Known to: {observer}");
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
                    if ui
                        .button("🗑  Remove this view")
                        .on_hover_text("Forget what this faction knows about this entity.")
                        .clicked()
                    {
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
    labeled(
        ui,
        "Last confirmed (tick)",
        "Game tick when this faction last verified what's here (schema: last_verified_tick). 0 means never confirmed.",
        |ui| {
            dirty |= ui
                .add(egui::DragValue::new(&mut view.last_verified_tick).range(0..=u32::MAX))
                .changed();
        },
    );
    labeled(
        ui,
        "Confidence",
        "How sure this faction is of the current picture, 0–100 (schema: confidence).",
        |ui| {
            dirty |= ui
                .add(egui::Slider::new(&mut view.confidence, 0..=100))
                .changed();
        },
    );
    labeled(
        ui,
        "Public stance",
        "The story this faction tells in public about who holds this place (schema: propaganda_state).",
        |ui| {
            dirty |= propaganda_combo(
                ui,
                &format!("{id_salt}_{observer}_prop"),
                &mut view.propaganda_state,
            );
        },
    );
    labeled(
        ui,
        "Secrecy",
        "How tightly this faction restricts the truth internally (schema: classified_state).",
        |ui| {
            dirty |= classified_combo(
                ui,
                &format!("{id_salt}_{observer}_cls"),
                &mut view.classified_state,
            );
        },
    );

    ui.add_space(2.0);
    ui.label(RichText::new("Factions believed present").strong())
        .on_hover_text("Who this observer thinks is here, and how good their information is (schema: suspected_presences).");
    if view.suspected_presences.is_empty() {
        ui_kit::placeholder(ui, "None suspected yet — add one from the list below.");
    }
    let mut remove_idx: Option<usize> = None;
    for (i, sus) in view.suspected_presences.iter_mut().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).monospace());
            ui.label(sus.faction_id.to_string())
                .on_hover_text("Suspected faction (schema: faction_id).");
            ui.separator();
            ui.label("via")
                .on_hover_text("How they learned of it (schema: source).");
            dirty |= source_combo(
                ui,
                &format!("{id_salt}_{observer}_src_{i}"),
                &mut sus.source,
            );
            ui.separator();
            ui.label("confidence")
                .on_hover_text("Certainty for this one faction, 0–100 (schema: confidence).");
            dirty |= ui
                .add(egui::Slider::new(&mut sus.confidence, 0..=100))
                .changed();
            if ui
                .small_button("🗑")
                .on_hover_text("Remove this suspected faction.")
                .clicked()
            {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = remove_idx {
        view.suspected_presences.remove(i);
        dirty = true;
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("➕  Suspect:");
        let existing: std::collections::BTreeSet<_> = view
            .suspected_presences
            .iter()
            .map(|s| s.faction_id.clone())
            .collect();
        let mut any = false;
        for (fid, name) in factions {
            if existing.contains(fid) {
                continue;
            }
            if fid.as_str() == observer {
                continue;
            }
            any = true;
            if ui
                .small_button(format!("+{name}"))
                .on_hover_text(format!(
                    "Mark {} as suspected here ({}).",
                    name,
                    fid.as_str()
                ))
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
        if !any {
            ui_kit::placeholder(ui, "every other faction is already suspected");
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
        ui.label("➕  Add a faction's view:");
        for (fid, name) in factions {
            if intel.by_observer.contains_key(fid.as_str()) {
                continue;
            }
            if ui
                .small_button(format!("+{name}"))
                .on_hover_text(format!(
                    "Start tracking what {} knows ({}).",
                    name,
                    fid.as_str()
                ))
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
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .id(id)
                    .desired_width(140.0)
                    .hint_text("other observer id…"),
            )
            .changed()
        {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
        if ui
            .small_button("➕  Add")
            .on_hover_text(
                "Track an observer that isn't in the faction roster (e.g. an outside power).",
            )
            .clicked()
        {
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
    let before = *value;
    ui_kit::combo(id, propaganda_label(*value)).show_ui(ui, |ui| {
        for v in [
            PropagandaState::None,
            PropagandaState::OfficialPacified,
            PropagandaState::OfficialContested,
            PropagandaState::OfficialLost,
            PropagandaState::Counterfactual,
        ] {
            if ui
                .selectable_label(*value == v, propaganda_label(v))
                .on_hover_text(format!("key: {}", propaganda_slug(v)))
                .clicked()
            {
                *value = v;
            }
        }
    });
    *value != before
}

fn classified_combo(ui: &mut Ui, id: &str, value: &mut ClassifiedState) -> bool {
    let before = *value;
    ui_kit::combo(id, classified_label(*value)).show_ui(ui, |ui| {
        for v in [
            ClassifiedState::Public,
            ClassifiedState::CodexRedactus,
            ClassifiedState::PurgatusSigillum,
            ClassifiedState::ExterminatusFlag,
        ] {
            if ui
                .selectable_label(*value == v, classified_label(v))
                .on_hover_text(format!("key: {}", classified_slug(v)))
                .clicked()
            {
                *value = v;
            }
        }
    });
    *value != before
}

fn source_combo(ui: &mut Ui, id: &str, value: &mut IntelSource) -> bool {
    let before = *value;
    ui_kit::combo(id, source_label(*value)).show_ui(ui, |ui| {
        for v in [
            IntelSource::DirectObservation,
            IntelSource::AstropathicReport,
            IntelSource::InquisitorialAnalysis,
            IntelSource::Rumor,
            IntelSource::ImaginedDeduction,
        ] {
            if ui
                .selectable_label(*value == v, source_label(v))
                .on_hover_text(format!("key: {}", source_slug(v)))
                .clicked()
            {
                *value = v;
            }
        }
    });
    *value != before
}

// Human-readable labels for the dropdowns; the raw serialization key is carried
// in each item's hover via the `*_slug` helpers below. Both arms cover the
// `#[non_exhaustive]` `sectorforge::intel` enums with a trailing wildcard.
fn propaganda_label(v: PropagandaState) -> &'static str {
    match v {
        PropagandaState::None => "No public line",
        PropagandaState::OfficialPacified => "Officially pacified",
        PropagandaState::OfficialContested => "Officially contested",
        PropagandaState::OfficialLost => "Officially lost",
        PropagandaState::Counterfactual => "Contradicts reality",
        _ => "Unknown",
    }
}

fn classified_label(v: ClassifiedState) -> &'static str {
    match v {
        ClassifiedState::Public => "Public",
        ClassifiedState::CodexRedactus => "Restricted (Codex Redactus)",
        ClassifiedState::PurgatusSigillum => "Sealed (Purgatus Sigillum)",
        ClassifiedState::ExterminatusFlag => "Exterminatus flag",
        _ => "Unknown",
    }
}

fn source_label(v: IntelSource) -> &'static str {
    match v {
        IntelSource::DirectObservation => "Direct observation",
        IntelSource::AstropathicReport => "Astropathic report",
        IntelSource::InquisitorialAnalysis => "Inquisitorial analysis",
        IntelSource::Rumor => "Rumour",
        IntelSource::ImaginedDeduction => "Guesswork",
        _ => "Unknown",
    }
}

fn propaganda_slug(v: PropagandaState) -> &'static str {
    match v {
        PropagandaState::None => "none",
        PropagandaState::OfficialPacified => "official_pacified",
        PropagandaState::OfficialContested => "official_contested",
        PropagandaState::OfficialLost => "official_lost",
        PropagandaState::Counterfactual => "counterfactual",
        _ => "unknown",
    }
}

fn classified_slug(v: ClassifiedState) -> &'static str {
    match v {
        ClassifiedState::Public => "public",
        ClassifiedState::CodexRedactus => "codex_redactus",
        ClassifiedState::PurgatusSigillum => "purgatus_sigillum",
        ClassifiedState::ExterminatusFlag => "exterminatus_flag",
        _ => "unknown",
    }
}

fn source_slug(v: IntelSource) -> &'static str {
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
    ui.label(RichText::new("What the player would see here").strong())
        .on_hover_text("Preview of this world after hiding presences below the cutoff, from the chosen faction's view.");
    if observer_str.is_empty() {
        ui_kit::placeholder(
            ui,
            &format!("Showing everyone's knowledge — cutoff {cutoff} not applied. Pick a faction in 'Seen through' to redact."),
        );
        return;
    }
    let kept = sectorforge::intel::redact_world_for_observer(world, observer_str, cutoff);
    if kept.is_empty() {
        ui_kit::placeholder(
            ui,
            "Nothing visible at this cutoff — all presences are hidden.",
        );
        return;
    }
    for p in kept {
        ui.label(format!(
            "{} — {} influence · visibility {} · confidence {}",
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
