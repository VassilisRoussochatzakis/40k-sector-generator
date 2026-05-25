//! SYSTEM tab (§N1 / §N2) — Phase B §S2..§S6 inspector.
//!
//! Covers every `GeneratedSystem` field via a per-section inspector, the
//! §S3 pinned toggle (driven by [`BuilderState::pinned_systems`]), the §S4
//! bulk-ops block over [`BuilderState::selected_systems`], the §S5
//! single-system regenerate (`sectorforge::generate_system_standalone`), and
//! the §S6 coord-validity check on inline coord edits. Fields managed by
//! sibling panels (worlds §8, primary factions §10, control §11, orbital
//! assets §31, conflict §28, intel §29, archetype §30) are shown read-only
//! with deep-link buttons.

use std::collections::BTreeSet;
use std::sync::Arc;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{HexCoord, SystemKind, SystemState};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, ModalKind};
use crate::builder::BuilderState;

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("System");
    ui.add_space(4.0);

    let count = state.sector.systems.len();
    if count == 0 {
        ui.colored_label(
            Color32::GRAY,
            "No systems in this sector — use the MAP tab's ADD SYSTEM tool.",
        );
        return;
    }

    show_system_picker(ui, state);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let selected = state.selected_system_id.clone();
            let Some(sys_id) = selected else {
                ui.colored_label(
                    Color32::GRAY,
                    "Select a system from the picker or the MAP tab.",
                );
                show_bulk_ops(ui, state);
                return;
            };

            let Some(sys_idx) = state.sector.systems.iter().position(|s| s.id == sys_id) else {
                state.selected_system_id = None;
                return;
            };

            show_header(ui, state, sys_idx);
            ui.separator();
            show_identity_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_star_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_tags_notes_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_worlds_link(ui, state, sys_idx);
            ui.add_space(4.0);
            show_routes_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_factions_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_control_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_overlays_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_archetype_section(ui, state, sys_idx);
            ui.add_space(4.0);
            show_archetype_auto_assign(ui, state);
            ui.add_space(4.0);
            show_archetype_rules(ui, state);
            ui.add_space(4.0);
            crate::builder::panels::orbital::show_orbital_section(ui, state, sys_idx);
            ui.add_space(4.0);
            crate::builder::panels::intel::show_system_intel_section(ui, state, sys_idx);
            ui.add_space(8.0);
            show_regen_section(ui, state, sys_idx);
            ui.add_space(8.0);
            show_bulk_ops(ui, state);
        });
}

// ── picker / header ─────────────────────────────────────────────────────────

fn show_system_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("system:");
        let current = state.selected_system_id.clone();
        let label = current
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("system_picker")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for sys in &state.sector.systems {
                    let sel = current.as_ref() == Some(&sys.id);
                    let label = format!("{} — {}", sys.id, sys.name);
                    if ui.selectable_label(sel, label).clicked() {
                        state.selected_system_id = Some(sys.id.clone());
                        state.selected_systems.clear();
                        state.selected_systems.insert(sys.id.clone());
                    }
                }
            });
    });
}

fn show_header(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    let sys = &state.sector.systems[sys_idx];
    let id = sys.id.clone();
    let pinned = state.pinned_systems.contains(&id);
    ui.horizontal_wrapped(|ui| {
        ui.heading(sys.name.to_string());
        ui.label(
            RichText::new(sys.id.to_string())
                .color(Color32::GRAY)
                .monospace(),
        );
        if pinned {
            ui.colored_label(Color32::from_rgb(255, 160, 100), "PINNED");
        }
    });
}

// ── identity (S2 + S6) ──────────────────────────────────────────────────────

