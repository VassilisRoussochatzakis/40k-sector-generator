//! FACTIONS tab — §F1..§F7 faction roster editor.
//!
//! Edits the in-memory `data_catalogs.factions` mirror of `factions.toml`. F6
//! writes the modified roster back via [`super::super::project_io::save_project`].
//! Style overrides and legend-visibility flags persist as optional fields on
//! [`FactionDef`] so existing factions.toml files round-trip cleanly.
//!
//! Inspector invariant: every faction property has exactly one input widget.
//! `kind` / `default_disposition` use [`editable_combo`] (preset list + a
//! single in-popup custom row — never a loose text box beside the combo);
//! `style_fill` / `style_accent` use [`color_override`] (swatch picker only,
//! hex read-only). This prevents a stray edit from silently retyping `kind`
//! and, via `top_faction_id()` / `subfaction_id()`, spawning a phantom
//! top-faction group in the roster.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use sectorforge::faction_style::{
    faction_style_rgb_with_overrides, parse_border, parse_hex_rgb, rgb_to_hex, FactionBorder,
};
use sectorforge::factions::{display_name_from_id, FactionDef, FactionsFile};
use sectorforge::ids::FactionId;
use sectorforge::worlds::{Government, NotableFeature, WorldType};
use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;

use crate::builder::state::{BuilderTab, EntityRef, ModalKind};
use crate::builder::BuilderState;

const KNOWN_KINDS: &[&str] = &[
    "imperial",
    "imperial_guard",
    "imperial_knight",
    "adepta_sororitas",
    "adeptus_astartes",
    "deathwatch",
    "grey_knights",
    "talons_of_the_emperor",
    "inquisition",
    "collegia_titanica",
    "mechanicus",
    "dark_mechanicum",
    "chaos_space_marine",
    "chaos_knight",
    "traitor_guard",
    "traitor_titan_legion",
    "daemon",
    "cult",
    "genestealer_cult",
    "tau",
    "aeldari",
    "drukhari",
    "harlequin",
    "leagues_of_votann",
    "ork",
    "tyranid",
    "necron",
    "minor_xenos",
    "xenos",
    "merchant",
    "criminal",
    "rebel",
];

const KNOWN_DISPOSITIONS: &[&str] = &[
    "lawful",
    "insular",
    "secretive",
    "opportunistic",
    "hostile",
    "zealous",
];

const KNOWN_BORDERS: &[(&str, FactionBorder)] = &[
    ("clean", FactionBorder::Clean),
    ("jagged", FactionBorder::Jagged),
    ("dotted", FactionBorder::Dotted),
    ("thin", FactionBorder::Thin),
];

const DEFAULT_FACTIONS_REL: &str = "data/factions/factions.toml";

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Factions");
    ui.label(
        RichText::new("Who lives in your sector — their look, behaviour, and where they appear.")
            .color(Color32::DARK_GRAY),
    );
    ui.add_space(4.0);

    if state.data_catalogs.factions.is_none() {
        show_empty_catalog(ui, state);
        return;
    }

    show_summary(ui, state);
    ui.separator();
    show_filter_bar(ui, state);
    ui.separator();

    egui::SidePanel::left("factions_roster_panel")
        .resizable(true)
        .default_width(280.0)
        .show_inside(ui, |ui| show_roster_list(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| {
        if let Some(idx) = selected_index(state) {
            show_inspector(ui, state, idx);
        } else {
            ui_kit::placeholder(ui, "Pick a faction on the left to edit it.");
        }
    });
}

// ── empty / scaffold ────────────────────────────────────────────────────────

fn show_empty_catalog(ui: &mut Ui, state: &mut BuilderState) {
    ui_kit::placeholder(
        ui,
        "No factions yet. Create a roster to start adding factions.",
    );
    ui.add_space(6.0);
    if ui
        .button("➕  Create faction roster")
        .on_hover_text("Starts a new, empty factions.toml you can add to")
        .clicked()
    {
        state.data_catalogs.factions = Some(FactionsFile {
            factions: Vec::new(),
        });
        if state.config.inputs.factions.is_none() {
            state.config.inputs.factions = Some(DEFAULT_FACTIONS_REL.to_string());
        }
        state.dirty = true;
        state.dirty_files.insert(DEFAULT_FACTIONS_REL.to_string());
    }
}

// ── summary + filter bar ────────────────────────────────────────────────────

