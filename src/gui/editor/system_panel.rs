//! Inspector panel for the selected system. Edits name/coord/star/worlds.

use egui::{RichText, Ui};

use crate::gui::palette::TEXT;

use super::enums::{star_colour_name, STAR_COLOUR_CODES};
use super::state::{empty_world, EditorState, Selection};
use super::ui_helpers::{combo_str, dim, label, mono, section, text_field};

pub fn show_system_inspector(ui: &mut Ui, state: &mut EditorState) {
    let Selection::System(id) = state.selection.clone() else {
        return;
    };
    let Some(sector) = state.sector.as_mut() else {
        return;
    };
    let Some(sys) = sector.systems.iter_mut().find(|s| s.id == id) else {
        state.selection = Selection::None;
        return;
    };

    let mut dirty = false;
    let mut delete_system = false;
    let mut add_world = false;
    let mut select_world: Option<usize> = None;
    let mut delete_world: Option<usize> = None;

    ui.label(
        RichText::new(format!("SYSTEM {}", sys.id.to_uppercase()))
            .color(TEXT)
            .font(mono(15.0)),
    );

    section(ui, "NAME");
    let mut name = sys.name.to_string();
    if text_field(ui, &mut name, "name").changed() {
        sys.name = name.into();
        dirty = true;
    }

    ui.horizontal(|ui| {
        label(ui, "COORD Q");
        if ui
            .add(egui::DragValue::new(&mut sys.coord.q).range(0..=63))
            .changed()
        {
            dirty = true;
        }
        label(ui, "R");
        if ui
            .add(egui::DragValue::new(&mut sys.coord.r).range(0..=63))
            .changed()
        {
            dirty = true;
        }
    });

    ui.add_space(6.0);
    section(ui, "STAR");
    ui.horizontal(|ui| {
        label(ui, "COLOUR");
        let mut star_code = sys.star.colour_code.to_string();
        if combo_str(ui, "sys_star_colour", &mut star_code, STAR_COLOUR_CODES) {
            sys.star.colour_code = star_code.into();
            sys.star.colour_name = star_colour_name(&sys.star.colour_code).into();
            sys.star.spectral_type = Some(sys.star.colour_code.clone());
            dirty = true;
        }
    });
    dim(ui, &format!("({})", sys.star.colour_name));

    ui.add_space(6.0);
    section(ui, &format!("WORLDS ({})", sys.worlds.len()));
    for w in &sys.worlds {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    matches!(&state.selection, Selection::World { system_id, world_index }
                        if *system_id == sys.id && *world_index == w.index),
                    RichText::new(format!(
                        "{:>2}  {}  {}",
                        w.orbit,
                        truncate(&w.name, 18),
                        truncate(&w.world.world_type, 14)
                    ))
                    .font(mono(12.0)),
                )
                .clicked()
            {
                select_world = Some(w.index);
            }
            if ui
                .small_button(RichText::new("x").font(mono(11.0)))
                .clicked()
            {
                delete_world = Some(w.index);
            }
        });
    }
    if ui
        .button(RichText::new("+ ADD WORLD").font(mono(12.0)))
        .clicked()
    {
        add_world = true;
    }

    ui.add_space(8.0);
    ui.separator();
    if ui
        .button(RichText::new("DELETE SYSTEM").font(mono(12.0)))
        .clicked()
    {
        delete_system = true;
    }

    // Mutations after the borrow ends.
    let sys_index = sys.index;
    let sys_id = sys.id.clone();
    if let Some(w_idx) = delete_world {
        sys.worlds.retain(|w| w.index != w_idx);
        dirty = true;
        if matches!(&state.selection, Selection::World { world_index, .. } if *world_index == w_idx)
        {
            state.selection = Selection::System(sys_id.clone());
        }
    }
    if add_world {
        let next = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1;
        let mut w = empty_world(sys_index, next, format!("World {next}"));
        w.world.star_colour = sys.star.colour_name.clone();
        w.world.star_colour_code = sys.star.colour_code.clone();
        sys.worlds.push(w);
        dirty = true;
        state.selection = Selection::World {
            system_id: sys_id.clone(),
            world_index: next,
        };
    }
    if let Some(idx) = select_world {
        state.selection = Selection::World {
            system_id: sys_id.clone(),
            world_index: idx,
        };
    }
    if delete_system {
        sector.systems.retain(|s| s.id != sys_id);
        sector
            .routes
            .retain(|r| r.from_system_id != sys_id && r.to_system_id != sys_id);
        for f in &mut sector.factions {
            f.system_presence.retain(|x| x != &sys_id);
        }
        state.selection = Selection::None;
        state.mark_dirty();
        return;
    }
    if dirty {
        state.mark_dirty();
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}
