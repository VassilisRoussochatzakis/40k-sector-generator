//! MISSIONS tab — Phase D §M1..§M5.
//!
//! §M1  Mission list panel. Rows render kind / patron / target /
//!      primary+secondary location / public objective / hidden complication /
//!      reward / consequence over the cached [`MissionsReport`] published by
//!      [`BuilderState::recompute_missions`]. Selecting a row populates
//!      [`BuilderState::selected_mission_id`] +
//!      [`BuilderState::missions_edit_target`] and focuses the primary
//!      location through [`BuilderState::focus_entity`] so cross-tab links
//!      land first-class.
//! §M2  Manual mission editor: kind picker plus the patron / target /
//!      primary+secondary / objective / hidden / reward / consequence
//!      fields. Manual rows live in
//!      [`MissionsConfig::manual`](sectorforge::missions::MissionsConfig)
//!      and survive every "Auto-derive" pass.
//! §M3  "Auto-derive missions" calls [`BuilderState::recompute_missions`]
//!      which runs [`sectorforge::missions::derive_with`]. Manual entries
//!      survive because `derive_with` appends `cfg.manual` after the
//!      per-anchor cap pass.
//! §M4  Player-edition toggle (mirrors `--player`): flips
//!      [`BuilderState::missions_player_edition`] and re-runs the recompute
//!      so the cached report has `gm_only` rows stripped.
//! §M5  Click-to-highlight: each row plus the "highlight location" button
//!      parses the mission's `primary_location` (`sys` or `sys/world`) into
//!      the matching [`EntityRef`] and calls
//!      [`BuilderState::focus_entity`], landing on the MAP tab via
//!      `EntityRef::Tab(BuilderTab::Map)` so the system / world is selected
//!      under the existing focus overlay.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};
use sectorforge::missions::{
    MissionKind, MissionScale, MissionSeed, MissionVisibility, MissionsConfig,
};
use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;
use sectorforge_gui_core::widgets;

use crate::builder::state::{BuilderTab, ConfirmAction, EntityRef, ModalKind};
use crate::builder::BuilderState;

const DEFAULT_MISSIONS_PATH: &str = "data/missions.toml";

/// Every [`MissionKind`] in panel-display order. Keep in sync with
/// `src/missions.rs::MissionKind`.
const KIND_VARIANTS: &[MissionKind] = &[
    MissionKind::Investigate,
    MissionKind::Escort,
    MissionKind::Sabotage,
    MissionKind::Diplomacy,
    MissionKind::Assassination,
    MissionKind::Recovery,
    MissionKind::Defense,
    MissionKind::Exploration,
];

const SCALE_VARIANTS: &[MissionScale] = &[
    MissionScale::OneShot,
    MissionScale::ShortArc,
    MissionScale::CampaignArc,
];

const VISIBILITY_VARIANTS: &[MissionVisibility] = &[
    MissionVisibility::Public,
    MissionVisibility::Restricted,
    MissionVisibility::Secret,
    MissionVisibility::Misinformation,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Missions");
    ui.label(
        RichText::new("Jobs the players can take on — auto-suggested from the sector, plus any you write by hand.")
            .color(Color32::DARK_GRAY),
    );
    ui.separator();

    // §COLUMNS — global controls (regenerate / player-edition / config knobs)
    // stay full-width on top, then master-detail: the ranked mission list pins
    // to a resizable left rail (filter + rows) and the detail card + manual
    // editor + save fill the rest. Replaces the single-column stack whose list
    // and detail scrolled past each other.
    show_header_actions(ui, state);
    ui.separator();
    show_config_section(ui, state);
    ui.separator();

    egui::SidePanel::left("missions_list")
        .resizable(true)
        .default_width(320.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| {
            show_filter_row(ui, state);
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| show_mission_list(ui, state));
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                show_detail_card(ui, state);
                ui.separator();
                show_manual_editor(ui, state);
                ui.separator();
                show_save_row(ui, state);
            });
    });
}

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Title", "Reward") while the tooltip names the
/// underlying TOML field plus a plain-language note, so power users keep the
/// schema mapping. Friendlier replacement for the old bare `egui::Grid` whose
/// row labels *were* the raw schema names.
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

