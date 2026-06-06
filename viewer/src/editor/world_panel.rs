//! World inspector. All world fields editable via dropdown except name/orbit.

use egui::{RichText, Ui};

use sectorforge::worlds::{
    Atmosphere, Biosphere, Government, NotableFeature, Population, StarColour, TechLevel,
    Temperature, WorldType,
};

use crate::palette;

use super::enums::{
    ATMOSPHERES, BIOSPHERES, GOVERNMENTS, NOTABLE_FEATURES, POPULATIONS, STAR_COLOUR_CODES,
    TECH_LEVELS, TEMPERATURES, WORLD_TYPES,
};
use super::state::{EditorState, Selection};
use super::ui_helpers::{combo_str, dim, label, section, text_field_id};

pub(crate) fn show_world_inspector(ui: &mut Ui, state: &mut EditorState) {
    let Selection::World {
        system_id,
        world_index,
    } = state.selection.clone()
    else {
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

    ui.label(RichText::new(format!("WORLD {}", w.id.to_uppercase())).color(palette::chrome_text()));

    section(ui, "NAME");
    if text_field_id(ui, &mut w.name, "name").changed() {
        dirty = true;
    }

    ui.horizontal(|ui| {
        label(ui, "ORBIT");
        let mut orbit_i = i32::from(w.orbit);
        if ui
            .add(egui::DragValue::new(&mut orbit_i).range(1..=99))
            .changed()
        {
            w.orbit = orbit_i.clamp(1, 99) as u8;
            dirty = true;
        }
    });

    ui.add_space(6.0);
    section(ui, "STAR COLOUR (display)");
    let mut star_code = w.world.star_colour.code().to_string();
    if combo_str(ui, "w_star_code", &mut star_code, STAR_COLOUR_CODES) {
        if let Ok(sc) = star_code.parse::<StarColour>() {
            w.world.star_colour = sc;
        }
        dirty = true;
    }
    dim(ui, &format!("({})", w.world.star_colour.short_name()));

    ui.add_space(6.0);
    section(ui, "CLASSIFICATION");
    row(
        ui,
        "TYPE",
        |ui| {
            let mut val = w.world.world_type.to_string();
            if combo_str(ui, "w_type", &mut val, WORLD_TYPES) {
                w.world.world_type =
                    parse_variant(WorldType::VARIANTS, &val, w.world.world_type.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );

    ui.add_space(6.0);
    section(ui, "ENVIRONMENT");
    row(
        ui,
        "ATMOSPHERE",
        |ui| {
            let mut val = w.world.atmosphere.to_string();
            if combo_str(ui, "w_atm", &mut val, ATMOSPHERES) {
                w.world.atmosphere =
                    parse_variant(Atmosphere::VARIANTS, &val, w.world.atmosphere.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );
    row(
        ui,
        "TEMPERATURE",
        |ui| {
            let mut val = w.world.temperature.to_string();
            if combo_str(ui, "w_temp", &mut val, TEMPERATURES) {
                w.world.temperature =
                    parse_variant(Temperature::VARIANTS, &val, w.world.temperature.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );
    row(
        ui,
        "BIOSPHERE",
        |ui| {
            let mut val = w.world.biosphere.to_string();
            if combo_str(ui, "w_bio", &mut val, BIOSPHERES) {
                w.world.biosphere =
                    parse_variant(Biosphere::VARIANTS, &val, w.world.biosphere.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );

    ui.add_space(6.0);
    section(ui, "SOCIETY");
    row(
        ui,
        "POPULATION",
        |ui| {
            let mut val = w.world.population.to_string();
            if combo_str(ui, "w_pop", &mut val, POPULATIONS) {
                w.world.population =
                    parse_variant(Population::VARIANTS, &val, w.world.population.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );
    row(
        ui,
        "TECH",
        |ui| {
            let mut val = w.world.tech_level.to_string();
            if combo_str(ui, "w_tech", &mut val, TECH_LEVELS) {
                w.world.tech_level =
                    parse_variant(TechLevel::VARIANTS, &val, w.world.tech_level.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );
    row(
        ui,
        "GOVERNMENT",
        |ui| {
            let mut val = w.world.government.to_string();
            if combo_str(ui, "w_gov", &mut val, GOVERNMENTS) {
                w.world.government =
                    parse_variant(Government::VARIANTS, &val, w.world.government.clone());
                true
            } else {
                false
            }
        },
        &mut dirty,
    );

    ui.add_space(6.0);
    section(
        ui,
        &format!("NOTABLE FEATURES ({})", w.world.notable_features.len()),
    );
    let mut remove: Option<usize> = None;
    for (i, feat) in w.world.notable_features.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let mut val = feat.to_string();
            if combo_str(ui, &format!("w_feat_{i}"), &mut val, NOTABLE_FEATURES) {
                *feat = parse_variant(NotableFeature::VARIANTS, &val, feat.clone());
                dirty = true;
            }
            if ui.small_button(RichText::new("x")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        w.world.notable_features.remove(i);
        dirty = true;
    }
    if ui.button(RichText::new("+ ADD FEATURE")).clicked() {
        w.world.notable_features.push(NotableFeature::Prosperous);
        dirty = true;
    }

    ui.add_space(8.0);
    ui.separator();
    if ui.button(RichText::new("← BACK TO SYSTEM")).clicked() {
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

/// Map a combo-box selection (a CamelCase variant-name string, as produced by
/// the enum's `Display`) back to the matching `worlds.rs` enum variant. The
/// dropdown option lists in [`super::enums`] are exactly those variant names, so
/// the lookup is total in practice; `fallback` (the unchanged current value) is
/// only returned for a defensive non-match.
fn parse_variant<T: std::fmt::Display + Clone>(variants: &[T], s: &str, fallback: T) -> T {
    variants
        .iter()
        .find(|v| v.to_string() == s)
        .cloned()
        .unwrap_or(fallback)
}
