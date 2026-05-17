//! Factions list editor. Edit id, name, kind, disposition; assign systems and
//! worlds for each faction.

use egui::{RichText, Ui};

use super::state::{empty_faction, EditorState};
use super::ui_helpers::{combo_str, dim, label, mono, section, text_field};

pub fn show_factions(ui: &mut Ui, state: &mut EditorState) {
    let Some(sector) = state.sector.as_mut() else {
        dim(ui, "no sector loaded");
        return;
    };

    section(ui, &format!("FACTIONS ({})", sector.factions.len()));

    let system_ids: Vec<String> = sector.systems.iter().map(|s| s.id.clone()).collect();
    let world_ids: Vec<String> = sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter().map(|w| w.id.clone()))
        .collect();
    let system_refs: Vec<&str> = system_ids.iter().map(|s| s.as_str()).collect();
    let world_refs: Vec<&str> = world_ids.iter().map(|s| s.as_str()).collect();

    let mut dirty = false;
    let mut remove: Option<usize> = None;

    for (i, fac) in sector.factions.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            label(ui, "ID");
            if text_field(ui, &mut fac.id, "faction_id").changed() {
                dirty = true;
            }
            if ui.small_button(RichText::new("x").font(mono(11.0))).clicked() {
                remove = Some(i);
            }
        });
        ui.horizontal(|ui| {
            label(ui, "NAME");
            if text_field(ui, &mut fac.name, "name").changed() {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            label(ui, "KIND");
            if text_field(ui, &mut fac.kind, "imperial / xenos / chaos…").changed() {
                dirty = true;
            }
            label(ui, "DISPOSITION");
            if text_field(ui, &mut fac.disposition, "neutral / hostile…").changed() {
                dirty = true;
            }
        });

        ui.label(RichText::new(format!("SYSTEMS ({})", fac.system_presence.len())).font(mono(12.0)));
        let mut remove_sys: Option<usize> = None;
        for (j, sid) in fac.system_presence.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if combo_str(ui, &format!("f_{}_sys_{}", i, j), sid, &system_refs) {
                    dirty = true;
                }
                if ui.small_button(RichText::new("x").font(mono(11.0))).clicked() {
                    remove_sys = Some(j);
                }
            });
        }
        if let Some(j) = remove_sys {
            fac.system_presence.remove(j);
            dirty = true;
        }
        if !system_ids.is_empty()
            && ui
                .button(RichText::new("+ ADD SYSTEM").font(mono(12.0)))
                .clicked()
        {
            fac.system_presence.push(system_ids[0].clone());
            dirty = true;
        }

        ui.label(RichText::new(format!("WORLDS ({})", fac.world_presence.len())).font(mono(12.0)));
        let mut remove_w: Option<usize> = None;
        for (j, wid) in fac.world_presence.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if combo_str(ui, &format!("f_{}_w_{}", i, j), wid, &world_refs) {
                    dirty = true;
                }
                if ui.small_button(RichText::new("x").font(mono(11.0))).clicked() {
                    remove_w = Some(j);
                }
            });
        }
        if let Some(j) = remove_w {
            fac.world_presence.remove(j);
            dirty = true;
        }
        if !world_ids.is_empty()
            && ui
                .button(RichText::new("+ ADD WORLD").font(mono(12.0)))
                .clicked()
        {
            fac.world_presence.push(world_ids[0].clone());
            dirty = true;
        }
        ui.separator();
    }

    if let Some(i) = remove {
        sector.factions.remove(i);
        dirty = true;
    }

    if ui
        .button(RichText::new("+ ADD FACTION").font(mono(12.0)))
        .clicked()
    {
        let id = format!("faction_{}", sector.factions.len() + 1);
        sector.factions.push(empty_faction(id));
        dirty = true;
    }

    if dirty {
        state.mark_dirty();
    }
}
