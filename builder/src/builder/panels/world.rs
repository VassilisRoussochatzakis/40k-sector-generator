//! WORLD tab (§N1 / §N2) — Phase B §W1..§W7 inspector.
//!
//! Covers every `GeneratedWorld` field via per-section editors backed by the
//! canonical `*::VARIANTS` arrays in [`sectorforge::worlds`] (§W2). Pinning
//! (§W3) is stored in [`BuilderState::pinned_worlds`]. Re-roll (§W4) calls
//! [`BuilderState::regenerate_world`]. Features picker (§W5) is a searchable
//! multi-select with weight preview against the candidate pool. Coupling
//! warnings (§W6) are inline non-blocking heuristics. Claims chip-row (§W7)
//! shows every entry in `world.claims` with quick deep-links to the faction.

use std::sync::Arc;

use egui::{Color32, RichText, Ui};

use sectorforge::ids::FactionId;
use sectorforge::sector_model::{
    ClaimType, DominanceState, FactionClaim, FactionInfluence, PresenceDimensions,
    WorldFactionPresence,
};
use sectorforge::worlds::{
    Atmosphere, Biosphere, Government, NotableFeature, Population, StarColour, TechLevel,
    Temperature, WorldType,
};

use crate::builder::command::BuilderCommand;
use crate::builder::state::{BuilderTab, EntityRef, ModalKind};
use crate::builder::BuilderState;

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("World");
    ui.add_space(4.0);

    let total_worlds: usize = state.sector.systems.iter().map(|s| s.worlds.len()).sum();
    if total_worlds == 0 {
        ui.colored_label(
            Color32::GRAY,
            "No worlds in this sector — add worlds to a system from the SYSTEM tab.",
        );
        return;
    }

    show_world_picker(ui, state);
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let selected = state.selected_world_id.clone();
            let Some(wid) = selected else {
                ui.colored_label(Color32::GRAY, "Select a world from the picker.");
                return;
            };

            let Some((sys_idx, w_idx)) = state.find_world_indices(&wid) else {
                state.selected_world_id = None;
                return;
            };

            show_header(ui, state, sys_idx, w_idx);
            ui.separator();
            show_identity_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_classification_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_environment_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_society_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_features_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_coupling_warnings(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_tags_notes_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_factions_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_claims_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_control_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_overlays_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            crate::builder::panels::conflict::show_world_conflict_section(
                ui, state, sys_idx, w_idx,
            );
            ui.add_space(4.0);
            crate::builder::panels::surface_regions::show_surface_regions_section(
                ui, state, sys_idx, w_idx,
            );
            ui.add_space(4.0);
            crate::builder::panels::intel::show_world_intel_section(ui, state, sys_idx, w_idx);
            ui.add_space(4.0);
            show_chronicle_section(ui, state, sys_idx, w_idx);
            ui.add_space(8.0);
            show_regen_section(ui, state, sys_idx, w_idx);
        });
}

// ── picker / header ─────────────────────────────────────────────────────────

fn show_world_picker(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal(|ui| {
        ui.label("world:");
        let current = state.selected_world_id.clone();
        let label = current
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("world_picker")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for sys in &state.sector.systems {
                    for w in &sys.worlds {
                        let sel = current.as_ref() == Some(&w.id);
                        let label = format!("{} — {} ({})", w.id, w.name, sys.name);
                        if ui.selectable_label(sel, label).clicked() {
                            state.selected_world_id = Some(w.id.clone());
                            state.selected_system_id = Some(sys.id.clone());
                        }
                    }
                }
            });
    });
}

fn show_header(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let w = &state.sector.systems[sys_idx].worlds[w_idx];
    let sys_name = state.sector.systems[sys_idx].name.to_string();
    let sys_id = state.sector.systems[sys_idx].id.clone();
    let wid = w.id.clone();
    let name = w.name.to_string();
    let pinned = state.pinned_worlds.contains(&wid);
    ui.horizontal_wrapped(|ui| {
        ui.heading(name);
        ui.label(
            RichText::new(wid.to_string())
                .color(Color32::GRAY)
                .monospace(),
        );
        ui.colored_label(Color32::DARK_GRAY, format!("in {sys_name}"));
        if sectorforge_gui_core::entity_link(ui, "open system", true).clicked() {
            state.focus_entity(EntityRef::System(sys_id));
        }
        if pinned {
            ui.colored_label(Color32::from_rgb(255, 160, 100), "PINNED");
        }
    });
}

// ── identity (W1 / W3) ──────────────────────────────────────────────────────

