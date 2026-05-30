//! HISTORY tab (§N1 / §N2) — Phase C §H1..§H8.
//!
//! §H1  Config editor (enabled / epoch_start / epoch_end + per-anchor caps).
//! §H2  Eras editor — id / label / relative_start / relative_end / weight /
//!      allowed_events.
//! §H3  Event-rule editor — when_system_state / prefer_event / minimum_events.
//! §H4  Chronicle event editor — list of [`HistoryEvent`] with per-row inline
//!      edit of date / weight / narrative / faction refs / consequences.
//! §H5  "Add event" wizard — pick anchor entity → event kind → suggested
//!      factions → prose preview, commits an event with `manual = true`.
//! §H6  "Regenerate chronicle" runs [`BuilderState::recompute_chronicle`].
//!      Manual events (flagged `manual = true`) survive.
//! §H7  Timeline list — chronological, click-to-focus the affected
//!      system / world / route / region on the corresponding inspector tab.
//! §H8  Chronicle snippets appear on the WORLD inspector — wired from
//!      `panels/world.rs`.

use egui::{Color32, RichText, Ui};

use sectorforge::history::{
    EventKind, HistoryAnchor, HistoryConsequence, HistoryConsequenceKind, HistoryEntityKind,
    HistoryEntityRef, HistoryEra, HistoryEvent, HistoryEventRule,
};
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::sector_model::SystemState;

use crate::builder::state::{BuilderTab, EntityRef, HistoryAnchorKind, HistoryWizardState};
use crate::builder::BuilderState;

const DEFAULT_HISTORY_PATH: &str = "data/history.toml";

const EVENT_KINDS: &[EventKind] = &[
    EventKind::Foundation,
    EventKind::Discovery,
    EventKind::Annexation,
    EventKind::ImperialMandateGranted,
    EventKind::Consecration,
    EventKind::CommercialCharter,
    EventKind::DynasticClaim,
    EventKind::Secession,
    EventKind::Uprising,
    EventKind::Reconquest,
    EventKind::Purge,
    EventKind::CultExposed,
    EventKind::NecronAwakening,
    EventKind::TyranidContact,
    EventKind::OrkWaaagh,
    EventKind::QuarantineDeclared,
    EventKind::Blockade,
    EventKind::WarpStormSurge,
    EventKind::TauContact,
    EventKind::AeldariActivity,
    EventKind::ChaosIncursion,
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
    ui.heading("History");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "chronicle config, eras, rules, events, add wizard, regenerate, timeline.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_config_section(ui, state);
            ui.separator();
            show_eras_editor(ui, state);
            ui.separator();
            show_event_rules_editor(ui, state);
            ui.separator();
            show_events_editor(ui, state);
            ui.separator();
            show_add_event_wizard(ui, state);
            ui.separator();
            show_timeline(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── §H6 header actions ──────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Regenerate chronicle").clicked() {
            state.recompute_chronicle();
        }
        ui.checkbox(&mut state.history_auto_recompute, "auto-recompute on edit");
        let total = state.sector.chronicle.events.len();
        let manual = state
            .sector
            .chronicle
            .events
            .iter()
            .filter(|e| e.manual)
            .count();
        ui.label(format!("events: {total}  (manual: {manual})"));
        if state.data_catalogs.history.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no history.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §H1 config ──────────────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("chronicle config").strong());
    ensure_history_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.history.as_mut() else {
        return;
    };
    let mut changed = false;
    egui::Grid::new("h1_cfg_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("enabled");
            changed |= ui
                .checkbox(&mut cfg.enabled, "embed chronicle in sector.json")
                .changed();
            ui.end_row();
            ui.label("epoch_start (millennium)");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.epoch_start).range(1..=99))
                .changed();
            ui.end_row();
            ui.label("epoch_end (millennium)");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.epoch_end).range(1..=99))
                .changed();
            ui.end_row();
            ui.label("max_events_per_world");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_world).range(0..=99))
                .changed();
            ui.end_row();
            ui.label("max_events_per_system");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_system).range(0..=99))
                .changed();
            ui.end_row();
            ui.label("max_events_per_route");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_route).range(0..=99))
                .changed();
            ui.end_row();
            ui.label("key_events_top_n");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.key_events_top_n).range(0..=200))
                .changed();
            ui.end_row();
            ui.label("max_subsector_events");
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_subsector_events).range(0..=1024))
                .changed();
            ui.end_row();
        });
    if changed {
        on_catalog_edited(state);
    }
}

// ── §H2 eras editor ─────────────────────────────────────────────────────────

