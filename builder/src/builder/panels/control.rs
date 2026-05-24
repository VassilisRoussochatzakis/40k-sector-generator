//! CONTROL tab (§N1 / §N2). Phase C §C1..§C8 presence/dominance/control-state
//! editor + Phase B §CL1..§CL4 claims editor.
//!
//! §C1  per-world presence rows: 10 dim sliders + influence tier + intel.
//! §C2  add / remove presence rows.
//! §C3  dominance state ComboBox per (world, faction); manual lock toggle.
//! §C4  per-system `control_state` ComboBox.
//! §C5  per-system `primary_factions` list with derive + manual lock.
//! §C6  per-faction `PowerProfile` preview, refreshed live from
//!      `aggregate_faction_power`.
//! §C7  toggle to project per-system power onto the MAP via the SectorView
//!      heatmap channel.
//! §C8  toggle to render the continuous influence-field on the MAP.
//!
//! §CL1 chip-row per world (faction_id, ClaimType, strength) + remove.
//! §CL2 add-claim picker. Multiple claims per faction allowed.
//! §CL3 Contested auto-flag — N>1 distinct claimants on a world.
//! §CL4 Bulk convert: every claim of kind X by faction Y becomes kind Z.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, RichText, Ui};

use sectorforge::control::{
    aggregate_faction_power, derive_system_control, derive_world_control,
};
use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sector_model::{
    ClaimType, DominanceState, FactionClaim, FactionInfluence, PowerProfile, PresenceDimensions,
    SystemState, WorldFactionPresence,
};

use crate::builder::state::{BuilderTab, ControlOverlay};
use crate::builder::BuilderState;

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

const SYSTEM_STATES: &[SystemState] = &[
    SystemState::Pacified,
    SystemState::Fragmented,
    SystemState::Blockaded,
    SystemState::Warzone,
    SystemState::Infiltrated,
    SystemState::Quarantined,
    SystemState::Uncharted,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Control");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§C1..§C8 presence / dominance / control-state + §CL1..§CL4 claims.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_overlay_toggles(ui, state);
            ui.separator();
            show_world_presence_editor(ui, state);
            ui.separator();
            show_system_control_editor(ui, state);
            ui.separator();
            show_power_profile_preview(ui, state);
            ui.separator();

            show_contested_summary(ui, state);
            ui.separator();
            show_bulk_convert(ui, state);
            ui.separator();
            show_world_list(ui, state);
        });
}

// ── C7 + C8 overlay toggles ──────────────────────────────────────────────

fn show_overlay_toggles(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§C7 / §C8 — MAP overlays")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("overlay:");
                for mode in [
                    ControlOverlay::None,
                    ControlOverlay::PowerProjection,
                    ControlOverlay::InfluenceField,
                ] {
                    let selected = state.control_overlay == mode;
                    if ui.selectable_label(selected, mode.label()).clicked() {
                        state.control_overlay = mode;
                    }
                }
                if ui.small_button("→ MAP").clicked() {
                    state.active_tab = BuilderTab::Map;
                }
            });
            ui.colored_label(
                Color32::DARK_GRAY,
                "Toggles override the MAP heatmap channel. POWER PROJECTION uses src/power_projection.rs; INFLUENCE FIELD samples src/influence_field.rs at every system coord.",
            );
        });
}

// ── C1 + C2 + C3 per-world presence editor ───────────────────────────────

