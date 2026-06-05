//! Inspector panel for the selected system. Edits name/coord/star/worlds.

use egui::{RichText, Ui};

use crate::palette;

use super::enums::{star_colour_name, STAR_COLOUR_CODES};
use super::state::{empty_world, EditorState, Selection};
use super::ui_helpers::{combo_str, dim, label, section, text_field};

pub(crate) fn show_system_inspector(ui: &mut Ui, state: &mut EditorState) {
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
        RichText::new(format!("SYSTEM {}", sys.id.to_uppercase())).color(palette::chrome_text()),
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
    let mut has_star = sys.star.is_some();
    if ui.checkbox(&mut has_star, "HAS STAR").changed() {
        if has_star {
            sys.star = Some(sectorforge::sector_model::GeneratedStar {
                colour_code: "W".into(),
                colour_name: "white".into(),
                spectral_type: Some("W".into()),
                source_row_index: None,
            });
            sys.kind = sectorforge::sector_model::SystemKind::Star;
        } else {
            sys.star = None;
            sys.kind = sectorforge::sector_model::SystemKind::SpecialLocation;
        }
        dirty = true;
    }

    if let Some(star) = &mut sys.star {
        ui.horizontal(|ui| {
            label(ui, "COLOUR");
            let mut star_code = star.colour_code.to_string();
            if combo_str(ui, "sys_star_colour", &mut star_code, STAR_COLOUR_CODES) {
                star.colour_code = star_code.into();
                star.colour_name = star_colour_name(&star.colour_code).into();
                star.spectral_type = Some(star.colour_code.clone());
                dirty = true;
            }
        });
        dim(ui, &format!("({})", star.colour_name));
    } else {
        ui.horizontal(|ui| {
            label(ui, "KIND");
            // I'll skip a combo for Kind for now to keep it simple, but at least show it.
            ui.label(RichText::new(format!("{}", sys.kind).to_uppercase()));
        });
    }

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
                    )),
                )
                .clicked()
            {
                select_world = Some(w.index);
            }
            if ui.small_button(RichText::new("x")).clicked() {
                delete_world = Some(w.index);
            }
        });
    }
    if ui.button(RichText::new("+ ADD WORLD")).clicked() {
        add_world = true;
    }

    ui.add_space(8.0);
    ui.separator();
    if ui.button(RichText::new("DELETE SYSTEM")).clicked() {
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
        if let Some(star) = &sys.star {
            w.world.star_colour = star.colour_name.clone();
            w.world.star_colour_code = star.colour_code.clone();
        } else {
            w.world.star_colour = "white".into();
            w.world.star_colour_code = "W".into();
        }
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