fn show_summary(ui: &mut Ui, state: &mut BuilderState) {
    let file = state.data_catalogs.factions.as_ref().expect("checked");
    let count = file.factions.len();
    let groups = count_groups(file);
    let path = state
        .config
        .inputs
        .factions
        .clone()
        .unwrap_or_else(|| "(unset)".into());
    let path_dirty = state.dirty_files.contains(path.as_str())
        || state.dirty_files.contains(DEFAULT_FACTIONS_REL);

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("{count} faction(s)")).strong());
        ui.label(
            RichText::new(format!(
                "· {} group(s) · {} sub-group(s)",
                groups.top, groups.sub
            ))
            .color(Color32::DARK_GRAY),
        );
        if path_dirty {
            ui.colored_label(Color32::from_rgb(240, 200, 90), "●  unsaved changes");
        }
    });
    ui.label(
        RichText::new(format!("file: {path}"))
            .small()
            .color(Color32::DARK_GRAY),
    );

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("💾  Save")
            .on_hover_text("Write all changes back to factions.toml")
            .clicked()
        {
            save_factions_only(state);
        }
        if ui
            .button("➕  Add faction")
            .on_hover_text("Add a new faction and select it")
            .clicked()
        {
            add_new_row(state);
        }
        if let Some(id) = state.selected_faction_id.clone() {
            if ui
                .button("⧉  Duplicate")
                .on_hover_text("Copy the selected faction with a new id")
                .clicked()
            {
                duplicate_row(state, &id);
            }
            if ui
                .button("🗑  Delete")
                .on_hover_text("Remove the selected faction from the roster")
                .clicked()
            {
                let name = faction_name_for(state, &id);
                state.modal = Some(ModalKind::ConfirmDeleteFaction {
                    id: id.clone(),
                    name,
                });
            }
        }
    });
}

fn show_filter_bar(ui: &mut Ui, _state: &mut BuilderState) {
    let filter_id = egui::Id::new("factions_filter");
    let kind_id = egui::Id::new("factions_kind_filter");
    let mut filter: String = ui.data_mut(|d| {
        d.get_temp_mut_or::<String>(filter_id, String::new())
            .clone()
    });
    let mut kind_filter: String =
        ui.data_mut(|d| d.get_temp_mut_or::<String>(kind_id, String::new()).clone());
    ui.horizontal_wrapped(|ui| {
        ui.label("Filter:");
        if ui
            .add(
                egui::TextEdit::singleline(&mut filter)
                    .hint_text("name, id, or type…")
                    .desired_width(180.0),
            )
            .changed()
        {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        ui.label("Type:");
        ui_kit::combo(
            "factions_kind_filter_combo",
            if kind_filter.is_empty() {
                "(any)"
            } else {
                kind_filter.as_str()
            },
        )
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(kind_filter.is_empty(), "(any)")
                .clicked()
            {
                kind_filter.clear();
                ui.data_mut(|d| d.insert_temp(kind_id, kind_filter.clone()));
            }
            for k in KNOWN_KINDS {
                if ui.selectable_label(kind_filter == *k, *k).clicked() {
                    kind_filter = (*k).into();
                    ui.data_mut(|d| d.insert_temp(kind_id, kind_filter.clone()));
                }
            }
        });
    });
}

// ── roster list (left pane) ─────────────────────────────────────────────────