fn show_identity_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Identity")
        .default_open(true)
        .show(ui, |ui| {
            let w = &state.sector.systems[sys_idx].worlds[w_idx];
            let wid = w.id.clone();
            let name_buf_key = egui::Id::new(("world_identity_name_buf", wid.as_str()));
            let name_src = w.name.to_string();
            let mut name_buf = name_src.clone();
            let mut orbit = i32::from(w.orbit);
            let mut name_changed = false;
            egui::Grid::new("w_identity_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("id");
                    ui.monospace(wid.to_string());
                    ui.end_row();
                    ui.label("index");
                    ui.monospace(w.index.to_string());
                    ui.end_row();
                    ui.label("source_row_index");
                    ui.monospace(w.source_row_index.to_string());
                    ui.end_row();
                    ui.label("name");
                    let (buf, resp) =
                        crate::builder::panels::persistent_singleline(ui, name_buf_key, &name_src);
                    name_buf = buf;
                    name_changed = resp.lost_focus();
                    ui.end_row();
                    ui.label("orbit");
                    ui.add(egui::DragValue::new(&mut orbit).range(1..=99));
                    ui.end_row();
                    ui.label("pinned");
                    let mut pinned = state.pinned_worlds.contains(&wid);
                    if ui
                        .checkbox(&mut pinned, "(pin from regen/preview)")
                        .changed()
                    {
                        if pinned {
                            state.pinned_worlds.insert(wid.clone());
                        } else {
                            state.pinned_worlds.remove(&wid);
                        }
                    }
                    ui.end_row();
                });
            // §R4: orbit/name now route through the narrow commands so the edit
            // is undoable. Snapshot the live values, then dispatch with no
            // borrow of `state.sector` held across `state.run`.
            let cur_orbit = state.sector.systems[sys_idx].worlds[w_idx].orbit;
            let cur_name = state.sector.systems[sys_idx].worlds[w_idx].name.to_string();
            let new_orbit = orbit.clamp(1, 99) as u8;
            if new_orbit != cur_orbit {
                if let Err(e) = state.run(BuilderCommand::SetWorldOrbit {
                    world: wid.clone(),
                    before: 0,
                    after: new_orbit,
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
            if name_changed && name_buf.trim() != cur_name {
                let after = name_buf.trim().to_string();
                if let Err(e) = state.run(BuilderCommand::RenameWorld {
                    world: wid.clone(),
                    before: String::new(),
                    after,
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
                crate::builder::panels::persistent_text_clear(ui, name_buf_key);
            }
        });
}

// ── classification ─────────────────────────────────────────────────────────

fn show_classification_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    egui::CollapsingHeader::new("Classification")
        .default_open(true)
        .show(ui, |ui| {
            // §R4: edit a clone and dispatch one EditWorld for the whole group
            // so classification changes are undoable.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
            let mut changed = false;
            egui::Grid::new("w_class_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("star_colour");
                    let current_code = draft.world.star_colour_code.to_string();
                    let mut selected = StarColour::VARIANTS
                        .iter()
                        .copied()
                        .find(|v| v.code() == current_code)
                        .unwrap_or(StarColour::Yellow);
                    let prev = selected;
                    egui::ComboBox::from_id_salt("w_star")
                        .selected_text(format!("{} ({})", selected.code(), selected.short_name()))
                        .show_ui(ui, |ui| {
                            for v in StarColour::VARIANTS {
                                ui.selectable_value(
                                    &mut selected,
                                    *v,
                                    format!("{} — {}", v.code(), v.short_name()),
                                );
                            }
                        });
                    if selected != prev {
                        draft.world.star_colour_code = Arc::from(selected.code());
                        draft.world.star_colour = Arc::from(selected.short_name());
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("world_type");
                    if combo_enum::<WorldType>(ui, "w_type", &mut draft.world.world_type) {
                        changed = true;
                    }
                    ui.end_row();
                });
            if changed {
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        });
}

// ── environment ────────────────────────────────────────────────────────────

fn show_environment_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Environment")
        .default_open(false)
        .show(ui, |ui| {
            // §R4: edit a clone and dispatch one EditWorld for the whole group.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
            let mut changed = false;
            egui::Grid::new("w_env_grid").num_columns(2).show(ui, |ui| {
                ui.label("atmosphere");
                if combo_enum::<Atmosphere>(ui, "w_atm", &mut draft.world.atmosphere) {
                    changed = true;
                }
                ui.end_row();
                ui.label("temperature");
                if combo_enum::<Temperature>(ui, "w_temp", &mut draft.world.temperature) {
                    changed = true;
                }
                ui.end_row();
                ui.label("biosphere");
                if combo_enum::<Biosphere>(ui, "w_bio", &mut draft.world.biosphere) {
                    changed = true;
                }
                ui.end_row();
            });
            if changed {
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        });
}

// ── society ────────────────────────────────────────────────────────────────

fn show_society_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Society")
        .default_open(false)
        .show(ui, |ui| {
            // §R4: edit a clone and dispatch one EditWorld for the whole group.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
            let mut changed = false;
            egui::Grid::new("w_soc_grid").num_columns(2).show(ui, |ui| {
                ui.label("population");
                if combo_enum::<Population>(ui, "w_pop", &mut draft.world.population) {
                    changed = true;
                }
                ui.end_row();
                ui.label("tech_level");
                if combo_enum::<TechLevel>(ui, "w_tech", &mut draft.world.tech_level) {
                    changed = true;
                }
                ui.end_row();
                ui.label("government");
                if combo_enum::<Government>(ui, "w_gov", &mut draft.world.government) {
                    changed = true;
                }
                ui.end_row();
            });
            if changed {
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        });
}

// ── features (W5) ──────────────────────────────────────────────────────────

fn show_features_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let feature_count = state.sector.systems[sys_idx].worlds[w_idx]
        .world
        .notable_features
        .len();
    let weights = feature_weights_for_world(state, sys_idx, w_idx);
    let weights = &*weights;
    egui::CollapsingHeader::new(format!("Notable features ({feature_count})"))
        .default_open(false)
        .show(ui, |ui| {
            // §R4: gather the requested mutation (one remove or one add per
            // frame), then apply it to a clone and dispatch one EditWorld below.
            let mut remove: Option<usize> = None;
            let mut add: Option<Arc<str>> = None;
            let cur: Vec<String> = state.sector.systems[sys_idx].worlds[w_idx]
                .world
                .notable_features
                .iter()
                .map(|s| s.to_string())
                .collect();
            for (i, name) in cur.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.monospace(name);
                    if let Some(w) = weights.get(name.as_str()) {
                        ui.colored_label(Color32::DARK_GRAY, format!("(w {w:.2})"));
                    }
                    if ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }

            ui.separator();
            ui.label("Add feature (searchable):");
            let filter_id = egui::Id::new(("w_feat_filter", w_idx));
            let mut filter = ui.data_mut(|d| {
                d.get_temp_mut_or::<String>(filter_id, String::new())
                    .clone()
            });
            if ui.text_edit_singleline(&mut filter).changed() {
                ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            }
            let needle = filter.to_lowercase();
            let already: std::collections::BTreeSet<String> = state.sector.systems[sys_idx].worlds
                [w_idx]
                .world
                .notable_features
                .iter()
                .map(|s| s.to_string())
                .collect();
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for v in NotableFeature::VARIANTS {
                        let display = v.display_name();
                        let key = format!("{v:?}");
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
                            Some(w) => format!("{display}  (w {w:.2})"),
                            None => format!("{display}  (–)"),
                        };
                        if ui.button(label).clicked() {
                            add = Some(Arc::from(key.as_str()));
                        }
                    }
                });

            // §R4: apply the pending add/remove to a world clone and dispatch
            // one EditWorld so the feature edit is undoable.
            if remove.is_some() || add.is_some() {
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                if let Some(i) = remove {
                    draft.world.notable_features.remove(i);
                }
                if let Some(key) = add {
                    draft.world.notable_features.push(key);
                }
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        });
}

