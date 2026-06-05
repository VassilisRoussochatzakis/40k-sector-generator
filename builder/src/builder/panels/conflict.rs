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
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::command::BuilderCommand;
use crate::builder::state::{ModalKind, TickLogScope};
use crate::builder::BuilderState;

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Momentum", "Public order") while the tooltip
/// names the underlying field plus a plain-language note, so power users keep
/// the schema mapping. Friendlier replacement for the old bare `egui::Grid`
/// whose row labels *were* the raw schema names.
fn labeled(ui: &mut Ui, label: &str, help: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [140.0, h],
            egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
        )
        .on_hover_text(help);
        add(ui);
    });
}

// ── §CF1 + §CF3: per-world conflict + stability editor ─────────────────────

pub(crate) fn show_world_conflict_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(
        ui,
        "cf_world_conflict",
        "Conflict & stability",
        false,
        |ui| {
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
            ui.label(RichText::new("Who is fighting here").strong());
            conflict_editor(ui, &format!("w_conf_{world_id}"), &mut working, &factions);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("↺  Re-derive from control")
                    .on_hover_text(
                        "Replace the conflict block with a fresh snapshot calculated from \
                         who currently controls this world.",
                    )
                    .clicked()
                {
                    let w = &state.sector.systems[sys_idx].worlds[w_idx];
                    working = derive_world_conflict(w);
                }
                if ui
                    .button("🗑  Clear conflict")
                    .on_hover_text("Reset every conflict field back to peaceful defaults.")
                    .clicked()
                {
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
            ui.label(RichText::new("Stability").strong());
            ui.label(
                RichText::new("Seven pressures on this world, each 0–100.")
                    .small()
                    .color(Color32::DARK_GRAY),
            );

            // §CF3 — stability editor.
            let mut stab = state.sector.systems[sys_idx].worlds[w_idx].stability;
            let stab_original = stab;
            stability_editor(ui, &format!("w_stab_{world_id}"), &mut stab);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("↺  Re-derive stability")
                    .on_hover_text(
                        "Recalculate all seven dimensions from this world's profile and the \
                         live faction roster.",
                    )
                    .clicked()
                {
                    let w = &state.sector.systems[sys_idx].worlds[w_idx];
                    stab = derive_world_stability(w, &state.sector.factions);
                }
                if ui
                    .button("🗑  Clear stability")
                    .on_hover_text("Reset every stability dimension back to defaults.")
                    .clicked()
                {
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
        },
    );
}

// ── §CF2: per-system conflict view + override toggle ──────────────────────

pub(crate) fn show_system_conflict_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    ui_kit::collapsing_section(ui, "cf_system_conflict", "Conflict", false, |ui| {
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
                .checkbox(&mut override_on, "Edit system conflict directly")
                .on_hover_text(
                    "Off: the system's conflict is rolled up automatically from its worlds. \
                     On: you set the system's conflict by hand, ignoring the rollup.",
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
                Color32::DARK_GRAY,
                format!("controller flips after {HYSTERESIS_TICKS} steady ticks"),
            )
            .on_hover_text(
                "Hysteresis: the public controller only changes once a takeover has held for \
                 this many ticks (schema: HYSTERESIS_TICKS).",
            );
        });

        if !override_on {
            // Aggregate-from-worlds view: keep `sys.conflict` in sync with
            // the world rollup so the read-only display matches what
            // `advance_sector` would write next tick. Only dispatch when
            // the rollup actually drifts from `sys.conflict`.
            let sys = &state.sector.systems[sys_idx];
            let derived = derive_system_conflict(sys);
            if derived != sys.conflict {
                let cmd = BuilderCommand::SetSystemConflict {
                    system: sys_id.clone(),
                    before: None,
                    after: derived,
                };
                if let Err(e) = state.run(cmd) {
                    state.last_command_error = Some(format!("aggregate sync: {e}"));
                }
            }
            ui.label(
                RichText::new("Rolled up from this system's worlds (read-only).")
                    .small()
                    .color(Color32::DARK_GRAY),
            );
            let sys = &state.sector.systems[sys_idx];
            show_conflict_readout(ui, &sys.conflict);
        } else {
            let mut working = state.sector.systems[sys_idx].conflict.clone();
            let original = working.clone();
            conflict_editor(ui, &format!("s_conf_{sys_id}"), &mut working, &factions);
            if ui
                .button("🗑  Clear conflict")
                .on_hover_text("Reset every conflict field back to peaceful defaults.")
                .clicked()
            {
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
    ui.label(RichText::new("Conflict heatmap on the map").strong());
    let mut on = matches!(state.map_heatmap_mode, HeatmapMode::ConflictIntensity);
    ui.horizontal_wrapped(|ui| {
        if ui
            .checkbox(&mut on, "Tint systems by fighting intensity")
            .on_hover_text(
                "Shades each system on the Map tab by how heavy its fighting is (0–100). \
                 A control overlay, if one is active, takes priority over this.",
            )
            .changed()
        {
            state.map_heatmap_mode = if on {
                HeatmapMode::ConflictIntensity
            } else {
                HeatmapMode::Off
            };
        }
        if ui
            .button("Open Map  →")
            .on_hover_text("Jump to the Map tab to see the heatmap")
            .clicked()
        {
            state.focus_entity(crate::builder::state::EntityRef::Tab(
                crate::builder::state::BuilderTab::Map,
            ));
        }
    });
}

// ── §CF4 advance ticks + §CF5 tick log shared widgets ─────────────────────

fn advance_ticks_block(ui: &mut Ui, state: &mut BuilderState, scope_id: &str) {
    ui.label(RichText::new("Advance time").strong());
    ui.horizontal(|ui| {
        ui.label("Ticks:");
        ui.add(
            egui::DragValue::new(&mut state.conflict_ticks_to_advance)
                .range(1..=u32::MAX)
                .speed(1.0),
        );
        let label = format!("▶  Advance {scope_id}");
        if ui
            .button(label)
            .on_hover_text(
                "Step the simulation forward by the chosen number of ticks. Each tick updates \
                 momentum, intensity, and — once a takeover holds long enough — the visible \
                 controller.",
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

pub(crate) fn show_tick_log(ui: &mut Ui, state: &mut BuilderState, filter_system: Option<&str>) {
    ui.label(RichText::new("Tick log").strong());
    if state.tick_log.is_empty() {
        ui_kit::placeholder(
            ui,
            "No ticks yet — use Advance time above to step the simulation.",
        );
        return;
    }
    ui.horizontal(|ui| {
        ui.label(format!("{} entries", state.tick_log.len()));
        if ui
            .small_button("🗑  Clear log")
            .on_hover_text("Discard all recorded tick history")
            .clicked()
        {
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
    labeled(
        ui,
        "Momentum",
        "Which side is winning, -100..100 (schema: momentum). Positive favours the attacker, \
         negative the defender.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.momentum, -100..=100));
        },
    );
    labeled(
        ui,
        "Intensity",
        "Overall ferocity of the fighting / unrest, 0..100 (schema: intensity).",
        |ui| {
            ui.add(egui::Slider::new(&mut state.intensity, 0..=100).text("/100"));
        },
    );
    labeled(
        ui,
        "Mobilisation",
        "How fully each side has committed its forces, 0..100 (schema: mobilisation).",
        |ui| {
            ui.add(egui::Slider::new(&mut state.mobilisation, 0..=100).text("/100"));
        },
    );
    labeled(
        ui,
        "Attacker",
        "Faction pressing the attack, if any (schema: attacker).",
        |ui| optional_faction_combo(ui, &format!("{salt}_att"), &mut state.attacker, factions),
    );
    labeled(
        ui,
        "Defender",
        "Faction holding the ground, usually the current controller (schema: defender).",
        |ui| optional_faction_combo(ui, &format!("{salt}_def"), &mut state.defender, factions),
    );
    labeled(
        ui,
        "Visible controller",
        "Who appears to be in charge publicly — only flips after a takeover holds for several \
         ticks (schema: visible_controller).",
        |ui| {
            optional_faction_combo(
                ui,
                &format!("{salt}_vis"),
                &mut state.visible_controller,
                factions,
            )
        },
    );
    labeled(
        ui,
        "Started on tick",
        "Tick on which the current fight began; 0 means pristine (schema: started_tick).",
        |ui| {
            ui.add(egui::DragValue::new(&mut state.started_tick).range(0..=u32::MAX));
        },
    );
    labeled(
        ui,
        "Last change tick",
        "Tick of the most recent control change (schema: last_change_tick).",
        |ui| {
            ui.add(egui::DragValue::new(&mut state.last_change_tick).range(0..=u32::MAX));
        },
    );
    labeled(
        ui,
        "Age",
        "Total ticks elapsed since this conflict was last reset (schema: age).",
        |ui| {
            ui.add(egui::DragValue::new(&mut state.age).range(0..=u32::MAX));
        },
    );
}

fn stability_editor(ui: &mut Ui, _salt: &str, state: &mut StabilityState) {
    labeled(
        ui,
        "Public order",
        "How calm and well-policed the world is (schema: public_order). Higher is calmer.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.public_order, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Corruption",
        "Extent of Chaos taint and graft (schema: corruption). Higher is worse.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.corruption, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Fear",
        "Level of dread gripping the populace (schema: fear). Higher is worse.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.fear, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Rebellion risk",
        "Chance of open revolt against the rulers (schema: rebellion_risk). Higher is worse.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.rebellion_risk, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Xenos threat",
        "Pressure from alien forces (schema: xenos_threat). Higher is worse.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.xenos_threat, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Warp instability",
        "Severity of warp storms and psychic turbulence (schema: warp_instability). Higher is \
         worse.",
        |ui| {
            ui.add(egui::Slider::new(&mut state.warp_instability, 0.0..=100.0));
        },
    );
    labeled(
        ui,
        "Famine / resource stress",
        "Strain on food and supplies (schema: famine_or_resource_stress). Higher is worse.",
        |ui| {
            ui.add(egui::Slider::new(
                &mut state.famine_or_resource_stress,
                0.0..=100.0,
            ));
        },
    );
}

fn show_conflict_readout(ui: &mut Ui, c: &ConflictState) {
    egui::Grid::new("conflict_readout_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Momentum")
                .on_hover_text("Which side is winning, -100..100 (schema: momentum).");
            ui.monospace(c.momentum.to_string());
            ui.end_row();
            ui.label("Intensity")
                .on_hover_text("Overall ferocity of the fighting, 0..100 (schema: intensity).");
            ui.monospace(format!("{}/100", c.intensity));
            ui.end_row();
            ui.label("Mobilisation").on_hover_text(
                "How fully each side has committed its forces, 0..100 (schema: mobilisation).",
            );
            ui.monospace(format!("{}/100", c.mobilisation));
            ui.end_row();
            ui.label("Attacker")
                .on_hover_text("Faction pressing the attack (schema: attacker).");
            ui.monospace(opt_id(&c.attacker));
            ui.end_row();
            ui.label("Defender")
                .on_hover_text("Faction holding the ground (schema: defender).");
            ui.monospace(opt_id(&c.defender));
            ui.end_row();
            ui.label("Visible controller").on_hover_text(
                "Who appears to be in charge publicly (schema: visible_controller).",
            );
            ui.monospace(opt_id(&c.visible_controller));
            ui.end_row();
            ui.label("Age")
                .on_hover_text("Ticks elapsed since last reset (schema: age).");
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
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "(none)").clicked() {
            *current = None;
        }
        for (fid, name) in factions {
            let sel = current.as_ref() == Some(fid);
            if ui
                .selectable_label(sel, format!("{name}  ({fid})"))
                .clicked()
            {
                *current = Some(fid.clone());
            }
        }
    });
}