fn show_eras_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("eras").strong())
        .default_open(false)
        .show(ui, |ui| {
            ensure_history_catalog_if_needed(state);
            let Some(cfg) = state.data_catalogs.history.as_mut() else {
                return;
            };
            let mut changed = false;
            let mut remove_idx: Option<usize> = None;
            for (idx, era) in cfg.eras.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("[{idx}]")).monospace());
                        ui.label("id");
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut era.id)
                                    .desired_width(140.0)
                                    .hint_text("id (snake_case)"),
                            )
                            .changed();
                        ui.label("label");
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(&mut era.label)
                                    .desired_width(220.0)
                                    .hint_text("display label"),
                            )
                            .changed();
                        if ui
                            .button(RichText::new("× remove").color(Color32::LIGHT_RED))
                            .clicked()
                        {
                            remove_idx = Some(idx);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("relative_start");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut era.relative_start)
                                    .range(-2000..=2000)
                                    .speed(1),
                            )
                            .changed();
                        ui.label("relative_end");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut era.relative_end)
                                    .range(-2000..=2000)
                                    .speed(1),
                            )
                            .changed();
                        ui.label("weight");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut era.weight)
                                    .range(0.0..=10.0)
                                    .speed(0.05),
                            )
                            .changed();
                    });
                    ui.label("allowed_events (click to toggle)");
                    egui::ScrollArea::horizontal()
                        .id_salt(format!("h2_era_kinds_{idx}"))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for k in EVENT_KINDS {
                                    let is_in = era.allowed_events.contains(k);
                                    let resp = ui.selectable_label(is_in, kind_label(*k));
                                    if resp.clicked() {
                                        if is_in {
                                            era.allowed_events.retain(|x| x != k);
                                        } else {
                                            era.allowed_events.push(*k);
                                        }
                                        changed = true;
                                    }
                                }
                            });
                        });
                });
            }
            if let Some(i) = remove_idx {
                cfg.eras.remove(i);
                changed = true;
            }
            ui.separator();
            if ui.button("+ era").clicked() {
                cfg.eras.push(HistoryEra {
                    id: format!("era_{}", cfg.eras.len() + 1),
                    label: "New era".into(),
                    relative_start: 0,
                    relative_end: 0,
                    weight: 1.0,
                    allowed_events: Vec::new(),
                });
                changed = true;
            }
            if changed {
                on_catalog_edited(state);
            }
        });
}

// ── §H3 event rules editor ──────────────────────────────────────────────────

fn show_event_rules_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("event rules").strong())
        .default_open(false)
        .show(ui, |ui| {
            ensure_history_catalog_if_needed(state);
            let Some(cfg) = state.data_catalogs.history.as_mut() else {
                return;
            };
            let mut changed = false;
            let mut remove_idx: Option<usize> = None;
            for (idx, rule) in cfg.event_rules.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("[{idx}]")).monospace());
                        ui.label("id");
                        let mut id_buf = rule.id.clone().unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut id_buf)
                                    .hint_text("(optional)")
                                    .desired_width(140.0),
                            )
                            .changed()
                        {
                            rule.id = if id_buf.is_empty() {
                                None
                            } else {
                                Some(id_buf)
                            };
                            changed = true;
                        }
                        if ui
                            .button(RichText::new("× remove").color(Color32::LIGHT_RED))
                            .clicked()
                        {
                            remove_idx = Some(idx);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("when_system_state");
                        let mut state_pick = rule
                            .when_system_state
                            .as_deref()
                            .and_then(parse_system_state);
                        let picked_before = state_pick;
                        egui::ComboBox::from_id_salt(format!("h3_when_{idx}"))
                            .selected_text(match state_pick {
                                Some(s) => system_state_label(s),
                                None => "(any)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut state_pick, None, "(any)");
                                for s in SYSTEM_STATES {
                                    ui.selectable_value(
                                        &mut state_pick,
                                        Some(*s),
                                        system_state_label(*s),
                                    );
                                }
                            });
                        if state_pick != picked_before {
                            rule.when_system_state =
                                state_pick.map(|s| system_state_key(s).to_string());
                            changed = true;
                        }

                        ui.label("prefer_event");
                        let mut kind_pick =
                            rule.prefer_event.as_deref().and_then(parse_event_kind_str);
                        let kind_before = kind_pick;
                        egui::ComboBox::from_id_salt(format!("h3_prefer_{idx}"))
                            .selected_text(match kind_pick {
                                Some(k) => kind_label(k),
                                None => "(none)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut kind_pick, None, "(none)");
                                for k in EVENT_KINDS {
                                    ui.selectable_value(&mut kind_pick, Some(*k), kind_label(*k));
                                }
                            });
                        if kind_pick != kind_before {
                            rule.prefer_event = kind_pick.map(|k| kind_slug(k).to_string());
                            changed = true;
                        }

                        ui.label("minimum_events");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut rule.minimum_events)
                                    .range(0..=99)
                                    .speed(1),
                            )
                            .changed();
                    });
                });
            }
            if let Some(i) = remove_idx {
                cfg.event_rules.remove(i);
                changed = true;
            }
            ui.separator();
            if ui.button("+ event rule").clicked() {
                cfg.event_rules.push(HistoryEventRule {
                    id: None,
                    when_system_state: None,
                    prefer_event: None,
                    minimum_events: 1,
                });
                changed = true;
            }
            if changed {
                on_catalog_edited(state);
            }
        });
}

// ── §H4 events editor ───────────────────────────────────────────────────────

fn show_events_editor(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("events").strong())
        .default_open(true)
        .show(ui, |ui| {
            if state.sector.chronicle.events.is_empty() {
                ui.colored_label(
                    Color32::GRAY,
                    "No chronicle events. Click Regenerate chronicle above or use the wizard.",
                );
                return;
            }

            let selected = state.selected_history_event.clone();
            egui::ScrollArea::vertical()
                .id_salt("h4_events_scroll")
                .max_height(320.0)
                .show(ui, |ui| {
                    egui::Grid::new("h4_events_grid")
                        .striped(true)
                        .num_columns(6)
                        .show(ui, |ui| {
                            ui.label(RichText::new("date").strong());
                            ui.label(RichText::new("kind").strong());
                            ui.label(RichText::new("anchor").strong());
                            ui.label(RichText::new("wt").strong());
                            ui.label(RichText::new("source").strong());
                            ui.label("");
                            ui.end_row();
                            let events = state.sector.chronicle.events.clone();
                            for ev in &events {
                                let is_sel = selected.as_deref() == Some(ev.id.as_str());
                                if ui
                                    .selectable_label(is_sel, RichText::new(&ev.date).monospace())
                                    .clicked()
                                {
                                    state.selected_history_event = Some(ev.id.clone());
                                }
                                ui.label(kind_label(ev.kind));
                                ui.label(anchor_label(&ev.anchor));
                                ui.label(format!("{}", ev.weight));
                                if ev.manual {
                                    ui.colored_label(Color32::from_rgb(200, 220, 120), "manual");
                                } else {
                                    ui.colored_label(Color32::DARK_GRAY, "derived");
                                }
                                if ui.button("edit").clicked() {
                                    state.selected_history_event = Some(ev.id.clone());
                                }
                                ui.end_row();
                            }
                        });
                });

            ui.separator();
            show_selected_event_inspector(ui, state);
        });
}