fn show_identity_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Identity")
        .default_open(true)
        .show(ui, |ui| {
            let sys = &state.sector.systems[sys_idx];
            let id = sys.id.clone();
            let coord = sys.coord;
            let kind = sys.kind;
            let mut name_buf = sys.name.to_string();
            let mut q = coord.q;
            let mut r = coord.r;
            let mut kind_choice = kind;

            let mut name_changed = false;
            egui::Grid::new("sys_identity_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("id");
                    ui.monospace(id.to_string());
                    ui.end_row();
                    ui.label("name");
                    name_changed = ui.text_edit_singleline(&mut name_buf).lost_focus();
                    ui.end_row();
                    ui.label("coord");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut q)
                                .range(0..=state.sector.width as i32 - 1)
                                .prefix("q "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut r)
                                .range(0..=state.sector.height as i32 - 1)
                                .prefix("r "),
                        );
                    });
                    ui.end_row();
                    ui.label("kind");
                    egui::ComboBox::from_id_salt("sys_kind")
                        .selected_text(format!("{:?}", kind_choice))
                        .show_ui(ui, |ui| {
                            for k in [
                                SystemKind::Star,
                                SystemKind::SpecialLocation,
                                SystemKind::BlackHole,
                                SystemKind::WarpAnomaly,
                                SystemKind::SpaceStation,
                            ] {
                                ui.selectable_value(&mut kind_choice, k, format!("{:?}", k));
                            }
                        });
                    ui.end_row();
                    ui.label("pinned");
                    let mut pinned = state.pinned_systems.contains(&id);
                    if ui
                        .checkbox(&mut pinned, "(§S3 pin from generator)")
                        .changed()
                    {
                        if pinned {
                            state.pinned_systems.insert(id.clone());
                        } else {
                            state.pinned_systems.remove(&id);
                        }
                    }
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if (ui.button("Apply name").clicked() || name_changed)
                    && name_buf != *state.sector.systems[sys_idx].name
                {
                    let from = state.sector.systems[sys_idx].name.to_string();
                    let cmd = BuilderCommand::RenameSystem {
                        id: id.clone(),
                        from,
                        to: name_buf.clone(),
                    };
                    if let Err(e) = state.run(cmd) {
                        state.modal = Some(ModalKind::Message(format!("Rename failed: {e}")));
                    }
                }
                if ui.button("Apply coord").clicked() {
                    let new_coord = HexCoord { q, r };
                    if new_coord != coord {
                        apply_coord_move(state, id.clone(), coord, new_coord);
                    }
                }
                if kind_choice != kind && ui.button("Apply kind").clicked() {
                    state.sector.systems[sys_idx].kind = kind_choice;
                    state.dirty = true;
                    state.mark_validation_dirty();
                }
            });
        });
}

fn apply_coord_move(state: &mut BuilderState, id: SystemId, from: HexCoord, to: HexCoord) {
    if to.q < 0
        || to.r < 0
        || (to.q as u32) >= state.sector.width
        || (to.r as u32) >= state.sector.height
    {
        state.modal = Some(ModalKind::Message(format!(
            "Coord ({},{}) out of bounds {}x{}.",
            to.q, to.r, state.sector.width, state.sector.height
        )));
        return;
    }
    let occupant = state
        .sector
        .systems
        .iter()
        .find(|s| s.coord == to && s.id != id)
        .map(|s| s.id.clone());
    if let Some(occupant) = occupant {
        state.pending_collision = Some(crate::builder::state::PendingCollision {
            dragging: id,
            target: to,
            occupant,
        });
        return;
    }
    let cmd = BuilderCommand::MoveSystem { id, from, to };
    if let Err(e) = state.run(cmd) {
        state.modal = Some(ModalKind::Message(format!("Move failed: {e}")));
    }
}

// ── star ────────────────────────────────────────────────────────────────────

