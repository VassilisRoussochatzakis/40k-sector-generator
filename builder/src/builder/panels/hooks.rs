//! HOOKS tab (§N1 / §N2) — Phase D §HK1..§HK6.
//!
//! §HK1  Ranked hook list filterable by [`HookKind`]. The list is the
//!        cached [`HooksReport`] published by
//!        [`BuilderState::recompute_hooks`]; `derive_with` already orders
//!        by descending dramatic weight, so the panel only re-applies the
//!        kind filter and §HK5 player-edition mask on top.
//! §HK2  Per-hook details: anchor link, situation/stakes/complications,
//!        weight, GM-only flag. Selecting a row populates
//!        [`BuilderState::selected_hook_id`] and
//!        [`BuilderState::hooks_edit_target`] so cross-tab links land here
//!        first-class.
//! §HK3  "+ manual hook" appends a blank [`Hook`] onto
//!        `HooksConfig::manual` with editors for kind / anchor / prose.
//! §HK4  "Regenerate hooks" calls [`BuilderState::recompute_hooks`].
//!        Manual entries survive because [`sectorforge::hooks::derive_with`]
//!        drops any derived hook sharing a manual id and then appends the
//!        whole `cfg.manual` block.
//! §HK5  Player-edition toggle (mirrors `--player`): flips
//!        [`BuilderState::hooks_player_edition`] and re-runs the recompute
//!        so the cached report has `gm_only = true` rows stripped.
//! §HK6  Click-to-highlight anchor on map: anchor link in every row +
//!        "highlight on map" button on the detail card use
//!        [`BuilderState::focus_entity`] to jump to the System / World /
//!        Route the hook references (`EntityRef::Tab(BuilderTab::Map)` as
//!        the route fallback when the lookup misses).
//!
//! The panel never edits derived `hooks_report` rows directly. All
//! mutations land in [`BuilderState::data_catalogs::hooks`] and the
//! recompute pass rewrites the published overlay.

use egui::{Color32, RichText, Ui};

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use sectorforge::hooks::{Hook, HookAnchor, HookKind, HooksConfig};
use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};

use crate::builder::state::{BuilderTab, ConfirmAction, EntityRef, ModalKind};
use crate::builder::BuilderState;

const DEFAULT_HOOKS_PATH: &str = "data/hooks.toml";

/// Every [`HookKind`] in panel-display order. Keep in sync with
/// `src/hooks.rs::HookKind`.
const KIND_VARIANTS: &[HookKind] = &[
    HookKind::CounterInfiltration,
    HookKind::Reconquest,
    HookKind::LostPassage,
    HookKind::ConvoyEscort,
    HookKind::BlockadeRun,
    HookKind::HoldTheLine,
    HookKind::SealedTombs,
    HookKind::CrushUprising,
    HookKind::SealedSystem,
    HookKind::CultPurge,
    HookKind::DiplomaticCrisis,
    HookKind::SuccessionDispute,
    HookKind::StarvingWorld,
    HookKind::LifelineLane,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Hooks");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "Story seeds for your sector — ranked by drama, with your own hooks mixed in. Click a hook to read it or jump to where it happens on the map.",
    );
    ui.separator();

    // §COLUMNS — global controls (regenerate / player-edition) stay full-width
    // on top, then master-detail: the ranked hook list pins to a resizable left
    // rail (filter + rows) and the detail card + manual editor + save fill the
    // rest. Click-to-highlight is preserved on every rail row and the detail
    // card. Replaces the single-column stack whose list and detail scrolled
    // past each other.
    show_header_actions(ui, state);
    ui.separator();

    egui::SidePanel::left("hooks_list")
        .resizable(true)
        .default_width(320.0)
        .width_range(220.0..=520.0)
        .show_inside(ui, |ui| {
            show_filter_row(ui, state);
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| show_hook_list(ui, state));
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

// ── §HK4 / §HK5 header actions ─────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("🔄  Regenerate hooks")
            .on_hover_text("Re-derive hooks from the current sector. Your manual hooks are kept.")
            .clicked()
        {
            ensure_hooks_catalog(state);
            state.recompute_hooks();
        }
        ui.checkbox(&mut state.hooks_auto_recompute, "Auto-refresh on edit")
            .on_hover_text("Regenerate automatically whenever you change a hook.");
        if ui
            .checkbox(&mut state.hooks_player_edition, "Player edition")
            .on_hover_text(
                "Hide GM-only hooks, as in a handout for players (matches the --player export).",
            )
            .changed()
        {
            state.recompute_hooks();
        }
        let total = state
            .hooks_report
            .as_ref()
            .map(|r| r.hooks.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .hooks
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("{total} hook(s)  ·  {manual} of yours"));
        if state.data_catalogs.hooks.is_none() {
            ui.colored_label(Color32::from_rgb(220, 170, 80), "● using built-in defaults")
                .on_hover_text("No saved hooks file yet — built-in defaults are in use until you save.");
        }
    });
}

