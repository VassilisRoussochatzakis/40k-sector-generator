//! World inspector. All world fields editable via dropdown except name/orbit.

use egui::{RichText, Ui};

use sectorforge::worlds::{
    Atmosphere, Biosphere, Government, NotableFeature, Population, StarColour, TechLevel,
    Temperature, WorldType,
};

use crate::palette;

use super::enums::STAR_COLOUR_CODES;
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
            enum_combo(
                ui,
                "w_type",
                &mut w.world.world_type,
                WorldType::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );

    ui.add_space(6.0);
    section(ui, "ENVIRONMENT");
    row(
        ui,
        "ATMOSPHERE",
        |ui| {
            enum_combo(
                ui,
                "w_atm",
                &mut w.world.atmosphere,
                Atmosphere::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );
    row(
        ui,
        "TEMPERATURE",
        |ui| {
            enum_combo(
                ui,
                "w_temp",
                &mut w.world.temperature,
                Temperature::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );
    row(
        ui,
        "BIOSPHERE",
        |ui| {
            enum_combo(
                ui,
                "w_bio",
                &mut w.world.biosphere,
                Biosphere::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );

    ui.add_space(6.0);
    section(ui, "SOCIETY");
    row(
        ui,
        "POPULATION",
        |ui| {
            enum_combo(
                ui,
                "w_pop",
                &mut w.world.population,
                Population::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );
    row(
        ui,
        "TECH",
        |ui| {
            enum_combo(
                ui,
                "w_tech",
                &mut w.world.tech_level,
                TechLevel::VARIANTS,
                |v| v.display_name(),
            )
        },
        &mut dirty,
    );
    row(
        ui,
        "GOVERNMENT",
        |ui| {
            enum_combo(
                ui,
                "w_gov",
                &mut w.world.government,
                Government::VARIANTS,
                |v| v.display_name(),
            )
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
            if enum_combo(ui, ("w_feat", i), feat, NotableFeature::VARIANTS, |v| {
                v.display_name()
            }) {
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

/// Enum dropdown over `variants` binding a **required** (non-`Option`) field.
/// Forwards to the shared [`crate::widgets::enum_combo`] (F2) — the same widget
/// the `worlds.toml` data editor uses — so the inspector's labels are the
/// canonical [`display_name`](sectorforge::worlds) form (e.g. `"Agri-World"`)
/// rather than the old hand-forked CamelCase strings (#33). The data editor
/// binds `Option<T>` because its `GenerationRow` fields are optional; here the
/// world always has a value, so we bridge through a temporary `Some` and ignore
/// the `—` sentinel (a required field can never be unset). No tooltips.
fn enum_combo<T, F>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut T,
    variants: &'static [T],
    label_of: F,
) -> bool
where
    T: Clone + PartialEq,
    F: Fn(&T) -> &'static str,
{
    let mut opt = Some(value.clone());
    let changed = crate::widgets::enum_combo(ui, id, &mut opt, variants, label_of, |_| None, None);
    // The `—` sentinel maps to `None`; a required field stays put in that case.
    if let (true, Some(v)) = (changed, opt) {
        *value = v;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #33: the inspector dropdowns now label variants with the canonical
    // `display_name()` (the form the `worlds.toml` data editor already shows),
    // not the old forked CamelCase `Display` string. This guards the user-visible
    // fix: the two surfaces agree, e.g. both read "Agri-World", not "AgriWorld".
    #[test]
    fn world_type_dropdown_uses_canonical_display_name() {
        assert_eq!(WorldType::AgriWorld.display_name(), "Agri-World");
        assert_ne!(
            WorldType::AgriWorld.display_name(),
            WorldType::AgriWorld.to_string()
        );
    }

    // The non-optional adapter writes a picked variant back and leaves the field
    // unchanged when nothing is selected. Exercised headlessly (no real click, so
    // the combo reports "unchanged") to prove the required-field semantics: an
    // unchanged interaction never clears the value.
    #[test]
    fn enum_combo_required_keeps_value_when_unchanged() {
        egui::__run_test_ui(|ui| {
            let mut wt = WorldType::HiveWorld;
            let changed = enum_combo(ui, "t", &mut wt, WorldType::VARIANTS, |v| v.display_name());
            assert!(!changed);
            assert_eq!(wt, WorldType::HiveWorld);
        });
    }
}