fn show_world_presence_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§C1..§C3 — World presence")
        .default_open(true)
        .show(ui, |ui| {
            let world_id = state.selected_world_id.clone();
            let Some(world_id) = world_id else {
                ui.colored_label(
                    Color32::GRAY,
                    "Pick a world in the WORLD tab to edit its presence rows.",
                );
                return;
            };
            let Some((sys_idx, w_idx)) = state.find_world_indices(&world_id) else {
                ui.colored_label(Color32::GRAY, "Selected world not in the sector.");
                return;
            };

            let header = {
                let w = &state.sector.systems[sys_idx].worlds[w_idx];
                format!("{}  ({})", w.name, w.id)
            };
            ui.label(RichText::new(header).strong());

            // Snapshot for stable iteration; edits applied back below.
            let presences = state.sector.systems[sys_idx].worlds[w_idx]
                .factions
                .clone();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();

            let mut remove_at: Option<usize> = None;
            let mut edits: Vec<(usize, WorldFactionPresence)> = Vec::new();
            let mut lock_toggles: Vec<(FactionId, bool)> = Vec::new();

            egui::ScrollArea::vertical()
                .id_salt(("c1_scroll", world_id.as_str()))
                .max_height(360.0)
                .show(ui, |ui| {
                    for (i, p) in presences.iter().enumerate() {
                        let mut edit = p.clone();
                        let mut row_remove = false;
                        let mut locked = state
                            .dominance_locked
                            .contains(&(world_id.clone(), p.faction_id.clone()));
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let label = factions
                                    .iter()
                                    .find(|(fid, _)| fid == &p.faction_id)
                                    .map(|(_, n)| n.clone())
                                    .unwrap_or_else(|| p.faction_id.to_string());
                                ui.label(RichText::new(label).strong());
                                ui.label(
                                    RichText::new(p.faction_id.as_str())
                                        .color(Color32::DARK_GRAY)
                                        .monospace(),
                                );
                                if ui.small_button("× remove").clicked() {
                                    row_remove = true;
                                }
                            });

                            // §C1 influence tier + intel + dimensions.
                            ui.horizontal(|ui| {
                                ui.label("tier:");
                                egui::ComboBox::from_id_salt(("c1_tier", p.faction_id.as_str()))
                                    .selected_text(format!("{:?}", edit.influence))
                                    .show_ui(ui, |ui| {
                                        for t in INFLUENCE_TIERS {
                                            ui.selectable_value(
                                                &mut edit.influence,
                                                *t,
                                                format!("{t:?}"),
                                            );
                                        }
                                    });
                                ui.label("intel:");
                                ui.add(
                                    egui::DragValue::new(&mut edit.intel_confidence).range(0..=100),
                                );
                            });

                            ui.label(RichText::new("dimensions (0..=100)").color(Color32::DARK_GRAY));
                            egui::Grid::new(("c1_dim", p.faction_id.as_str()))
                                .num_columns(2)
                                .spacing([8.0, 2.0])
                                .show(ui, |ui| {
                                    dim_slider(ui, "admin", &mut edit.dimensions.admin);
                                    ui.end_row();
                                    dim_slider(ui, "military", &mut edit.dimensions.military);
                                    ui.end_row();
                                    dim_slider(ui, "orbital", &mut edit.dimensions.orbital);
                                    ui.end_row();
                                    dim_slider(ui, "economic", &mut edit.dimensions.economic);
                                    ui.end_row();
                                    dim_slider(ui, "industrial", &mut edit.dimensions.industrial);
                                    ui.end_row();
                                    dim_slider(ui, "ideological", &mut edit.dimensions.ideological);
                                    ui.end_row();
                                    dim_slider(ui, "covert", &mut edit.dimensions.covert);
                                    ui.end_row();
                                    dim_slider(ui, "logistics", &mut edit.dimensions.logistics);
                                    ui.end_row();
                                    dim_slider(ui, "legitimacy", &mut edit.dimensions.legitimacy);
                                    ui.end_row();
                                    dim_slider(ui, "visibility", &mut edit.dimensions.visibility);
                                    ui.end_row();
                                });

                            // §C3 dominance picker + manual lock.
                            ui.horizontal(|ui| {
                                ui.label("§C3 dominance:");
                                let score = edit.dimensions.local_control_score();
                                let derived = DominanceState::from_score(score);
                                if !locked {
                                    edit.dominance = derived;
                                }
                                egui::ComboBox::from_id_salt(("c3_dom", p.faction_id.as_str()))
                                    .selected_text(format!("{:?}", edit.dominance))
                                    .show_ui(ui, |ui| {
                                        for d in DOMINANCE_STATES {
                                            if ui
                                                .selectable_label(
                                                    edit.dominance == *d,
                                                    format!("{d:?}"),
                                                )
                                                .clicked()
                                            {
                                                edit.dominance = *d;
                                                locked = true;
                                            }
                                        }
                                    });
                                let was_locked = locked;
                                ui.checkbox(&mut locked, "manual lock");
                                if was_locked != locked {
                                    lock_toggles.push((p.faction_id.clone(), locked));
                                }
                                ui.colored_label(
                                    Color32::DARK_GRAY,
                                    format!(
                                        "auto = {:?} (score {:.1})",
                                        derived, score
                                    ),
                                );
                            });
                        });
                        if row_remove {
                            remove_at = Some(i);
                        } else if presence_changed(p, &edit) {
                            edits.push((i, edit));
                        }
                        ui.add_space(2.0);
                    }
                });

            // Apply lock toggles to side-table first.
            for (faction_id, locked) in lock_toggles {
                if locked {
                    state
                        .dominance_locked
                        .insert((world_id.clone(), faction_id));
                } else {
                    state
                        .dominance_locked
                        .remove(&(world_id.clone(), faction_id));
                }
            }
            if !edits.is_empty() {
                let target = &mut state.sector.systems[sys_idx].worlds[w_idx].factions;
                for (i, p) in edits {
                    if i < target.len() {
                        target[i] = p;
                    }
                }
                state.dirty = true;
                state.mark_validation_dirty();
            }
            if let Some(i) = remove_at {
                let target = &mut state.sector.systems[sys_idx].worlds[w_idx].factions;
                if i < target.len() {
                    let removed = target.remove(i);
                    state
                        .dominance_locked
                        .remove(&(world_id.clone(), removed.faction_id));
                    state.dirty = true;
                    state.mark_validation_dirty();
                }
            }

            // §C2 add-presence row.
            ui.separator();
            show_add_presence_row(ui, state, &world_id, sys_idx, w_idx, &factions);
        });
}