// ── §HK1 kind filter ───────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Show").strong());
        let label = match state.hooks_filter_kind {
            None => "All kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        ui_kit::combo("hk1_kind", label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.hooks_filter_kind, None, "All kinds");
                for k in KIND_VARIANTS {
                    ui.selectable_value(&mut state.hooks_filter_kind, Some(*k), kind_label(*k))
                        .on_hover_text(format!("schema: {}", k.as_slug()));
                }
            })
            .response
            .on_hover_text("Narrow the list to one kind of hook.");
    });
}

// ── §HK1 / §HK2 ranked list ────────────────────────────────────────────────

fn show_hook_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Ranked by drama").strong());
    let Some(report) = state.hooks_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No hooks yet — press “Regenerate hooks” above to build them from your sector.",
        );
        return;
    };
    let filter = state.hooks_filter_kind;
    let rows: Vec<&Hook> = report
        .hooks
        .iter()
        .filter(|h| filter.is_none_or(|k| h.kind == k))
        .collect();
    if rows.is_empty() {
        ui_kit::placeholder(
            ui,
            "Nothing matches — try “All kinds”, or turn off Player edition to show GM-only hooks.",
        );
        return;
    }
    let selected = state.selected_hook_id.clone();
    // §COLUMNS — compact rail rows: a selectable title line per hook with
    // kind / weight subline + click-to-highlight; full fields and the anchor
    // deep-link live in the detail card on the right.
    for h in &rows {
        let is_selected = selected.as_deref() == Some(h.id.as_str());
        let resp = ui.selectable_label(is_selected, RichText::new(h.title.clone()).strong());
        if resp.clicked() {
            state.selected_hook_id = Some(h.id.to_string());
            state.hooks_edit_target = Some(h.id.to_string());
        }
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                Color32::DARK_GRAY,
                format!("{} · weight {}", kind_label(h.kind), h.weight),
            )
            .on_hover_text(format!(
                "Kind: {} · higher weight ranks higher in the list.",
                kind_label(h.kind)
            ));
            if h.gm_only {
                ui.colored_label(Color32::from_rgb(200, 90, 90), "GM only")
                    .on_hover_text("Hidden from the player edition.");
            }
            if ui
                .small_button("📍 Highlight")
                .on_hover_text("Select this hook and jump to where it happens on the map.")
                .clicked()
            {
                state.selected_hook_id = Some(h.id.to_string());
                state.hooks_edit_target = Some(h.id.to_string());
                focus_anchor(state, &h.anchor);
            }
        });
        ui.separator();
    }
}

// ── §HK2 / §HK6 detail card ────────────────────────────────────────────────

