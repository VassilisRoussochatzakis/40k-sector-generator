//! SYSTEM tab — AR1 / AR2 / AR3 archetype sections (§30).

use egui::{RichText, Ui};

use sectorforge_gui_core::{palette, ui_kit};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

use super::pretty_slug;

// ── AR1 / AR2 / AR3 — Archetypes (§30) ─────────────────────────────────────

pub(super) fn show_archetype_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize) {
    use sectorforge::archetypes::{
        ArchetypeState, GscStage, NecronPhase, TauSphereBand, TyranidStage,
    };

    let sys_id = state.sector.systems[sys_idx].id.clone();
    let mut working = state.sector.systems[sys_idx].archetype.clone();
    let original = working.clone();

    ui_kit::collapsing_section(ui, "sys_archetypes", "Archetypes", false, |ui| {
        ui.label(
            RichText::new(
                "How far each faction-themed storyline has progressed here. Flavour notes live in the Tags + Notes section.",
            )
            .small()
            .color(palette::chrome_text_dim()),
        );
        ui.add_space(4.0);

        egui::Grid::new("archetype_axes")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Imperial co-sovereigns").on_hover_text(
                    "Additional Imperial factions sharing rule here (schema: archetype.imperial_co_sovereigns).",
                );
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
                        ui_kit::combo("arch_imp_add", "➕ Add faction").show_ui(ui, |ui| {
                            for f in &state.sector.factions {
                                if working.imperial_co_sovereigns.contains(&f.id) {
                                    continue;
                                }
                                if ui
                                    .button(format!("{} ({})", f.name, f.id))
                                    .clicked()
                                {
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

                ui.label("Necron phase")
                    .on_hover_text("How awake the Necrons are here (schema: archetype.necron_phase).");
                ui_kit::combo("arch_necron", pretty_slug(working.necron_phase.as_slug())).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            NecronPhase::None,
                            NecronPhase::Dormant,
                            NecronPhase::Awakening,
                            NecronPhase::Awake,
                        ] {
                            ui.selectable_value(
                                &mut working.necron_phase,
                                v,
                                pretty_slug(v.as_slug()),
                            )
                            .on_hover_text(format!("schema: {}", v.as_slug()));
                        }
                    },
                );
                ui.end_row();

                ui.label("Tyranid stage")
                    .on_hover_text("Tyranid infestation progress (schema: archetype.tyranid_stage).");
                ui_kit::combo("arch_tyranid", pretty_slug(working.tyranid_stage.as_slug())).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            TyranidStage::None,
                            TyranidStage::Inhabited,
                            TyranidStage::Besieged,
                            TyranidStage::Consumed,
                        ] {
                            ui.selectable_value(
                                &mut working.tyranid_stage,
                                v,
                                pretty_slug(v.as_slug()),
                            )
                            .on_hover_text(format!("schema: {}", v.as_slug()));
                        }
                    },
                );
                ui.end_row();

                ui.label("Ork Waaagh!")
                    .on_hover_text("Strength of the Ork Waaagh!, 0–100 (schema: archetype.ork_waaagh).");
                ui.add(egui::Slider::new(&mut working.ork_waaagh, 0..=100).text("/100"));
                ui.end_row();

                ui.label("Genestealer stage").on_hover_text(
                    "Genestealer cult infiltration progress (schema: archetype.gsc_stage).",
                );
                ui_kit::combo("arch_gsc", pretty_slug(working.gsc_stage.as_slug())).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            GscStage::None,
                            GscStage::Rumor,
                            GscStage::HiddenCell,
                            GscStage::DistrictControl,
                            GscStage::ParallelGovernment,
                            GscStage::Uprising,
                            GscStage::PlanetarySeizure,
                        ] {
                            ui.selectable_value(&mut working.gsc_stage, v, pretty_slug(v.as_slug()))
                                .on_hover_text(format!("schema: {}", v.as_slug()));
                        }
                    },
                );
                ui.end_row();

                ui.label("T'au sphere")
                    .on_hover_text("How far into the T'au Empire's sphere this sits (schema: archetype.tau_sphere).");
                ui_kit::combo("arch_tau", pretty_slug(working.tau_sphere.as_slug())).show_ui(
                    ui,
                    |ui| {
                        for v in [
                            TauSphereBand::None,
                            TauSphereBand::Contact,
                            TauSphereBand::Fringe,
                            TauSphereBand::Client,
                            TauSphereBand::Core,
                        ] {
                            ui.selectable_value(&mut working.tau_sphere, v, pretty_slug(v.as_slug()))
                                .on_hover_text(format!("schema: {}", v.as_slug()));
                        }
                    },
                );
                ui.end_row();

                ui.label("Aeldari activity")
                    .on_hover_text("Level of Aeldari presence, 0–100 (schema: archetype.aeldari_activity).");
                ui.add(egui::Slider::new(&mut working.aeldari_activity, 0..=100).text("/100"));
                ui.end_row();

                ui.label("Chaos corruption")
                    .on_hover_text("Degree of Chaos taint, 0–100 (schema: archetype.chaos_corruption).");
                ui.add(egui::Slider::new(&mut working.chaos_corruption, 0..=100).text("/100"));
                ui.end_row();

                ui.label("Daemon manifestation")
                    .on_hover_text("Strength of daemonic incursion, 0–100 (schema: archetype.daemon_manifestation).");
                ui.add(egui::Slider::new(&mut working.daemon_manifestation, 0..=100).text("/100"));
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button("↺ Reset to default")
                .on_hover_text("Clear every archetype marker on this system")
                .clicked()
            {
                working = ArchetypeState::default();
            }
            if ui
                .button("🔄 Auto-assign (this system)")
                .on_hover_text(
                    "Re-derives archetype markers from the whole sector and keeps only \
                         this system's result.",
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
            state.feedback.modal =
                Some(ModalKind::Message(format!("Archetype update failed: {e}")));
        }
    }
}