/// Compare only the panel-editable subset of a [`WorldFactionPresence`].
/// `WorldFactionPresence` doesn't derive `PartialEq` (it carries `Arc<str>`
/// fields), so we limit the dirty-check to what §C1 / §C3 can actually edit.
fn presence_changed(a: &WorldFactionPresence, b: &WorldFactionPresence) -> bool {
    a.influence != b.influence
        || a.intel_confidence != b.intel_confidence
        || a.dominance != b.dominance
        || a.dimensions != b.dimensions
}

fn dim_slider(ui: &mut Ui, label: &str, value: &mut f32) {
    ui.label(label);
    ui.add(egui::Slider::new(value, 0.0..=100.0).clamping(egui::SliderClamping::Always));
}

fn show_add_presence_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    world_id: &WorldId,
    sys_idx: usize,
    w_idx: usize,
    factions: &[(FactionId, String)],
) {
    if factions.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "no factions in this sector — add factions to enable presence.",
        );
        return;
    }
    // Filter out factions already present on this world.
    let already: BTreeSet<FactionId> = state.sector.systems[sys_idx].worlds[w_idx]
        .factions
        .iter()
        .map(|p| p.faction_id.clone())
        .collect();
    let candidates: Vec<&(FactionId, String)> = factions
        .iter()
        .filter(|(fid, _)| !already.contains(fid))
        .collect();
    if candidates.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "every faction in this sector already has a presence row here.",
        );
        return;
    }

    let buf_id = egui::Id::new(("c2_add_buf", world_id.as_str()));
    #[derive(Clone)]
    struct Buf {
        faction: FactionId,
        tier: FactionInfluence,
    }
    let default = Buf {
        faction: candidates[0].0.clone(),
        tier: FactionInfluence::Minor,
    };
    let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(buf_id).unwrap_or(default));

    ui.horizontal(|ui| {
        ui.label("§C2 add presence:");
        let label = candidates
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| candidates[0].1.clone());
        if !candidates.iter().any(|(fid, _)| fid == &buf.faction) {
            buf.faction = candidates[0].0.clone();
        }
        egui::ComboBox::from_id_salt(("c2_fac", world_id.as_str()))
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (fid, n) in &candidates {
                    if ui.selectable_label(&buf.faction == fid, n).clicked() {
                        buf.faction = (*fid).clone();
                    }
                }
            });
        egui::ComboBox::from_id_salt(("c2_tier", world_id.as_str()))
            .selected_text(format!("{:?}", buf.tier))
            .show_ui(ui, |ui| {
                for t in INFLUENCE_TIERS {
                    ui.selectable_value(&mut buf.tier, *t, format!("{t:?}"));
                }
            });
        if ui.button("+ presence").clicked() {
            state.sector.systems[sys_idx].worlds[w_idx]
                .factions
                .push(WorldFactionPresence {
                    faction_id: buf.faction.clone(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: buf.tier,
                    relationship_to_government: "neutral".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                });
            state.dirty = true;
            state.mark_validation_dirty();
        }
    });
    ui.data_mut(|d| d.insert_temp(buf_id, buf));
}

// ── C4 + C5 per-system control editor ────────────────────────────────────