fn show_roster_list(ui: &mut Ui, state: &mut BuilderState) {
    let filter: String = ui
        .data_mut(|d| d.get_temp::<String>(egui::Id::new("factions_filter")))
        .unwrap_or_default();
    let kind_filter: String = ui
        .data_mut(|d| d.get_temp::<String>(egui::Id::new("factions_kind_filter")))
        .unwrap_or_default();
    let needle = filter.to_lowercase();

    let Some(file) = state.data_catalogs.factions.as_ref() else {
        return;
    };
    // Group the flat list by top_faction_id() > subfaction_id().
    let mut groups: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, Vec<usize>>,
    > = std::collections::BTreeMap::new();
    for (i, f) in file.factions.iter().enumerate() {
        if !kind_filter.is_empty() && f.kind != kind_filter {
            continue;
        }
        if !needle.is_empty()
            && !f.id.as_str().to_lowercase().contains(&needle)
            && !f.name.to_lowercase().contains(&needle)
            && !f.kind.to_lowercase().contains(&needle)
        {
            continue;
        }
        let top = f.top_faction_id().to_string();
        let sub = f.subfaction_id().to_string();
        groups
            .entry(top)
            .or_default()
            .entry(sub)
            .or_default()
            .push(i);
    }

    let selected = state.selected_faction_id.clone();
    let mut new_selection: Option<sectorforge::ids::FactionId> = None;
    let mut confirm_delete: Option<(sectorforge::ids::FactionId, String)> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (top_id, subs) in &groups {
            // Resolve the group label from the first member's display field
            // (`faction_name`), matching what the generated legend renders. Fall
            // back to the id-derived prettyname only when the override is unset.
            let top_name = subs
                .values()
                .flatten()
                .next()
                .map(|&i| file.factions[i].top_faction_name())
                .unwrap_or_else(|| display_name_from_id(top_id).into_owned());
            ui_kit::collapsing_section(
                ui,
                format!("fac_top_{top_id}"),
                &format!("{top_name} ({top_id})"),
                true,
                |ui| {
                    for (sub_id, rows) in subs {
                        // Same as the top group: prefer the member's `subfaction_name`
                        // override, fall back to the id-derived name.
                        let sub_name = rows
                            .first()
                            .map(|&i| file.factions[i].subfaction_name())
                            .unwrap_or_else(|| display_name_from_id(sub_id).into_owned());
                        ui_kit::collapsing_section(
                            ui,
                            format!("fac_sub_{top_id}_{sub_id}"),
                            &format!("{sub_name} — {sub_id} ({})", rows.len()),
                            true,
                            |ui| {
                                for &i in rows {
                                    let f = &file.factions[i];
                                    let is_selected = selected.as_ref() == Some(&f.id);
                                    let dimmed = f.legend_visible == Some(false);
                                    let name_color = if dimmed {
                                        palette::chrome_text_dim()
                                    } else {
                                        palette::chrome_text()
                                    };
                                    // §BEAUTY hero: each roster entry is a custom-painted
                                    // selectable plate (hover lift + brass selection bar +
                                    // accent glow) rather than a stock selectable_label.
                                    let (resp, del_clicked) = card::selectable_plate(
                                        ui,
                                        ("fac_row", &f.id),
                                        is_selected,
                                        |ui| {
                                            palette::draw_faction_swatch(ui, resolve_style(f));
                                            ui.label(
                                                RichText::new(f.name.as_str())
                                                    .color(name_color)
                                                    .strong(),
                                            );
                                            ui.label(
                                                RichText::new(format!("({})", f.id))
                                                    .color(palette::chrome_text_dim())
                                                    .small(),
                                            );
                                            let rem = ui.available_width();
                                            ui.add_space((rem - 22.0).max(0.0));
                                            ui.small_button("×")
                                                .on_hover_text(
                                                    "Remove this faction from the roster",
                                                )
                                                .clicked()
                                        },
                                    );
                                    if del_clicked {
                                        confirm_delete = Some((f.id.clone(), f.name.clone()));
                                    } else if resp.clicked() {
                                        new_selection = Some(f.id.clone());
                                    }
                                }
                            },
                        );
                    }
                },
            );
        }
        if groups.is_empty() {
            ui_kit::placeholder(ui, "No factions match your filter.");
        }
    });
    if let Some(id) = new_selection {
        state.selected_faction_id = Some(id);
    }
    if let Some((id, name)) = confirm_delete {
        state.modal = Some(ModalKind::ConfirmDeleteFaction { id, name });
    }
}

// ── inspector (right pane) ──────────────────────────────────────────────────

fn show_inspector(ui: &mut Ui, state: &mut BuilderState, idx: usize) {
    let original = {
        let file = state.data_catalogs.factions.as_ref().expect("checked");
        file.factions[idx].clone()
    };
    let mut draft = original.clone();
    let (existing_top_ids, existing_sub_ids) = {
        let file = state.data_catalogs.factions.as_ref().expect("checked");
        existing_group_ids(file, &original.id)
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // §COLUMNS — heading + style-preview chip stay full-width on top, then
            // the §F1..§F7 section boxes flow across two responsive columns (RC-2)
            // that collapse to one when the central pane is narrow. The sections
            // edit `&mut draft` sequentially (one reborrow at a time per column),
            // mirroring the system.rs hand-assigned split. `draft != original` is
            // diffed once after the columns so the commit block is unchanged.
            ui.horizontal_wrapped(|ui| {
                ui.heading(draft.name.clone());
                ui.label(RichText::new(format!("id: {}", draft.id)).color(Color32::DARK_GRAY));
            });
            show_style_preview(ui, &draft);
            ui.add_space(4.0);

            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left: identity + hierarchy + preferences.
                    let left = &mut cols[0];
                    ui_kit::collapsing_section(left, "fac_identity", "Identity", true, |ui| {
                        identity_grid(ui, &mut draft)
                    });
                    left.add_space(4.0);
                    ui_kit::collapsing_section(left, "fac_hierarchy", "Grouping", true, |ui| {
                        hierarchy_grid(ui, &mut draft, &existing_top_ids, &existing_sub_ids)
                    });
                    left.add_space(4.0);
                    ui_kit::collapsing_section(
                        left,
                        "fac_preferences",
                        "Spawn preferences",
                        false,
                        |ui| {
                            preferred_picker_world_types(ui, &mut draft);
                            preferred_picker_governments(ui, &mut draft);
                            preferred_picker_features(ui, &mut draft);
                        },
                    );
                }
                {
                    // Right — or the same single column when collapsed to one:
                    // style override + presence deep-link + legend visibility.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    ui_kit::collapsing_section(
                        right,
                        "fac_style_override",
                        "Appearance",
                        true,
                        |ui| style_overrides(ui, &mut draft),
                    );
                    right.add_space(4.0);
                    ui_kit::collapsing_section(right, "fac_presence", "Presence", false, |ui| {
                        presence_link(ui, state, &draft)
                    });
                    right.add_space(4.0);
                    ui_kit::collapsing_section(right, "fac_legend", "Map legend", false, |ui| {
                        legend_visibility(ui, &mut draft)
                    });
                }
            });
        });

    if draft != original {
        let id = draft.id.clone();
        let file = state.data_catalogs.factions.as_mut().expect("checked");
        file.factions[idx] = draft;
        state.dirty = true;
        let rel = state
            .config
            .inputs
            .factions
            .clone()
            .unwrap_or_else(|| DEFAULT_FACTIONS_REL.to_string());
        state.dirty_files.insert(rel);
        state.selected_faction_id = Some(id);
    }
}