pub(super) fn show_archetype_auto_assign(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "sys_archetype_auto",
        "Auto-assign archetypes (sector-wide)",
        false,
        |ui| {
            ui.label(
                RichText::new(
                    "Derives archetype markers for every system at once, limited to the storylines enabled below. This can be undone.",
                )
                .small()
                .color(palette::chrome_text_dim()),
            );
            if ui
                .button("▶ Run on whole sector")
                .on_hover_text("Re-derive archetype markers across every system")
                .clicked()
            {
                let flags = state.archetype_flags;
                let cmd = BuilderCommand::AutoAssignArchetypes {
                    flags,
                    before: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("Auto-assign failed: {e}")));
                }
            }
        },
    );
}

pub(super) fn show_archetype_rules(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(
        ui,
        "sys_archetype_rules",
        "Which storylines to auto-assign",
        false,
        |ui| {
            ui.label(
                RichText::new(
                    "Tick the faction storylines that auto-assign is allowed to set. These choices apply to this session only and aren't saved with the sector; unticked storylines are reset to their defaults when you run it.",
                )
                .small()
                .color(palette::chrome_text_dim()),
            );
            let flags = &mut state.archetype_flags;
            ui.checkbox(&mut flags.imperial, "Imperial governance");
            ui.checkbox(&mut flags.necron, "Necron phase");
            ui.checkbox(&mut flags.tyranid, "Tyranid front");
            ui.checkbox(&mut flags.ork, "Ork Waaagh!");
            ui.checkbox(&mut flags.gsc, "Genestealer stages");
            ui.checkbox(&mut flags.tau, "T'au sphere");
            ui.checkbox(&mut flags.aeldari, "Aeldari activity");
            ui.checkbox(&mut flags.chaos, "Chaos corruption + daemons");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Enable all")
                    .on_hover_text("Turn on every storyline")
                    .clicked()
                {
                    *flags = crate::builder::command::ArchetypeApplyFlags::default();
                }
                if ui
                    .button("Disable all")
                    .on_hover_text("Turn off every storyline")
                    .clicked()
                {
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
        },
    );
}