fn show_selected_event_inspector(ui: &mut Ui, state: &mut BuilderState) {
    let Some(id) = state.selected_history_event.clone() else {
        ui.colored_label(Color32::GRAY, "Select an event above to inspect / edit.");
        return;
    };
    let Some(idx) = state
        .sector
        .chronicle
        .events
        .iter()
        .position(|e| e.id == id)
    else {
        state.selected_history_event = None;
        return;
    };

    let mut changed = false;
    let mut delete = false;
    let mut highlight = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let ev = &mut state.sector.chronicle.events[idx];
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("id").strong());
            ui.monospace(&ev.id);
            if ev.manual {
                ui.colored_label(Color32::from_rgb(200, 220, 120), "manual");
            } else {
                ui.colored_label(Color32::DARK_GRAY, "derived (regen-overwrites)");
            }
            if ui
                .button(RichText::new("× delete").color(Color32::LIGHT_RED))
                .clicked()
            {
                delete = true;
            }
            if ui.button("highlight on map").clicked() {
                highlight = true;
            }
        });

        egui::Grid::new("h4_inspector_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("date");
                changed |= ui.text_edit_singleline(&mut ev.date).changed();
                ui.end_row();
                ui.label("kind");
                let mut kind = ev.kind;
                let kind_before = kind;
                egui::ComboBox::from_id_salt("h4_kind")
                    .selected_text(kind_label(kind))
                    .show_ui(ui, |ui| {
                        for k in EVENT_KINDS {
                            ui.selectable_value(&mut kind, *k, kind_label(*k));
                        }
                    });
                if kind != kind_before {
                    ev.kind = kind;
                    changed = true;
                }
                ui.end_row();
                ui.label("era_label");
                changed |= ui.text_edit_singleline(&mut ev.era_label).changed();
                ui.end_row();
                ui.label("weight (0..=100)");
                changed |= ui
                    .add(egui::DragValue::new(&mut ev.weight).range(0..=100))
                    .changed();
                ui.end_row();
                ui.label("summary");
                changed |= ui.text_edit_singleline(&mut ev.summary).changed();
                ui.end_row();
                ui.label("narrative");
                changed |= ui
                    .add(egui::TextEdit::multiline(&mut ev.narrative).desired_rows(3))
                    .changed();
                ui.end_row();
            });

        ui.label(RichText::new("anchor").strong());
        ui.monospace(anchor_label(&ev.anchor));

        ui.label(RichText::new("factions").strong());
        let mut remove_f: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (fi, f) in ev.factions.iter().enumerate() {
                let chip = format!("{f} ×");
                if ui.small_button(chip).clicked() {
                    remove_f = Some(fi);
                }
            }
        });
        if let Some(i) = remove_f {
            ev.factions.remove(i);
            changed = true;
        }
    });

    // Add-faction row (after the frame to avoid borrowing `state.sector` twice).
    let factions_snapshot: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    let mut to_add: Option<FactionId> = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("+ faction");
        egui::ComboBox::from_id_salt("h4_add_fac")
            .selected_text("(pick)")
            .show_ui(ui, |ui| {
                for (id, name) in &factions_snapshot {
                    if ui
                        .selectable_label(false, format!("{id} — {name}"))
                        .clicked()
                    {
                        to_add = Some(id.clone());
                    }
                }
            });
    });
    if let Some(fid) = to_add {
        let ev = &mut state.sector.chronicle.events[idx];
        if !ev.factions.iter().any(|f| f == &fid) {
            ev.factions.push(fid);
            changed = true;
        }
    }

    // Consequences sub-editor.
    egui::CollapsingHeader::new("consequences")
        .default_open(false)
        .show(ui, |ui| {
            let ev = &mut state.sector.chronicle.events[idx];
            let mut remove_c: Option<usize> = None;
            for (ci, c) in ev.consequences.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("[{ci}]"));
                    changed |= ui.text_edit_singleline(&mut c.description).changed();
                    changed |= ui
                        .add(egui::DragValue::new(&mut c.severity).range(0..=100))
                        .changed();
                    if ui
                        .button(RichText::new("×").color(Color32::LIGHT_RED))
                        .clicked()
                    {
                        remove_c = Some(ci);
                    }
                });
            }
            if let Some(i) = remove_c {
                ev.consequences.remove(i);
                changed = true;
            }
            if ui.button("+ consequence").clicked() {
                ev.consequences.push(HistoryConsequence {
                    kind: HistoryConsequenceKind::RegionRecorded,
                    description: String::new(),
                    severity: 30,
                    entity_id: None,
                });
                changed = true;
            }
        });

    if delete {
        let anchor = state.sector.chronicle.events[idx].anchor.clone();
        state.sector.chronicle.events.remove(idx);
        state.selected_history_event = None;
        let _ = anchor;
        on_chronicle_mutated(state);
        return;
    }
    if highlight {
        focus_anchor(state, idx);
    }
    if changed {
        // Hand-edited events are pinned so they survive regeneration.
        state.sector.chronicle.events[idx].manual = true;
        on_chronicle_mutated(state);
    }
}