fn show_star_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Star")
        .default_open(false)
        .show(ui, |ui| {
            let mut has_star = state.sector.systems[sys_idx].star.is_some();
            let mut toggle_star = false;
            if ui.checkbox(&mut has_star, "present").changed() {
                toggle_star = true;
            }
            let mut star_buf = state.sector.systems[sys_idx].star.clone();
            let mut field_changed = false;
            if let Some(star) = star_buf.as_mut() {
                let mut code = star.colour_code.to_string();
                let mut name = star.colour_name.to_string();
                let mut spectral = star
                    .spectral_type
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                egui::Grid::new("sys_star_grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("colour_code");
                        field_changed |= ui.text_edit_singleline(&mut code).lost_focus();
                        ui.end_row();
                        ui.label("colour_name");
                        field_changed |= ui.text_edit_singleline(&mut name).lost_focus();
                        ui.end_row();
                        ui.label("spectral_type");
                        field_changed |= ui.text_edit_singleline(&mut spectral).lost_focus();
                        ui.end_row();
                    });
                star.colour_code = Arc::from(code.as_str());
                star.colour_name = Arc::from(name.as_str());
                star.spectral_type = if spectral.trim().is_empty() {
                    None
                } else {
                    Some(Arc::from(spectral.as_str()))
                };
            }

            let sys = &mut state.sector.systems[sys_idx];
            if toggle_star {
                if has_star && sys.star.is_none() {
                    sys.star = Some(sectorforge::sector_model::GeneratedStar {
                        colour_code: Arc::from("G"),
                        colour_name: Arc::from("Yellow"),
                        spectral_type: None,
                        source_row_index: None,
                    });
                } else if !has_star {
                    sys.star = None;
                }
            } else if field_changed {
                sys.star = star_buf;
            }
            if toggle_star || field_changed {
                state.dirty = true;
                state.mark_validation_dirty();
            }
        });
}

// ── tags + notes ────────────────────────────────────────────────────────────

fn show_tags_notes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Tags + Notes")
        .default_open(false)
        .show(ui, |ui| {
            let mut tags_buf = state.sector.systems[sys_idx]
                .tags
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut notes_buf = state.sector.systems[sys_idx]
                .notes
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            ui.label("tags (comma-separated)");
            let tags_changed = ui.text_edit_singleline(&mut tags_buf).lost_focus();
            ui.label("notes (one per line)");
            let notes_changed = ui.text_edit_multiline(&mut notes_buf).lost_focus();
            if tags_changed {
                state.sector.systems[sys_idx].tags = tags_buf
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
                state.dirty = true;
                state.mark_validation_dirty();
            }
            if notes_changed {
                state.sector.systems[sys_idx].notes = notes_buf
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
                state.dirty = true;
                state.mark_validation_dirty();
            }
        });
}

// ── deep-links ──────────────────────────────────────────────────────────────

fn show_worlds_link(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Worlds (§8)")
        .default_open(false)
        .show(ui, |ui| {
            let sys = &state.sector.systems[sys_idx];
            ui.label(format!("{} world(s)", sys.worlds.len()));
            let world_ids: Vec<_> = sys
                .worlds
                .iter()
                .map(|w| (w.id.clone(), w.name.to_string()))
                .collect();
            for (wid, name) in world_ids {
                if ui.link(format!("→ {wid} {name}")).clicked() {
                    state.selected_world_id = Some(wid);
                    state.active_tab = BuilderTab::World;
                }
            }
        });
}

fn show_routes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Routes (§9 — read-only here)")
        .default_open(false)
        .show(ui, |ui| {
            let id = state.sector.systems[sys_idx].id.clone();
            let touching: Vec<_> = state
                .sector
                .routes
                .iter()
                .filter(|r| r.from_system_id == id || r.to_system_id == id)
                .map(|r| {
                    (
                        r.id.clone(),
                        r.from_system_id.clone(),
                        r.to_system_id.clone(),
                        r.distance,
                    )
                })
                .collect();
            ui.label(format!("{} route(s) touching", touching.len()));
            for (rid, from, to, dist) in touching {
                if ui
                    .link(format!("→ {rid}  {from} → {to}  d={dist}"))
                    .clicked()
                {
                    state.selected_route_id = Some(rid);
                    state.active_tab = BuilderTab::Routes;
                }
            }
        });
}