/// §W5 + TF-NT-3: cache-backed feature → weight map. The expensive path
/// (`synthesize_project_input` → `build_pool` → `apply_authored_features`)
/// only runs when the per-world input digest changes; same-frame and
/// adjacent-frame reads return the cached `Arc` without rebuilding the pool.
fn feature_weights_for_world(
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) -> std::sync::Arc<std::collections::BTreeMap<String, f64>> {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let world_type = state.sector.systems[sys_idx].worlds[w_idx]
        .world
        .world_type
        .clone();
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
        world_type.as_ref(),
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
    let mut pool = sectorforge::world_pool::build_pool(
        &input.catalogs.world_rows,
        &input.catalogs.world_tables,
        &input.config.generation.world_selection,
    );
    if let Some(features) = &input.catalogs.authored_features {
        sectorforge::world_pool::apply_authored_features(&mut pool, features);
    }
    let world = &state.sector.systems[sys_idx].worlds[w_idx];
    let wt: Option<WorldType> = world.world.world_type.as_ref().parse().ok();
    let sc: Option<StarColour> = world.world.star_colour_code.as_ref().parse().ok();
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    let mut push = |list: &[sectorforge::world_pool::WeightedFeature]| {
        for wf in list {
            let k = format!("{:?}", wf.feature);
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

// ── coupling warnings (W6) ─────────────────────────────────────────────────

fn show_coupling_warnings(ui: &mut Ui, state: &BuilderState, sys_idx: usize, w_idx: usize) {
    let warnings = coupling_warnings(&state.sector.systems[sys_idx].worlds[w_idx].world);
    if warnings.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("Coupling warnings ({})", warnings.len()))
        .default_open(true)
        .show(ui, |ui| {
            for msg in warnings {
                ui.colored_label(Color32::from_rgb(220, 180, 60), format!("⚠ {msg}"));
            }
            ui.colored_label(Color32::DARK_GRAY, "non-blocking — adjust if intentional");
        });
}

fn coupling_warnings(dto: &sectorforge::sector_model::WorldDto) -> Vec<String> {
    let mut out = Vec::new();
    let wt = dto.world_type.as_ref();
    let pop = dto.population.as_ref();
    let tech = dto.tech_level.as_ref();
    let bio = dto.biosphere.as_ref();
    let gov = dto.government.as_ref();
    let atm = dto.atmosphere.as_ref();
    let is_dense = matches!(pop, "DenselyPopulated" | "ExtremelyDense");
    let is_uninhabited = pop == "Uninhabited";

    if wt == "DeathWorld" && matches!(tech, "High" | "Archaeotech") {
        out.push("DeathWorld with High/Archaeotech tech is unusual.".into());
    }
    if wt == "DeadWorld" && !is_uninhabited {
        out.push(format!("DeadWorld is normally Uninhabited (got {pop})."));
    }
    if wt == "TombWorld" && bio == "Thriving" {
        out.push("TombWorld with Thriving biosphere is unusual.".into());
    }
    if wt == "Asteroid" && is_dense {
        out.push("Asteroid with dense population is unusual.".into());
    }
    if wt == "WarpLostWorld" && tech == "High" {
        out.push("Warp-Lost world with High tech is unusual.".into());
    }
    if wt == "ForgeWorld" && matches!(tech, "Primitive" | "Low") {
        out.push("ForgeWorld with low tech contradicts its Mechanicus role.".into());
    }
    if wt == "FeralWorld" && matches!(tech, "High" | "Archaeotech") {
        out.push("FeralWorld with High/Archaeotech tech is unusual.".into());
    }
    if is_uninhabited && gov != "None" {
        out.push(format!(
            "Uninhabited world has a government ({gov}); normally None."
        ));
    }
    if atm == "Airless" && matches!(bio, "Thriving" | "XenoHybrid" | "XenoDominance") {
        out.push(format!(
            "Airless atmosphere with {bio} biosphere is contradictory."
        ));
    }
    if atm == "Toxic" && bio == "Thriving" {
        out.push("Toxic atmosphere with Thriving biosphere is unusual.".into());
    }
    out
}

// ── tags + notes ────────────────────────────────────────────────────────────

fn show_tags_notes_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Tags + Notes")
        .default_open(false)
        .show(ui, |ui| {
            let wid_key = state.sector.systems[sys_idx].worlds[w_idx]
                .id
                .as_str()
                .to_string();
            let tags_key = egui::Id::new(("world_tags_buf", wid_key.as_str()));
            let notes_key = egui::Id::new(("world_notes_buf", wid_key.as_str()));
            let tags_src = state.sector.systems[sys_idx].worlds[w_idx]
                .tags
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let notes_src = state.sector.systems[sys_idx].worlds[w_idx]
                .notes
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            ui.label("tags (comma-separated)");
            let (tags_buf, tags_resp) =
                crate::builder::panels::persistent_singleline(ui, tags_key, &tags_src);
            let tags_changed = tags_resp.lost_focus();
            ui.label("notes (one per line)");
            let (notes_buf, notes_resp) =
                crate::builder::panels::persistent_multiline(ui, notes_key, &notes_src);
            let notes_changed = notes_resp.lost_focus();
            if tags_changed {
                // §R4: commit tags via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                draft.tags = tags_buf
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
                crate::builder::panels::persistent_text_clear(ui, tags_key);
            }
            if notes_changed {
                // §R4: commit notes via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                draft.notes = notes_buf
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(Arc::from)
                    .collect();
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
                crate::builder::panels::persistent_text_clear(ui, notes_key);
            }
        });
}

// ── factions ──────────────────────────────────────────────────────────────

fn show_factions_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Faction presence")
        .default_open(false)
        .show(ui, |ui| {
            let presences = state.sector.systems[sys_idx].worlds[w_idx].factions.clone();
            if presences.is_empty() {
                ui.colored_label(Color32::GRAY, "no faction presence on this world");
            }
            let mut remove_idx: Option<usize> = None;
            for (i, p) in presences.iter().enumerate() {
                ui.horizontal(|ui| {
                    if sectorforge_gui_core::entity_link(ui, p.faction_id.to_string(), true)
                        .clicked()
                    {
                        state.focus_entity(EntityRef::Faction(p.faction_id.clone()));
                    }
                    ui.colored_label(
                        Color32::DARK_GRAY,
                        format!(
                            "infl {:?} · {} · dom {:?}",
                            p.influence, p.relationship_to_government, p.dominance
                        ),
                    );
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove this presence")
                        .clicked()
                    {
                        remove_idx = Some(i);
                    }
                });
            }
            if let Some(i) = remove_idx {
                // §R4: remove the presence via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                draft.factions.remove(i);
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
            ui.separator();
            show_add_presence_row(ui, state, sys_idx, w_idx);
        });
}

const INFLUENCE_TIERS: &[FactionInfluence] = &[
    FactionInfluence::Dominant,
    FactionInfluence::Significant,
    FactionInfluence::Minor,
    FactionInfluence::Hidden,
];

const DOMINANCE_STATES: &[DominanceState] = &[
    DominanceState::Rumored,
    DominanceState::Presence,
    DominanceState::Influence,
    DominanceState::Contested,
    DominanceState::Controlled,
    DominanceState::Stronghold,
];

fn show_add_presence_row(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let factions: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    if factions.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "no factions in the sector roster — add factions in FACTIONS first.",
        );
        return;
    }
    let already: std::collections::BTreeSet<FactionId> = state.sector.systems[sys_idx].worlds
        [w_idx]
        .factions
        .iter()
        .map(|p| p.faction_id.clone())
        .collect();
    let candidates: Vec<&(FactionId, String)> = factions
        .iter()
        .filter(|(fid, _)| !already.contains(fid))
        .collect();
    if candidates.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "every faction in the roster already has a presence row here.",
        );
        return;
    }

    let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
    let buf_id = egui::Id::new(("w_add_presence", world_id.as_str()));
    #[derive(Clone)]
    struct Buf {
        faction: FactionId,
        tier: FactionInfluence,
        dominance: DominanceState,
    }
    let default = Buf {
        faction: candidates[0].0.clone(),
        tier: FactionInfluence::Minor,
        dominance: DominanceState::Presence,
    };
    let mut buf: Buf = ui.data_mut(|d| d.get_temp::<Buf>(buf_id).unwrap_or(default));
    if !candidates.iter().any(|(fid, _)| fid == &buf.faction) {
        buf.faction = candidates[0].0.clone();
    }

    ui.horizontal_wrapped(|ui| {
        ui.label("+ Add presence:");
        let label = candidates
            .iter()
            .find(|(fid, _)| fid == &buf.faction)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| candidates[0].1.clone());
        egui::ComboBox::from_id_salt(("w_add_fac", world_id.as_str()))
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (fid, n) in &candidates {
                    if ui.selectable_label(&buf.faction == fid, n).clicked() {
                        buf.faction = (*fid).clone();
                    }
                }
            });
        egui::ComboBox::from_id_salt(("w_add_tier", world_id.as_str()))
            .selected_text(format!("{}", buf.tier))
            .show_ui(ui, |ui| {
                for t in INFLUENCE_TIERS {
                    ui.selectable_value(&mut buf.tier, *t, format!("{t}"));
                }
            });
        egui::ComboBox::from_id_salt(("w_add_dom", world_id.as_str()))
            .selected_text(format!("{}", buf.dominance))
            .show_ui(ui, |ui| {
                for d in DOMINANCE_STATES {
                    ui.selectable_value(&mut buf.dominance, *d, format!("{d}"));
                }
            });
        if ui.button("+ presence").clicked() {
            // §R4: add the presence via EditWorld on a world clone.
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
            draft.factions.push(WorldFactionPresence {
                faction_id: buf.faction.clone(),
                subfaction_id: None,
                subfaction_name: None,
                force_id: None,
                force_name: None,
                influence: buf.tier,
                relationship_to_government: "neutral".into(),
                dimensions: PresenceDimensions::default(),
                dominance: buf.dominance,
                intel_confidence: 100,
            });
            if let Err(e) = state.run(BuilderCommand::EditWorld {
                world: wid,
                before: None,
                after: Box::new(draft),
            }) {
                state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(buf_id, buf));
}

