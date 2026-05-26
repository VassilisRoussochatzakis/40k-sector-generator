//! §CF1..§CF6 (BUILDER_REQS §28) — conflict + stability editors.
//!
//! Three call-sites:
//!   * [`show_world_conflict_section`] — under WORLD tab. §CF1 edits
//!     `GeneratedWorld::conflict` (momentum / intensity / mobilisation /
//!     attacker / defender / visible_controller) plus §CF3 stability
//!     (7 dimensions) plus §CF4 "Advance N ticks".
//!   * [`show_system_conflict_section`] — under SYSTEM tab. §CF2 shows the
//!     aggregated system conflict (read-only by default) with an
//!     "Override aggregate" toggle that switches to direct editing of
//!     `GeneratedSystem::conflict` via [`BuilderCommand::SetSystemConflict`].
//!     Also surfaces §CF4 Advance + §CF5 tick log.
//!
//! Every mutation routes through the command bus so undo/redo (R4) covers
//! conflict + stability touches alongside the rest of the builder.

use egui::{Color32, RichText, Ui};

use sectorforge::conflict::{
    derive_system_conflict, derive_world_conflict, ConflictState, HYSTERESIS_TICKS,
};
use sectorforge::ids::FactionId;
use sectorforge::stability::{derive_world_stability, StabilityState};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{ModalKind, TickLogScope};
use crate::builder::BuilderState;

// ── §CF1 + §CF3: per-world conflict + stability editor ─────────────────────

pub fn show_world_conflict_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    egui::CollapsingHeader::new("§CF1 / §CF3 — Conflict + stability (§28)")
        .default_open(false)
        .show(ui, |ui| {
            let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();

            // §CF1 — conflict editor.
            let mut working = state.sector.systems[sys_idx].worlds[w_idx].conflict.clone();
            let original = working.clone();
            ui.label(RichText::new("Conflict state").strong());
            conflict_editor(ui, &format!("w_conf_{world_id}"), &mut working, &factions);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Re-derive from control (§CF1)")
                    .on_hover_text(
                        "Calls sectorforge::conflict::derive_world_conflict for this world \
                         and replaces the conflict block with the seed-derived snapshot.",
                    )
                    .clicked()
                {
                    let w = &state.sector.systems[sys_idx].worlds[w_idx];
                    working = derive_world_conflict(w);
                }
                if ui.button("Clear conflict").clicked() {
                    working = ConflictState::default();
                }
            });

            if working != original {
                let cmd = BuilderCommand::SetWorldConflict {
                    world: world_id.clone(),
                    before: None,
                    after: working,
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!(
                        "World conflict update failed: {e}"
                    )));
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.label(RichText::new("Stability (7 dimensions)").strong());

            // §CF3 — stability editor.
            let mut stab = state.sector.systems[sys_idx].worlds[w_idx]
                .stability
                .clone();
            let stab_original = stab.clone();
            stability_editor(ui, &format!("w_stab_{world_id}"), &mut stab);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Re-derive stability (§CF3)")
                    .on_hover_text(
                        "Calls sectorforge::stability::derive_world_stability for this world \
                         using the live faction roster.",
                    )
                    .clicked()
                {
                    let factions_full = state.sector.factions.clone();
                    let w = &state.sector.systems[sys_idx].worlds[w_idx];
                    stab = derive_world_stability(w, &factions_full);
                }
                if ui.button("Clear stability").clicked() {
                    stab = StabilityState::default();
                }
            });

            if stab != stab_original {
                let cmd = BuilderCommand::SetWorldStability {
                    world: world_id,
                    before: None,
                    after: stab,
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!(
                        "World stability update failed: {e}"
                    )));
                }
            }

            ui.add_space(8.0);
            ui.separator();
            advance_ticks_block(ui, state, "world");
        });
}

// ── §CF2: per-system conflict view + override toggle ──────────────────────