fn show_factions_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Primary factions (§10)")
        .default_open(false)
        .show(ui, |ui| {
            let primary: Vec<_> = state.sector.systems[sys_idx].primary_factions.to_vec();
            for fid in &primary {
                if ui.link(format!("→ {fid}")).clicked() {
                    state.selected_faction_id = Some(fid.clone());
                    state.active_tab = BuilderTab::Factions;
                }
            }
            if primary.is_empty() {
                ui.colored_label(Color32::GRAY, "no primary factions");
            }
        });
}

fn show_control_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Control (§11)")
        .default_open(false)
        .show(ui, |ui| {
            let id = state.sector.systems[sys_idx].id.clone();
            let mut current = state.sector.systems[sys_idx].control.state;
            let summary = state.sector.systems[sys_idx].control.clone();
            ui.label("control.state");
            egui::ComboBox::from_id_salt("sys_control_state")
                .selected_text(match current {
                    None => "(none)".into(),
                    Some(s) => format!("{s:?}"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut current, None, "(none)");
                    for s in [
                        SystemState::Pacified,
                        SystemState::Fragmented,
                        SystemState::Blockaded,
                        SystemState::Warzone,
                        SystemState::Infiltrated,
                        SystemState::Quarantined,
                        SystemState::Uncharted,
                    ] {
                        ui.selectable_value(&mut current, Some(s), format!("{s:?}"));
                    }
                });
            if current != state.sector.systems[sys_idx].control.state {
                if let Err(e) = state.sector.set_system_control_state(&id, current) {
                    state.modal = Some(ModalKind::Message(format!("Control update failed: {e}")));
                } else {
                    state.dirty = true;
                    state.mark_validation_dirty();
                }
            }
            ui.label(format!("dominant: {:?}", summary.dominant));
            ui.label(format!("sovereign: {:?}", summary.sovereign));
            ui.label(format!(
                "orbital_controller: {:?}",
                summary.orbital_controller
            ));
            ui.label(format!("economic_hegemon: {:?}", summary.economic_hegemon));
            ui.label(format!("hidden_master: {:?}", summary.hidden_master));
        });
}

fn show_overlays_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("Overlays (§28..§32 — managed elsewhere)")
        .default_open(false)
        .show(ui, |ui| {
            let sys = &state.sector.systems[sys_idx];
            ui.label(format!(
                "orbital_assets: {} (edit below in §O1)",
                sys.orbital_assets.len()
            ));
            ui.label(format!(
                "blockade present: {} (edit below in §O2)",
                !sectorforge::orbital_assets::BlockadeReport::is_default(&sys.blockade)
            ));
            ui.label(format!(
                "conflict default: {}",
                sectorforge::conflict::ConflictState::is_default(&sys.conflict)
            ));
            ui.label(format!(
                "intel observers: {} (empty? {})",
                sys.intel.by_observer.len(),
                sectorforge::intel::SystemIntel::is_empty(&sys.intel)
            ));
            ui.label(format!(
                "archetype default: {} (see §AR1 / §30 — Archetypes section)",
                sectorforge::archetypes::ArchetypeState::is_default(&sys.archetype)
            ));
            ui.horizontal(|ui| {
                if ui.button("Open REGIONS").clicked() {
                    state.active_tab = BuilderTab::Regions;
                }
            });
        });
}

// ── AR1 / AR2 / AR3 — Archetypes (§30) ─────────────────────────────────────