// ── §M3 / §M4 header actions ───────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Auto-derive missions")
            .on_hover_text("Suggest a fresh set of missions from the current sector. Your hand-written ones are kept.")
            .clicked()
        {
            ensure_missions_catalog(state);
            state.recompute_missions();
        }
        ui.checkbox(&mut state.missions_auto_recompute, "Re-suggest as I edit")
            .on_hover_text("Re-run the suggestions automatically whenever you change a knob or a manual mission.");
        if ui
            .checkbox(&mut state.missions_player_edition, "Player edition")
            .on_hover_text(
                "Hide GM-only missions, as a player would see them. Turn off to see everything.",
            )
            .changed()
        {
            state.recompute_missions();
        }
        let total = state
            .missions_report
            .as_ref()
            .map(|r| r.missions.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .missions
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("{total} mission(s)  ·  {manual} hand-written"));
        if state.data_catalogs.missions.is_none() {
            ui.colored_label(
                palette::warning(),
                "● using built-in defaults",
            )
            .on_hover_text("No saved mission settings yet — defaults apply until you save.");
        }
    });
}

// ── §M3 config knobs ───────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Mission settings").strong());
    ensure_missions_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.missions.as_mut() else {
        return;
    };
    let mut changed = false;
    labeled(
        ui,
        "Max per location",
        "Cap on suggested missions for any one world, system, or route (schema: max_per_anchor). Higher means more variety per place.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_per_anchor).range(0..=32))
                .changed();
        },
    );
    labeled(
        ui,
        "Digest length",
        "How many top missions appear in the exported Markdown digest (schema: top_n_digest). Does not change the list here.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.top_n_digest).range(0..=200))
                .changed();
        },
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §M1 kind filter ────────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Show").strong());
        let label = match state.missions_filter_kind {
            None => "all kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        ui_kit::combo("m1_kind", label).show_ui(ui, |ui| {
            ui.selectable_value(&mut state.missions_filter_kind, None, "all kinds");
            for k in KIND_VARIANTS {
                ui.selectable_value(&mut state.missions_filter_kind, Some(*k), kind_label(*k))
                    .on_hover_text(format!("key: {}", k.as_slug()));
            }
        });
    });
}

// ── §M1 ranked list ────────────────────────────────────────────────────────

fn show_mission_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Missions").strong());
    let Some(report) = state.missions_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No missions yet — click \"Auto-derive missions\" above to suggest some.",
        );
        return;
    };
    let filter = state.missions_filter_kind;
    let rows: Vec<&MissionSeed> = report
        .missions
        .iter()
        .filter(|m| filter.is_none_or(|k| m.kind == k))
        .collect();
    if rows.is_empty() {
        ui_kit::placeholder(
            ui,
            "Nothing matches the current filter. Pick \"all kinds\", or turn off Player edition to reveal GM missions.",
        );
        return;
    }
    let selected = state.selected_mission_id.clone();
    let show_hidden = !state.missions_player_edition;
    // §COLUMNS — compact rail rows: a selectable title line per mission with
    // kind / scale subline; full fields live in the detail card on the right.
    for m in &rows {
        let is_selected = selected.as_deref() == Some(m.id.as_str());
        let title = if m.title.is_empty() {
            kind_label(m.kind).to_string()
        } else {
            m.title.clone()
        };
        // §BEAUTY — each rail entry is a custom-painted selectable plate (hover
        // lift + brass selection bar + accent glow) rather than a stock
        // selectable_label; the trailing 📍 highlights the mission's location.
        let (resp, focus_clicked) =
            card::selectable_plate(ui, ("mission_row", &m.id), is_selected, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).strong());
                    ui.label(
                        RichText::new(format!("{} · {}", kind_label(m.kind), scale_label(m.scale)))
                            .color(palette::chrome_text_dim())
                            .small(),
                    );
                });
                if show_hidden && m.gm_only {
                    ui.colored_label(palette::warning(), "GM")
                        .on_hover_text("GM-only — hidden under Player edition.");
                }
                let rem = ui.available_width();
                ui.add_space((rem - 22.0).max(0.0));
                ui.small_button("📍")
                    .on_hover_text("Select this mission and show its location on the map.")
                    .clicked()
            });
        if focus_clicked {
            select_mission(state, m);
            focus_primary_location(state, &m.primary_location, &m.route_ids);
        }
        if resp.clicked() {
            select_mission(state, m);
        }
    }
}

// ── §M1 detail card ────────────────────────────────────────────────────────