fn show_system_control_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§C4 / §C5 — System control")
        .default_open(true)
        .show(ui, |ui| {
            let system_id = state.selected_system_id.clone();
            let Some(system_id) = system_id else {
                ui.colored_label(
                    Color32::GRAY,
                    "Pick a system in the SYSTEM tab to edit its control state and primary factions.",
                );
                return;
            };
            let Some(sys_idx) = state
                .sector
                .systems
                .iter()
                .position(|s| s.id == system_id)
            else {
                ui.colored_label(Color32::GRAY, "Selected system not in the sector.");
                return;
            };
            let header = {
                let s = &state.sector.systems[sys_idx];
                format!("{}  ({})", s.name, s.id)
            };
            ui.label(RichText::new(header).strong());

            // §C4 control_state picker.
            let mut new_state: Option<Option<SystemState>> = None;
            {
                let current = state.sector.systems[sys_idx].control.state;
                ui.horizontal(|ui| {
                    ui.label("§C4 control_state:");
                    egui::ComboBox::from_id_salt(("c4_state", system_id.as_str()))
                        .selected_text(match current {
                            Some(s) => format!("{s:?}"),
                            None => "(none)".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_none(), "(none)").clicked() {
                                new_state = Some(None);
                            }
                            for s in SYSTEM_STATES {
                                if ui
                                    .selectable_label(current == Some(*s), format!("{s:?}"))
                                    .clicked()
                                {
                                    new_state = Some(Some(*s));
                                }
                            }
                        });
                });
            }
            if let Some(ns) = new_state {
                state.sector.systems[sys_idx].control.state = ns;
                state.dirty = true;
                state.mark_validation_dirty();
            }

            // §C5 primary_factions: auto-derive top-3 unless locked.
            ui.add_space(4.0);
            let mut locked = state.primary_factions_locked.contains(&system_id);
            let derived: Vec<FactionId> = {
                let summary = derive_system_control(&state.sector.systems[sys_idx]);
                summary
                    .top_factions
                    .iter()
                    .take(3)
                    .map(|s| s.faction_id.clone())
                    .collect()
            };
            if !locked {
                state.sector.systems[sys_idx].primary_factions = derived.clone();
            }
            let factions = &state.sector.systems[sys_idx].primary_factions.clone();
            ui.horizontal(|ui| {
                ui.label(RichText::new("§C5 primary_factions:").strong());
                let was_locked = locked;
                ui.checkbox(&mut locked, "manual override");
                if was_locked != locked {
                    if locked {
                        state.primary_factions_locked.insert(system_id.clone());
                    } else {
                        state.primary_factions_locked.remove(&system_id);
                    }
                    state.dirty = true;
                }
                if ui.small_button("Recompute").clicked() {
                    state.sector.systems[sys_idx].primary_factions = derived.clone();
                    state.primary_factions_locked.remove(&system_id);
                    state.dirty = true;
                    state.mark_validation_dirty();
                }
            });
            if factions.is_empty() {
                ui.colored_label(Color32::GRAY, "(no factions ranked here yet)");
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (rank, fid) in factions.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("{}. {}", rank + 1, fid))
                                .monospace()
                                .color(Color32::LIGHT_BLUE),
                        );
                    }
                });
            }

            // Read-only system summary echo.
            ui.add_space(4.0);
            let summary = derive_system_control(&state.sector.systems[sys_idx]);
            ui.colored_label(
                Color32::DARK_GRAY,
                format!(
                    "derived: state={:?}  dom={}  sov={}  orb={}  econ={}  hidden={}",
                    summary.state,
                    summary
                        .dominant
                        .as_ref()
                        .map(|f| f.as_str())
                        .unwrap_or("-"),
                    summary
                        .sovereign
                        .as_ref()
                        .map(|f| f.as_str())
                        .unwrap_or("-"),
                    summary
                        .orbital_controller
                        .as_ref()
                        .map(|f| f.as_str())
                        .unwrap_or("-"),
                    summary
                        .economic_hegemon
                        .as_ref()
                        .map(|f| f.as_str())
                        .unwrap_or("-"),
                    summary
                        .hidden_master
                        .as_ref()
                        .map(|f| f.as_str())
                        .unwrap_or("-"),
                ),
            );

            // Echo derived world-control for the selected world to make the
            // §C3 dominance loop legible from the system view too.
            if let Some(wid) = state.selected_world_id.clone() {
                if let Some((wsi, wwi)) = state.find_world_indices(&wid) {
                    if wsi == sys_idx {
                        let w = &state.sector.systems[wsi].worlds[wwi];
                        let s = derive_world_control(w);
                        ui.colored_label(
                            Color32::DARK_GRAY,
                            format!(
                                "world {}: dom={} sov={} occ={} score={:.1} contested={}",
                                w.name,
                                s.dominant.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                                s.sovereign.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                                s.occupier.as_ref().map(|f| f.as_str()).unwrap_or("-"),
                                s.control_score,
                                s.contested
                            ),
                        );
                    }
                }
            }
        });
}

// ── C6 PowerProfile preview ──────────────────────────────────────────────