fn show_archetype_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    use sectorforge::archetypes::{
        ArchetypeState, GscStage, NecronPhase, TauSphereBand, TyranidStage,
    };

    let sys_id = state.sector.systems[sys_idx].id.clone();
    let mut working = state.sector.systems[sys_idx].archetype.clone();
    let original = working.clone();

    egui::CollapsingHeader::new("§AR1 — Archetypes (§30)")
        .default_open(false)
        .show(ui, |ui| {
            ui.colored_label(
                Color32::GRAY,
                "per-axis progression markers. flavour notes live in the Tags / Notes section.",
            );
            ui.add_space(4.0);

            egui::Grid::new("archetype_axes")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("imperial co-sovereigns");
                    ui.vertical(|ui| {
                        let mut remove_at: Option<usize> = None;
                        for (i, fid) in working.imperial_co_sovereigns.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.monospace(fid.to_string());
                                if ui.small_button("×").clicked() {
                                    remove_at = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_at {
                            working.imperial_co_sovereigns.remove(i);
                        }
                        ui.horizontal(|ui| {
                            let mut to_add: Option<sectorforge::ids::FactionId> = None;
                            egui::ComboBox::from_id_salt("arch_imp_add")
                                .selected_text("+ add")
                                .show_ui(ui, |ui| {
                                    for f in &state.sector.factions {
                                        if working.imperial_co_sovereigns.contains(&f.id) {
                                            continue;
                                        }
                                        if ui.button(format!("{} ({})", f.id, f.name)).clicked() {
                                            to_add = Some(f.id.clone());
                                        }
                                    }
                                });
                            if let Some(fid) = to_add {
                                working.imperial_co_sovereigns.push(fid);
                            }
                        });
                    });
                    ui.end_row();

                    ui.label("necron phase");
                    egui::ComboBox::from_id_salt("arch_necron")
                        .selected_text(format!("{:?}", working.necron_phase))
                        .show_ui(ui, |ui| {
                            for v in [
                                NecronPhase::None,
                                NecronPhase::Dormant,
                                NecronPhase::Awakening,
                                NecronPhase::Awake,
                            ] {
                                ui.selectable_value(&mut working.necron_phase, v, format!("{v:?}"));
                            }
                        });
                    ui.end_row();

                    ui.label("tyranid stage");
                    egui::ComboBox::from_id_salt("arch_tyranid")
                        .selected_text(format!("{:?}", working.tyranid_stage))
                        .show_ui(ui, |ui| {
                            for v in [
                                TyranidStage::None,
                                TyranidStage::Inhabited,
                                TyranidStage::Besieged,
                                TyranidStage::Consumed,
                            ] {
                                ui.selectable_value(
                                    &mut working.tyranid_stage,
                                    v,
                                    format!("{v:?}"),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("ork waaagh!");
                    ui.add(egui::Slider::new(&mut working.ork_waaagh, 0..=100).text("/100"));
                    ui.end_row();

                    ui.label("genestealer stage");
                    egui::ComboBox::from_id_salt("arch_gsc")
                        .selected_text(format!("{:?}", working.gsc_stage))
                        .show_ui(ui, |ui| {
                            for v in [
                                GscStage::None,
                                GscStage::Rumor,
                                GscStage::HiddenCell,
                                GscStage::DistrictControl,
                                GscStage::ParallelGovernment,
                                GscStage::Uprising,
                                GscStage::PlanetarySeizure,
                            ] {
                                ui.selectable_value(&mut working.gsc_stage, v, format!("{v:?}"));
                            }
                        });
                    ui.end_row();

                    ui.label("tau sphere");
                    egui::ComboBox::from_id_salt("arch_tau")
                        .selected_text(format!("{:?}", working.tau_sphere))
                        .show_ui(ui, |ui| {
                            for v in [
                                TauSphereBand::None,
                                TauSphereBand::Contact,
                                TauSphereBand::Fringe,
                                TauSphereBand::Client,
                                TauSphereBand::Core,
                            ] {
                                ui.selectable_value(&mut working.tau_sphere, v, format!("{v:?}"));
                            }
                        });
                    ui.end_row();

                    ui.label("aeldari activity");
                    ui.add(egui::Slider::new(&mut working.aeldari_activity, 0..=100).text("/100"));
                    ui.end_row();

                    ui.label("chaos corruption");
                    ui.add(egui::Slider::new(&mut working.chaos_corruption, 0..=100).text("/100"));
                    ui.end_row();

                    ui.label("daemon manifestation");
                    ui.add(
                        egui::Slider::new(&mut working.daemon_manifestation, 0..=100).text("/100"),
                    );
                    ui.end_row();
                });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Reset to default").clicked() {
                    working = ArchetypeState::default();
                }
                if ui
                    .button("Auto-assign from sector data (this system only)")
                    .on_hover_text(
                        "Runs the §AR2 derivation over the full sector and keeps only \
                         this system's freshly derived archetype.",
                    )
                    .clicked()
                {
                    let mut scratch = state.sector.clone();
                    sectorforge::archetypes::apply_all(&mut scratch);
                    if let Some(s) = scratch.systems.iter().find(|s| s.id == sys_id) {
                        working = s.archetype.clone();
                        state.archetype_flags.mask(&mut working);
                    }
                }
            });
        });

    if working != original {
        let cmd = BuilderCommand::SetArchetype {
            system: sys_id,
            before: None,
            after: working,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Archetype update failed: {e}")));
        }
    }
}

