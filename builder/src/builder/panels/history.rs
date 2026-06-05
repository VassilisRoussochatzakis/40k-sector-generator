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
use sectorforge_gui_core::{palette, ui_kit::{self, labeled}, widgets};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{
    BuilderTab, EntityRef, HistoryAnchorKind, HistoryWizardState, ModalKind,
};
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

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("History");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "The story of your sector — set the timeframe, shape the eras, then build a chronicle of events.",
    );
    ui.separator();

    // §COLUMNS — "Regenerate chronicle" + the event-count summary stay
    // full-width on top, then RC-2 (hand-assigned, collapse-safe): the chronicle
    // config + eras + event-rule editors go in the left column; the event list,
    // add-event wizard and timeline (which want the width) go in the right
    // column. Collapses to a single stacked column on a narrow window. Replaces
    // the single-column stack that pushed the timeline far below the fold.
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();

            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left: config + eras + event rules.
                    let left = &mut cols[0];
                    show_config_section(left, state);
                    left.separator();
                    show_eras_editor(left, state);
                    left.separator();
                    show_event_rules_editor(left, state);
                }
                {
                    // Right — or the same single column when collapsed to one:
                    // event list + add-event wizard + timeline + save.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    if n == 1 {
                        right.separator();
                    }
                    show_events_editor(right, state);
                    right.separator();
                    show_add_event_wizard(right, state);
                    right.separator();
                    show_timeline(right, state);
                    right.separator();
                    show_save_row(right, state);
                }
            });
        });
}

/// Compact inline label with a hover tooltip, for the wrapped multi-field rows
/// (eras / rules / consequences) where a fixed-width column would break the
/// side-by-side editing layout. Same intent as [`labeled`]: a human word on
/// screen, the raw schema field + note in the tooltip.
fn hint_label(ui: &mut Ui, label: &str, help: &str) {
    ui.label(RichText::new(label).color(palette::chrome_text_dim()))
        .on_hover_text(help);
}

// ── §H6 header actions ──────────────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Regenerate chronicle")
            .on_hover_text(
                "Rebuild the chronicle from the current sector. Events you added or hand-edited are kept.",
            )
            .clicked()
        {
            // D2/D11: undoable regenerate — lands on the command bus.
            if let Err(e) = state.recompute_chronicle_undoable() {
                state.last_save_error = Some(format!("chronicle regenerate failed: {e}"));
            }
        }
        ui.checkbox(&mut state.history_auto_recompute, "Rebuild after every edit")
            .on_hover_text(
                "Regenerate the chronicle automatically whenever you change settings, eras, or rules.",
            );
        let total = state.sector.chronicle.events.len();
        let manual = state
            .sector
            .chronicle
            .events
            .iter()
            .filter(|e| e.manual)
            .count();
        ui.label(format!("{total} event(s)  ({manual} hand-added)"));
        if state.data_catalogs.history.is_none() {
            ui.colored_label(
                palette::warning(),
                "● using default history settings",
            )
            .on_hover_text(
                "No history settings file is loaded yet — sensible defaults apply. Editing here creates one.",
            );
        }
    });
}

// ── §H1 config ──────────────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Chronicle settings").strong());
    ensure_history_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.history.as_mut() else {
        return;
    };
    let mut changed = false;
    labeled(
        ui,
        "Save with sector",
        "Embed the generated chronicle in the exported sector (schema: enabled).",
        |ui| {
            changed |= ui
                .checkbox(&mut cfg.enabled, "Include in sector export")
                .changed();
        },
    );
    labeled(
        ui,
        "Start millennium",
        "First millennium the chronicle can place events in (schema: epoch_start). 40 = M40.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.epoch_start).range(1..=99))
                .changed();
        },
    );
    labeled(
        ui,
        "End millennium",
        "Last millennium the chronicle can place events in (schema: epoch_end).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.epoch_end).range(1..=99))
                .changed();
        },
    );
    labeled(
        ui,
        "Max events / world",
        "Most chronicle events kept for any single world (schema: max_events_per_world).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_world).range(0..=99))
                .changed();
        },
    );
    labeled(
        ui,
        "Max events / system",
        "Most chronicle events kept for any single system (schema: max_events_per_system).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_system).range(0..=99))
                .changed();
        },
    );
    labeled(
        ui,
        "Max events / route",
        "Most chronicle events kept for any single route (schema: max_events_per_route).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_events_per_route).range(0..=99))
                .changed();
        },
    );
    labeled(
        ui,
        "Key events shown",
        "How many top-weighted events to surface as sector highlights (schema: key_events_top_n).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.key_events_top_n).range(0..=200))
                .changed();
        },
    );
    labeled(
        ui,
        "Max subsector events",
        "Most chronicle events kept per subsector (schema: max_subsector_events).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_subsector_events).range(0..=1024))
                .changed();
        },
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §H2 eras editor ─────────────────────────────────────────────────────────