fn show_power_profile_preview(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§C6 — PowerProfile preview")
        .default_open(false)
        .show(ui, |ui| {
            let power = aggregate_faction_power(&state.sector.systems);
            if power.is_empty() {
                ui.colored_label(Color32::GRAY, "no presence rows yet.");
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("c6_scroll")
                .max_height(220.0)
                .show(ui, |ui| {
                    egui::Grid::new("c6_grid")
                        .num_columns(10)
                        .striped(true)
                        .show(ui, |ui| {
                            for h in [
                                "faction",
                                "admin",
                                "mil",
                                "naval",
                                "econ",
                                "ind",
                                "ideo",
                                "covert",
                                "logi",
                                "legit",
                                "total",
                            ] {
                                ui.label(RichText::new(h).strong().monospace());
                            }
                            ui.end_row();
                            for (fid, p) in &power {
                                ui.label(RichText::new(fid.as_str()).monospace());
                                for v in [
                                    p.administrative,
                                    p.military,
                                    p.naval,
                                    p.economic,
                                    p.industrial,
                                    p.ideological,
                                    p.covert,
                                    p.logistical,
                                    p.legitimacy,
                                ] {
                                    ui.label(
                                        RichText::new(format!("{v:5.1}"))
                                            .monospace()
                                            .color(power_color(v)),
                                    );
                                }
                                ui.label(
                                    RichText::new(format!("{:5.1}", p.total_projection()))
                                        .strong()
                                        .monospace(),
                                );
                                ui.end_row();
                            }
                        });
                });
            if ui.small_button("Apply to sector rollups").clicked() {
                let power = aggregate_faction_power(&state.sector.systems);
                sectorforge::control::apply_faction_power(&mut state.sector.factions, &power);
                state.dirty = true;
                state.mark_validation_dirty();
            }
        });
}

fn power_color(v: f32) -> Color32 {
    if v >= 30.0 {
        Color32::LIGHT_GREEN
    } else if v >= 10.0 {
        Color32::LIGHT_YELLOW
    } else if v >= 1.0 {
        Color32::LIGHT_GRAY
    } else {
        Color32::DARK_GRAY
    }
}

// ── CL3 contested summary ─────────────────────────────────────────────────

fn contested_worlds(state: &BuilderState) -> Vec<(usize, usize, BTreeSet<FactionId>)> {
    let mut out = Vec::new();
    for (si, sys) in state.sector.systems.iter().enumerate() {
        for (wi, w) in sys.worlds.iter().enumerate() {
            let distinct: BTreeSet<FactionId> =
                w.claims.iter().map(|c| c.faction_id.clone()).collect();
            if distinct.len() > 1 {
                out.push((si, wi, distinct));
            }
        }
    }
    out
}

fn show_contested_summary(ui: &mut Ui, state: &mut BuilderState) {
    let contested = contested_worlds(state);
    egui::CollapsingHeader::new(format!("§CL3 — Contested ({})", contested.len()))
        .default_open(false)
        .show(ui, |ui| {
            if contested.is_empty() {
                ui.colored_label(
                    Color32::GRAY,
                    "no contested worlds — every world has ≤ 1 distinct claimant.",
                );
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("cl3_scroll")
                .max_height(140.0)
                .show(ui, |ui| {
                    for (si, wi, distinct) in contested {
                        let w = &state.sector.systems[si].worlds[wi];
                        let label = format!(
                            "{}  ({} distinct claimants, {} claims)",
                            w.name,
                            distinct.len(),
                            w.claims.len()
                        );
                        let wid = w.id.clone();
                        ui.horizontal(|ui| {
                            if ui.small_button("→ WORLD").clicked() {
                                state.selected_world_id = Some(wid);
                                state.active_tab = BuilderTab::World;
                            }
                            ui.label(RichText::new(label).color(Color32::from_rgb(220, 160, 80)));
                        });
                    }
                });
        });
}

// ── CL4 bulk convert ──────────────────────────────────────────────────────

fn show_bulk_convert(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§CL4 — Bulk convert claims")
        .default_open(false)
        .show(ui, |ui| {
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            if factions.is_empty() {
                ui.colored_label(Color32::GRAY, "no factions in this sector.");
                return;
            }
            let id = egui::Id::new("cl4_bulk_buf");
            #[derive(Clone)]
            struct Buf {
                faction: FactionId,
                from: ClaimType,
                to: ClaimType,
            }
            let default = Buf {
                faction: factions[0].0.clone(),
                from: ClaimType::LegalSovereignty,
                to: ClaimType::LegalSovereignty,
            };
            let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(id).unwrap_or(default));

            ui.horizontal(|ui| {
                ui.label("faction Y:");
                let label = factions
                    .iter()
                    .find(|(fid, _)| fid == &buf.faction)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| "(none)".into());
                egui::ComboBox::from_id_salt("cl4_faction")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (fid, n) in &factions {
                            if ui.selectable_label(&buf.faction == fid, n).clicked() {
                                buf.faction = fid.clone();
                            }
                        }
                    });
                ui.label("claim X:");
                egui::ComboBox::from_id_salt("cl4_from")
                    .selected_text(format!("{:?}", buf.from))
                    .show_ui(ui, |ui| {
                        for k in CLAIM_TYPES {
                            ui.selectable_value(&mut buf.from, *k, format!("{k:?}"));
                        }
                    });
                ui.label("→ Z:");
                egui::ComboBox::from_id_salt("cl4_to")
                    .selected_text(format!("{:?}", buf.to))
                    .show_ui(ui, |ui| {
                        for k in CLAIM_TYPES {
                            ui.selectable_value(&mut buf.to, *k, format!("{k:?}"));
                        }
                    });
            });

            let matches = count_bulk_matches(state, &buf.faction, buf.from);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("matches: {matches}"))
                        .color(Color32::DARK_GRAY)
                        .monospace(),
                );
                let same = buf.from == buf.to;
                let disabled = matches == 0 || same;
                let btn = egui::Button::new(format!("Convert {matches} claims"));
                if ui.add_enabled(!disabled, btn).clicked() {
                    let n = apply_bulk_convert(state, &buf.faction, buf.from, buf.to);
                    if n > 0 {
                        state.dirty = true;
                        state.mark_validation_dirty();
                    }
                }
                if same {
                    ui.colored_label(Color32::GRAY, "X = Z — nothing to do");
                }
            });

            ui.data_mut(|d| d.insert_temp(id, buf));
        });
}