fn show_archetype_auto_assign(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§AR2 — Auto-assign archetypes (sector-wide)")
        .default_open(false)
        .show(ui, |ui| {
            ui.colored_label(
                Color32::GRAY,
                "runs `sectorforge::archetypes::apply_all` over the whole sector, \
                 masked by the §AR3 enable flags below. Undoable.",
            );
            if ui.button("Run apply_all now").clicked() {
                let flags = state.archetype_flags;
                let cmd = BuilderCommand::AutoAssignArchetypes {
                    flags,
                    before: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!("Auto-assign failed: {e}")));
                }
            }
        });
}

fn show_archetype_rules(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§AR3 — Archetype rules (builder-only defaults)")
        .default_open(false)
        .show(ui, |ui| {
            ui.colored_label(
                Color32::GRAY,
                "`src/archetypes.rs` ships no TOML config layer, so these flags \
                 live on `BuilderState` only and are not serialised into \
                 `sector.json`. Disabled axes are reset to defaults after §AR2.",
            );
            let flags = &mut state.archetype_flags;
            ui.checkbox(&mut flags.imperial, "imperial governance stack (§16.1)");
            ui.checkbox(&mut flags.necron, "necron phase (§16.9)");
            ui.checkbox(&mut flags.tyranid, "tyranid front (§16.8)");
            ui.checkbox(&mut flags.ork, "ork waaagh! (§16.7)");
            ui.checkbox(&mut flags.gsc, "genestealer stages (§16.6)");
            ui.checkbox(&mut flags.tau, "tau sphere (§16.11)");
            ui.checkbox(&mut flags.aeldari, "aeldari intermittent (§16.10)");
            ui.checkbox(&mut flags.chaos, "chaos corruption + daemon (§16.12)");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Enable all").clicked() {
                    *flags = crate::builder::command::ArchetypeApplyFlags::default();
                }
                if ui.button("Disable all").clicked() {
                    *flags = crate::builder::command::ArchetypeApplyFlags {
                        imperial: false,
                        necron: false,
                        tyranid: false,
                        ork: false,
                        gsc: false,
                        tau: false,
                        aeldari: false,
                        chaos: false,
                    };
                }
            });
        });
}

// ── S5 regen ────────────────────────────────────────────────────────────────