fn show_detail_card(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Hook details").strong());
    let target = state
        .hooks_edit_target
        .clone()
        .or_else(|| state.selected_hook_id.clone());
    let Some(target_id) = target else {
        ui_kit::placeholder(ui, "Pick a hook on the left to read its details here.");
        return;
    };
    let Some(hook) = state
        .hooks_report
        .as_ref()
        .and_then(|r| r.hooks.iter().find(|h| h.id == target_id))
        .cloned()
    else {
        ui_kit::placeholder(
            ui,
            "This hook is no longer in the list — press “Regenerate hooks” to refresh.",
        );
        return;
    };
    labeled(
        ui,
        "Reference",
        "Stable identifier used in saved files and cross-tab links (schema: id).",
        |ui| {
            ui.label(RichText::new(hook.id.clone()).monospace());
        },
    );
    labeled(
        ui,
        "Kind",
        "What sort of mission seed this is (schema: kind).",
        |ui| {
            ui.label(kind_label(hook.kind))
                .on_hover_text(format!("schema: {}", hook.kind.as_slug()));
        },
    );
    labeled(
        ui,
        "Happens at",
        "The place on the map this hook is about (schema: anchor).",
        |ui| show_anchor_link(ui, state, &hook.anchor),
    );
    labeled(
        ui,
        "Drama weight",
        "How prominent this hook is. Higher sorts nearer the top (schema: weight).",
        |ui| {
            ui.label(format!("{}", hook.weight));
        },
    );
    labeled(
        ui,
        "Visibility",
        "Whether players see this hook or only the GM (schema: gm_only).",
        |ui| {
            ui.label(if hook.gm_only {
                "GM only"
            } else {
                "Players + GM"
            });
        },
    );
    labeled(
        ui,
        "Title",
        "One-line name for the hook (schema: title).",
        |ui| {
            ui.label(RichText::new(hook.title.clone()).strong());
        },
    );
    labeled(
        ui,
        "Situation",
        "What is going on (schema: situation).",
        |ui| {
            ui.label(hook.situation.clone());
        },
    );
    labeled(
        ui,
        "Stakes",
        "What is at risk or to be won (schema: stakes).",
        |ui| {
            ui.label(hook.stakes.clone());
        },
    );
    labeled(
        ui,
        "Factions involved",
        "Factions tied to this hook (schema: factions).",
        |ui| {
            if hook.factions.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(
                    hook.factions
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
        },
    );
    labeled(
        ui,
        "Complications",
        "Twists that can be dropped in (schema: complications).",
        |ui| {
            if hook.complications.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(hook.complications.join("\n"));
            }
        },
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("📍 Highlight on map")
            .on_hover_text("Jump to where this hook happens on the map.")
            .clicked()
        {
            focus_anchor(state, &hook.anchor);
        }
    });
}

