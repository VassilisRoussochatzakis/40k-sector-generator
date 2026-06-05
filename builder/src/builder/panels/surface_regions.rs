//! §SU1 / §SU2 (BUILDER_REQS §32) — per-world surface-region editor.
//!
//! Rendered from the WORLD tab via [`show_surface_regions_section`].
//! Mutations route through [`BuilderCommand::SetSurfaceRegions`] so every
//! edit is undoable. The `Auto-seed` button calls
//! [`sectorforge::surface_region::derive_regions`] (§SU2) and replaces the
//! list wholesale. Each row exposes name, kind, dominant faction, control
//! score, population weight, visibility, and free-form notes.

use egui::Ui;

use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};

use sectorforge::ids::FactionId;
use sectorforge::surface_region::{derive_regions, RegionKind, SurfaceRegion};

use crate::builder::command::BuilderCommand;
use crate::builder::state::ModalKind;
use crate::builder::BuilderState;

const REGION_KINDS: [RegionKind; 12] = [
    RegionKind::Capital,
    RegionKind::Hive,
    RegionKind::Underhive,
    RegionKind::ForgeComplex,
    RegionKind::ShrineContinent,
    RegionKind::AgriBelt,
    RegionKind::CardinalSpire,
    RegionKind::KnightHousehold,
    RegionKind::Wilderness,
    RegionKind::TombComplex,
    RegionKind::Hideout,
    RegionKind::Other,
];

/// Human-readable name for a region kind. The domain enum only exposes a
/// snake_case slug (`as_slug`), so we title-case it locally for display and
/// keep the raw slug in a hover. Editing `src/` to add a `display_name()` is
/// out of scope for the friendly pass.
fn region_kind_label(kind: RegionKind) -> String {
    kind.as_slug()
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn show_surface_regions_section(
    ui: &mut Ui,
    state: &mut BuilderState,
    sys_idx: usize,
    w_idx: usize,
) {
    ui_kit::collapsing_section(ui, "sreg_surface_regions", "Surface regions", false, |ui| {
        let world_id = state.sector.systems[sys_idx].worlds[w_idx].id.clone();
        let factions: Vec<(FactionId, String)> = state
            .sector
            .factions
            .iter()
            .map(|f| (f.id.clone(), f.name.to_string()))
            .collect();

        let mut working = state.sector.systems[sys_idx].worlds[w_idx].regions.clone();
        let original = working.clone();
        show_regions_editor(ui, &world_id, &mut working, &factions);

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("✨  Auto-seed")
                .on_hover_text(
                    "Generate a starting set of regions for this world from its type and \
                     population, replacing the list below.",
                )
                .clicked()
            {
                let w = &state.sector.systems[sys_idx].worlds[w_idx];
                working = derive_regions(w);
            }
            if ui
                .button("🗑  Clear regions")
                .on_hover_text("Remove every region from this world (not undoable via this list)")
                .clicked()
            {
                working.clear();
            }
        });

        if working != original {
            let cmd = BuilderCommand::SetSurfaceRegions {
                world: world_id,
                before: None,
                after: working,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!(
                    "Surface region update failed: {e}"
                )));
            }
        }
    });
}