fn show_regen_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("§S5 — Generate one system here")
        .default_open(false)
        .show(ui, |ui| {
            let sys = &state.sector.systems[sys_idx];
            let mut q = sys.coord.q;
            let mut r = sys.coord.r;
            let mut index = sys.index;
            let mut seed = state.config.generation.seed.clone();
            let id = sys.id.clone();

            egui::Grid::new("sys_regen_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("coord");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut q)
                                .range(0..=state.sector.width as i32 - 1)
                                .prefix("q "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut r)
                                .range(0..=state.sector.height as i32 - 1)
                                .prefix("r "),
                        );
                    });
                    ui.end_row();
                    ui.label("index");
                    ui.add(egui::DragValue::new(&mut index).range(1..=usize::MAX));
                    ui.end_row();
                    ui.label("seed");
                    ui.text_edit_singleline(&mut seed);
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if ui.button("Regenerate this system").clicked() {
                    run_regen(state, HexCoord { q, r }, index, &seed);
                }
                if ui.button("Regenerate at coord (replace)").clicked() {
                    run_regen(state, HexCoord { q, r }, index, &seed);
                }
            });
            ui.colored_label(
                Color32::GRAY,
                format!("(current id: {id} — pinned systems refuse regen)"),
            );
        });
}

fn run_regen(state: &mut BuilderState, coord: HexCoord, index: usize, seed: &str) {
    let seed_override = if seed == state.config.generation.seed {
        None
    } else {
        Some(seed)
    };
    match state.generate_system_here(coord, index, seed_override) {
        Ok(id) => {
            state.focus_system(id);
        }
        Err(e) => {
            state.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
        }
    }
}

// ── S4 bulk ops ─────────────────────────────────────────────────────────────

fn show_bulk_ops(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new("§S4 — Bulk operations")
        .default_open(false)
        .show(ui, |ui| {
            let n = state.selected_systems.len();
            ui.label(format!("{n} system(s) selected"));
            if n == 0 {
                ui.colored_label(
                    Color32::GRAY,
                    "Shift-click systems or drag a rect on the MAP tab.",
                );
                return;
            }

            ui.horizontal(|ui| {
                if ui.button("Clear selection").clicked() {
                    state.selected_systems.clear();
                }
                if ui.button("Pin all").clicked() {
                    for id in state.selected_systems.iter().cloned().collect::<Vec<_>>() {
                        state.pinned_systems.insert(id);
                    }
                }
                if ui.button("Unpin all").clicked() {
                    for id in state.selected_systems.iter().cloned().collect::<Vec<_>>() {
                        state.pinned_systems.remove(&id);
                    }
                }
            });

            ui.separator();
            ui.label(
                "Rename pattern — `{n}` = sequence, `{id}` = system id, `{name}` = current name",
            );
            let pattern = ui.data_mut(|d| {
                d.get_temp_mut_or::<String>(egui::Id::new("bulk_rename_pat"), "Sys-{n}".into())
                    .clone()
            });
            let mut pattern_buf = pattern;
            if ui.text_edit_singleline(&mut pattern_buf).changed() {
                ui.data_mut(|d| {
                    d.insert_temp(egui::Id::new("bulk_rename_pat"), pattern_buf.clone());
                });
            }
            if ui.button("Apply rename pattern").clicked() {
                apply_bulk_rename(state, &pattern_buf);
            }

            ui.separator();
            ui.label("Reassign primary faction");
            let factions: Vec<_> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            ui.horizontal_wrapped(|ui| {
                for (fid, name) in &factions {
                    if ui.button(format!("→ {name} ({fid})")).clicked() {
                        apply_bulk_primary_faction(state, fid.clone());
                    }
                }
            });
            if ui.button("Clear primary factions").clicked() {
                apply_bulk_clear_factions(state);
            }

            ui.separator();
            ui.label("Flip control state");
            ui.horizontal_wrapped(|ui| {
                for s in [
                    None,
                    Some(SystemState::Pacified),
                    Some(SystemState::Fragmented),
                    Some(SystemState::Blockaded),
                    Some(SystemState::Warzone),
                    Some(SystemState::Infiltrated),
                    Some(SystemState::Quarantined),
                    Some(SystemState::Uncharted),
                ] {
                    let label = match s {
                        None => "(none)".to_string(),
                        Some(v) => format!("{v:?}"),
                    };
                    if ui.button(label).clicked() {
                        apply_bulk_control_state(state, s);
                    }
                }
            });

            ui.separator();
            ui.label("Reseed worlds (drops + re-runs §S5)");
            if ui.button("Reseed worlds for selection").clicked() {
                apply_bulk_reseed(state);
            }
        });
}