fn count_bulk_matches(state: &BuilderState, faction: &FactionId, from: ClaimType) -> usize {
    let mut n = 0usize;
    for sys in &state.sector.systems {
        for w in &sys.worlds {
            for c in &w.claims {
                if &c.faction_id == faction && c.claim_type == from {
                    n += 1;
                }
            }
        }
    }
    n
}

fn apply_bulk_convert(
    state: &mut BuilderState,
    faction: &FactionId,
    from: ClaimType,
    to: ClaimType,
) -> usize {
    let mut n = 0usize;
    for sys in &mut state.sector.systems {
        for w in &mut sys.worlds {
            for c in &mut w.claims {
                if &c.faction_id == faction && c.claim_type == from {
                    c.claim_type = to;
                    n += 1;
                }
            }
        }
    }
    n
}

// ── CL1 + CL2 per-world chip-row ─────────────────────────────────────────

fn show_world_list(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§CL1 / §CL2 — Per-world claims")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("filter:");
                let id = egui::Id::new("cl_world_filter");
                let mut filter: String =
                    ui.data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
                let r = ui.add(egui::TextEdit::singleline(&mut filter).desired_width(180.0));
                if r.changed() {
                    ui.data_mut(|d| d.insert_temp(id, filter.clone()));
                }
                let only_contested_id = egui::Id::new("cl_only_contested");
                let mut only_contested: bool =
                    ui.data_mut(|d| d.get_temp::<bool>(only_contested_id).unwrap_or(false));
                if ui.checkbox(&mut only_contested, "contested only").changed() {
                    ui.data_mut(|d| d.insert_temp(only_contested_id, only_contested));
                }
            });

            let filter: String = ui.data_mut(|d| {
                d.get_temp::<String>(egui::Id::new("cl_world_filter"))
                    .unwrap_or_default()
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
                        ui.colored_label(Color32::GRAY, "no worlds match the current filter.");
                        return;
                    }
                    for (si, wi) in rows {
                        show_world_row(ui, state, si, wi, &factions);
                        ui.add_space(2.0);
                    }
                });
        });
}

fn show_world_row(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
    factions: &[(FactionId, String)],
) {
    let (name, wid, claims_snapshot, contested) = {
        let w = &state.sector.systems[sys_idx].worlds[w_idx];
        let distinct: BTreeSet<FactionId> =
            w.claims.iter().map(|c| c.faction_id.clone()).collect();
        (
            w.name.to_string(),
            w.id.clone(),
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
                        .color(Color32::from_rgb(220, 160, 80))
                        .monospace(),
                );
            }
            ui.label(
                RichText::new(format!("{} claims", claims_snapshot.len()))
                    .color(Color32::DARK_GRAY)
                    .monospace(),
            );
            if ui.small_button("→ WORLD").clicked() {
                state.selected_world_id = Some(wid.clone());
                state.active_tab = BuilderTab::World;
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
                            let label =
                                format!("{}  {:?}  {}", c.faction_id, c.claim_type, c.strength);
                            ui.label(RichText::new(label).color(fg).monospace());
                            if ui.small_button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
            }
            if claims_snapshot.is_empty() {
                ui.colored_label(Color32::GRAY, "no claims");
            }
        });
        if let Some(i) = remove {
            state.sector.systems[sys_idx].worlds[w_idx].claims.remove(i);
            state.dirty = true;
            state.mark_validation_dirty();
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
        ui.colored_label(
            Color32::GRAY,
            "no factions in this sector — add factions to enable claims.",
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
        ui.label("add:");
        let label = factions
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt(("cl_add_fac", wid.as_str()))
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (fid, n) in factions {
                    if ui.selectable_label(&buf.faction == fid, n).clicked() {
                        buf.faction = fid.clone();
                    }
                }
            });
        egui::ComboBox::from_id_salt(("cl_add_kind", wid.as_str()))
            .selected_text(format!("{:?}", buf.claim_type))
            .show_ui(ui, |ui| {
                for k in CLAIM_TYPES {
                    ui.selectable_value(&mut buf.claim_type, *k, format!("{k:?}"));
                }
            });
        ui.add(egui::DragValue::new(&mut buf.strength).range(0..=100));
        if ui.button("+ claim").clicked() {
            state.sector.systems[sys_idx].worlds[w_idx]
                .claims
                .push(FactionClaim {
                    faction_id: buf.faction.clone(),
                    claim_type: buf.claim_type,
                    strength: buf.strength,
                });
            state.dirty = true;
            state.mark_validation_dirty();
        }
    });
    ui.data_mut(|d| d.insert_temp(row_id, buf));
}