fn show_detail_card(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Mission details").strong());
    let target = state
        .missions_edit_target
        .clone()
        .or_else(|| state.selected_mission_id.clone());
    let Some(target_id) = target else {
        ui_kit::placeholder(ui, "Pick a mission on the left to see its full details.");
        return;
    };
    let Some(mission) = state
        .missions_report
        .as_ref()
        .and_then(|r| r.missions.iter().find(|m| m.id == target_id))
        .cloned()
    else {
        ui_kit::placeholder(
            ui,
            "That mission is no longer in the list — click \"Auto-derive missions\" to refresh.",
        );
        return;
    };
    let show_hidden = !state.missions_player_edition;

    labeled(
        ui,
        "ID",
        "Stable identifier for this mission (schema: id).",
        |ui| {
            ui.label(RichText::new(mission.id.clone()).monospace());
        },
    );
    labeled(ui, "Kind", "Mission archetype (schema: kind).", |ui| {
        ui.label(kind_label(mission.kind))
            .on_hover_text(format!("key: {}", mission.kind.as_slug()));
    });
    labeled(
        ui,
        "Title",
        "Headline shown to players (schema: title).",
        |ui| {
            ui.label(RichText::new(mission.title.clone()).strong());
        },
    );
    labeled(
        ui,
        "Patron",
        "Faction offering the job (schema: patron). Click to jump to it.",
        |ui| {
            if let Some(f) = &mission.patron {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
        },
    );
    labeled(
        ui,
        "Target",
        "Faction the job is aimed at (schema: target). Click to jump to it.",
        |ui| {
            if let Some(f) = &mission.target {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
        },
    );
    labeled(
        ui,
        "Primary location",
        "Main place the mission happens (schema: primary_location). Click to highlight it on the map.",
        |ui| {
            if ui
                .link(RichText::new(mission.primary_location.clone()).monospace())
                .clicked()
            {
                focus_primary_location(state, &mission.primary_location, &mission.route_ids);
            }
        },
    );
    labeled(
        ui,
        "Secondary location",
        "Optional follow-up place (schema: secondary_location). Click to highlight it.",
        |ui| {
            if let Some(sec) = &mission.secondary_location {
                let sec_clone = sec.clone();
                let route_ids = mission.route_ids.clone();
                if ui.link(RichText::new(sec.clone()).monospace()).clicked() {
                    focus_primary_location(state, &sec_clone, &route_ids);
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
        },
    );
    labeled(
        ui,
        "Routes",
        "Travel routes this mission touches (schema: route_ids). Click one to jump to it.",
        |ui| {
            if mission.route_ids.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.horizontal_wrapped(|ui| {
                    for rid in &mission.route_ids {
                        if ui.link(rid.to_string()).clicked() {
                            state.focus_entity(EntityRef::Route(rid.clone()));
                        }
                    }
                });
            }
        },
    );
    labeled(
        ui,
        "Objective",
        "What the players are openly asked to do (schema: public_objective).",
        |ui| {
            ui.label(mission.public_objective.clone());
        },
    );
    if show_hidden {
        labeled(
            ui,
            "Hidden complication",
            "GM-only twist behind the job (schema: hidden_complication).",
            |ui| {
                if let Some(c) = &mission.hidden_complication {
                    ui.label(RichText::new(c.clone()).color(palette::warning()));
                } else {
                    ui.colored_label(Color32::DARK_GRAY, "—");
                }
            },
        );
    }
    labeled(
        ui,
        "Reward",
        "What the patron offers on success (schema: reward).",
        |ui| {
            ui.label(mission.reward.clone());
        },
    );
    labeled(
        ui,
        "If ignored",
        "What happens if the players walk away (schema: if_ignored).",
        |ui| {
            ui.label(mission.if_ignored.clone());
        },
    );
    labeled(
        ui,
        "Scale / visibility",
        "How long the mission runs and how openly it is known (schema: scale, visibility).",
        |ui| {
            ui.label(format!(
                "{} / {}",
                scale_label(mission.scale),
                visibility_label(mission.visibility)
            ));
        },
    );
    labeled(
        ui,
        "Weight",
        "Ranking priority — higher sorts nearer the top (schema: weight).",
        |ui| {
            ui.label(mission.weight.to_string());
        },
    );

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("📍 Highlight primary")
            .on_hover_text("Show the primary location on the map.")
            .clicked()
        {
            focus_primary_location(state, &mission.primary_location, &mission.route_ids);
        }
        if let Some(sec) = mission.secondary_location.clone() {
            let route_ids = mission.route_ids.clone();
            if ui
                .button("📍 Highlight secondary")
                .on_hover_text("Show the secondary location on the map.")
                .clicked()
            {
                focus_primary_location(state, &sec, &route_ids);
            }
        }
    });
}

// ── §M2 manual editor ──────────────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Your own missions").strong());
    // Existing sector ids feed the location / faction pickers below. Built
    // once here (sorted, deterministic) and shared across every manual row.
    let existing = ExistingRefs::from_state(state);
    ensure_missions_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.missions.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("➕  Add mission")
            .on_hover_text("Write a new mission by hand. It survives every Auto-derive.")
            .clicked()
        {
            cfg.manual.push(blank_manual_mission(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Hand-written missions are added on top of the suggestions and are never overwritten.",
        );
    });
    if cfg.manual.is_empty() {
        ui_kit::placeholder(
            ui,
            "No hand-written missions yet — use \"Add mission\" to write one.",
        );
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, m) in cfg.manual.iter_mut().enumerate() {
            let title = format!(
                "[{idx}] {} — {}",
                if m.title.is_empty() {
                    "(untitled)"
                } else {
                    m.title.as_str()
                },
                kind_label(m.kind),
            );
            ui_kit::collapsing_section(ui, ("mis_manual", idx), &title, idx == last_idx, |ui| {
                changed |= manual_mission_editor(ui, idx, m, &existing);
                if ui
                    .button(
                        RichText::new("🗑  Delete mission").color(palette::danger()),
                    )
                    .on_hover_text("Remove this hand-written mission. This cannot be undone.")
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
    }
    // §FRIENDLY_PANEL_PASS transform #7: a hand-written mission bypasses the undo
    // bus, so confirm the delete the in-card 🗑 requested (label read while `cfg`
    // is still borrowed; dispatch happens after it is released).
    let pending_delete = remove_idx.and_then(|idx| {
        cfg.manual.get(idx).map(|m| {
            let label = if m.title.is_empty() {
                format!("mission #{}", idx + 1)
            } else {
                m.title.clone()
            };
            (idx, label)
        })
    });
    if changed {
        on_catalog_edited(state);
    }
    if let Some((idx, label)) = pending_delete {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete mission?".into(),
            body: format!("Remove the manual mission “{label}”."),
            action: ConfirmAction::DeleteManualMission(idx),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: delete the manual mission at `idx`
/// (confirmed payload of [`ModalKind::ConfirmDestructive`]) and run the
/// catalogue-edited bookkeeping. Manual missions bypass the undo bus.
pub(crate) fn delete_manual(state: &mut BuilderState, idx: usize) {
    {
        let Some(cfg) = state.data_catalogs.missions.as_mut() else {
            return;
        };
        if idx >= cfg.manual.len() {
            return;
        }
        cfg.manual.remove(idx);
    }
    on_catalog_edited(state);
}

fn manual_mission_editor(
    ui: &mut Ui,
    idx: usize,
    m: &mut MissionSeed,
    existing: &ExistingRefs,
) -> bool {
    let mut changed = false;
    labeled(
        ui,
        "ID",
        "Stable identifier — keep it unique (schema: id).",
        |ui| {
            let mut id_buf = m.id.to_string();
            if ui.text_edit_singleline(&mut id_buf).changed() {
                m.id = id_buf.into();
                changed = true;
            }
        },
    );
    labeled(ui, "Kind", "Mission archetype (schema: kind).", |ui| {
        ui_kit::combo(format!("m_manual_kind_{idx}"), kind_label(m.kind)).show_ui(ui, |ui| {
            for k in KIND_VARIANTS {
                if ui
                    .selectable_value(&mut m.kind, *k, kind_label(*k))
                    .on_hover_text(format!("key: {}", k.as_slug()))
                    .changed()
                {
                    changed = true;
                }
            }
        });
    });
    labeled(
        ui,
        "Title",
        "Headline shown to players (schema: title).",
        |ui| {
            changed |= ui.text_edit_singleline(&mut m.title).changed();
        },
    );
    labeled(
        ui,
        "Patron",
        "Faction offering the job (schema: patron). Pick one from the sector, or type any id.",
        |ui| {
            changed |= faction_field(ui, format!("m_patron_{idx}"), &mut m.patron, existing);
        },
    );
    labeled(
        ui,
        "Target",
        "Faction the job is aimed at (schema: target). Pick one from the sector, or type any id.",
        |ui| {
            changed |= faction_field(ui, format!("m_target_{idx}"), &mut m.target, existing);
        },
    );
    labeled(
        ui,
        "Primary location",
        "Main place the mission happens, as a system or system/world (schema: primary_location). Pick from the sector, or type one.",
        |ui| {
            changed |= location_field(
                ui,
                format!("m_primary_{idx}"),
                &mut m.primary_location,
                existing,
            );
        },
    );
    labeled(
        ui,
        "Secondary location",
        "Optional follow-up place (schema: secondary_location). Pick from the sector, or type one.",
        |ui| {
            let mut sec = m.secondary_location.clone().unwrap_or_default();
            if location_field(ui, format!("m_secondary_{idx}"), &mut sec, existing) {
                let trimmed = sec.trim();
                m.secondary_location = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Routes",
        "Travel routes this mission touches, as a comma-separated list of route ids (schema: route_ids).",
        |ui| {
            let mut csv = m
                .route_ids
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if ui
                .add(egui::TextEdit::singleline(&mut csv).hint_text("route-1, route-2…"))
                .changed()
            {
                m.route_ids = csv
                    .split(',')
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .map(RouteId::new)
                    .collect();
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Objective",
        "What the players are openly asked to do (schema: public_objective).",
        |ui| {
            changed |= ui.text_edit_multiline(&mut m.public_objective).changed();
        },
    );
    labeled(
        ui,
        "Hidden complication",
        "GM-only twist behind the job (schema: hidden_complication). Leave blank for none.",
        |ui| {
            let mut hidden = m.hidden_complication.clone().unwrap_or_default();
            if ui.text_edit_multiline(&mut hidden).changed() {
                let trimmed = hidden.trim();
                m.hidden_complication = if trimmed.is_empty() {
                    None
                } else {
                    Some(hidden.clone())
                };
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Reward",
        "What the patron offers on success (schema: reward).",
        |ui| {
            changed |= ui.text_edit_singleline(&mut m.reward).changed();
        },
    );
    labeled(
        ui,
        "If ignored",
        "What happens if the players walk away (schema: if_ignored).",
        |ui| {
            changed |= ui.text_edit_multiline(&mut m.if_ignored).changed();
        },
    );
    labeled(
        ui,
        "Scale",
        "How long the mission runs (schema: scale).",
        |ui| {
            ui_kit::combo(format!("m_manual_scale_{idx}"), scale_label(m.scale)).show_ui(
                ui,
                |ui| {
                    for v in SCALE_VARIANTS {
                        if ui
                            .selectable_value(&mut m.scale, *v, scale_label(*v))
                            .on_hover_text(format!("key: {}", v.as_slug()))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                },
            );
        },
    );
    labeled(
        ui,
        "Visibility",
        "How openly the mission is known (schema: visibility).",
        |ui| {
            ui_kit::combo(
                format!("m_manual_vis_{idx}"),
                visibility_label(m.visibility),
            )
            .show_ui(ui, |ui| {
                for v in VISIBILITY_VARIANTS {
                    if ui
                        .selectable_value(&mut m.visibility, *v, visibility_label(*v))
                        .on_hover_text(format!("key: {}", v.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "Weight",
        "Ranking priority — higher sorts nearer the top (schema: weight).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut m.weight).range(0..=1000))
                .changed();
        },
    );
    labeled(
        ui,
        "GM only",
        "Hide this mission under Player edition (schema: gm_only).",
        |ui| {
            changed |= ui.checkbox(&mut m.gm_only, "Hide from players").changed();
        },
    );
    changed
}

fn blank_manual_mission(seq: usize) -> MissionSeed {
    MissionSeed {
        id: format!("mission-manual-{seq:04}").into(),
        kind: MissionKind::Investigate,
        title: String::new(),
        patron: None,
        target: None,
        primary_location: String::new(),
        secondary_location: None,
        route_ids: Vec::new(),
        public_objective: String::new(),
        hidden_complication: None,
        reward: String::new(),
        if_ignored: String::new(),
        scale: MissionScale::OneShot,
        visibility: MissionVisibility::Public,
        weight: 50,
        gm_only: false,
    }
}

// ── pick-from-existing helpers (§M2) ────────────────────────────────────────

/// Sorted, deterministic snapshots of the sector's system / `system/world` /
/// faction ids, used to seed the manual-editor pickers. Built from
/// `state.sector`, which is plain `Vec`s — no `FxMap`/`FxSet` iteration.
struct ExistingRefs {
    /// `"sys"` system anchors and `"sys/world"` per-world anchors, sorted.
    locations: Vec<String>,
    /// Faction ids present in the generated sector, sorted.
    factions: Vec<String>,
}

impl ExistingRefs {
    fn from_state(state: &BuilderState) -> Self {
        let mut locations: BTreeSet<String> = BTreeSet::new();
        for sys in &state.sector.systems {
            locations.insert(sys.id.to_string());
            for w in &sys.worlds {
                locations.insert(format!("{}/{}", sys.id, w.id));
            }
        }
        let mut factions: BTreeSet<String> = BTreeSet::new();
        for f in &state.sector.factions {
            factions.insert(f.id.to_string());
        }
        Self {
            locations: locations.into_iter().collect(),
            factions: factions.into_iter().collect(),
        }
    }
}

/// Free-text id box with an adjacent "pick from sector" dropdown. Typing stays
/// available for ids that don't exist yet; the dropdown turns a known id into a
/// single click instead of remembering and retyping it. Returns `true` when the
/// value changed, preserving the caller's existing `changed` bookkeeping.
fn location_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    value: &mut String,
    refs: &ExistingRefs,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .add(egui::TextEdit::singleline(value).hint_text("sys or sys/world"))
            .changed();
        ui_kit::combo(salt, "pick…").show_ui(ui, |ui| {
            if refs.locations.is_empty() {
                ui_kit::placeholder(ui, "no systems in the sector yet");
            }
            for opt in &refs.locations {
                if ui.selectable_label(value == opt, opt.as_str()).clicked() {
                    *value = opt.clone();
                    changed = true;
                }
            }
        });
    });
    changed
}

/// Free-text faction-id box with a "pick from sector" dropdown plus a clear
/// (`×`) button. Mirrors [`location_field`]; keeps arbitrary ids typeable while
/// offering the sector's known factions in one click.
fn faction_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    slot: &mut Option<FactionId>,
    refs: &ExistingRefs,
) -> bool {
    let mut changed = false;
    let mut buf = slot.as_ref().map(|f| f.to_string()).unwrap_or_default();
    ui.horizontal(|ui| {
        if ui
            .add(egui::TextEdit::singleline(&mut buf).hint_text("faction id"))
            .changed()
        {
            let trimmed = buf.trim();
            *slot = if trimmed.is_empty() {
                None
            } else {
                Some(FactionId::new(trimmed))
            };
            changed = true;
        }
        ui_kit::combo(salt, "pick…").show_ui(ui, |ui| {
            if refs.factions.is_empty() {
                ui_kit::placeholder(ui, "no factions in the sector yet");
            }
            for opt in &refs.factions {
                let selected = slot.as_ref().map(|f| f.as_str()) == Some(opt.as_str());
                if ui.selectable_label(selected, opt.as_str()).clicked() {
                    *slot = Some(FactionId::new(opt.as_str()));
                    changed = true;
                }
            }
        });
        if slot.is_some() && ui.small_button("×").on_hover_text("clear").clicked() {
            *slot = None;
            changed = true;
        }
    });
    changed
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.missions.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("💾  Save missions"))
            .on_hover_text("Write the mission settings and hand-written missions to disk.")
            .clicked()
        {
            if state.config.inputs.missions.is_none() {
                state.config.inputs.missions = Some(DEFAULT_MISSIONS_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Saving missions failed: {e}"
                )));
            }
        }
        if state.config.inputs.missions.is_none() {
            ui.colored_label(Color32::DARK_GRAY, "not saved yet")
                .on_hover_text(format!("Will be written to {DEFAULT_MISSIONS_PATH}."));
        }
    });
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn ensure_missions_catalog(state: &mut BuilderState) {
    if state.data_catalogs.missions.is_none() {
        state.data_catalogs.missions = Some(MissionsConfig::default());
    }
    if state.config.inputs.missions.is_none() {
        state.config.inputs.missions = Some(DEFAULT_MISSIONS_PATH.into());
    }
}

fn ensure_missions_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.missions.is_none() {
        state.data_catalogs.missions = Some(MissionsConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.missions.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_MISSIONS_PATH.into());
    }
    state.mark_validation_dirty();
    if state.missions_auto_recompute {
        state.recompute_missions();
    }
}

fn select_mission(state: &mut BuilderState, m: &MissionSeed) {
    state.selected_mission_id = Some(m.id.to_string());
    state.missions_edit_target = Some(m.id.to_string());
}

/// §M5 — resolve a mission location string into an [`EntityRef`] and call
/// [`BuilderState::focus_entity`]. Accepts either `"sys"` (system anchor) or
/// `"sys/world"` (per-world anchor). When the string is empty the focus
/// falls through to the MAP tab plus the first route id, when one exists.
fn focus_primary_location(state: &mut BuilderState, loc: &str, routes: &[RouteId]) {
    let trimmed = loc.trim();
    if trimmed.is_empty() {
        if let Some(rid) = routes.first() {
            state.focus_entity(EntityRef::Route(rid.clone()));
        } else {
            state.focus_entity(EntityRef::Tab(BuilderTab::Map));
        }
        return;
    }
    if let Some((sys, world)) = trimmed.split_once('/') {
        state.focus_entity(EntityRef::World {
            system: SystemId::new(sys),
            world: WorldId::new(world),
        });
    } else {
        state.focus_entity(EntityRef::System(SystemId::new(trimmed)));
    }
}

fn kind_label(k: MissionKind) -> &'static str {
    match k {
        MissionKind::Investigate => "Investigate",
        MissionKind::Escort => "Escort",
        MissionKind::Sabotage => "Sabotage",
        MissionKind::Diplomacy => "Diplomacy",
        MissionKind::Assassination => "Assassination",
        MissionKind::Recovery => "Recovery",
        MissionKind::Defense => "Defense",
        MissionKind::Exploration => "Exploration",
        _ => "Unknown",
    }
}

fn scale_label(s: MissionScale) -> &'static str {
    match s {
        MissionScale::OneShot => "One-shot",
        MissionScale::ShortArc => "Short arc",
        MissionScale::CampaignArc => "Campaign arc",
        _ => "Unknown",
    }
}

fn visibility_label(v: MissionVisibility) -> &'static str {
    match v {
        MissionVisibility::Public => "Public",
        MissionVisibility::Restricted => "Restricted",
        MissionVisibility::Secret => "Secret",
        MissionVisibility::Misinformation => "Misinformation",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.missions.is_none());
        ensure_missions_catalog(&mut state);
        assert!(state.data_catalogs.missions.is_some());
        assert_eq!(
            state.config.inputs.missions.as_deref(),
            Some(DEFAULT_MISSIONS_PATH)
        );
    }

    #[test]
    fn recompute_missions_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.missions = Some(MissionsConfig::default());
        state.recompute_missions();
        assert!(state.missions_report.is_some());
    }

    #[test]
    fn manual_mission_survives_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = MissionsConfig::default();
        let mut m = blank_manual_mission(0);
        m.title = "Test Op".into();
        m.primary_location = "sys-0001".into();
        cfg.manual.push(m);
        state.data_catalogs.missions = Some(cfg);
        state.recompute_missions();
        let report = state.missions_report.as_ref().unwrap();
        assert!(report
            .missions
            .iter()
            .any(|m| m.id == "mission-manual-0000"));
    }

    #[test]
    fn player_edition_flag_threads_into_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.missions = Some(MissionsConfig::default());
        state.missions_player_edition = true;
        state.recompute_missions();
        let report = state.missions_report.as_ref().unwrap();
        assert!(report.missions.iter().all(|m| !m.gm_only));
    }

    #[test]
    fn focus_primary_location_parses_sys_world() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        focus_primary_location(&mut state, "sys-0001/sys-0001-w1", &[]);
        assert_eq!(
            state.selected_system_id.as_ref().map(|s| s.as_str()),
            Some("sys-0001")
        );
        assert_eq!(
            state.selected_world_id.as_ref().map(|w| w.as_str()),
            Some("sys-0001-w1")
        );
    }

    #[test]
    fn focus_primary_location_parses_sys_only() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        focus_primary_location(&mut state, "sys-0002", &[]);
        assert_eq!(
            state.selected_system_id.as_ref().map(|s| s.as_str()),
            Some("sys-0002")
        );
    }
}
