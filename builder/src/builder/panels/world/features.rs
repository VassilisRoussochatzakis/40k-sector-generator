//! WORLD tab — notable features (§W5) and coupling warnings (§W6).

use egui::{Color32, Ui};

use sectorforge::worlds::{
    Atmosphere, Biosphere, Government, NotableFeature, Population, StarColour, TechLevel, WorldType,
};
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

// ── features (W5) ──────────────────────────────────────────────────────────

pub(super) fn show_features_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let feature_count = state.sector.systems[sys_idx].worlds[w_idx]
        .world
        .notable_features
        .len();
    let weights = feature_weights_for_world(state, sys_idx, w_idx);
    let weights = &*weights;
    ui_kit::collapsing_section(
        ui,
        "world_features",
        &format!("Notable features ({feature_count})"),
        false,
        |ui| {
            // §R4: gather the requested mutation (one remove or one add per
            // frame), then apply it to a clone and dispatch one EditWorld below.
            let mut remove: Option<usize> = None;
            let mut add: Option<NotableFeature> = None;
            let cur: Vec<String> = state.sector.systems[sys_idx].worlds[w_idx]
                .world
                .notable_features
                .iter()
                .map(|s| s.as_ref().to_string())
                .collect();
            if cur.is_empty() {
                ui_kit::placeholder(ui, "None yet — pick from the list below.");
            }
            for (i, name) in cur.iter().enumerate() {
                ui.horizontal(|ui| {
                    let display = feature_display_name(name);
                    ui.label(display).on_hover_text(format!("id: {name}"));
                    if let Some(w) = weights.get(name.as_str()) {
                        ui.colored_label(Color32::DARK_GRAY, format!("(weight {w:.2})"));
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove this feature")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }

            ui.separator();
            ui.label("Add a feature:");
            let filter_id = egui::Id::new(("w_feat_filter", w_idx));
            let mut filter = ui.data_mut(|d| {
                d.get_temp_mut_or::<String>(filter_id, String::new())
                    .clone()
            });
            if ui
                .add(egui::TextEdit::singleline(&mut filter).hint_text("search features…"))
                .changed()
            {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
            let needle = filter.to_lowercase();
            let already: std::collections::BTreeSet<String> = state.sector.systems[sys_idx].worlds
                [w_idx]
                .world
                .notable_features
                .iter()
                .map(|s| s.as_ref().to_string())
                .collect();
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for v in NotableFeature::VARIANTS {
                        let display = v.display_name();
                        // Storage/lookup key = the stable variant name via `AsRef<str>`
                        // (rename-safe single source; byte-equal to the old `{v:?}` form).
                        let key = v.as_ref().to_string();
                        if !needle.is_empty()
                            && !display.to_lowercase().contains(&needle)
                            && !key.to_lowercase().contains(&needle)
                        {
                            continue;
                        }
                        if already.contains(&key) {
                            continue;
                        }
                        let weight = weights.get(key.as_str()).copied();
                        let label = match weight {
                            Some(w) => format!("{display}  (weight {w:.2})"),
                            None => format!("{display}  (–)"),
                        };
                        if ui
                            .button(label)
                            .on_hover_text(format!("id: {key}"))
                            .clicked()
                        {
                            add = Some(v.clone());
                        }
                    }
                });

            // §R4: apply the pending add/remove to a world clone and dispatch
            // one EditWorld so the feature edit is undoable.
            if remove.is_some() || add.is_some() {
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                if let Err(e) = state.edit_world(wid, |w| {
                    if let Some(i) = remove {
                        w.world.notable_features.remove(i);
                    }
                    if let Some(feature) = add {
                        w.world.notable_features.push(feature);
                    }
                }) {
                    state.feedback.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        },
    );
}

/// §W5 + TF-NT-3: cache-backed feature → weight map. The expensive path
/// (`synthesize_project_input` → `build_pool` → `apply_authored_features`)
/// only runs when the per-world input digest changes; same-frame and
/// adjacent-frame reads return the cached `Arc` without rebuilding the pool.
pub(super) fn feature_weights_for_world(
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) -> std::sync::Arc<std::collections::BTreeMap<String, f64>> {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let world_type = state.sector.systems[sys_idx].worlds[w_idx]
        .world
        .world_type
        .to_string();
    let star_colour = state.sector.systems[sys_idx]
        .star
        .as_ref()
        .map(|s| s.colour_code.clone())
        .unwrap_or_default();
    let worlds_sig = state
        .data_catalogs
        .worlds
        .as_ref()
        .map(|w| {
            crate::builder::derivation_cache::digest_input(&(
                w.generation.len(),
                w.features.global.len(),
                w.features.by_world_type.len(),
                w.features.by_star_colour.len(),
            ))
        })
        .unwrap_or_default();
    let digest = crate::builder::derivation_cache::digest_input(&(
        world_type.as_str(),
        star_colour.as_ref(),
        worlds_sig,
    ));

    let key = (sys_idx, w_idx);
    if let Some(entry) = state.feature_weights_cache.get(&key) {
        if entry.digest == digest {
            return Arc::clone(&entry.weights);
        }
    }

    let Some(input) = state.synthesize_project_input() else {
        let empty = Arc::new(BTreeMap::new());
        state.feature_weights_cache.insert(
            key,
            crate::builder::state::FeatureWeightsCacheValue {
                digest,
                weights: Arc::clone(&empty),
            },
        );
        return empty;
    };
    let mut pool = sectorforge::build_pool(
        &input.catalogs.world_rows,
        &input.catalogs.world_tables,
        &input.config.generation.world_selection,
    );
    if let Some(features) = &input.catalogs.authored_features {
        sectorforge::apply_authored_features(&mut pool, features);
    }
    let world = &state.sector.systems[sys_idx].worlds[w_idx];
    let wt: Option<WorldType> = Some(world.world.world_type.clone());
    let sc: Option<StarColour> = Some(world.world.star_colour);
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    let mut push = |list: &[sectorforge::world_pool::WeightedFeature]| {
        for wf in list {
            let k = wf.feature.as_ref().to_string();
            let entry = out.entry(k).or_insert(0.0);
            *entry += wf.weight;
        }
    };
    if let Some(wt) = wt.as_ref() {
        if let Some(list) = pool.feature_pool.by_world_type.get(wt) {
            push(list);
        }
    }
    if let Some(sc) = sc {
        if let Some(list) = pool.feature_pool.by_star_colour.get(&sc) {
            push(list);
        }
    }
    push(&pool.feature_pool.global);
    let arc = Arc::new(out);
    state.feature_weights_cache.insert(
        key,
        crate::builder::state::FeatureWeightsCacheValue {
            digest,
            weights: Arc::clone(&arc),
        },
    );
    arc
}

/// Human-readable name for a stored notable-feature key (the debug-form variant
/// name persisted on the world). Falls back to the raw key when it doesn't match
/// a known variant, so unknown/legacy values still render.
pub(super) fn feature_display_name(key: &str) -> String {
    NotableFeature::VARIANTS
        .iter()
        .find(|v| v.as_ref() == key)
        .map(|v| v.display_name().to_string())
        .unwrap_or_else(|| key.to_string())
}

// ── coupling warnings (W6) ─────────────────────────────────────────────────

pub(super) fn show_coupling_warnings(
    ui: &mut Ui,
    state: &BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    let warnings = coupling_warnings(&state.sector.systems[sys_idx].worlds[w_idx].world);
    if warnings.is_empty() {
        return;
    }
    ui_kit::collapsing_section(
        ui,
        "world_coupling",
        &format!("Coupling warnings ({})", warnings.len()),
        true,
        |ui| {
            for msg in warnings {
                ui.colored_label(palette::warning(), format!("⚠ {msg}"));
            }
            ui.colored_label(Color32::DARK_GRAY, "non-blocking — adjust if intentional");
        },
    );
}

pub(super) fn coupling_warnings(dto: &sectorforge::sector_model::WorldDto) -> Vec<String> {
    let mut out = Vec::new();
    let wt = &dto.world_type;
    let pop = &dto.population;
    let tech = &dto.tech_level;
    let bio = &dto.biosphere;
    let gov = &dto.government;
    let atm = &dto.atmosphere;
    let is_dense = matches!(pop, Population::DenselyPopulated | Population::ExtremelyDense);
    let is_uninhabited = *pop == Population::Uninhabited;

    if *wt == WorldType::DeathWorld && matches!(tech, TechLevel::High | TechLevel::Archaeotech) {
        out.push("DeathWorld with High/Archaeotech tech is unusual.".into());
    }
    if *wt == WorldType::DeadWorld && !is_uninhabited {
        out.push(format!("DeadWorld is normally Uninhabited (got {pop})."));
    }
    if *wt == WorldType::TombWorld && *bio == Biosphere::Thriving {
        out.push("TombWorld with Thriving biosphere is unusual.".into());
    }
    if *wt == WorldType::Asteroid && is_dense {
        out.push("Asteroid with dense population is unusual.".into());
    }
    if *wt == WorldType::WarpLostWorld && *tech == TechLevel::High {
        out.push("Warp-Lost world with High tech is unusual.".into());
    }
    if *wt == WorldType::ForgeWorld && matches!(tech, TechLevel::Primitive | TechLevel::Low) {
        out.push("ForgeWorld with low tech contradicts its Mechanicus role.".into());
    }
    if *wt == WorldType::FeralWorld && matches!(tech, TechLevel::High | TechLevel::Archaeotech) {
        out.push("FeralWorld with High/Archaeotech tech is unusual.".into());
    }
    if is_uninhabited && *gov != Government::None {
        out.push(format!(
            "Uninhabited world has a government ({gov}); normally None."
        ));
    }
    if *atm == Atmosphere::Airless
        && matches!(
            bio,
            Biosphere::Thriving | Biosphere::XenoHybrid | Biosphere::XenoDominance
        )
    {
        out.push(format!(
            "Airless atmosphere with {bio} biosphere is contradictory."
        ));
    }
    if *atm == Atmosphere::Toxic && *bio == Biosphere::Thriving {
        out.push("Toxic atmosphere with Thriving biosphere is unusual.".into());
    }
    out
}