// ── claims chip-row (W7) ───────────────────────────────────────────────────

fn show_claims_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let claim_count = state.sector.systems[sys_idx].worlds[w_idx].claims.len();
    egui::CollapsingHeader::new(format!("Claims ({claim_count})"))
        .default_open(true)
        .show(ui, |ui| {
            let claims = state.sector.systems[sys_idx].worlds[w_idx].claims.clone();
            let mut remove: Option<usize> = None;
            ui.horizontal_wrapped(|ui| {
                for (i, c) in claims.iter().enumerate() {
                    let (bg, fg) = claim_chip_colours(c.claim_type);
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, fg))
                        .rounding(4.0)
                        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let label =
                                    format!("{}  {}  {}", c.faction_id, c.claim_type, c.strength);
                                let resp = ui.label(RichText::new(label).color(fg).monospace());
                                if resp.clicked() {
                                    state.focus_entity(EntityRef::Faction(c.faction_id.clone()));
                                }
                                if ui.small_button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                }
                if claims.is_empty() {
                    ui.colored_label(Color32::GRAY, "no claims on this world");
                }
            });
            if let Some(i) = remove {
                // §R4: remove the claim via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                draft.claims.remove(i);
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
            ui.add_space(4.0);
            show_add_claim_row(ui, state, sys_idx, w_idx);
        });
}