// ── §HK3 manual entry editor ───────────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Your own hooks").strong());
    // Read-only snapshot of the ids already in the sector so the anchor pickers
    // below can offer "choose from existing" instead of bare text entry. Taken
    // before the `&mut cfg` borrow because the editor closure also needs `cfg`.
    let anchors = existing_anchor_ids(state);
    ensure_hooks_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.hooks.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("➕  Add manual hook")
            .on_hover_text("Add a blank hook you can write yourself.")
            .clicked()
        {
            cfg.manual.push(blank_manual_hook(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Your hooks are kept when you regenerate.",
        );
    });
    if cfg.manual.is_empty() {
        ui_kit::placeholder(
            ui,
            "None yet — press “Add manual hook” to write your own.",
        );
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, h) in cfg.manual.iter_mut().enumerate() {
            ui_kit::collapsing_section(
                ui,
                ("hk_manual", idx),
                &format!(
                    "Hook {}: {}",
                    idx + 1,
                    if h.title.is_empty() {
                        "(untitled)"
                    } else {
                        h.title.as_str()
                    }
                ),
                idx == last_idx,
                |ui| {
                    changed |= manual_hook_editor(ui, idx, h, &anchors);
                    if ui
                        .button(RichText::new("🗑  Delete").color(Color32::from_rgb(200, 90, 90)))
                        .on_hover_text("Remove this hook. This cannot be undone.")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                },
            );
        }
    }
    // §FRIENDLY_PANEL_PASS transform #7: a hand-written hook bypasses the undo bus,
    // so the in-card 🗑 only *requests* the delete; confirm it after the `cfg`
    // borrow ends (the label is read while it is still live).
    let pending_delete = remove_idx.and_then(|idx| {
        cfg.manual.get(idx).map(|h| {
            let label = if h.title.is_empty() {
                format!("hook #{}", idx + 1)
            } else {
                h.title.clone()
            };
            (idx, label)
        })
    });
    if changed {
        on_catalog_edited(state);
    }
    if let Some((idx, label)) = pending_delete {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete hook?".into(),
            body: format!("Remove the manual hook “{label}”."),
            action: ConfirmAction::DeleteManualHook(idx),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: delete the manual hook at `idx` (confirmed
/// payload of [`ModalKind::ConfirmDestructive`]) and run the catalogue-edited
/// bookkeeping. Manual hooks bypass the undo bus.
pub(crate) fn delete_manual(state: &mut BuilderState, idx: usize) {
    {
        let Some(cfg) = state.data_catalogs.hooks.as_mut() else {
            return;
        };
        if idx >= cfg.manual.len() {
            return;
        }
        cfg.manual.remove(idx);
    }
    on_catalog_edited(state);
}

fn manual_hook_editor(ui: &mut Ui, idx: usize, h: &mut Hook, anchors: &AnchorIds) -> bool {
    let mut changed = false;
    labeled(
        ui,
        "Reference",
        "Stable identifier for this hook. Lowercase, no spaces (schema: id).",
        |ui| {
            let mut id_buf = h.id.to_string();
            if ui.text_edit_singleline(&mut id_buf).changed() {
                h.id = id_buf.into();
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Kind",
        "What sort of mission seed this is (schema: kind).",
        |ui| {
            ui_kit::combo(format!("hk_manual_kind_{idx}"), kind_label(h.kind)).show_ui(ui, |ui| {
                for k in KIND_VARIANTS {
                    if ui
                        .selectable_value(&mut h.kind, *k, kind_label(*k))
                        .on_hover_text(format!("schema: {}", k.as_slug()))
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
        "Happens at",
        "Whether this hook is anchored to a system, a world, or a route (schema: anchor).",
        |ui| {
            let mut scope = anchor_scope(&h.anchor);
            ui_kit::combo(
                format!("hk_manual_scope_{idx}"),
                match scope {
                    AnchorScope::System => "A system",
                    AnchorScope::World => "A world",
                    AnchorScope::Route => "A route",
                },
            )
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut scope, AnchorScope::System, "A system")
                    .changed()
                {
                    h.anchor = HookAnchor::System {
                        system_id: SystemId::new(""),
                    };
                    changed = true;
                }
                if ui
                    .selectable_value(&mut scope, AnchorScope::World, "A world")
                    .changed()
                {
                    h.anchor = HookAnchor::World {
                        system_id: SystemId::new(""),
                        world_id: WorldId::new(""),
                    };
                    changed = true;
                }
                if ui
                    .selectable_value(&mut scope, AnchorScope::Route, "A route")
                    .changed()
                {
                    h.anchor = HookAnchor::Route {
                        route_id: RouteId::new(""),
                    };
                    changed = true;
                }
            });
        },
    );
    match &mut h.anchor {
        HookAnchor::System { system_id } => {
            labeled(
                ui,
                "System",
                "Which system the hook is in. Pick an existing one, or type a custom id (schema: anchor.system_id).",
                |ui| {
                    let mut s = system_id.to_string();
                    if id_or_custom_combo(ui, format!("hk_sys_{idx}"), &mut s, &anchors.systems) {
                        *system_id = SystemId::new(s.as_str());
                        changed = true;
                    }
                },
            );
        }
        HookAnchor::World {
            system_id,
            world_id,
        } => {
            labeled(
                ui,
                "System",
                "Which system the world is in (schema: anchor.system_id).",
                |ui| {
                    let mut s = system_id.to_string();
                    if id_or_custom_combo(ui, format!("hk_wsys_{idx}"), &mut s, &anchors.systems) {
                        *system_id = SystemId::new(s.as_str());
                        changed = true;
                    }
                },
            );
            labeled(
                ui,
                "World",
                "Which world the hook is on (schema: anchor.world_id).",
                |ui| {
                    let mut w = world_id.to_string();
                    if id_or_custom_combo(ui, format!("hk_world_{idx}"), &mut w, &anchors.worlds) {
                        *world_id = WorldId::new(w.as_str());
                        changed = true;
                    }
                },
            );
        }
        HookAnchor::Route { route_id } => {
            labeled(
                ui,
                "Route",
                "Which route the hook is along (schema: anchor.route_id).",
                |ui| {
                    let mut r = route_id.to_string();
                    if id_or_custom_combo(ui, format!("hk_route_{idx}"), &mut r, &anchors.routes) {
                        *route_id = RouteId::new(r.as_str());
                        changed = true;
                    }
                },
            );
        }
        _ => {}
    }
    labeled(
        ui,
        "Title",
        "One-line name for the hook (schema: title).",
        |ui| {
            changed |= ui.text_edit_singleline(&mut h.title).changed();
        },
    );
    labeled(
        ui,
        "Situation",
        "What is going on (schema: situation).",
        |ui| {
            changed |= ui.text_edit_multiline(&mut h.situation).changed();
        },
    );
    labeled(
        ui,
        "Stakes",
        "What is at risk or to be won (schema: stakes).",
        |ui| {
            changed |= ui.text_edit_multiline(&mut h.stakes).changed();
        },
    );
    labeled(
        ui,
        "Drama weight",
        "How prominent this hook is. Higher sorts nearer the top (schema: weight).",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut h.weight).range(0..=200))
                .changed();
        },
    );
    labeled(
        ui,
        "Visibility",
        "Tick to hide this hook from the player edition (schema: gm_only).",
        |ui| {
            changed |= ui.checkbox(&mut h.gm_only, "GM only").changed();
        },
    );
    labeled(
        ui,
        "Factions involved",
        "Faction ids tied to this hook, separated by commas (schema: factions).",
        |ui| {
            let mut csv = h
                .factions
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if ui
                .add(egui::TextEdit::singleline(&mut csv).hint_text("e.g. imperial, ork"))
                .changed()
            {
                h.factions = csv
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(FactionId::new)
                    .collect();
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Complications",
        "Optional twists, one per line (schema: complications).",
        |ui| {
            let mut comp = h.complications.join("\n");
            if ui
                .add(egui::TextEdit::multiline(&mut comp).hint_text("one per line"))
                .changed()
            {
                h.complications = comp
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                changed = true;
            }
        },
    );
    changed
}

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Drama weight", "Happens at") while the tooltip
/// names the underlying field plus a plain-language note, so the schema mapping
/// stays discoverable. Friendlier replacement for the old bare `egui::Grid`
/// whose row labels *were* the raw field names.
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

/// Sorted, de-duplicated ids already present in the sector, used to seed the
/// "choose from existing" anchor pickers. A read-only snapshot taken before the
/// `&mut data_catalogs.hooks` borrow, so the editor closure can hold both.
struct AnchorIds {
    systems: Vec<String>,
    worlds: Vec<String>,
    routes: Vec<String>,
}

fn existing_anchor_ids(state: &BuilderState) -> AnchorIds {
    let mut systems: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut worlds: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut routes: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for sys in &state.sector.systems {
        systems.insert(sys.id.to_string());
        for w in &sys.worlds {
            worlds.insert(w.id.to_string());
        }
    }
    for r in &state.sector.routes {
        routes.insert(r.id.to_string());
    }
    AnchorIds {
        systems: systems.into_iter().collect(),
        worlds: worlds.into_iter().collect(),
        routes: routes.into_iter().collect(),
    }
}

/// Dropdown over the ids already in the sector, plus an in-popup custom row.
/// Friendlier than free-typing a raw id: the anchor is usually a place that
/// already exists, so it becomes one click instead of recalling the exact
/// string. The custom row keeps brand-new ids reachable. Returns `true` when
/// `value` changed.
fn id_or_custom_combo(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    value: &mut String,
    options: &[String],
) -> bool {
    let before = value.clone();
    ui_kit::combo(
        salt,
        if value.is_empty() {
            "(choose…)".to_owned()
        } else {
            value.clone()
        },
    )
    .show_ui(ui, |ui| {
        for opt in options {
            if ui
                .selectable_label(value.as_str() == opt.as_str(), opt.as_str())
                .clicked()
            {
                *value = opt.clone();
            }
        }
        ui.separator();
        ui.label(RichText::new("custom…").small().color(Color32::DARK_GRAY));
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text("type an id")
                .desired_width(160.0),
        );
    });
    *value != before
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorScope {
    System,
    World,
    Route,
}

fn anchor_scope(a: &HookAnchor) -> AnchorScope {
    match a {
        HookAnchor::System { .. } => AnchorScope::System,
        HookAnchor::World { .. } => AnchorScope::World,
        HookAnchor::Route { .. } => AnchorScope::Route,
        _ => AnchorScope::System,
    }
}

fn blank_manual_hook(seq: usize) -> Hook {
    Hook {
        id: format!("hook-manual-{seq:04}").into(),
        kind: HookKind::DiplomaticCrisis,
        anchor: HookAnchor::System {
            system_id: SystemId::new(""),
        },
        title: String::new(),
        situation: String::new(),
        stakes: String::new(),
        factions: Vec::new(),
        complications: Vec::new(),
        weight: 50,
        gm_only: false,
    }
}

// ── §HK6 anchor link / highlight ───────────────────────────────────────────

fn show_anchor_link(ui: &mut Ui, state: &mut BuilderState, a: &HookAnchor) {
    let label = anchor_label(a);
    if ui.link(label).clicked() {
        focus_anchor(state, a);
    }
}

fn focus_anchor(state: &mut BuilderState, a: &HookAnchor) {
    let target = match a {
        HookAnchor::System { system_id } => {
            if system_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::System(system_id.clone())
            }
        }
        HookAnchor::World {
            system_id,
            world_id,
        } => {
            if system_id.as_ref().is_empty() || world_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::World {
                    system: system_id.clone(),
                    world: world_id.clone(),
                }
            }
        }
        HookAnchor::Route { route_id } => {
            if route_id.as_ref().is_empty() {
                EntityRef::Tab(BuilderTab::Map)
            } else {
                EntityRef::Route(route_id.clone())
            }
        }
        _ => EntityRef::Tab(BuilderTab::Map),
    };
    state.focus_entity(target);
}

fn anchor_label(a: &HookAnchor) -> String {
    match a {
        HookAnchor::System { system_id } => {
            if system_id.as_ref().is_empty() {
                "(no system yet)".into()
            } else {
                format!("System {system_id}")
            }
        }
        HookAnchor::World {
            system_id,
            world_id,
        } => {
            if system_id.as_ref().is_empty() || world_id.as_ref().is_empty() {
                "(no world yet)".into()
            } else {
                format!("World {world_id} in {system_id}")
            }
        }
        HookAnchor::Route { route_id } => {
            if route_id.as_ref().is_empty() {
                "(no route yet)".into()
            } else {
                format!("Route {route_id}")
            }
        }
        _ => "(unknown place)".into(),
    }
}

fn kind_label(k: HookKind) -> &'static str {
    match k {
        HookKind::CounterInfiltration => "Counter-infiltration",
        HookKind::Reconquest => "Reconquest",
        HookKind::LostPassage => "Lost passage",
        HookKind::ConvoyEscort => "Convoy escort",
        HookKind::BlockadeRun => "Blockade run",
        HookKind::HoldTheLine => "Hold the line",
        HookKind::SealedTombs => "Sealed tombs",
        HookKind::CrushUprising => "Crush uprising",
        HookKind::SealedSystem => "Sealed system",
        HookKind::CultPurge => "Cult purge",
        HookKind::DiplomaticCrisis => "Diplomatic crisis",
        HookKind::SuccessionDispute => "Succession dispute",
        HookKind::StarvingWorld => "Starving world",
        HookKind::LifelineLane => "Lifeline lane",
        _ => "Unknown",
    }
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.hooks.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("💾  Save hooks"))
            .on_hover_text("Write all hooks back to the project's hooks file.")
            .clicked()
        {
            if state.config.inputs.hooks.is_none() {
                state.config.inputs.hooks = Some(DEFAULT_HOOKS_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Could not save hooks: {e}"
                )));
            }
        }
        match state.config.inputs.hooks.clone() {
            Some(path) => {
                ui.colored_label(Color32::DARK_GRAY, format!("file: {path}"));
            }
            None => {
                ui.colored_label(Color32::DARK_GRAY, "not saved yet")
                    .on_hover_text(format!("Will be written to {DEFAULT_HOOKS_PATH}."));
            }
        }
    });
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn ensure_hooks_catalog(state: &mut BuilderState) {
    if state.data_catalogs.hooks.is_none() {
        state.data_catalogs.hooks = Some(HooksConfig::default());
    }
    if state.config.inputs.hooks.is_none() {
        state.config.inputs.hooks = Some(DEFAULT_HOOKS_PATH.into());
    }
}