fn claim_chip_colours(kind: ClaimType) -> (Color32, Color32) {
    match kind {
        ClaimType::LegalSovereignty => (Color32::from_rgb(40, 60, 100), Color32::LIGHT_BLUE),
        ClaimType::ImperialMandate => (Color32::from_rgb(80, 70, 30), Color32::YELLOW),
        ClaimType::TreatyRight => (Color32::from_rgb(40, 80, 80), Color32::LIGHT_GREEN),
        ClaimType::ReligiousMandate => (Color32::from_rgb(80, 60, 30), Color32::LIGHT_YELLOW),
        ClaimType::DynasticRight => (Color32::from_rgb(80, 30, 70), Color32::LIGHT_RED),
        ClaimType::CommercialCharter => (Color32::from_rgb(40, 90, 50), Color32::GREEN),
        ClaimType::MilitaryOccupation => (Color32::from_rgb(100, 30, 30), Color32::LIGHT_RED),
        ClaimType::AncientDomain => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
        ClaimType::HuntingGround => (Color32::from_rgb(60, 50, 30), Color32::LIGHT_YELLOW),
        ClaimType::CovertWrit => (Color32::from_rgb(30, 30, 60), Color32::LIGHT_BLUE),
        ClaimType::Rebellion => (Color32::from_rgb(120, 30, 30), Color32::LIGHT_RED),
    }
}

// ── Public helpers used by the MAP overlay (§C7 / §C8) ───────────────────

/// §C7 / §C8: build the per-system overlay cell map the MAP panel feeds into
/// [`sectorforge_gui_core::sector_view::SectorView::heatmap`] when
/// [`crate::builder::state::ControlOverlay`] is on. The returned map is
/// keyed by system id; [`None`] when the overlay is `None`.
#[must_use]
pub fn build_overlay_cells(
    sector: &sectorforge::sector_model::GeneratedSector,
    factions: &[sectorforge::sector_model::GeneratedFaction],
    overlay: ControlOverlay,
) -> Option<std::collections::HashMap<SystemId, sectorforge_gui_core::heatmap::HeatCell>> {
    use sectorforge_gui_core::heatmap::HeatCell;
    match overlay {
        ControlOverlay::None => None,
        ControlOverlay::PowerProjection => {
            let map = sectorforge::power_projection::project_sector(sector);
            let max = map
                .by_faction
                .values()
                .flat_map(|m| m.values().copied())
                .fold(0.0_f32, f32::max)
                .max(1.0);
            let mut out: std::collections::HashMap<SystemId, HeatCell> =
                std::collections::HashMap::new();
            for sys in &sector.systems {
                let best = sectorforge::power_projection::system_top_reach(&map, sys.id.as_str());
                if let Some((fid, v)) = best {
                    let style = sectorforge_gui_core::palette::faction_style_by_id(
                        factions,
                        fid.as_str(),
                    );
                    out.insert(
                        sys.id.clone(),
                        HeatCell {
                            color: style.fill,
                            intensity: (v / max).clamp(0.0, 1.0),
                        },
                    );
                }
            }
            Some(out)
        }
        ControlOverlay::InfluenceField => {
            let field = sectorforge::influence_field::build(sector);
            let width = field.width as i32;
            let mut out: std::collections::HashMap<SystemId, HeatCell> =
                std::collections::HashMap::new();
            for sys in &sector.systems {
                let q = sys.coord.q;
                let r = sys.coord.r;
                if q < 0 || r < 0 || q >= width || r >= field.height as i32 {
                    continue;
                }
                let idx = (r as usize) * (field.width as usize) + q as usize;
                let Some(cell) = field.cells.get(idx) else {
                    continue;
                };
                let Some(fid) = cell.dominant.as_ref() else {
                    continue;
                };
                let style = sectorforge_gui_core::palette::faction_style_by_id(
                    factions,
                    fid.as_str(),
                );
                out.insert(
                    sys.id.clone(),
                    HeatCell {
                        color: style.fill,
                        intensity: (cell.score as f32 / 100.0).clamp(0.0, 1.0),
                    },
                );
            }
            Some(out)
        }
    }
}