fn show_eras_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "hist_eras", "Eras", false, |ui| {
        ensure_history_catalog_if_needed(state);
        let Some(cfg) = state.data_catalogs.history.as_mut() else {
            return;
        };
        if cfg.eras.is_empty() {
            ui_kit::placeholder(
                ui,
                "No eras yet — add one to carve the timeline into named ages.",
            );
        }
        let mut changed = false;
        let mut remove_idx: Option<usize> = None;
        for (idx, era) in cfg.eras.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("[{idx}]")).monospace());
                    hint_label(ui, "ID", "Unique identifier for this era (schema: id).");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut era.id)
                                .desired_width(140.0)
                                .hint_text("lowercase, no spaces"),
                        )
                        .changed();
                    hint_label(ui, "Name", "Display name for this era (schema: label).");
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(&mut era.label)
                                .desired_width(220.0)
                                .hint_text("e.g. Age of Discovery"),
                        )
                        .changed();
                    if ui
                        .button(RichText::new("🗑  Remove").color(palette::danger()))
                        .on_hover_text("Remove this era")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    hint_label(
                        ui,
                        "Starts (years)",
                        "Era start, in years relative to the epoch (schema: relative_start).",
                    );
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut era.relative_start)
                                .range(-2000..=2000)
                                .speed(1),
                        )
                        .changed();
                    hint_label(
                        ui,
                        "Ends (years)",
                        "Era end, in years relative to the epoch (schema: relative_end).",
                    );
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut era.relative_end)
                                .range(-2000..=2000)
                                .speed(1),
                        )
                        .changed();
                    hint_label(
                        ui,
                        "Weight",
                        "How heavily events cluster in this era (schema: weight). Higher = busier.",
                    );
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut era.weight)
                                .range(0.0..=10.0)
                                .speed(0.05),
                        )
                        .changed();
                });
                hint_label(
                    ui,
                    "Allowed event types (click to toggle)",
                    "Only these event types may occur during this era (schema: allowed_events). None selected = all allowed.",
                );
                egui::ScrollArea::horizontal()
                    .id_salt(format!("h2_era_kinds_{idx}"))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for k in EVENT_KINDS {
                                let is_in = era.allowed_events.contains(k);
                                let resp = ui
                                    .selectable_label(is_in, kind_label(*k))
                                    .on_hover_text(format!("key: {}", k.as_slug()));
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
        if ui
            .button("➕  Era")
            .on_hover_text("Add a new era to the timeline")
            .clicked()
        {
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
    ui_kit::collapsing_section(ui, "hist_event_rules", "Event rules", false, |ui| {
        ensure_history_catalog_if_needed(state);
        let Some(cfg) = state.data_catalogs.history.as_mut() else {
            return;
        };
        if cfg.event_rules.is_empty() {
            ui_kit::placeholder(
                ui,
                "No rules yet — add one to nudge which events appear in which systems.",
            );
        }
        let mut changed = false;
        let mut remove_idx: Option<usize> = None;
        for (idx, rule) in cfg.event_rules.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("[{idx}]")).monospace());
                    hint_label(ui, "Name", "Optional label for this rule (schema: id).");
                    let mut id_buf = rule.id.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut id_buf)
                                .hint_text("optional")
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
                        .button(RichText::new("🗑  Remove").color(palette::danger()))
                        .on_hover_text("Remove this rule")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    hint_label(
                        ui,
                        "When system is",
                        "Apply this rule only to systems in this state (schema: when_system_state). (any) = always.",
                    );
                    let mut state_pick = rule
                        .when_system_state
                        .as_deref()
                        .and_then(parse_system_state);
                    let picked_before = state_pick;
                    ui_kit::combo(
                        format!("h3_when_{idx}"),
                        match state_pick {
                            Some(s) => system_state_label(s),
                            None => "(any)",
                        },
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state_pick, None, "(any)");
                        for s in super::SYSTEM_STATES {
                            ui.selectable_value(&mut state_pick, Some(*s), system_state_label(*s))
                                .on_hover_text(format!("key: {}", system_state_key(*s)));
                        }
                    });
                    if state_pick != picked_before {
                        rule.when_system_state =
                            state_pick.map(|s| system_state_key(s).to_string());
                        changed = true;
                    }

                    hint_label(
                        ui,
                        "Prefer event",
                        "Favour this event type when the rule matches (schema: prefer_event).",
                    );
                    let mut kind_pick = rule.prefer_event.as_deref().and_then(parse_event_kind_str);
                    let kind_before = kind_pick;
                    ui_kit::combo(
                        format!("h3_prefer_{idx}"),
                        match kind_pick {
                            Some(k) => kind_label(k),
                            None => "(none)",
                        },
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut kind_pick, None, "(none)");
                        for k in EVENT_KINDS {
                            ui.selectable_value(&mut kind_pick, Some(*k), kind_label(*k))
                                .on_hover_text(format!("key: {}", k.as_slug()));
                        }
                    });
                    if kind_pick != kind_before {
                        rule.prefer_event = kind_pick.map(|k| kind_slug(k).to_string());
                        changed = true;
                    }

                    hint_label(
                        ui,
                        "At least",
                        "Guarantee at least this many matching events (schema: minimum_events).",
                    );
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
        if ui
            .button("➕  Event rule")
            .on_hover_text("Add a new event rule")
            .clicked()
        {
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
    ui_kit::collapsing_section(ui, "hist_events", "Chronicle events", true, |ui| {
        if state.sector.chronicle.events.is_empty() {
            ui_kit::placeholder(
                ui,
                "No events yet — use Regenerate chronicle above, or add one with the wizard below.",
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
                        ui.label(RichText::new("Date").strong());
                        ui.label(RichText::new("Type").strong());
                        ui.label(RichText::new("Where").strong());
                        ui.label(RichText::new("Weight").strong());
                        ui.label(RichText::new("Source").strong());
                        ui.label("");
                        ui.end_row();
                        // §E9: iterate by reference — the loop only mutates the
                        // disjoint `selected_history_event` field, so no clone of
                        // the whole event Vec is needed.
                        for ev in &state.sector.chronicle.events {
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
                                ui.colored_label(palette::success(), "hand-added")
                                    .on_hover_text(
                                        "You added or edited this event; it survives regeneration.",
                                    );
                            } else {
                                ui.colored_label(Color32::DARK_GRAY, "generated")
                                    .on_hover_text(
                                    "Produced by Regenerate chronicle; may change on the next run.",
                                );
                            }
                            if ui
                                .button("✏  Edit")
                                .on_hover_text("Select this event to edit it below")
                                .clicked()
                            {
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
        ui_kit::placeholder(ui, "Select an event above to inspect / edit.");
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

    // §R4: all field edits accumulate on a clone of the chronicle and commit
    // through `BuilderCommand::EditChronicle` — the widgets bind to
    // `chron.events[idx].*` so in-frame display is unchanged, but the write is
    // undoable. `state.run` handles dirty / invariants / validation /
    // auto-save, so no `on_chronicle_mutated` call is needed afterward.
    let mut chron = state.sector.chronicle.clone();
    let mut changed = false;
    let mut delete = false;
    let mut highlight = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let ev = &mut chron.events[idx];
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("ID").strong())
                .on_hover_text("Unique identifier for this event (schema: id).");
            ui.monospace(&ev.id);
            if ev.manual {
                ui.colored_label(palette::success(), "hand-added")
                    .on_hover_text("You added or edited this event; it survives regeneration.");
            } else {
                ui.colored_label(Color32::DARK_GRAY, "generated")
                    .on_hover_text("Produced by Regenerate chronicle; editing it pins it so it is kept.");
            }
            if ui
                .button(RichText::new("🗑  Delete").color(palette::danger()))
                .on_hover_text("Remove this event from the chronicle")
                .clicked()
            {
                delete = true;
            }
            if ui
                .button("◎  Show on map")
                .on_hover_text("Jump to and highlight this event's location")
                .clicked()
            {
                highlight = true;
            }
        });

        labeled(
            ui,
            "Date",
            "When it happened (schema: date). Imperial dating, e.g. M40.500.",
            |ui| {
                changed |= ui.text_edit_singleline(&mut ev.date).changed();
            },
        );
        labeled(
            ui,
            "Type",
            "What kind of event this is (schema: kind).",
            |ui| {
                let mut kind = ev.kind;
                let kind_before = kind;
                ui_kit::combo("h4_kind", kind_label(kind)).show_ui(ui, |ui| {
                    for k in EVENT_KINDS {
                        ui.selectable_value(&mut kind, *k, kind_label(*k))
                            .on_hover_text(format!("key: {}", k.as_slug()));
                    }
                });
                if kind != kind_before {
                    ev.kind = kind;
                    changed = true;
                }
            },
        );
        labeled(
            ui,
            "Era name",
            "Name of the era this event falls in (schema: era_label).",
            |ui| {
                changed |= ui.text_edit_singleline(&mut ev.era_label).changed();
            },
        );
        labeled(
            ui,
            "Weight",
            "Prominence of the event (schema: weight, 0–100). Higher = more likely to be a highlight.",
            |ui| {
                changed |= ui
                    .add(egui::DragValue::new(&mut ev.weight).range(0..=100))
                    .changed();
            },
        );
        labeled(
            ui,
            "Summary",
            "One-line summary (schema: summary).",
            |ui| {
                changed |= ui.text_edit_singleline(&mut ev.summary).changed();
            },
        );
        labeled(
            ui,
            "Narrative",
            "Full prose for this event (schema: narrative).",
            |ui| {
                changed |= ui
                    .add(egui::TextEdit::multiline(&mut ev.narrative).desired_rows(3))
                    .changed();
            },
        );

        ui.label(RichText::new("Location").strong())
            .on_hover_text("Where this event is anchored (schema: anchor).");
        ui.monospace(anchor_label(&ev.anchor));

        ui.label(RichText::new("Factions involved").strong())
            .on_hover_text("Factions taking part (schema: factions). Click a chip to remove it.");
        if ev.factions.is_empty() {
            ui_kit::placeholder(ui, "none — add one below");
        }
        let mut remove_f: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (fi, f) in ev.factions.iter().enumerate() {
                let chip = format!("{f} ×");
                if ui
                    .small_button(chip)
                    .on_hover_text("Remove this faction")
                    .clicked()
                {
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
        ui.label("Add faction");
        ui_kit::combo("h4_add_fac", "(pick one)").show_ui(ui, |ui| {
            for (id, name) in &factions_snapshot {
                if ui
                    .selectable_label(false, format!("{name} ({id})"))
                    .clicked()
                {
                    to_add = Some(id.clone());
                }
            }
        });
    });
    if let Some(fid) = to_add {
        let ev = &mut chron.events[idx];
        if !ev.factions.iter().any(|f| f == &fid) {
            ev.factions.push(fid);
            changed = true;
        }
    }

    // Consequences sub-editor.
    ui_kit::collapsing_section(ui, "hist_consequences", "Consequences", false, |ui| {
        let ev = &mut chron.events[idx];
        if ev.consequences.is_empty() {
            ui_kit::placeholder(ui, "none yet — add a lasting effect of this event below");
        }
        let mut remove_c: Option<usize> = None;
        for (ci, c) in ev.consequences.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("[{ci}]")).monospace());
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut c.description).hint_text("what changed"))
                    .changed();
                hint_label(
                    ui,
                    "Severity",
                    "How serious the consequence is (schema: severity, 0–100).",
                );
                changed |= ui
                    .add(egui::DragValue::new(&mut c.severity).range(0..=100))
                    .changed();
                if ui
                    .button(RichText::new("🗑").color(palette::danger()))
                    .on_hover_text("Remove this consequence")
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
        if ui
            .button("➕  Consequence")
            .on_hover_text("Add a lasting effect of this event")
            .clicked()
        {
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
        // §R4: delete via EditChronicle (was `chronicle.events.remove(idx)`).
        chron.events.remove(idx);
        if let Err(e) = state.run(BuilderCommand::EditChronicle {
            before: None,
            after: Box::new(chron),
        }) {
            state.modal = Some(ModalKind::Message(format!("Chronicle edit failed: {e}")));
        } else {
            state.selected_history_event = None;
        }
        return;
    }
    if highlight {
        focus_anchor(state, idx);
    }
    if changed {
        // §R4: commit hand-edits via EditChronicle (was direct field writes +
        // a manual-pin assignment). Hand-edited events are pinned so they
        // survive regeneration.
        chron.events[idx].manual = true;
        if let Err(e) = state.run(BuilderCommand::EditChronicle {
            before: None,
            after: Box::new(chron),
        }) {
            state.modal = Some(ModalKind::Message(format!("Chronicle edit failed: {e}")));
        }
    }
}