fn show_regions_editor(
    ui: &mut Ui,
    world_id: &sectorforge::ids::WorldId,
    regions: &mut Vec<SurfaceRegion>,
    factions: &[(FactionId, String)],
) {
    if regions.is_empty() {
        ui_kit::placeholder(
            ui,
            "No regions yet — use ✨ Auto-seed to generate them, or ➕ Add region below.",
        );
    }
    let mut remove_at: Option<usize> = None;
    for (i, region) in regions.iter_mut().enumerate() {
        let dominant_label = region
            .dominant
            .as_ref()
            .map(|f| faction_name(factions, f))
            .unwrap_or_else(|| "unclaimed".into());
        let header_label = format!(
            "{n}. {kind} — {name}  · {fid}",
            n = i + 1,
            kind = region_kind_label(region.kind),
            name = region.name,
            fid = dominant_label,
        );
        ui_kit::collapsing_section(
            ui,
            ("sreg_region", world_id, i),
            &header_label,
            false,
            |ui| {
                labeled(
                    ui,
                    "Name",
                    "Display name for this region (schema: name).",
                    |ui| {
                        ui.text_edit_singleline(&mut region.name);
                    },
                );
                labeled(
                    ui,
                    "Kind",
                    "What kind of place this is (schema: kind). Sets the flavour and the \
                     default control bias when auto-seeding.",
                    |ui| {
                        ui_kit::combo(format!("sr_kind_{i}"), region_kind_label(region.kind))
                            .show_ui(ui, |ui| {
                                for k in REGION_KINDS {
                                    ui.selectable_value(&mut region.kind, k, region_kind_label(k))
                                        .on_hover_text(format!("key: {}", k.as_slug()));
                                }
                            });
                    },
                );
                labeled(
                    ui,
                    "Dominant faction",
                    "Which faction holds this region (schema: dominant). May differ from the \
                     world's overall ruler. Leave unclaimed for contested ground.",
                    |ui| {
                        optional_faction_combo(
                            ui,
                            &format!("sr_fac_{i}"),
                            &mut region.dominant,
                            factions,
                        );
                    },
                );
                labeled(
                    ui,
                    "Control",
                    "How firmly the dominant faction holds this region, 0–100 (schema: \
                     control_score).",
                    |ui| {
                        ui.add(egui::Slider::new(&mut region.control_score, 0..=100).text("/100"));
                    },
                );
                labeled(
                    ui,
                    "Population share",
                    "This region's share of the world's people, 0–100 (schema: \
                     population_weight). Shares across regions should add up to at most 100.",
                    |ui| {
                        ui.add(
                            egui::Slider::new(&mut region.population_weight, 0..=100).text("/100"),
                        );
                    },
                );
                labeled(
                    ui,
                    "Visibility",
                    "How visible this region is from orbit, 0–100 (schema: visibility). \
                     Underground places read low even on a bright world.",
                    |ui| {
                        ui.add(egui::Slider::new(&mut region.visibility, 0..=100).text("/100"));
                    },
                );
                labeled(
                    ui,
                    "Notes",
                    "Free-form notes for your own reference (schema: notes).",
                    |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut region.notes)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                    },
                );

                ui.add_space(4.0);
                if ui
                    .button("🗑  Remove region")
                    .on_hover_text("Delete this region from the world")
                    .clicked()
                {
                    remove_at = Some(i);
                }
            },
        );
    }
    if let Some(i) = remove_at {
        regions.remove(i);
    }
    ui.add_space(4.0);
    let total: u32 = regions.iter().map(|r| u32::from(r.population_weight)).sum();
    if total > 100 {
        ui.colored_label(
            palette::warning(),
            format!("Population shares add up to {total} (over 100 — trim some down)"),
        );
    }
    if ui
        .button("➕  Add region")
        .on_hover_text("Add a new, blank surface region to this world")
        .clicked()
    {
        let default_fid = factions.first().map(|(f, _)| f.clone());
        regions.push(SurfaceRegion {
            name: format!("Region {}", regions.len() + 1),
            kind: RegionKind::Other,
            dominant: default_fid,
            control_score: 50,
            population_weight: 10,
            visibility: 50,
            notes: String::new(),
        });
    }
}

fn optional_faction_combo(
    ui: &mut Ui,
    id_salt: &str,
    current: &mut Option<FactionId>,
    factions: &[(FactionId, String)],
) {
    let label = current
        .as_ref()
        .map(|f| faction_name(factions, f))
        .unwrap_or_else(|| "(unclaimed)".into());
    ui_kit::combo(id_salt, label).show_ui(ui, |ui| {
        if ui
            .selectable_label(current.is_none(), "(unclaimed)")
            .clicked()
        {
            *current = None;
        }
        for (fid, name) in factions {
            let sel = current.as_ref() == Some(fid);
            if ui
                .selectable_label(sel, name.as_str())
                .on_hover_text(format!("id: {fid}"))
                .clicked()
            {
                *current = Some(fid.clone());
            }
        }
    });
}

/// Display name for a faction id, falling back to the raw id when it isn't in
/// the sector roster (e.g. a region carried over from an external file).
fn faction_name(factions: &[(FactionId, String)], id: &FactionId) -> String {
    factions
        .iter()
        .find(|(fid, _)| fid == id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| id.to_string())
}