// ── §H5 add-event wizard ────────────────────────────────────────────────────

fn show_add_event_wizard(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("add event").strong())
        .default_open(false)
        .show(ui, |ui| {
            if state.history_wizard.is_none() {
                if ui.button("+ event (open wizard)").clicked() {
                    state.history_wizard = Some(HistoryWizardState::default());
                }
                return;
            }
            // Snapshot the bits we need before taking a mutable borrow on the wizard.
            let systems: Vec<(SystemId, String)> = state
                .sector
                .systems
                .iter()
                .map(|s| (s.id.clone(), s.name.to_string()))
                .collect();
            let worlds: Vec<(WorldId, String, SystemId)> = state
                .sector
                .systems
                .iter()
                .flat_map(|s| {
                    let sid = s.id.clone();
                    s.worlds
                        .iter()
                        .map(move |w| (w.id.clone(), w.name.to_string(), sid.clone()))
                })
                .collect();
            let routes: Vec<(RouteId, SystemId, SystemId, String)> = state
                .sector
                .routes
                .iter()
                .map(|r| {
                    (
                        r.id.clone(),
                        r.from_system_id.clone(),
                        r.to_system_id.clone(),
                        format!("{} ↔ {}", r.from_system_id, r.to_system_id),
                    )
                })
                .collect();
            let regions: Vec<(String, String)> = state
                .sector
                .regions
                .iter()
                .map(|r| (r.id.clone(), r.name.clone()))
                .collect();
            let factions: Vec<(FactionId, String)> = state
                .sector
                .factions
                .iter()
                .map(|f| (f.id.clone(), f.name.to_string()))
                .collect();

            let mut close = false;
            let mut commit = false;
            {
                let w = state.history_wizard.as_mut().unwrap();
                ui.horizontal_wrapped(|ui| {
                    ui.label("anchor kind");
                    egui::ComboBox::from_id_salt("h5_anchor_kind")
                        .selected_text(w.anchor_kind.label())
                        .show_ui(ui, |ui| {
                            for k in HistoryAnchorKind::ALL {
                                ui.selectable_value(&mut w.anchor_kind, *k, k.label());
                            }
                        });
                    ui.label("event kind");
                    egui::ComboBox::from_id_salt("h5_event_kind")
                        .selected_text(kind_label(w.kind))
                        .show_ui(ui, |ui| {
                            for k in EVENT_KINDS {
                                ui.selectable_value(&mut w.kind, *k, kind_label(*k));
                            }
                        });
                });

                match w.anchor_kind {
                    HistoryAnchorKind::Sector => {}
                    HistoryAnchorKind::System => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("system");
                            let cur = w
                                .anchor_system
                                .as_ref()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "(pick)".into());
                            egui::ComboBox::from_id_salt("h5_sys")
                                .selected_text(cur)
                                .show_ui(ui, |ui| {
                                    for (id, name) in &systems {
                                        if ui
                                            .selectable_label(
                                                w.anchor_system.as_ref() == Some(id),
                                                format!("{id} — {name}"),
                                            )
                                            .clicked()
                                        {
                                            w.anchor_system = Some(id.clone());
                                        }
                                    }
                                });
                        });
                    }
                    HistoryAnchorKind::World => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("world");
                            let cur = w
                                .anchor_world
                                .as_ref()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "(pick)".into());
                            egui::ComboBox::from_id_salt("h5_wld")
                                .selected_text(cur)
                                .show_ui(ui, |ui| {
                                    for (wid, name, sid) in &worlds {
                                        if ui
                                            .selectable_label(
                                                w.anchor_world.as_ref() == Some(wid),
                                                format!("{wid} — {name} ({sid})"),
                                            )
                                            .clicked()
                                        {
                                            w.anchor_world = Some(wid.clone());
                                            w.anchor_system = Some(sid.clone());
                                        }
                                    }
                                });
                        });
                    }
                    HistoryAnchorKind::Route => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("route");
                            let cur = w
                                .anchor_route
                                .as_ref()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "(pick)".into());
                            egui::ComboBox::from_id_salt("h5_route")
                                .selected_text(cur)
                                .show_ui(ui, |ui| {
                                    for (rid, _, _, label) in &routes {
                                        if ui
                                            .selectable_label(
                                                w.anchor_route.as_ref() == Some(rid),
                                                format!("{rid} — {label}"),
                                            )
                                            .clicked()
                                        {
                                            w.anchor_route = Some(rid.clone());
                                        }
                                    }
                                });
                        });
                    }
                    HistoryAnchorKind::Region => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("region");
                            let cur = w.anchor_region.clone().unwrap_or_else(|| "(pick)".into());
                            egui::ComboBox::from_id_salt("h5_region")
                                .selected_text(cur)
                                .show_ui(ui, |ui| {
                                    for (rid, name) in &regions {
                                        if ui
                                            .selectable_label(
                                                w.anchor_region.as_deref() == Some(rid.as_str()),
                                                format!("{rid} — {name}"),
                                            )
                                            .clicked()
                                        {
                                            w.anchor_region = Some(rid.clone());
                                        }
                                    }
                                });
                        });
                    }
                }

                // Auto-suggest participating factions: per-anchor presence
                // intersected with the global faction roster. Already-picked
                // factions render with a darker chip.
                let suggested = suggest_factions_for_wizard(&systems, &worlds, &factions, w);
                ui.label(RichText::new("participating factions").strong());
                ui.horizontal_wrapped(|ui| {
                    if suggested.is_empty() {
                        ui.colored_label(Color32::GRAY, "(no suggestions for this anchor)");
                    }
                    for (fid, _name) in &suggested {
                        let active = w.selected_factions.contains(fid);
                        let label = if active {
                            format!("✓ {fid}")
                        } else {
                            format!("+ {fid}")
                        };
                        if ui.small_button(label).clicked() {
                            if active {
                                w.selected_factions.remove(fid);
                            } else {
                                w.selected_factions.insert(fid.clone());
                            }
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("manual add");
                    egui::ComboBox::from_id_salt("h5_add_fac")
                        .selected_text("(pick)")
                        .show_ui(ui, |ui| {
                            for (fid, name) in &factions {
                                if ui
                                    .selectable_label(
                                        w.selected_factions.contains(fid),
                                        format!("{fid} — {name}"),
                                    )
                                    .clicked()
                                {
                                    w.selected_factions.insert(fid.clone());
                                }
                            }
                        });
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("date (optional, M{epoch}.{ddd})");
                    ui.add(
                        egui::TextEdit::singleline(&mut w.date)
                            .desired_width(120.0)
                            .hint_text("M40.500"),
                    );
                });

                let preview = preview_narrative(w, &systems, &worlds, &routes, &regions);
                ui.label(RichText::new("narrative (override; blank = preview)").strong());
                ui.add(
                    egui::TextEdit::multiline(&mut w.narrative)
                        .desired_rows(3)
                        .hint_text(preview.clone()),
                );
                if !preview.is_empty() {
                    ui.colored_label(Color32::DARK_GRAY, format!("preview: {preview}"));
                }

                ui.horizontal_wrapped(|ui| {
                    let ready = wizard_anchor_ready(w);
                    if ui
                        .add_enabled(ready, egui::Button::new("Commit event"))
                        .clicked()
                    {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            }

            if commit {
                if let Some(w) = state.history_wizard.take() {
                    let ev = build_manual_event(&w, &systems, &worlds, &routes, &regions);
                    state.sector.chronicle.events.push(ev);
                    state
                        .sector
                        .chronicle
                        .events
                        .sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
                    on_chronicle_mutated(state);
                }
            } else if close {
                state.history_wizard = None;
            }
        });
}

fn wizard_anchor_ready(w: &HistoryWizardState) -> bool {
    match w.anchor_kind {
        HistoryAnchorKind::Sector => true,
        HistoryAnchorKind::System => w.anchor_system.is_some(),
        HistoryAnchorKind::World => w.anchor_world.is_some() && w.anchor_system.is_some(),
        HistoryAnchorKind::Route => w.anchor_route.is_some(),
        HistoryAnchorKind::Region => w.anchor_region.is_some(),
    }
}

fn build_manual_event(
    w: &HistoryWizardState,
    systems: &[(SystemId, String)],
    worlds: &[(WorldId, String, SystemId)],
    routes: &[(RouteId, SystemId, SystemId, String)],
    regions: &[(String, String)],
) -> HistoryEvent {
    let anchor = wizard_anchor(w, systems, worlds, routes, regions);
    let narrative = if w.narrative.trim().is_empty() {
        preview_narrative(w, systems, worlds, routes, regions)
    } else {
        w.narrative.clone()
    };
    let date = if w.date.trim().is_empty() {
        "M40.500".to_string()
    } else {
        w.date.clone()
    };
    let id = format!(
        "evt-manual-{}-{:x}",
        kind_slug(w.kind),
        hash_str(&narrative)
    );
    let mut entities = entities_from_anchor(&anchor);
    for f in &w.selected_factions {
        entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::Faction,
            id: f.to_string(),
            role: Some("participant".into()),
        });
    }
    HistoryEvent {
        id,
        date,
        era_id: String::new(),
        era_label: String::new(),
        relative_year: 0,
        anchor,
        kind: w.kind,
        summary: narrative.clone(),
        narrative,
        factions: w.selected_factions.iter().cloned().collect(),
        entities,
        consequences: Vec::new(),
        weight: 50,
        manual: true,
    }
}