pub fn show_system_conflict_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    egui::CollapsingHeader::new("§CF2 / §CF4 / §CF5 — Conflict (§28)")
        .default_open(false)
        .show(ui, |ui| {
            let sys_id = state.sector.systems[sys_idx].id.clone();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();
            let mut override_on = state.system_conflict_override.contains(&sys_id);

            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut override_on, "Override aggregate")
                    .on_hover_text(
                        "When off: conflict block re-derives from worlds via \
                         conflict::derive_system_conflict every frame. When on: edits go \
                         straight to GeneratedSystem::conflict via SetSystemConflict.",
                    )
                    .changed()
                {
                    if override_on {
                        state.system_conflict_override.insert(sys_id.clone());
                    } else {
                        state.system_conflict_override.remove(&sys_id);
                    }
                }
                ui.colored_label(
                    Color32::GRAY,
                    format!("hysteresis = {} ticks", HYSTERESIS_TICKS),
                );
            });

            if !override_on {
                // Aggregate-from-worlds view: keep `sys.conflict` in sync with
                // the world rollup so the read-only display matches what
                // `advance_sector` would write next tick.
                let sys = &state.sector.systems[sys_idx];
                let derived = derive_system_conflict(sys);
                if derived != sys.conflict {
                    let cmd = BuilderCommand::SetSystemConflict {
                        system: sys_id.clone(),
                        before: None,
                        after: derived,
                    };
                    let _ = state.run(cmd);
                }
                let sys = &state.sector.systems[sys_idx];
                show_conflict_readout(ui, &sys.conflict);
            } else {
                let mut working = state.sector.systems[sys_idx].conflict.clone();
                let original = working.clone();
                conflict_editor(ui, &format!("s_conf_{sys_id}"), &mut working, &factions);
                if ui.button("Clear conflict").clicked() {
                    working = ConflictState::default();
                }
                if working != original {
                    let cmd = BuilderCommand::SetSystemConflict {
                        system: sys_id.clone(),
                        before: None,
                        after: working,
                    };
                    if let Err(e) = state.run(cmd) {
                        state.modal = Some(ModalKind::Message(format!(
                            "System conflict update failed: {e}"
                        )));
                    }
                }
            }

            ui.add_space(8.0);
            ui.separator();
            advance_ticks_block(ui, state, "system");
            ui.add_space(8.0);
            ui.separator();
            show_tick_log(ui, state, Some(&sys_id));
            ui.add_space(8.0);
            ui.separator();
            show_conflict_heatmap_picker(ui, state);
        });
}

// ── §CF6: conflict-intensity heatmap toggle ───────────────────────────────

fn show_conflict_heatmap_picker(ui: &mut Ui, state: &mut BuilderState) {
    use sectorforge::heatmap::HeatmapMode;
    ui.label(RichText::new("§CF6 — MAP conflict heatmap").strong());
    let mut on = matches!(state.map_heatmap_mode, HeatmapMode::ConflictIntensity);
    ui.horizontal_wrapped(|ui| {
        if ui
            .checkbox(&mut on, "Show conflict intensity on MAP")
            .on_hover_text(
                "Wires state.map_heatmap_mode = HeatmapMode::ConflictIntensity so the MAP \
                 tab tints each system by its per-system conflict.intensity (0..=100). \
                 Overridden by §C7/§C8 when a control overlay is on.",
            )
            .changed()
        {
            state.map_heatmap_mode = if on {
                HeatmapMode::ConflictIntensity
            } else {
                HeatmapMode::Off
            };
        }
        if ui.button("→ MAP").clicked() {
            state.focus_entity(crate::builder::state::EntityRef::Tab(
                crate::builder::state::BuilderTab::Map,
            ));
        }
    });
}

// ── §CF4 advance ticks + §CF5 tick log shared widgets ─────────────────────

fn advance_ticks_block(ui: &mut Ui, state: &mut BuilderState, scope_id: &str) {
    ui.label(RichText::new("§CF4 — Advance ticks").strong());
    ui.horizontal(|ui| {
        ui.label("ticks:");
        ui.add(
            egui::DragValue::new(&mut state.conflict_ticks_to_advance)
                .range(1..=u32::MAX)
                .speed(1.0),
        );
        let label = format!("Advance N ticks ({scope_id})");
        if ui
            .button(label)
            .on_hover_text(
                "Calls sectorforge::conflict::advance_sector once per tick. Hysteresis \
                 (HYSTERESIS_TICKS) is preserved — the visible controller only flips after a \
                 control change has held that many ticks.",
            )
            .clicked()
        {
            let ticks = state.conflict_ticks_to_advance.max(1);
            if let Err(e) = state.advance_conflict_ticks(ticks) {
                state.modal = Some(ModalKind::Message(format!("Advance ticks failed: {e}")));
            }
        }
    });
}