fn show_add_claim_row(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let factions: Vec<_> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    if factions.is_empty() {
        return;
    }
    let row_id = egui::Id::new(("w_add_claim", sys_idx, w_idx));
    #[derive(Clone, Default)]
    struct AddBuf {
        faction: Option<sectorforge::ids::FactionId>,
        claim_type: Option<ClaimType>,
        strength: u8,
    }
    let mut buf: AddBuf = ui.data_mut(|d| d.get_temp::<AddBuf>(row_id).unwrap_or_default());
    if buf.faction.is_none() {
        buf.faction = Some(factions[0].0.clone());
    }
    if buf.claim_type.is_none() {
        buf.claim_type = Some(ClaimType::LegalSovereignty);
    }
    if buf.strength == 0 {
        buf.strength = 50;
    }

    ui.horizontal(|ui| {
        ui.label("add:");
        let selected_label = buf
            .faction
            .as_ref()
            .and_then(|fid| {
                factions
                    .iter()
                    .find(|(id, _)| id == fid)
                    .map(|(_, n)| n.clone())
            })
            .unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt(("w_add_claim_fac", sys_idx, w_idx))
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                for (fid, name) in &factions {
                    if ui
                        .selectable_label(buf.faction.as_ref() == Some(fid), name)
                        .clicked()
                    {
                        buf.faction = Some(fid.clone());
                    }
                }
            });
        egui::ComboBox::from_id_salt(("w_add_claim_kind", sys_idx, w_idx))
            .selected_text(format!(
                "{}",
                buf.claim_type.unwrap_or(ClaimType::LegalSovereignty)
            ))
            .show_ui(ui, |ui| {
                for c in CLAIM_TYPES {
                    ui.selectable_value(&mut buf.claim_type, Some(*c), format!("{c}"));
                }
            });
        ui.add(egui::DragValue::new(&mut buf.strength).range(0..=100));
        if ui.button("+ claim").clicked() {
            if let (Some(fid), Some(kind)) = (buf.faction.clone(), buf.claim_type) {
                // §R4: add the claim via EditWorld on a world clone.
                let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
                let mut draft = state.sector.systems[sys_idx].worlds[w_idx].clone();
                draft.claims.push(FactionClaim {
                    faction_id: fid,
                    claim_type: kind,
                    strength: buf.strength,
                });
                if let Err(e) = state.run(BuilderCommand::EditWorld {
                    world: wid,
                    before: None,
                    after: Box::new(draft),
                }) {
                    state.modal = Some(ModalKind::Message(format!("World edit failed: {e}")));
                }
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(row_id, buf));
}

const CLAIM_TYPES: &[ClaimType] = &[
    ClaimType::LegalSovereignty,
    ClaimType::ImperialMandate,
    ClaimType::TreatyRight,
    ClaimType::ReligiousMandate,
    ClaimType::DynasticRight,
    ClaimType::CommercialCharter,
    ClaimType::MilitaryOccupation,
    ClaimType::AncientDomain,
    ClaimType::HuntingGround,
    ClaimType::CovertWrit,
    ClaimType::Rebellion,
];

fn claim_chip_colours(kind: ClaimType) -> (Color32, Color32) {
    match kind {
        ClaimType::LegalSovereignty => (Color32::from_rgb(40, 60, 100), Color32::LIGHT_BLUE),
        ClaimType::ImperialMandate => (Color32::from_rgb(80, 70, 30), Color32::YELLOW),
        ClaimType::TreatyRight => (Color32::from_rgb(40, 80, 80), Color32::LIGHT_GREEN),
        ClaimType::ReligiousMandate => (Color32::from_rgb(80, 60, 30), Color32::LIGHT_YELLOW),
        ClaimType::DynasticRight => (Color32::from_rgb(80, 30, 70), Color32::LIGHT_RED),
        ClaimType::CommercialCharter => (Color32::from_rgb(40, 90, 50), Color32::GREEN),
        ClaimType::MilitaryOccupation => (Color32::from_rgb(100, 30, 30), Color32::LIGHT_RED),
        ClaimType::AncientDomain => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
        ClaimType::HuntingGround => (Color32::from_rgb(60, 50, 30), Color32::LIGHT_YELLOW),
        ClaimType::CovertWrit => (Color32::from_rgb(30, 30, 60), Color32::LIGHT_BLUE),
        ClaimType::Rebellion => (Color32::from_rgb(120, 30, 30), Color32::LIGHT_RED),
        _ => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
    }
}

// ── control summary ────────────────────────────────────────────────────────

fn show_control_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Control summary (read-only)")
        .default_open(false)
        .show(ui, |ui| {
            let c = state.sector.systems[sys_idx].worlds[w_idx].control.clone();
            ui.label(format!("dominant: {:?}", c.dominant));
            ui.label(format!("sovereign: {:?}", c.sovereign));
            ui.label(format!("occupier: {:?}", c.occupier));
            ui.label(format!("economic_hegemon: {:?}", c.economic_hegemon));
            ui.label(format!("popular_authority: {:?}", c.popular_authority));
            ui.label(format!("hidden_master: {:?}", c.hidden_master));
            ui.label(format!(
                "contested: {} · control_score: {:.1}",
                c.contested, c.control_score
            ));
        });
}