fn wizard_anchor(
    w: &HistoryWizardState,
    _systems: &[(SystemId, String)],
    worlds: &[(WorldId, String, SystemId)],
    routes: &[(RouteId, SystemId, SystemId, String)],
    _regions: &[(String, String)],
) -> HistoryAnchor {
    match w.anchor_kind {
        HistoryAnchorKind::Sector => HistoryAnchor::Sector,
        HistoryAnchorKind::System => HistoryAnchor::System {
            system_id: w.anchor_system.clone().unwrap(),
        },
        HistoryAnchorKind::World => {
            let wid = w.anchor_world.clone().unwrap();
            let sid = w
                .anchor_system
                .clone()
                .or_else(|| {
                    worlds
                        .iter()
                        .find(|(id, _, _)| *id == wid)
                        .map(|(_, _, s)| s.clone())
                })
                .unwrap_or_default();
            HistoryAnchor::World {
                system_id: sid,
                world_id: wid,
            }
        }
        HistoryAnchorKind::Route => {
            let rid = w.anchor_route.clone().unwrap();
            let (from, to) = routes
                .iter()
                .find(|(id, _, _, _)| *id == rid)
                .map(|(_, f, t, _)| (f.clone(), t.clone()))
                .unwrap_or_else(|| (SystemId::from(""), SystemId::from("")));
            HistoryAnchor::Route {
                route_id: rid,
                from_system_id: from,
                to_system_id: to,
            }
        }
        HistoryAnchorKind::Region => HistoryAnchor::Region {
            region_id: w.anchor_region.clone().unwrap(),
        },
    }
}