fn apply_bulk_rename(state: &mut BuilderState, pattern: &str) {
    let selection: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for (n, id) in selection.into_iter().enumerate() {
        let from = match state.sector.systems.iter().find(|s| s.id == id) {
            Some(s) => s.name.to_string(),
            None => continue,
        };
        let to = pattern
            .replace("{n}", &(n + 1).to_string())
            .replace("{id}", id.as_ref())
            .replace("{name}", &from);
        if to == from {
            continue;
        }
        let cmd = BuilderCommand::RenameSystem {
            id: id.clone(),
            from,
            to,
        };
        if let Err(e) = state.run(cmd) {
            state.modal = Some(ModalKind::Message(format!("Bulk rename failed: {e}")));
            return;
        }
    }
}

fn apply_bulk_primary_faction(state: &mut BuilderState, fid: sectorforge::ids::FactionId) {
    let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for id in ids {
        if let Some(sys) = state.sector.systems.iter_mut().find(|s| s.id == id) {
            if !sys.primary_factions.contains(&fid) {
                sys.primary_factions.push(fid.clone());
            }
        }
    }
    state.dirty = true;
    state.mark_validation_dirty();
}

fn apply_bulk_clear_factions(state: &mut BuilderState) {
    let ids: BTreeSet<SystemId> = state.selected_systems.clone();
    for sys in &mut state.sector.systems {
        if ids.contains(&sys.id) {
            sys.primary_factions.clear();
        }
    }
    state.dirty = true;
    state.mark_validation_dirty();
}

fn apply_bulk_control_state(state: &mut BuilderState, value: Option<SystemState>) {
    let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
    for id in ids {
        if let Err(e) = state.sector.set_system_control_state(&id, value) {
            state.modal = Some(ModalKind::Message(format!("Control flip failed: {e}")));
            return;
        }
    }
    state.dirty = true;
    state.mark_validation_dirty();
}

fn apply_bulk_reseed(state: &mut BuilderState) {
    let targets: Vec<(SystemId, HexCoord, usize)> = state
        .selected_systems
        .iter()
        .filter_map(|id| {
            let sys = state.sector.systems.iter().find(|s| s.id == *id)?;
            if state.pinned_systems.contains(id) {
                return None;
            }
            Some((id.clone(), sys.coord, sys.index))
        })
        .collect();
    for (_id, coord, index) in targets {
        if let Err(e) = state.generate_system_here(coord, index, None) {
            state.modal = Some(ModalKind::Message(format!("Reseed failed: {e}")));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    #[test]
    fn bulk_rename_applies_pattern() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 0 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        apply_bulk_rename(&mut state, "Bulk-{n}");
        let names: Vec<_> = state
            .sector
            .systems
            .iter()
            .map(|s| s.name.to_string())
            .collect();
        assert!(names.contains(&"Bulk-1".to_string()));
        assert!(names.contains(&"Bulk-2".to_string()));
    }

    #[test]
    fn bulk_control_state_flips_selection() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.selected_systems.insert(a.clone());
        apply_bulk_control_state(&mut state, Some(SystemState::Warzone));
        assert_eq!(
            state.sector.systems[0].control.state,
            Some(SystemState::Warzone)
        );
    }

    #[test]
    fn bulk_pin_unpin_round_trip() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.pinned_systems.insert(a.clone());
        assert!(state.pinned_systems.contains(&a));
        state.pinned_systems.remove(&a);
        assert!(!state.pinned_systems.contains(&a));
    }

    #[test]
    fn apply_coord_move_rejects_out_of_bounds() {
        let mut state = blank();
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        apply_coord_move(
            &mut state,
            a.clone(),
            HexCoord { q: 1, r: 1 },
            HexCoord { q: 99, r: 99 },
        );
        assert!(matches!(state.modal, Some(ModalKind::Message(_))));
    }
}