// ── overlays read-only ─────────────────────────────────────────────────────

fn show_overlays_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Overlays (summary)")
        .default_open(false)
        .show(ui, |ui| {
            let w = &state.sector.systems[sys_idx].worlds[w_idx];
            ui.label(format!("surface_regions: {} (edit below)", w.regions.len()));
            ui.label(format!(
                "conflict default: {}",
                sectorforge::conflict::ConflictState::is_default(&w.conflict)
            ));
            ui.label(format!(
                "stability default: {}",
                sectorforge::stability::StabilityState::is_default(&w.stability)
            ));
        });
}

// ── §H8 chronicle snippets ─────────────────────────────────────────────────

fn show_chronicle_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    let sys_id = state.sector.systems[sys_idx].id.clone();
    let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
    // Snapshot rows up-front so the closure body can mutate `state` freely.
    let rows: Vec<(String, String, String, String, bool)> = {
        let events =
            crate::builder::panels::history::world_chronicle_events(state, &sys_id, &world_id);
        events
            .iter()
            .map(|e| {
                (
                    e.id.clone(),
                    e.date.clone(),
                    crate::builder::panels::history::kind_label(e.kind).to_string(),
                    e.narrative.clone(),
                    e.manual,
                )
            })
            .collect()
    };
    let count = rows.len();
    egui::CollapsingHeader::new(format!("Chronicle snippets ({count})"))
        .default_open(false)
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.colored_label(
                    Color32::GRAY,
                    "No chronicle events anchored at this world. Open HISTORY → Regenerate.",
                );
                if sectorforge_gui_core::entity_link(ui, "HISTORY tab", true).clicked() {
                    state.focus_entity(EntityRef::Tab(BuilderTab::History));
                }
                return;
            }
            let mut jump_to: Option<String> = None;
            for (id, date, kind, narrative, manual) in &rows {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(date).monospace().strong());
                    ui.label(kind.as_str());
                    if *manual {
                        ui.colored_label(Color32::from_rgb(200, 220, 120), "manual");
                    }
                    if ui.small_button("→ HISTORY").clicked() {
                        jump_to = Some(id.clone());
                    }
                });
                ui.colored_label(Color32::DARK_GRAY, narrative);
                ui.separator();
            }
            if let Some(id) = jump_to {
                state.focus_entity(EntityRef::HistoryEvent(id));
            }
        });
}