fn entities_from_anchor(anchor: &HistoryAnchor) -> Vec<HistoryEntityRef> {
    match anchor {
        HistoryAnchor::Sector => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Sector,
            id: "sector".into(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::System { system_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: system_id.to_string(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::World {
            system_id,
            world_id,
        } => vec![
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: system_id.to_string(),
                role: Some("parent_system".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: world_id.to_string(),
                role: Some("anchor".into()),
            },
        ],
        HistoryAnchor::Route { route_id, .. } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Route,
            id: route_id.to_string(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Subsector { subsector_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Subsector,
            id: subsector_id.clone(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Region { region_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Region,
            id: region_id.clone(),
            role: Some("anchor".into()),
        }],
        _ => vec![],
    }
}

fn preview_narrative(
    w: &HistoryWizardState,
    systems: &[(SystemId, String)],
    worlds: &[(WorldId, String, SystemId)],
    routes: &[(RouteId, SystemId, SystemId, String)],
    regions: &[(String, String)],
) -> String {
    let anchor_name = match w.anchor_kind {
        HistoryAnchorKind::Sector => "the sector".to_string(),
        HistoryAnchorKind::System => w
            .anchor_system
            .as_ref()
            .and_then(|id| systems.iter().find(|(s, _)| s == id))
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "an unnamed system".into()),
        HistoryAnchorKind::World => w
            .anchor_world
            .as_ref()
            .and_then(|id| worlds.iter().find(|(w, _, _)| w == id))
            .map(|(_, n, _)| n.clone())
            .unwrap_or_else(|| "an unnamed world".into()),
        HistoryAnchorKind::Route => w
            .anchor_route
            .as_ref()
            .and_then(|id| routes.iter().find(|(r, _, _, _)| r == id))
            .map(|(_, _, _, label)| label.clone())
            .unwrap_or_else(|| "an unnamed lane".into()),
        HistoryAnchorKind::Region => w
            .anchor_region
            .as_ref()
            .and_then(|id| regions.iter().find(|(r, _)| r == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "an unnamed region".into()),
    };
    let factions: Vec<String> = w.selected_factions.iter().map(|f| f.to_string()).collect();
    let actors = if factions.is_empty() {
        "Local authorities".to_string()
    } else {
        factions.join("and")
    };
    format!(
        "{actors} entered the chronicle around {anchor_name} as a {} event.",
        kind_label(w.kind)
    )
}

fn suggest_factions_for_wizard(
    _systems: &[(SystemId, String)],
    worlds: &[(WorldId, String, SystemId)],
    factions: &[(FactionId, String)],
    _w: &HistoryWizardState,
) -> Vec<(FactionId, String)> {
    // Minimal heuristic: when an anchor is a world / system, suggest the
    // sector roster as-is (callers can refine by clicking through). For
    // route/region/sector anchors we fall back to the full roster.
    let _ = worlds;
    factions.to_vec()
}

// ── §H7 timeline ────────────────────────────────────────────────────────────

fn show_timeline(ui: &mut Ui, state: &mut BuilderState) {
    egui::CollapsingHeader::new(RichText::new("timeline").strong())
        .default_open(true)
        .show(ui, |ui| {
            if state.sector.chronicle.events.is_empty() {
                ui.colored_label(Color32::GRAY, "Empty chronicle. Regenerate above.");
                return;
            }
            let events = state.sector.chronicle.events.clone();
            egui::ScrollArea::vertical()
                .id_salt("h7_timeline_scroll")
                .max_height(280.0)
                .show(ui, |ui| {
                    for (i, ev) in events.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&ev.date).monospace().strong());
                            ui.label(format!("{}", ev.kind));
                            ui.colored_label(
                                Color32::DARK_GRAY,
                                format!("({})", anchor_label(&ev.anchor)),
                            );
                            if sectorforge_gui_core::entity_link(
                                ui,
                                short_narrative(&ev.narrative),
                                false,
                            )
                            .clicked()
                            {
                                state.focus_entity(EntityRef::HistoryEvent(ev.id.clone()));
                            }
                            if ui.small_button("focus").clicked() {
                                focus_anchor(state, i);
                            }
                        });
                    }
                });
        });
}

fn short_narrative(s: &str) -> String {
    if s.len() <= 110 {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(107).collect();
        out.push('…');
        out
    }
}