/// Aligned label-left / control-right row with a hover tooltip. The visible
/// label reads in human terms ("Type", "Spawn weight") while the tooltip names
/// the underlying TOML field plus a plain-language note, so power users keep the
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

fn identity_grid(ui: &mut Ui, draft: &mut FactionDef) {
    labeled(
        ui,
        "ID",
        "Unique identifier (schema: id). Lowercase, no spaces — used by routes, presence, and saved files.",
        |ui| {
            let mut id_buf = draft.id.to_string();
            if ui.text_edit_singleline(&mut id_buf).changed() {
                draft.id = FactionId::new(id_buf.trim());
            }
        },
    );
    labeled(
        ui,
        "Name",
        "Display name shown in lists and on the map legend (schema: name).",
        |ui| {
            ui.text_edit_singleline(&mut draft.name);
        },
    );
    labeled(
        ui,
        "Type",
        "Faction archetype (schema: kind). Sets the default colour, glyph, and behaviour.",
        |ui| editable_combo(ui, "faction_kind_combo", &mut draft.kind, KNOWN_KINDS),
    );
    labeled(
        ui,
        "Disposition",
        "Default stance toward other factions (schema: default_disposition).",
        |ui| {
            editable_combo(
                ui,
                "faction_disposition_combo",
                &mut draft.default_disposition,
                KNOWN_DISPOSITIONS,
            )
        },
    );
    labeled(
        ui,
        "Spawn weight",
        "How often the generator places this faction (schema: weight). Higher = more common.",
        |ui| {
            ui.add(
                egui::DragValue::new(&mut draft.weight)
                    .speed(0.1)
                    .range(0.0..=100.0)
                    .max_decimals(2),
            );
        },
    );
}

fn hierarchy_grid(ui: &mut Ui, draft: &mut FactionDef, top_ids: &[String], sub_ids: &[String]) {
    ui_kit::placeholder(
        ui,
        "Optional. Nest this faction under a parent and sub-group so the map legend can roll them together. Leave blank for a stand-alone faction.",
    );
    ui.add_space(4.0);
    labeled(
        ui,
        "Parent faction",
        "The larger faction this belongs to (schema: faction). Pick an existing group, or leave blank for top-level.",
        |ui| id_combo(ui, "fac_top_id", &mut draft.faction, top_ids),
    );
    labeled(
        ui,
        "Parent name",
        "Display name for the parent group on the legend (schema: faction_name).",
        |ui| {
            optional_text(ui, "fac_top_name", &mut draft.faction_name, |s| {
                s.to_string()
            })
        },
    );
    labeled(
        ui,
        "Sub-group",
        "The mid-level group within the parent (schema: subfaction).",
        |ui| id_combo(ui, "fac_sub_id", &mut draft.subfaction, sub_ids),
    );
    labeled(
        ui,
        "Sub-group name",
        "Display name for the sub-group on the legend (schema: subfaction_name).",
        |ui| {
            optional_text(ui, "fac_sub_name", &mut draft.subfaction_name, |s| {
                s.to_string()
            })
        },
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Shows in the roster as:  {}  ›  {}  ›  {}",
            draft.top_faction_name(),
            draft.subfaction_name(),
            draft.name
        ))
        .color(Color32::DARK_GRAY),
    );
}