// ── W4 regen ───────────────────────────────────────────────────────────────

fn show_regen_section(ui: &mut Ui, state: &mut BuilderState, sys_idx: usize, w_idx: usize) {
    egui::CollapsingHeader::new("Re-roll from candidate pool")
        .default_open(false)
        .show(ui, |ui| {
            let pinned = state
                .pinned_worlds
                .contains(&state.sector.systems[sys_idx].worlds[w_idx].id);
            let wid = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
            if pinned {
                ui.colored_label(
                    Color32::from_rgb(255, 160, 100),
                    "pinned — unpin to re-roll",
                );
                return;
            }
            ui.label("Redraws star_colour/world_type/atmosphere/temperature/biosphere/population/tech_level/government and notable_features against the current data catalogs.");
            ui.label(
                RichText::new(format!("counter: {}", state.world_reroll_counter))
                    .color(Color32::DARK_GRAY)
                    .monospace(),
            );
            if ui.button("Re-roll this world").clicked() {
                if let Err(e) = state.regenerate_world(&wid) {
                    state.modal = Some(ModalKind::Message(format!("World re-roll failed: {e}")));
                }
            }
        });
}

// ── combo helper ───────────────────────────────────────────────────────────

trait EnumPicker: Sized + Clone + PartialEq + 'static {
    fn variants() -> &'static [Self];
    fn display(&self) -> &'static str;
    fn debug_key(&self) -> String;
}

