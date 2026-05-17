//! World inspector. All world fields editable via dropdown except name/orbit.

use egui::{RichText, Ui};

use crate::gui::palette::TEXT;

use super::enums::{
    star_colour_name, ATMOSPHERES, BIOSPHERES, GOVERNMENTS, NOTABLE_FEATURES, POPULATIONS,
    STAR_COLOUR_CODES, TECH_LEVELS, TEMPERATURES, WORLD_TYPES,
};
use super::state::{EditorState, Selection};
use super::ui_helpers::{combo_str, dim, label, mono, section, text_field};

pub fn show_world_inspector(ui: &mut Ui, state: &mut EditorState) {
    let Selection::World { system_id, world_index } = state.selection.clone() else {
        return;
    };
    let Some(sector) = state.sector.as_mut() else {
        return;
    };
    let Some(sys) = sector.systems.iter_mut().find(|s| s.id == system_id) else {
        return;
    };
    let Some(w) = sys.worlds.iter_mut().find(|w| w.index == world_index) else {
        return;
    };

    let mut dirty = false;

    ui.label(
        RichText::new(format!("WORLD {}", w.id.to_uppercase()))
            .color(TEXT)
            .font(mono(15.0)),
    );

    section(ui, "NAME");
    if text_field(ui, &mut w.name, "name").changed() {
        dirty = true;
    }

    ui.horizontal(|ui| {
        label(ui, "ORBIT");
        let mut orbit_i = w.orbit as i32;
        if ui.add(egui::DragValue::new(&mut orbit_i).range(1..=99)).changed() {
            w.orbit = orbit_i.clamp(1, 99) as u8;
            dirty = true;
        }
    });

    ui.add_space(6.0);
    section(ui, "STAR COLOUR (display)");
    if combo_str(ui, "w_star_code", &mut w.world.star_colour_code, STAR_COLOUR_CODES) {
        w.world.star_colour = star_colour_name(&w.world.star_colour_code).to_string();
        dirty = true;
    }
    dim(ui, &format!("({})", w.world.star_colour));

    ui.add_space(6.0);
    section(ui, "CLASSIFICATION");
    row(ui, "TYPE", |ui| combo_str(ui, "w_type", &mut w.world.world_type, WORLD_TYPES), &mut dirty);

    ui.add_space(6.0);
    section(ui, "ENVIRONMENT");
    row(ui, "ATMOSPHERE", |ui| combo_str(ui, "w_atm", &mut w.world.atmosphere, ATMOSPHERES), &mut dirty);
    row(ui, "TEMPERATURE", |ui| combo_str(ui, "w_temp", &mut w.world.temperature, TEMPERATURES), &mut dirty);
    row(ui, "BIOSPHERE", |ui| combo_str(ui, "w_bio", &mut w.world.biosphere, BIOSPHERES), &mut dirty);

    ui.add_space(6.0);
    section(ui, "SOCIETY");
    row(ui, "POPULATION", |ui| combo_str(ui, "w_pop", &mut w.world.population, POPULATIONS), &mut dirty);
    row(ui, "TECH", |ui| combo_str(ui, "w_tech", &mut w.world.tech_level, TECH_LEVELS), &mut dirty);
    row(ui, "GOVERNMENT", |ui| combo_str(ui, "w_gov", &mut w.world.government, GOVERNMENTS), &mut dirty);

    ui.add_space(6.0);
    section(ui, &format!("NOTABLE FEATURES ({})", w.world.notable_features.len()));
    let mut remove: Option<usize> = None;
    for (i, feat) in w.world.notable_features.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if combo_str(ui, &format!("w_feat_{}", i), feat, NOTABLE_FEATURES) {
                dirty = true;
            }
            if ui.small_button(RichText::new("x").font(mono(11.0))).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        w.world.notable_features.remove(i);
        dirty = true;
    }
    if ui
        .button(RichText::new("+ ADD FEATURE").font(mono(12.0)))
        .clicked()
    {
        w.world.notable_features.push("Prosperous".to_string());
        dirty = true;
    }

    ui.add_space(8.0);
    ui.separator();
    if ui.button(RichText::new("← BACK TO SYSTEM").font(mono(12.0))).clicked() {
        state.selection = Selection::System(system_id.clone());
    }

    if dirty {
        state.mark_dirty();
    }
}

fn row(ui: &mut Ui, k: &str, mut body: impl FnMut(&mut Ui) -> bool, dirty: &mut bool) {
    ui.horizontal(|ui| {
        label(ui, k);
        if body(ui) {
            *dirty = true;
        }
    });
}