fn optional_text<T, F>(ui: &mut Ui, salt: &str, slot: &mut Option<T>, ctor: F)
where
    T: ToString + Clone,
    F: Fn(&str) -> T,
{
    let mut buf = slot.as_ref().map(|v| v.to_string()).unwrap_or_default();
    let id = egui::Id::new(salt);
    ui.horizontal(|ui| {
        let resp = ui.add(egui::TextEdit::singleline(&mut buf).id(id));
        if resp.changed() {
            let trimmed = buf.trim();
            *slot = if trimmed.is_empty() {
                None
            } else {
                Some(ctor(trimmed))
            };
        }
        if slot.is_some() && ui.small_button("×").clicked() {
            *slot = None;
        }
    });
}

/// Single-surface editable combo (§F1). The trigger button shows the current
/// value; the popup lists the known `presets` plus one "custom…" text row for
/// values outside that list. Collapsing the picker and the free-text entry into
/// one widget removes the old combo-plus-always-visible-text-box pairing, where
/// a stray edit to the loose box could silently retype `kind` /
/// `default_disposition`. Because `top_faction_id()` / `subfaction_id()` fall
/// back to `kind`, such a stray edit would spawn a brand-new top-faction group
/// in the roster. Custom values stay reachable, but only from inside the opened
/// popup, so they can no longer be entered by accident.
fn editable_combo(ui: &mut Ui, salt: &str, value: &mut String, presets: &[&str]) {
    ui_kit::combo(
        salt,
        if value.is_empty() {
            "(unset)".to_owned()
        } else {
            value.clone()
        },
    )
    .show_ui(ui, |ui| {
        for p in presets {
            if ui.selectable_label(value == *p, *p).clicked() {
                *value = (*p).into();
            }
        }
        ui.separator();
        ui.label(RichText::new("custom…").small().color(Color32::DARK_GRAY));
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text("non-preset value")
                .desired_width(160.0),
        );
    });
}

// ── preferences (multi-pickers) ─────────────────────────────────────────────

fn preferred_picker_world_types(ui: &mut Ui, draft: &mut FactionDef) {
    pick_multi(
        ui,
        "preferred_world_types",
        "Preferred world types",
        &mut draft.preferred_world_types,
        WorldType::VARIANTS
            .iter()
            .map(|v| (format!("{v:?}"), v.display_name().to_string()))
            .collect(),
    );
}

fn preferred_picker_governments(ui: &mut Ui, draft: &mut FactionDef) {
    pick_multi(
        ui,
        "preferred_governments",
        "Preferred governments",
        &mut draft.preferred_governments,
        Government::VARIANTS
            .iter()
            .map(|v| (format!("{v:?}"), v.display_name().to_string()))
            .collect(),
    );
}

fn preferred_picker_features(ui: &mut Ui, draft: &mut FactionDef) {
    pick_multi(
        ui,
        "preferred_notable_features",
        "Preferred notable features",
        &mut draft.preferred_notable_features,
        NotableFeature::VARIANTS
            .iter()
            .map(|v| (format!("{v:?}"), v.display_name().to_string()))
            .collect(),
    );
}

fn pick_multi(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    bucket: &mut Vec<String>,
    options: Vec<(String, String)>,
) {
    ui.label(RichText::new(label).strong());
    ui.indent(format!("{salt}_indent"), |ui| {
        if bucket.is_empty() {
            ui_kit::placeholder(ui, "none yet — pick from the list below");
        }
        let mut remove: Option<usize> = None;
        for (i, current) in bucket.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(current.as_str());
                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            bucket.remove(i);
        }

        let filter_id = egui::Id::new(format!("{salt}_filter"));
        let mut filter: String = ui.data_mut(|d| {
            d.get_temp_mut_or::<String>(filter_id, String::new())
                .clone()
        });
        if ui
            .add(egui::TextEdit::singleline(&mut filter).hint_text("filter options…"))
            .changed()
        {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        let needle = filter.to_lowercase();
        let already: BTreeSet<String> = bucket.iter().cloned().collect();
        egui::ScrollArea::vertical()
            .id_salt(format!("{salt}_scroll"))
            .max_height(140.0)
            .show(ui, |ui| {
                for (key, display) in &options {
                    if !needle.is_empty()
                        && !display.to_lowercase().contains(&needle)
                        && !key.to_lowercase().contains(&needle)
                    {
                        continue;
                    }
                    if already.contains(key) {
                        continue;
                    }
                    if ui
                        .button(display.as_str())
                        .on_hover_text(format!("id: {key}"))
                        .clicked()
                    {
                        bucket.push(key.clone());
                    }
                }
            });
    });
    ui.separator();
}

// ── style override + recompute ──────────────────────────────────────────────