// ── save row ────────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("history.toml").strong());
    let has_catalog = state.data_catalogs.history.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save history.toml"))
            .clicked()
        {
            if state.config.inputs.history.is_none() {
                state.config.inputs.history = Some(DEFAULT_HISTORY_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save history.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .history
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_HISTORY_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── focus / highlight ───────────────────────────────────────────────────────

fn focus_anchor(state: &mut BuilderState, event_idx: usize) {
    let Some(ev) = state.sector.chronicle.events.get(event_idx).cloned() else {
        return;
    };
    let target = match ev.anchor {
        HistoryAnchor::Sector => EntityRef::Tab(BuilderTab::Map),
        HistoryAnchor::System { system_id } => EntityRef::System(system_id),
        HistoryAnchor::World {
            system_id,
            world_id,
        } => EntityRef::World {
            system: system_id,
            world: world_id,
        },
        HistoryAnchor::Route { route_id, .. } => EntityRef::Route(route_id),
        HistoryAnchor::Subsector { subsector_id } => EntityRef::Subsector(subsector_id),
        HistoryAnchor::Region { region_id } => EntityRef::Region(region_id),
        _ => EntityRef::Tab(BuilderTab::Map),
    };
    state.focus_entity(target);
}

// ── shared helpers ──────────────────────────────────────────────────────────

fn ensure_history_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.history.is_none() {
        state.data_catalogs.history = Some(sectorforge::history::HistoryConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.history.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_HISTORY_PATH.into());
    }
    state.mark_validation_dirty();
    if state.history_auto_recompute {
        state.recompute_chronicle();
    }
}

fn on_chronicle_mutated(state: &mut BuilderState) {
    state.dirty = true;
    state.mark_validation_dirty();
    state.trigger_auto_save();
}

pub(crate) fn kind_label(k: EventKind) -> &'static str {
    match k {
        EventKind::Foundation => "Foundation",
        EventKind::Discovery => "Discovery",
        EventKind::Annexation => "Annexation",
        EventKind::ImperialMandateGranted => "ImperialMandateGranted",
        EventKind::Consecration => "Consecration",
        EventKind::CommercialCharter => "CommercialCharter",
        EventKind::DynasticClaim => "DynasticClaim",
        EventKind::Secession => "Secession",
        EventKind::Uprising => "Uprising",
        EventKind::Reconquest => "Reconquest",
        EventKind::Purge => "Purge",
        EventKind::CultExposed => "CultExposed",
        EventKind::NecronAwakening => "NecronAwakening",
        EventKind::TyranidContact => "TyranidContact",
        EventKind::OrkWaaagh => "OrkWaaagh",
        EventKind::QuarantineDeclared => "QuarantineDeclared",
        EventKind::Blockade => "Blockade",
        EventKind::WarpStormSurge => "WarpStormSurge",
        EventKind::TauContact => "TauContact",
        EventKind::AeldariActivity => "AeldariActivity",
        EventKind::ChaosIncursion => "ChaosIncursion",
        _ => "UNKNOWN",
    }
}

fn kind_slug(k: EventKind) -> &'static str {
    match k {
        EventKind::Foundation => "foundation",
        EventKind::Discovery => "discovery",
        EventKind::Annexation => "annexation",
        EventKind::ImperialMandateGranted => "mandate",
        EventKind::Consecration => "consecration",
        EventKind::CommercialCharter => "charter",
        EventKind::DynasticClaim => "dynasty",
        EventKind::Secession => "secession",
        EventKind::Uprising => "uprising",
        EventKind::Reconquest => "reconquest",
        EventKind::Purge => "purge",
        EventKind::CultExposed => "cult",
        EventKind::NecronAwakening => "necron",
        EventKind::TyranidContact => "tyranid",
        EventKind::OrkWaaagh => "waaagh",
        EventKind::QuarantineDeclared => "quarantine",
        EventKind::Blockade => "blockade",
        EventKind::WarpStormSurge => "warpstorm",
        EventKind::TauContact => "tau",
        EventKind::AeldariActivity => "aeldari",
        EventKind::ChaosIncursion => "chaos",
        _ => "unknown",
    }
}

fn parse_event_kind_str(s: &str) -> Option<EventKind> {
    let key: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    use EventKind::*;
    match key.as_str() {
        "foundation" | "founding" => Some(Foundation),
        "discovery" => Some(Discovery),
        "annexation" => Some(Annexation),
        "compliance" | "imperialmandate" | "imperialmandategranted" | "mandate" => {
            Some(ImperialMandateGranted)
        }
        "consecration" => Some(Consecration),
        "treaty" | "commercialcharter" | "charter" => Some(CommercialCharter),
        "dynasticclaim" | "dynasty" => Some(DynasticClaim),
        "schism" | "secession" => Some(Secession),
        "rebellion" | "uprising" => Some(Uprising),
        "war" | "reconquest" | "crusade" => Some(Reconquest),
        "purge" => Some(Purge),
        "cultexposed" | "cult" => Some(CultExposed),
        "awakening" | "necronawakening" | "necron" => Some(NecronAwakening),
        "tyranidcontact" | "tyranid" => Some(TyranidContact),
        "orkwaaagh" | "waaagh" | "ork" => Some(OrkWaaagh),
        "quarantinedeclared" | "quarantine" => Some(QuarantineDeclared),
        "blockade" => Some(Blockade),
        "plague" | "warpstormsurge" | "warpstorm" => Some(WarpStormSurge),
        "taucontact" | "tau" => Some(TauContact),
        "aeldariactivity" | "aeldari" => Some(AeldariActivity),
        "chaosincursion" | "chaos" => Some(ChaosIncursion),
        _ => None,
    }
}

fn parse_system_state(s: &str) -> Option<SystemState> {
    let key: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match key.as_str() {
        "pacified" => Some(SystemState::Pacified),
        "fragmented" => Some(SystemState::Fragmented),
        "blockaded" => Some(SystemState::Blockaded),
        "warzone" => Some(SystemState::Warzone),
        "infiltrated" => Some(SystemState::Infiltrated),
        "quarantined" => Some(SystemState::Quarantined),
        "uncharted" => Some(SystemState::Uncharted),
        _ => None,
    }
}

fn system_state_label(s: SystemState) -> &'static str {
    match s {
        SystemState::Pacified => "Pacified",
        SystemState::Fragmented => "Fragmented",
        SystemState::Blockaded => "Blockaded",
        SystemState::Warzone => "Warzone",
        SystemState::Infiltrated => "Infiltrated",
        SystemState::Quarantined => "Quarantined",
        SystemState::Uncharted => "Uncharted",
        _ => "Unknown",
    }
}