// silence unused-import lint when only the helpers above are reached
#[allow(dead_code)]
fn _power_profile_marker(_p: PowerProfile) {}
#[allow(dead_code)]
fn _btreemap_marker(_p: BTreeMap<FactionId, PowerProfile>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{GeneratedSector, HexCoord};

    fn faction_id(s: &str) -> FactionId {
        FactionId::from(s)
    }

    fn empty() -> GeneratedSector {
        GeneratedSector::empty("t", "T", "seed", 8, 8)
    }

    fn install_world_with_claims(s: &mut GeneratedSector, claims: Vec<FactionClaim>) {
        let sys_id = s.add_system(HexCoord { q: 0, r: 0 }, "Sys").unwrap();
        let _ = s.add_world_to_system(&sys_id, "World").unwrap();
        if let Some(sys) = s.systems.first_mut() {
            if let Some(w) = sys.worlds.first_mut() {
                w.claims = claims;
            }
        }
    }

    #[test]
    fn cl3_contested_when_distinct_claimants_gt_1() {
        let fa = faction_id("alpha");
        let fb = faction_id("beta");
        let mut s = empty();
        install_world_with_claims(
            &mut s,
            vec![
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 50,
                },
                FactionClaim {
                    faction_id: fb,
                    claim_type: ClaimType::Rebellion,
                    strength: 30,
                },
                FactionClaim {
                    faction_id: fa,
                    claim_type: ClaimType::MilitaryOccupation,
                    strength: 60,
                },
            ],
        );
        let mut contested = 0usize;
        for sys in &s.systems {
            for w in &sys.worlds {
                let distinct: BTreeSet<FactionId> =
                    w.claims.iter().map(|c| c.faction_id.clone()).collect();
                if distinct.len() > 1 {
                    contested += 1;
                }
            }
        }
        assert_eq!(contested, 1);
    }

    #[test]
    fn cl4_bulk_match_count_predicate() {
        let fa = faction_id("alpha");
        let mut s = empty();
        install_world_with_claims(
            &mut s,
            vec![
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 50,
                },
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::LegalSovereignty,
                    strength: 70,
                },
                FactionClaim {
                    faction_id: fa.clone(),
                    claim_type: ClaimType::Rebellion,
                    strength: 30,
                },
            ],
        );
        let mut matches = 0usize;
        for sys in &s.systems {
            for w in &sys.worlds {
                for c in &w.claims {
                    if c.faction_id == fa && c.claim_type == ClaimType::LegalSovereignty {
                        matches += 1;
                    }
                }
            }
        }
        assert_eq!(matches, 2);
    }

    // ── §C7 / §C8 overlay helpers ──────────────────────────────────────

    #[test]
    fn build_overlay_returns_none_for_off() {
        let s = empty();
        assert!(build_overlay_cells(&s, &s.factions, ControlOverlay::None).is_none());
    }

    #[test]
    fn build_overlay_power_projection_keys_systems_with_power() {
        use sectorforge::sector_model::{GeneratedFaction, PowerProfile};
        let mut s = empty();
        let a = s.add_system(HexCoord { q: 0, r: 0 }, "A").unwrap();
        let b = s.add_system(HexCoord { q: 1, r: 0 }, "B").unwrap();
        s.add_route(
            &a,
            &b,
            sectorforge::sector_model::RouteType::StableWarpLane,
            sectorforge::sector_model::RouteStability::Stable,
        )
        .unwrap();
        s.factions.push(GeneratedFaction {
            id: faction_id("alpha"),
            name: "Alpha".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![a.clone()],
            world_presence: vec![],
            power: PowerProfile {
                military: 50.0,
                ..Default::default()
            },
        });
        let cells = build_overlay_cells(&s, &s.factions, ControlOverlay::PowerProjection)
            .expect("PowerProjection always returns a map");
        assert!(cells.contains_key(&a));
        assert!(cells.contains_key(&b), "BFS reaches one-hop neighbour");
    }

    #[test]
    fn build_overlay_influence_field_handles_empty_sector() {
        let s = empty();
        let cells = build_overlay_cells(&s, &s.factions, ControlOverlay::InfluenceField)
            .expect("InfluenceField always returns a map");
        assert!(cells.is_empty(), "empty sector → empty influence map");
    }
}