fn show_style_preview(ui: &mut Ui, draft: &FactionDef) {
    let style = resolve_style(draft);
    ui.horizontal(|ui| {
        palette::draw_faction_style_rgb_preview(ui, style);
        ui.label(
            RichText::new(format!(
                "Glyph {} · Fill {} · Accent {} · Border {:?}",
                style.glyph,
                rgb_to_hex(style.fill),
                rgb_to_hex(style.accent),
                style.border
            ))
            .color(Color32::DARK_GRAY),
        );
    });
}

fn style_overrides(ui: &mut Ui, draft: &mut FactionDef) {
    let derived = faction_style_rgb_with_overrides(
        &draft.kind,
        draft.id.as_str(),
        &draft.default_disposition,
        None,
        None,
        None,
        None,
    );

    labeled(
        ui,
        "Fill",
        "Main fill colour (schema: style_fill). Leave default to inherit the Type's colour.",
        |ui| color_override(ui, &mut draft.style_fill, derived.fill),
    );
    labeled(
        ui,
        "Accent",
        "Accent / outline colour (schema: style_accent).",
        |ui| color_override(ui, &mut draft.style_accent, derived.accent),
    );
    labeled(
        ui,
        "Glyph",
        "Single character drawn for this faction (schema: style_glyph).",
        |ui| glyph_override(ui, "fac_glyph_text", &mut draft.style_glyph, derived.glyph),
    );
    labeled(
        ui,
        "Border",
        "Border style on the map (schema: style_border).",
        |ui| {
            border_override(
                ui,
                "fac_border_combo",
                &mut draft.style_border,
                derived.border,
            )
        },
    );

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .button("↺  Reset to Type default")
            .on_hover_text("Clear every appearance override and use the colours derived from Type")
            .clicked()
        {
            draft.style_fill = None;
            draft.style_accent = None;
            draft.style_glyph = None;
            draft.style_border = None;
        }
        ui.label(
            RichText::new(format!("Type: {}", draft.kind))
                .small()
                .color(Color32::DARK_GRAY),
        );
    });
}

/// Single-input colour override (§F2). The swatch button is the only editor; it
/// opens egui's colour-picker popup. The resolved hex is shown read-only beside
/// it for reference, and `×` clears the override back to the kind-derived
/// default. Writes are gated on `resp.changed()`, so merely *viewing* a faction
/// no longer materialises an override equal to the derived colour — which used
/// to defeat "§F4 Recompute style from kind" and spuriously mark the row dirty.
fn color_override(ui: &mut Ui, slot: &mut Option<String>, derived: (u8, u8, u8)) {
    let rgb = slot.as_deref().and_then(parse_hex_rgb).unwrap_or(derived);
    let mut color = [rgb.0, rgb.1, rgb.2];
    ui.horizontal(|ui| {
        let resp = egui::color_picker::color_edit_button_srgb(ui, &mut color);
        if resp.changed() {
            *slot = Some(rgb_to_hex((color[0], color[1], color[2])));
        }
        ui.monospace(slot.clone().unwrap_or_else(|| rgb_to_hex(derived)));
        if slot.is_some()
            && ui
                .small_button("×")
                .on_hover_text("clear override")
                .clicked()
        {
            *slot = None;
        }
    });
}

fn glyph_override(ui: &mut Ui, salt: &str, slot: &mut Option<String>, derived: char) {
    ui.horizontal(|ui| {
        let mut buf = slot.clone().unwrap_or_else(|| derived.to_string());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut buf)
                .id_source(salt)
                .desired_width(40.0)
                .char_limit(1),
        );
        if resp.changed() {
            let trimmed = buf.trim();
            *slot = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        if slot.is_some() && ui.small_button("×").clicked() {
            *slot = None;
        }
    });
}

fn border_override(ui: &mut Ui, salt: &str, slot: &mut Option<String>, derived: FactionBorder) {
    let current = slot.as_deref().and_then(parse_border).unwrap_or(derived);
    let mut next = current;
    ui.horizontal(|ui| {
        ui_kit::combo(salt, format!("{current:?}")).show_ui(ui, |ui| {
            for (label, value) in KNOWN_BORDERS {
                if ui.selectable_label(current == *value, *label).clicked() {
                    next = *value;
                }
            }
        });
        if slot.is_some() && ui.small_button("×").clicked() {
            *slot = None;
        }
    });
    if next != current {
        let label = KNOWN_BORDERS
            .iter()
            .find(|(_, v)| *v == next)
            .map(|(l, _)| (*l).to_string())
            .unwrap_or_else(|| "thin".into());
        *slot = Some(label);
    }
}

// ── presence link (F5) ──────────────────────────────────────────────────────

