//! Factions list editor. Edit id, name, kind, disposition; assign systems and
//! worlds for each faction. Includes filter / sort / pin controls (§14) and a
//! visual style chip (§8) so faction colour + glyph match the sector map.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};

use super::state::{empty_faction, EditorState, FactionSort};
use super::ui_helpers::{
    combo_kv_id, combo_str_id, dim, label, section, text_field, text_field_id,
};
use crate::palette::{draw_faction_chip, faction_style};
use sectorforge::ids::{FactionId, SystemId, WorldId};

pub(crate) fn show_factions(ui: &mut Ui, state: &mut EditorState) {
    let Some(sector) = state.sector.as_mut() else {
        dim(ui, "no sector loaded");
        return;
    };

    section(ui, &format!("FACTIONS ({})", sector.factions.len()));

    // ── Filter / sort row (§14) ────────────────────────────────────────────
    let kinds: BTreeSet<String> = sector.factions.iter().map(|f| f.kind.to_string()).collect();
    let dispositions: BTreeSet<String> = sector
        .factions
        .iter()
        .map(|f| f.disposition.to_string())
        .collect();
    ui.horizontal_wrapped(|ui| {
        label(ui, "KIND");
        filter_combo(
            ui,
            "fac_filter_kind",
            &mut state.faction_filter_kind,
            kinds.iter(),
        );
        label(ui, "DISP");
        filter_combo(
            ui,
            "fac_filter_disp",
            &mut state.faction_filter_disposition,
            dispositions.iter(),
        );
        label(ui, "SORT");
        crate::ui_kit::combo("fac_sort", RichText::new(state.faction_sort.label())).show_ui(
            ui,
            |ui| {
                for s in FactionSort::ALL {
                    if ui
                        .selectable_label(state.faction_sort == s, RichText::new(s.label()))
                        .clicked()
                    {
                        state.faction_sort = s;
                    }
                }
            },
        );
        if (state.faction_filter_kind.is_some()
            || state.faction_filter_disposition.is_some()
            || state.faction_sort != FactionSort::default())
            && ui.small_button(RichText::new("clear")).clicked()
        {
            state.faction_filter_kind = None;
            state.faction_filter_disposition = None;
            state.faction_sort = FactionSort::default();
        }
    });
    if !state.faction_pinned.is_empty() {
        dim(
            ui,
            &format!("{} pinned (shown first)", state.faction_pinned.len()),
        );
    }
    ui.separator();

    let system_ids: Vec<SystemId> = sector.systems.iter().map(|s| s.id.clone()).collect();
    let world_ids: Vec<WorldId> = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter().map(|w| w.id.clone()))
        .collect();
    let system_labels: Vec<(SystemId, String)> = sector
        .systems
        .iter()
        .map(|s| (s.id.clone(), s.name.to_string()))
        .collect();
    let system_kv: Vec<(&str, &str)> = system_labels
        .iter()
        .map(|(id, name)| (id.as_str(), name.as_str()))
        .collect();
    let world_refs: Vec<&str> = world_ids.iter().map(|s| s.as_str()).collect();

    // Build visible-index list. Filter by kind/disposition, then sort with
    // pinned ids floated to the top regardless of secondary sort.
    let mut visible: Vec<usize> = (0..sector.factions.len())
        .filter(|&i| {
            let f = &sector.factions[i];
            state
                .faction_filter_kind
                .as_deref()
                .is_none_or(|k| f.kind.as_ref() == k)
                && state
                    .faction_filter_disposition
                    .as_deref()
                    .is_none_or(|d| f.disposition.as_ref() == d)
        })
        .collect();
    let pinned = &state.faction_pinned;
    let sort_mode = state.faction_sort;
    visible.sort_by(|&a, &b| {
        let fa = &sector.factions[a];
        let fb = &sector.factions[b];
        let pa = pinned.contains(&fa.id);
        let pb = pinned.contains(&fb.id);
        // Pinned first; both pinned or both unpinned fall through.
        match (pa, pb) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        match sort_mode {
            FactionSort::PowerDesc => fb
                .power
                .total_projection()
                .partial_cmp(&fa.power.total_projection())
                .unwrap_or(std::cmp::Ordering::Equal),
            FactionSort::PowerAsc => fa
                .power
                .total_projection()
                .partial_cmp(&fb.power.total_projection())
                .unwrap_or(std::cmp::Ordering::Equal),
            FactionSort::NameAsc => fa.name.cmp(&fb.name),
        }
    });

    if visible.is_empty() && !sector.factions.is_empty() {
        dim(ui, "(no factions match current filters)");
    }

    let mut dirty = false;
    let mut remove: Option<usize> = None;
    let mut toggle_pin: Option<FactionId> = None;

    // Walk in display order. We need the real index `i` to call `iter_mut`
    // safely, so split the borrow per-entry instead of using a `for` loop
    // over `factions.iter_mut()`.
    for &i in &visible {
        let fac = &mut sector.factions[i];
        let style = faction_style(&fac.kind, &fac.id, &fac.disposition);
        let pinned_now = pinned.contains(&fac.id);

        // Header row: chip + name + power + pin + delete.
        ui.horizontal(|ui| {
            draw_faction_chip(ui, style);
            ui.label(RichText::new(fac.name.to_uppercase()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(RichText::new("x"))
                    .on_hover_text("delete faction")
                    .clicked()
                {
                    remove = Some(i);
                }
                let pin_glyph = if pinned_now { "★" } else { "☆" };
                if ui
                    .small_button(RichText::new(pin_glyph))
                    .on_hover_text("pin to top")
                    .clicked()
                {
                    toggle_pin = Some(fac.id.clone());
                }
                ui.label(
                    RichText::new(format!("pwr {:.0}", fac.power.total_projection()))
                        .color(palette_dim()),
                );
            });
        });

        ui.horizontal(|ui| {
            label(ui, "ID");
            if text_field_id(ui, &mut fac.id, "faction_id").changed() {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            label(ui, "NAME");
            let mut name = fac.name.to_string();
            if text_field(ui, &mut name, "name").changed() {
                fac.name = name.into();
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            label(ui, "KIND");
            let mut kind = fac.kind.to_string();
            if text_field(ui, &mut kind, "imperial / xenos / chaos…").changed() {
                fac.kind = kind.into();
                dirty = true;
            }
            label(ui, "DISPOSITION");
            let mut disposition = fac.disposition.to_string();
            if text_field(ui, &mut disposition, "neutral / hostile…").changed() {
                fac.disposition = disposition.into();
                dirty = true;
            }
        });

        ui.label(RichText::new(format!(
            "SYSTEMS ({})",
            fac.system_presence.len()
        )));
        let mut remove_sys: Option<usize> = None;
        for (j, sid) in fac.system_presence.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if combo_kv_id(ui, &format!("f_{i}_sys_{j}"), sid, &system_kv) {
                    dirty = true;
                }
                if ui.small_button(RichText::new("x")).clicked() {
                    remove_sys = Some(j);
                }
            });
        }
        if let Some(j) = remove_sys {
            fac.system_presence.remove(j);
            dirty = true;
        }
        if !system_ids.is_empty() && ui.button(RichText::new("+ ADD SYSTEM")).clicked() {
            fac.system_presence.push(system_ids[0].clone());
            dirty = true;
        }

        ui.label(RichText::new(format!(
            "WORLDS ({})",
            fac.world_presence.len()
        )));
        let mut remove_w: Option<usize> = None;
        for (j, wid) in fac.world_presence.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if combo_str_id(ui, &format!("f_{i}_w_{j}"), wid, &world_refs) {
                    dirty = true;
                }
                if ui.small_button(RichText::new("x")).clicked() {
                    remove_w = Some(j);
                }
            });
        }
        if let Some(j) = remove_w {
            fac.world_presence.remove(j);
            dirty = true;
        }
        if !world_ids.is_empty() && ui.button(RichText::new("+ ADD WORLD")).clicked() {
            fac.world_presence.push(world_ids[0].clone());
            dirty = true;
        }
        ui.separator();
    }

    if let Some(i) = remove {
        let id = sector.factions[i].id.clone();
        sector.factions.remove(i);
        state.faction_pinned.remove(&id);
        dirty = true;
    }
    if let Some(id) = toggle_pin {
        if state.faction_pinned.contains(&id) {
            state.faction_pinned.remove(&id);
        } else {
            state.faction_pinned.insert(id);
        }
    }

    if ui.button(RichText::new("+ ADD FACTION")).clicked() {
        let id = FactionId::new(format!("faction_{}", sector.factions.len() + 1));
        sector.factions.push(empty_faction(&id));
        dirty = true;
    }

    if dirty {
        state.mark_dirty();
    }
}

fn filter_combo<'a, I: IntoIterator<Item = &'a String>>(
    ui: &mut Ui,
    id: &str,
    value: &mut Option<String>,
    options: I,
) {
    let label = value.as_deref().unwrap_or("(any)").to_string();
    crate::ui_kit::combo(id, RichText::new(label)).show_ui(ui, |ui| {
        if ui
            .selectable_label(value.is_none(), RichText::new("(any)"))
            .clicked()
        {
            *value = None;
        }
        for opt in options {
            let sel = value.as_deref() == Some(opt.as_str());
            if ui.selectable_label(sel, RichText::new(opt)).clicked() {
                *value = Some(opt.clone());
            }
        }
    });
}

fn palette_dim() -> Color32 {
    Color32::from_rgb(150, 145, 165)
}