impl EnumPicker for WorldType {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for Atmosphere {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for Temperature {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for Biosphere {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for Population {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for TechLevel {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}
impl EnumPicker for Government {
    fn variants() -> &'static [Self] {
        Self::VARIANTS
    }
    fn display(&self) -> &'static str {
        self.display_name()
    }
    fn debug_key(&self) -> String {
        format!("{self:?}")
    }
}

fn combo_enum<E: EnumPicker>(ui: &mut Ui, salt: &str, target: &mut Arc<str>) -> bool {
    let current_key = target.to_string();
    let mut selected = E::variants()
        .iter()
        .find(|v| v.debug_key() == current_key)
        .cloned()
        .unwrap_or_else(|| E::variants()[0].clone());
    let prev = selected.clone();
    egui::ComboBox::from_id_salt(salt)
        .selected_text(selected.display())
        .show_ui(ui, |ui| {
            for v in E::variants() {
                ui.selectable_value(&mut selected, v.clone(), v.display());
            }
        });
    if selected != prev {
        *target = Arc::from(selected.debug_key().as_str());
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{HexCoord, WorldDto};

    fn world_dto() -> WorldDto {
        WorldDto {
            star_colour: Arc::from("yellow"),
            star_colour_code: Arc::from("G"),
            world_type: Arc::from("AgriWorld"),
            atmosphere: Arc::from("Breathable"),
            temperature: Arc::from("Temperate"),
            biosphere: Arc::from("Thriving"),
            population: Arc::from("DenselyPopulated"),
            tech_level: Arc::from("Standard"),
            government: Arc::from("ImperialWorld"),
            notable_features: Vec::new(),
        }
    }

    #[test]
    fn coupling_flags_dead_world_with_population() {
        let mut dto = world_dto();
        dto.world_type = Arc::from("DeadWorld");
        let warns = coupling_warnings(&dto);
        assert!(warns.iter().any(|w| w.contains("DeadWorld")));
    }

    #[test]
    fn coupling_flags_uninhabited_with_government() {
        let mut dto = world_dto();
        dto.population = Arc::from("Uninhabited");
        let warns = coupling_warnings(&dto);
        assert!(warns.iter().any(|w| w.contains("government")));
    }

    #[test]
    fn coupling_silent_on_normal_world() {
        let dto = world_dto();
        let warns = coupling_warnings(&dto);
        assert!(warns.is_empty(), "got unexpected warnings: {warns:?}");
    }

    #[test]
    fn pinned_world_refuses_regen() {
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let sid = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "S")
            .unwrap();
        let wid = state.sector.add_world_to_system(&sid, "W").unwrap();
        state.pinned_worlds.insert(wid.clone());
        let err = state.regenerate_world(&wid).unwrap_err();
        assert!(err.to_string().contains("pinned"));
    }

    #[test]
    fn enum_picker_variants_match_worlds_authoritative_set() {
        // §W2 audit: panel uses VARIANTS directly so it cannot drift.
        assert_eq!(WorldType::VARIANTS.len(), 24);
        assert_eq!(Atmosphere::VARIANTS.len(), 7);
        assert_eq!(Temperature::VARIANTS.len(), 5);
        assert_eq!(Biosphere::VARIANTS.len(), 6);
        assert_eq!(Population::VARIANTS.len(), 6);
        assert_eq!(TechLevel::VARIANTS.len(), 6);
        assert_eq!(Government::VARIANTS.len(), 30);
        assert!(NotableFeature::VARIANTS.len() >= 90);
    }
}