// ── §H5 add-event wizard ────────────────────────────────────────────────────

fn show_add_event_wizard(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::collapsing_section(ui, "hist_add_event", "Add an event", false, |ui| {
        if state.history_wizard.is_none() {
            ui_kit::placeholder(
                ui,
                "Write your own chronicle event — pick where and what, then commit.",
            );
            if ui
                .button("➕  New event…")
                .on_hover_text("Open the guided event builder")
                .clicked()
            {
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
                hint_label(
                    ui,
                    "Happens to",
                    "What the event is anchored to — the whole sector, a system, world, route, or region.",
                );
                ui_kit::combo("h5_anchor_kind", w.anchor_kind.label()).show_ui(ui, |ui| {
                    for k in HistoryAnchorKind::ALL {
                        ui.selectable_value(&mut w.anchor_kind, *k, k.label());
                    }
                });
                hint_label(ui, "Event type", "What kind of event this is.");
                ui_kit::combo("h5_event_kind", kind_label(w.kind)).show_ui(ui, |ui| {
                    for k in EVENT_KINDS {
                        ui.selectable_value(&mut w.kind, *k, kind_label(*k))
                            .on_hover_text(format!("key: {}", k.as_slug()));
                    }
                });
            });

            match w.anchor_kind {
                HistoryAnchorKind::Sector => {}
                HistoryAnchorKind::System => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("System");
                        let cur = w
                            .anchor_system
                            .as_ref()
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "(pick)".into());
                        ui_kit::combo("h5_sys", cur).show_ui(ui, |ui| {
                            for (id, name) in &systems {
                                if ui
                                    .selectable_label(
                                        w.anchor_system.as_ref() == Some(id),
                                        format!("{name} ({id})"),
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
                        ui.label("World");
                        let cur = w
                            .anchor_world
                            .as_ref()
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "(pick)".into());
                        ui_kit::combo("h5_wld", cur).show_ui(ui, |ui| {
                            for (wid, name, sid) in &worlds {
                                if ui
                                    .selectable_label(
                                        w.anchor_world.as_ref() == Some(wid),
                                        format!("{name} ({wid}, in {sid})"),
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
                        ui.label("Route");
                        let cur = w
                            .anchor_route
                            .as_ref()
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "(pick)".into());
                        ui_kit::combo("h5_route", cur).show_ui(ui, |ui| {
                            for (rid, _, _, label) in &routes {
                                if ui
                                    .selectable_label(
                                        w.anchor_route.as_ref() == Some(rid),
                                        format!("{label}  ({rid})"),
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
                        ui.label("Region");
                        let cur = w.anchor_region.clone().unwrap_or_else(|| "(pick)".into());
                        ui_kit::combo("h5_region", cur).show_ui(ui, |ui| {
                            for (rid, name) in &regions {
                                if ui
                                    .selectable_label(
                                        w.anchor_region.as_deref() == Some(rid.as_str()),
                                        format!("{name} ({rid})"),
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
            ui.label(RichText::new("Factions involved").strong())
                .on_hover_text("Click to add or remove a faction as a participant.");
            ui.horizontal_wrapped(|ui| {
                if suggested.is_empty() {
                    ui_kit::placeholder(ui, "no suggestions here — add one below");
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
                ui.label("Add another");
                ui_kit::combo("h5_add_fac", "(pick one)").show_ui(ui, |ui| {
                    for (fid, name) in &factions {
                        if ui
                            .selectable_label(
                                w.selected_factions.contains(fid),
                                format!("{name} ({fid})"),
                            )
                            .clicked()
                        {
                            w.selected_factions.insert(fid.clone());
                        }
                    }
                });
            });

            ui.horizontal_wrapped(|ui| {
                hint_label(
                    ui,
                    "Date",
                    "Optional. Imperial dating — millennium then fraction, e.g. M40.500. Left blank, a default is used.",
                );
                ui.add(
                    egui::TextEdit::singleline(&mut w.date)
                        .desired_width(120.0)
                        .hint_text("M40.500"),
                );
            });

            let preview = preview_narrative(w, &systems, &worlds, &routes, &regions);
            ui.label(RichText::new("Narrative").strong()).on_hover_text(
                "Optional prose. Leave blank to use the auto-generated preview below.",
            );
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
                    .add_enabled(ready, egui::Button::new("✔  Add event"))
                    .on_hover_text("Add this event to the chronicle")
                    .clicked()
                {
                    commit = true;
                }
                if ui
                    .button("Cancel")
                    .on_hover_text("Discard this draft and close the builder")
                    .clicked()
                {
                    close = true;
                }
            });
        }

        if commit {
            if let Some(w) = state.history_wizard.take() {
                let ev = build_manual_event(&w, &systems, &worlds, &routes, &regions);
                // §R4: push + re-sort on a clone and commit via
                // EditChronicle (was a direct `chronicle.events.push` +
                // `.sort_by`). `state.run` handles dirty / invariants /
                // validation / auto-save.
                let mut chron = state.sector.chronicle.clone();
                chron.events.push(ev);
                chron
                    .events
                    .sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
                if let Err(e) = state.run(BuilderCommand::EditChronicle {
                    before: None,
                    after: Box::new(chron),
                }) {
                    state.modal = Some(ModalKind::Message(format!("Chronicle edit failed: {e}")));
                }
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
    ui_kit::collapsing_section(ui, "hist_timeline", "Timeline", true, |ui| {
        if state.sector.chronicle.events.is_empty() {
            ui_kit::placeholder(
                ui,
                "Nothing on the timeline yet — regenerate the chronicle or add an event above.",
            );
            return;
        }
        let event_count = state.sector.chronicle.events.len();
        egui::ScrollArea::vertical()
            .id_salt("h7_timeline_scroll")
            .max_height(280.0)
            .show(ui, |ui| {
                for i in 0..event_count {
                    // §E9: read the display fields under a scoped borrow that ends
                    // before the row's mutating calls (focus_entity / focus_anchor
                    // both take `&mut state`), instead of cloning the whole event
                    // Vec every frame.
                    let ev = &state.sector.chronicle.events[i];
                    let date = ev.date.clone();
                    let kind = ev.kind;
                    let anchor = anchor_label(&ev.anchor);
                    let narrative = short_narrative(&ev.narrative);
                    let id = ev.id.clone();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&date).monospace().strong());
                        ui.label(kind_label(kind));
                        ui.colored_label(Color32::DARK_GRAY, format!("({anchor})"));
                        if sectorforge_gui_core::entity_link(ui, narrative, false)
                            .on_hover_text("Open this event")
                            .clicked()
                        {
                            state.focus_entity(EntityRef::HistoryEvent(id));
                        }
                        if ui
                            .small_button("◎")
                            .on_hover_text("Show this event's location on the map")
                            .clicked()
                        {
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
    ui.label(RichText::new("Save history settings").strong());
    let has_catalog = state.data_catalogs.history.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("💾  Save"))
            .on_hover_text("Write the chronicle settings, eras, and rules to disk")
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
            .unwrap_or_else(|| format!("will be saved to {DEFAULT_HISTORY_PATH}"));
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
        // D2/D11: route through the bus so the auto-recompute is undoable and
        // cannot silently diverge from a prior EditChronicle on the undo stack.
        if let Err(e) = state.recompute_chronicle_undoable() {
            state.last_save_error = Some(format!("chronicle regenerate failed: {e}"));
        }
    }
}

// §R4: `on_chronicle_mutated` (dirty + mark_validation_dirty + trigger_auto_save)
// was removed — every chronicle edit now goes through
// `BuilderCommand::EditChronicle`, and `BuilderState::run` performs that exact
// bookkeeping (plus index rebuild, derivation invalidation, invariant check,
// and undo-log push). The "Regenerate chronicle" path uses
// `recompute_chronicle`, a derivation, and is intentionally not a field edit.

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
pub(crate) fn world_chronicle_events<'a>(
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
        state.sector = GeneratedSector::empty("h-test", "H", "seed", 8, 8).into();
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