fn presence_link(ui: &mut Ui, state: &mut BuilderState, draft: &FactionDef) {
    let assigned_worlds = sector_world_count(state, &draft.id);
    let assigned_systems = sector_system_count(state, &draft.id);
    ui.label(format!(
        "On the map: {assigned_systems} system(s), {assigned_worlds} world(s)."
    ));
    if ui
        .button("Edit presence in Control tab  →")
        .on_hover_text("Jump to the Control tab to assign this faction to systems and worlds")
        .clicked()
    {
        state.selected_faction_id = Some(draft.id.clone());
        state.focus_entity(EntityRef::Tab(BuilderTab::Control));
    }
}

fn sector_world_count(state: &BuilderState, id: &FactionId) -> usize {
    state
        .sector
        .factions
        .iter()
        .find(|f| f.id == *id)
        .map(|f| f.world_presence.len())
        .unwrap_or(0)
}

fn sector_system_count(state: &BuilderState, id: &FactionId) -> usize {
    state
        .sector
        .factions
        .iter()
        .find(|f| f.id == *id)
        .map(|f| f.system_presence.len())
        .unwrap_or(0)
}

// ── legend visibility (F7) ──────────────────────────────────────────────────

fn legend_visibility(ui: &mut Ui, draft: &mut FactionDef) {
    let mut current = match draft.legend_visible {
        None => 0u8,
        Some(true) => 1,
        Some(false) => 2,
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Show on map legend:");
        ui.radio_value(&mut current, 0, "Automatic");
        ui.radio_value(&mut current, 1, "Always");
        ui.radio_value(&mut current, 2, "Never");
    });
    draft.legend_visible = match current {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    };
    ui_kit::placeholder(
        ui,
        "Automatic hides very minor factions, rolling them into their Type group on the legend.",
    );
}

// ── helpers ─────────────────────────────────────────────────────────────────

struct GroupCounts {
    top: usize,
    sub: usize,
}

fn count_groups(file: &FactionsFile) -> GroupCounts {
    let mut tops: BTreeSet<String> = BTreeSet::new();
    let mut subs: BTreeSet<(String, String)> = BTreeSet::new();
    for f in &file.factions {
        let top = f.top_faction_id().to_string();
        let sub = f.subfaction_id().to_string();
        subs.insert((top.clone(), sub));
        tops.insert(top);
    }
    GroupCounts {
        top: tops.len(),
        sub: subs.len(),
    }
}

/// Dropdown over the group ids already present in the roster, plus "(none)" and
/// an in-popup custom row. Friendlier than free-typing a raw id: nesting is
/// usually under a group that already exists, so it becomes one click instead
/// of remembering and retyping the exact id string. The custom row keeps brand
/// new ids reachable.
fn id_combo(ui: &mut Ui, salt: &str, slot: &mut Option<FactionId>, options: &[String]) {
    let current = slot.as_ref().map(|f| f.to_string());
    let label = current.clone().unwrap_or_else(|| "(none)".to_owned());
    ui.horizontal(|ui| {
        ui_kit::combo(salt, label).show_ui(ui, |ui| {
            if ui.selectable_label(slot.is_none(), "(none)").clicked() {
                *slot = None;
            }
            for opt in options {
                let selected = current.as_deref() == Some(opt.as_str());
                if ui.selectable_label(selected, opt.as_str()).clicked() {
                    *slot = Some(FactionId::new(opt.as_str()));
                }
            }
            ui.separator();
            ui.label(RichText::new("custom…").small().color(Color32::DARK_GRAY));
            let mut buf = current.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut buf)
                        .hint_text("new id")
                        .desired_width(160.0),
                )
                .changed()
            {
                let trimmed = buf.trim();
                *slot = if trimmed.is_empty() {
                    None
                } else {
                    Some(FactionId::new(trimmed))
                };
            }
        });
        if slot.is_some() && ui.small_button("×").on_hover_text("clear").clicked() {
            *slot = None;
        }
    });
}

/// Unique, sorted top-level and mid-level group ids currently used in the
/// roster, excluding `self_id` (a faction can't be its own parent). Feeds the
/// [`id_combo`] pickers in the Grouping section.
fn existing_group_ids(file: &FactionsFile, self_id: &FactionId) -> (Vec<String>, Vec<String>) {
    let mut tops: BTreeSet<String> = BTreeSet::new();
    let mut subs: BTreeSet<String> = BTreeSet::new();
    for f in &file.factions {
        if &f.id == self_id {
            continue;
        }
        tops.insert(f.top_faction_id().to_string());
        subs.insert(f.subfaction_id().to_string());
    }
    (tops.into_iter().collect(), subs.into_iter().collect())
}