pub fn show_tick_log(ui: &mut Ui, state: &mut BuilderState, filter_system: Option<&str>) {
    ui.label(RichText::new("§CF5 — Tick log").strong());
    if state.tick_log.is_empty() {
        ui.colored_label(Color32::GRAY, "(empty — run Advance N ticks above)");
        return;
    }
    ui.horizontal(|ui| {
        ui.label(format!("{} entries", state.tick_log.len()));
        if ui.small_button("× clear").clicked() {
            state.tick_log.clear();
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("cf5_tick_log_scroll")
        .max_height(180.0)
        .show(ui, |ui| {
            for entry in state.tick_log.iter().rev() {
                if let Some(sys_filter) = filter_system {
                    let in_scope = match &entry.scope {
                        TickLogScope::System(id) => id.as_str() == sys_filter,
                        TickLogScope::World { system, .. } => system.as_str() == sys_filter,
                    };
                    if !in_scope {
                        continue;
                    }
                }
                let scope = match &entry.scope {
                    TickLogScope::System(id) => format!("sys {id}"),
                    TickLogScope::World { system, world } => format!("{system} / {world}"),
                };
                let mom = format!("mom {}->{}", entry.momentum_before, entry.momentum_after);
                let inten = format!("int {}->{}", entry.intensity_before, entry.intensity_after);
                let mut bits = vec![scope, mom, inten];
                if entry.defender_before != entry.defender_after {
                    bits.push(format!(
                        "def {} -> {}",
                        opt_id(&entry.defender_before),
                        opt_id(&entry.defender_after),
                    ));
                }
                if entry.visible_before != entry.visible_after {
                    bits.push(format!(
                        "vis {} -> {}",
                        opt_id(&entry.visible_before),
                        opt_id(&entry.visible_after),
                    ));
                }
                ui.monospace(format!("t{:>4}  {}", entry.tick_index, bits.join("  ")));
            }
        });
}

fn opt_id(v: &Option<FactionId>) -> String {
    v.as_ref()
        .map(|f| f.to_string())
        .unwrap_or_else(|| "—".into())
}

// ── shared editors / readout ──────────────────────────────────────────────

fn conflict_editor(
    ui: &mut Ui,
    salt: &str,
    state: &mut ConflictState,
    factions: &[(FactionId, String)],
) {
    egui::Grid::new(format!("{salt}_grid"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("momentum");
            ui.add(egui::Slider::new(&mut state.momentum, -100..=100));
            ui.end_row();

            ui.label("intensity");
            ui.add(egui::Slider::new(&mut state.intensity, 0..=100).text("/100"));
            ui.end_row();

            ui.label("mobilisation");
            ui.add(egui::Slider::new(&mut state.mobilisation, 0..=100).text("/100"));
            ui.end_row();

            ui.label("attacker");
            optional_faction_combo(ui, &format!("{salt}_att"), &mut state.attacker, factions);
            ui.end_row();

            ui.label("defender");
            optional_faction_combo(ui, &format!("{salt}_def"), &mut state.defender, factions);
            ui.end_row();

            ui.label("visible_controller");
            optional_faction_combo(
                ui,
                &format!("{salt}_vis"),
                &mut state.visible_controller,
                factions,
            );
            ui.end_row();

            ui.label("started_tick");
            ui.add(egui::DragValue::new(&mut state.started_tick).range(0..=u32::MAX));
            ui.end_row();

            ui.label("last_change_tick");
            ui.add(egui::DragValue::new(&mut state.last_change_tick).range(0..=u32::MAX));
            ui.end_row();

            ui.label("age");
            ui.add(egui::DragValue::new(&mut state.age).range(0..=u32::MAX));
            ui.end_row();
        });
}

fn stability_editor(ui: &mut Ui, salt: &str, state: &mut StabilityState) {
    egui::Grid::new(format!("{salt}_grid"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("public_order");
            ui.add(egui::Slider::new(&mut state.public_order, 0.0..=100.0));
            ui.end_row();
            ui.label("corruption");
            ui.add(egui::Slider::new(&mut state.corruption, 0.0..=100.0));
            ui.end_row();
            ui.label("fear");
            ui.add(egui::Slider::new(&mut state.fear, 0.0..=100.0));
            ui.end_row();
            ui.label("rebellion_risk");
            ui.add(egui::Slider::new(&mut state.rebellion_risk, 0.0..=100.0));
            ui.end_row();
            ui.label("xenos_threat");
            ui.add(egui::Slider::new(&mut state.xenos_threat, 0.0..=100.0));
            ui.end_row();
            ui.label("warp_instability");
            ui.add(egui::Slider::new(&mut state.warp_instability, 0.0..=100.0));
            ui.end_row();
            ui.label("famine_or_resource_stress");
            ui.add(egui::Slider::new(
                &mut state.famine_or_resource_stress,
                0.0..=100.0,
            ));
            ui.end_row();
        });
}

fn show_conflict_readout(ui: &mut Ui, c: &ConflictState) {
    egui::Grid::new("conflict_readout_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("momentum");
            ui.monospace(c.momentum.to_string());
            ui.end_row();
            ui.label("intensity");
            ui.monospace(format!("{}/100", c.intensity));
            ui.end_row();
            ui.label("mobilisation");
            ui.monospace(format!("{}/100", c.mobilisation));
            ui.end_row();
            ui.label("attacker");
            ui.monospace(opt_id(&c.attacker));
            ui.end_row();
            ui.label("defender");
            ui.monospace(opt_id(&c.defender));
            ui.end_row();
            ui.label("visible_controller");
            ui.monospace(opt_id(&c.visible_controller));
            ui.end_row();
            ui.label("age");
            ui.monospace(c.age.to_string());
            ui.end_row();
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
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(label)
        .show_ui(ui, |ui| {
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