fn ensure_hooks_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.hooks.is_none() {
        state.data_catalogs.hooks = Some(HooksConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.hooks.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_HOOKS_PATH.into());
    }
    state.mark_validation_dirty();
    if state.hooks_auto_recompute {
        state.recompute_hooks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.hooks.is_none());
        ensure_hooks_catalog(&mut state);
        assert!(state.data_catalogs.hooks.is_some());
        assert_eq!(
            state.config.inputs.hooks.as_deref(),
            Some(DEFAULT_HOOKS_PATH)
        );
    }

    #[test]
    fn recompute_hooks_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.hooks = Some(HooksConfig::default());
        state.recompute_hooks();
        assert!(state.hooks_report.is_some());
    }

    #[test]
    fn manual_hook_survives_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = HooksConfig::default();
        let mut h = blank_manual_hook(0);
        h.title = "Test".into();
        cfg.manual.push(h);
        state.data_catalogs.hooks = Some(cfg);
        state.recompute_hooks();
        let report = state.hooks_report.as_ref().unwrap();
        assert!(report.hooks.iter().any(|h| h.id == "hook-manual-0000"));
    }

    #[test]
    fn player_edition_strips_gm_only_hooks() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = HooksConfig::default();
        let mut h = blank_manual_hook(0);
        h.title = "Secret".into();
        h.gm_only = true;
        cfg.manual.push(h);
        state.data_catalogs.hooks = Some(cfg);
        state.hooks_player_edition = true;
        state.recompute_hooks();
        let report = state.hooks_report.as_ref().unwrap();
        assert!(!report.hooks.iter().any(|h| h.id == "hook-manual-0000"));
    }
}