/// Display name of the faction with `id`, falling back to the id itself. Used to
/// label the delete-confirmation prompt.
fn faction_name_for(state: &BuilderState, id: &FactionId) -> String {
    state
        .data_catalogs
        .factions
        .as_ref()
        .and_then(|f| f.factions.iter().find(|x| x.id == *id))
        .map(|x| x.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn selected_index(state: &mut BuilderState) -> Option<usize> {
    let file = state.data_catalogs.factions.as_ref()?;
    if let Some(id) = &state.selected_faction_id {
        if let Some((i, _)) = file.factions.iter().enumerate().find(|(_, f)| f.id == *id) {
            return Some(i);
        }
    }
    if let Some(f) = file.factions.first() {
        state.selected_faction_id = Some(f.id.clone());
        return Some(0);
    }
    None
}

fn add_new_row(state: &mut BuilderState) {
    let Some(file) = state.data_catalogs.factions.as_mut() else {
        return;
    };
    let mut suffix = 1u32;
    let mut id = format!("new_faction_{suffix}");
    while file.factions.iter().any(|f| f.id.as_str() == id) {
        suffix += 1;
        id = format!("new_faction_{suffix}");
    }
    let new = FactionDef {
        faction: None,
        faction_name: None,
        subfaction: None,
        subfaction_name: None,
        id: FactionId::new(id.as_str()),
        name: "New faction".into(),
        kind: "imperial".into(),
        weight: 1.0,
        default_disposition: "lawful".into(),
        preferred_world_types: Vec::new(),
        preferred_governments: Vec::new(),
        preferred_notable_features: Vec::new(),
        style_fill: None,
        style_accent: None,
        style_glyph: None,
        style_border: None,
        legend_visible: None,
    };
    file.factions.push(new);
    state.selected_faction_id = Some(FactionId::new(id.as_str()));
    state.dirty = true;
    let rel = state
        .config
        .inputs
        .factions
        .clone()
        .unwrap_or_else(|| DEFAULT_FACTIONS_REL.to_string());
    state.dirty_files.insert(rel);
}

fn duplicate_row(state: &mut BuilderState, id: &FactionId) {
    let Some(file) = state.data_catalogs.factions.as_mut() else {
        return;
    };
    let Some(idx) = file.factions.iter().position(|f| f.id == *id) else {
        return;
    };
    let mut copy = file.factions[idx].clone();
    let mut n = 2u32;
    let base = copy.id.to_string();
    loop {
        let candidate = format!("{base}_copy{n}");
        if !file.factions.iter().any(|f| f.id.as_str() == candidate) {
            copy.id = FactionId::new(candidate);
            break;
        }
        n += 1;
    }
    let new_id = copy.id.clone();
    file.factions.insert(idx + 1, copy);
    state.selected_faction_id = Some(new_id);
    state.dirty = true;
    let rel = state
        .config
        .inputs
        .factions
        .clone()
        .unwrap_or_else(|| DEFAULT_FACTIONS_REL.to_string());
    state.dirty_files.insert(rel);
}

pub(crate) fn delete_row(state: &mut BuilderState, id: &FactionId) {
    let Some(file) = state.data_catalogs.factions.as_mut() else {
        return;
    };
    let Some(idx) = file.factions.iter().position(|f| f.id == *id) else {
        return;
    };
    file.factions.remove(idx);
    state.selected_faction_id = file.factions.first().map(|f| f.id.clone());
    state.dirty = true;
    let rel = state
        .config
        .inputs
        .factions
        .clone()
        .unwrap_or_else(|| DEFAULT_FACTIONS_REL.to_string());
    state.dirty_files.insert(rel);
}

fn save_factions_only(state: &mut BuilderState) {
    if state.config.inputs.factions.is_none() {
        state.config.inputs.factions = Some(DEFAULT_FACTIONS_REL.to_string());
    }
    match super::super::project_io::save_project(state) {
        Ok(()) => {
            state.dirty_files.retain(|f| {
                f != "data/factions/factions.toml"
                    && Some(f.as_str()) != state.config.inputs.factions.as_deref()
            });
        }
        Err(e) => {
            state.modal = Some(ModalKind::Message(format!(
                "Save factions.toml failed: {e}"
            )));
        }
    }
}

fn resolve_style(draft: &FactionDef) -> sectorforge::faction_style::FactionStyleRgb {
    faction_style_rgb_with_overrides(
        &draft.kind,
        draft.id.as_str(),
        &draft.default_disposition,
        draft.style_fill.as_deref(),
        draft.style_accent.as_deref(),
        draft.style_glyph.as_deref(),
        draft.style_border.as_deref(),
    )
}