fn system_state_key(s: SystemState) -> &'static str {
    match s {
        SystemState::Pacified => "pacified",
        SystemState::Fragmented => "fragmented",
        SystemState::Blockaded => "blockaded",
        SystemState::Warzone => "warzone",
        SystemState::Infiltrated => "infiltrated",
        SystemState::Quarantined => "quarantined",
        SystemState::Uncharted => "uncharted",
        _ => "unknown",
    }
}

fn anchor_label(a: &HistoryAnchor) -> String {
    match a {
        HistoryAnchor::Sector => "sector".into(),
        HistoryAnchor::System { system_id } => format!("system:{system_id}"),
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("world:{system_id}/{world_id}"),
        HistoryAnchor::Route { route_id, .. } => format!("route:{route_id}"),
        HistoryAnchor::Subsector { subsector_id } => format!("subsector:{subsector_id}"),
        HistoryAnchor::Region { region_id } => format!("region:{region_id}"),
        _ => "unknown".into(),
    }
}

fn hash_str(s: &str) -> u64 {
    let seed = sectorforge::rng::derive_stage_seed("", "chronicle", s);
    u64::from_le_bytes(seed[..8].try_into().expect("blake3 returns 32 bytes"))
}

// ── §H8 helpers (consumed by panels/world.rs) ───────────────────────────────

/// §H8: collect every chronicle event anchored at (system, world). Returns a
/// stable date-sorted slice for the caller to render inside the WORLD
/// inspector.
pub fn world_chronicle_events<'a>(
    state: &'a BuilderState,
    sys_id: &SystemId,
    world_id: &WorldId,
) -> Vec<&'a HistoryEvent> {
    let mut events: Vec<&HistoryEvent> = state
        .sector
        .chronicle
        .events
        .iter()
        .filter(|e| match &e.anchor {
            HistoryAnchor::World {
                system_id,
                world_id: wid,
            } => system_id == sys_id && wid == world_id,
            _ => false,
        })
        .collect();
    events.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{GeneratedSector, HexCoord};

    fn seed_state() -> BuilderState {
        let mut state = BuilderState::new_blank("h-test", "H", "seed", 8, 8);
        state.sector = GeneratedSector::empty("h-test", "H", "seed", 8, 8);
        let sid = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "Alpha")
            .unwrap();
        let _ = state.sector.add_world_to_system(&sid, "Alpha I").unwrap();
        state
    }

    #[test]
    fn recompute_chronicle_preserves_manual_event() {
        let mut state = seed_state();
        let manual = HistoryEvent {
            id: "evt-manual-test".into(),
            date: "M40.500".into(),
            era_id: String::new(),
            era_label: "Custom".into(),
            relative_year: 0,
            anchor: HistoryAnchor::Sector,
            kind: EventKind::Foundation,
            summary: "manual".into(),
            narrative: "manual".into(),
            factions: Vec::new(),
            entities: Vec::new(),
            consequences: Vec::new(),
            weight: 70,
            manual: true,
        };
        state.sector.chronicle.events.push(manual);
        state.recompute_chronicle();
        assert!(state
            .sector
            .chronicle
            .events
            .iter()
            .any(|e| e.id == "evt-manual-test" && e.manual));
    }

    #[test]
    fn wizard_anchor_ready_world_requires_both_ids() {
        let mut w = HistoryWizardState {
            anchor_kind: HistoryAnchorKind::World,
            ..Default::default()
        };
        assert!(!wizard_anchor_ready(&w));
        w.anchor_world = Some(WorldId::from("wrld-0001-1"));
        w.anchor_system = Some(SystemId::from("sys-0001"));
        assert!(wizard_anchor_ready(&w));
    }

    #[test]
    fn build_manual_event_pins_manual_flag() {
        let w = HistoryWizardState {
            anchor_kind: HistoryAnchorKind::Sector,
            kind: EventKind::Discovery,
            narrative: "test narrative".into(),
            ..HistoryWizardState::default()
        };
        let ev = build_manual_event(&w, &[], &[], &[], &[]);
        assert!(ev.manual);
        assert_eq!(ev.kind, EventKind::Discovery);
        assert!(ev.id.starts_with("evt-manual-"));
    }

    #[test]
    fn world_chronicle_events_filters_to_anchor() {
        let mut state = seed_state();
        let sid = state.sector.systems[0].id.clone();
        let wid = state.sector.systems[0].worlds[0].id.clone();
        state.sector.chronicle.events.push(HistoryEvent {
            id: "evt-w".into(),
            date: "M40.100".into(),
            era_id: String::new(),
            era_label: String::new(),
            relative_year: 0,
            anchor: HistoryAnchor::World {
                system_id: sid.clone(),
                world_id: wid.clone(),
            },
            kind: EventKind::Foundation,
            summary: "w".into(),
            narrative: "w".into(),
            factions: Vec::new(),
            entities: Vec::new(),
            consequences: Vec::new(),
            weight: 30,
            manual: true,
        });
        state.sector.chronicle.events.push(HistoryEvent {
            id: "evt-s".into(),
            date: "M40.200".into(),
            era_id: String::new(),
            era_label: String::new(),
            relative_year: 0,
            anchor: HistoryAnchor::Sector,
            kind: EventKind::Discovery,
            summary: "s".into(),
            narrative: "s".into(),
            factions: Vec::new(),
            entities: Vec::new(),
            consequences: Vec::new(),
            weight: 30,
            manual: true,
        });
        let evs = world_chronicle_events(&state, &sid, &wid);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].id, "evt-w");
    }
}
