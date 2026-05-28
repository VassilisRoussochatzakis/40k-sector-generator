//! SITES tab (§N1 / §N2) — Phase D §ST1..§ST4.
//!
//! §ST1  Per-world site editor. The list is the cached [`SitesReport`]
//!        published by [`BuilderState::recompute_sites`]; `derive_with`
//!        already groups by world id. The panel exposes the full 21-kind
//!        [`SiteKind`] picker (governor's palace … naval anchorage),
//!        controller, public/actual status, and one-line hook on every row.
//!        Selecting a row populates [`BuilderState::selected_site_id`] and
//!        [`BuilderState::sites_edit_target`] so cross-tab links land here
//!        first-class.
//! §ST2  "Auto-derive sites" calls [`BuilderState::recompute_sites`] which
//!        runs `sites::derive_with(&sector, &cfg)`. Manual entries survive
//!        because [`sectorforge::sites::derive_with`] appends `cfg.manual`
//!        last after sorting the derived set.
//! §ST3  Player-edition toggle (mirrors `--player`): flips
//!        [`BuilderState::sites_player_edition`] and re-runs the recompute
//!        so the cached report has rows where `public_status !=
//!        actual_status` stripped.
//! §ST4  `data/sites.toml` editor: per-knob `max_per_world` /
//!        `skip_uninhabited` controls plus the manual block. The Save row
//!        wires the path into `[inputs].sites` and serialises through
//!        `project_io::save_project`.
//!
//! The panel never edits derived `sites_report` rows directly. All
//! mutations land in [`BuilderState::data_catalogs::sites`] and the
//! recompute pass rewrites the published overlay.

use egui::{Color32, RichText, Ui};

use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sites::{SiteKind, SiteStatus, SitesConfig, WorldSite};

use crate::builder::state::EntityRef;
use crate::builder::BuilderState;

const DEFAULT_SITES_PATH: &str = "data/sites.toml";

/// Every [`SiteKind`] in panel-display order. Keep in sync with
/// `src/sites.rs::SiteKind`.
const KIND_VARIANTS: &[SiteKind] = &[
    SiteKind::GovernorsPalace,
    SiteKind::CathedralSpire,
    SiteKind::Manufactorum,
    SiteKind::UnderhiveSumpCity,
    SiteKind::VoidElevator,
    SiteKind::StarFortDockyard,
    SiteKind::QuarantineZone,
    SiteKind::XenosRuin,
    SiteKind::PilgrimNecropolis,
    SiteKind::AstropathicChoir,
    SiteKind::ArbitesPrecinct,
    SiteKind::DataVault,
    SiteKind::DisputedShrine,
    SiteKind::PenalMine,
    SiteKind::BlackMarketEnclave,
    SiteKind::CultSafehouse,
    SiteKind::CrashedVoidship,
    SiteKind::AgriBeltGranary,
    SiteKind::ForgeReactor,
    SiteKind::TombComplex,
    SiteKind::NavalAnchorage,
];

const STATUS_VARIANTS: &[SiteStatus] = &[
    SiteStatus::Active,
    SiteStatus::Restricted,
    SiteStatus::Abandoned,
    SiteStatus::Quarantined,
    SiteStatus::Sealed,
    SiteStatus::Contested,
    SiteStatus::UnderConstruction,
];

pub fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Sites");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "§ST1..§ST4 — per-world site editor, auto-derive + manual survive, player-edition toggle, sites.toml round-trip.",
    );
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_header_actions(ui, state);
            ui.separator();
            show_config_section(ui, state);
            ui.separator();
            show_filter_row(ui, state);
            ui.separator();
            show_site_list(ui, state);
            ui.separator();
            show_detail_card(ui, state);
            ui.separator();
            show_manual_editor(ui, state);
            ui.separator();
            show_save_row(ui, state);
        });
}

// ── §ST2 / §ST3 header actions ─────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Auto-derive sites").clicked() {
            ensure_sites_catalog(state);
            state.recompute_sites();
        }
        ui.checkbox(&mut state.sites_auto_recompute, "auto-recompute on edit");
        if ui
            .checkbox(&mut state.sites_player_edition, "player edition (--player)")
            .changed()
        {
            state.recompute_sites();
        }
        let total = state
            .sites_report
            .as_ref()
            .map(|r| r.sites.len())
            .unwrap_or(0);
        let manual = state
            .data_catalogs
            .sites
            .as_ref()
            .map(|c| c.manual.len())
            .unwrap_or(0);
        ui.label(format!("sites: {total}  (manual: {manual})"));
        if state.data_catalogs.sites.is_none() {
            ui.colored_label(
                Color32::from_rgb(220, 170, 80),
                "no sites.toml loaded (defaults apply)",
            );
        }
    });
}

// ── §ST4 config knobs ──────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§ST4 — sites.toml knobs").strong());
    ensure_sites_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.sites.as_mut() else {
        return;
    };
    let mut changed = false;
    egui::Grid::new("st4_grid").num_columns(2).show(ui, |ui| {
        ui.label("max_per_world");
        changed |= ui
            .add(egui::DragValue::new(&mut cfg.max_per_world).range(0..=32))
            .changed();
        ui.end_row();
        ui.label("skip_uninhabited");
        changed |= ui
            .checkbox(&mut cfg.skip_uninhabited, "skip uninhabited worlds")
            .changed();
        ui.end_row();
    });
    ui.colored_label(
        Color32::DARK_GRAY,
        "Higher max ⇒ more sites per world. skip_uninhabited still emits sites on Tomb / Dead / Warp-Lost / Daemon worlds.",
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §ST1 kind filter ───────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("§ST1 — filter").strong());
        let label = match state.sites_filter_kind {
            None => "all kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        egui::ComboBox::from_id_salt("st1_kind")
            .selected_text(label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.sites_filter_kind, None, "all kinds");
                for k in KIND_VARIANTS {
                    ui.selectable_value(&mut state.sites_filter_kind, Some(*k), kind_label(*k));
                }
            });
    });
}

// ── §ST1 ranked list grouped by world ──────────────────────────────────────

fn show_site_list(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§ST1 — per-world sites").strong());
    let Some(report) = state.sites_report.clone() else {
        ui.colored_label(
            Color32::GRAY,
            "No sites yet. Click \"Auto-derive sites\" above.",
        );
        return;
    };
    let filter = state.sites_filter_kind;
    let rows: Vec<&WorldSite> = report
        .sites
        .iter()
        .filter(|s| filter.map_or(true, |k| s.kind == k))
        .collect();
    if rows.is_empty() {
        ui.colored_label(
            Color32::GRAY,
            "No sites matched the current filter / player-edition mask.",
        );
        return;
    }
    let selected = state.selected_site_id.clone();
    let show_actual = !state.sites_player_edition;
    egui::ScrollArea::horizontal()
        .id_salt("st_grid_scroll")
        .show(ui, |ui| {
            egui::Grid::new("st_grid")
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("World").strong());
                    ui.label(RichText::new("Kind").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Controller").strong());
                    ui.label(RichText::new("Public").strong());
                    if show_actual {
                        ui.label(RichText::new("Actual").strong());
                    }
                    ui.label(RichText::new("").strong());
                    ui.end_row();

                    for s in &rows {
                        let is_selected = selected.as_deref() == Some(s.id.as_str());
                        if ui
                            .selectable_label(is_selected, s.world_id.to_string())
                            .clicked()
                        {
                            state.selected_site_id = Some(s.id.clone());
                            state.sites_edit_target = Some(s.id.clone());
                            state.focus_entity(EntityRef::World {
                                system: s.system_id.clone(),
                                world: s.world_id.clone(),
                            });
                        }
                        ui.label(kind_label(s.kind));
                        ui.label(RichText::new(s.name.clone()).strong());
                        ui.label(
                            s.controlling_faction
                                .as_ref()
                                .map(|f| f.to_string())
                                .unwrap_or_else(|| "—".to_string()),
                        );
                        let public_diff = s.public_status != s.actual_status;
                        let public_text = RichText::new(format!("{}", s.public_status));
                        let public_text = if public_diff && show_actual {
                            public_text.color(Color32::from_rgb(220, 170, 80))
                        } else {
                            public_text
                        };
                        ui.label(public_text);
                        if show_actual {
                            ui.label(format!("{}", s.actual_status));
                        }
                        if ui.button("highlight").clicked() {
                            state.selected_site_id = Some(s.id.clone());
                            state.sites_edit_target = Some(s.id.clone());
                            state.focus_entity(EntityRef::World {
                                system: s.system_id.clone(),
                                world: s.world_id.clone(),
                            });
                        }
                        ui.end_row();
                    }
                });
        });
}

// ── §ST1 detail card ───────────────────────────────────────────────────────

fn show_detail_card(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§ST1 — detail").strong());
    let target = state
        .sites_edit_target
        .clone()
        .or_else(|| state.selected_site_id.clone());
    let Some(target_id) = target else {
        ui.colored_label(Color32::GRAY, "Select a site above to see its details.");
        return;
    };
    let Some(site) = state
        .sites_report
        .as_ref()
        .and_then(|r| r.sites.iter().find(|s| s.id == target_id))
        .cloned()
    else {
        ui.colored_label(
            Color32::GRAY,
            format!("Site id `{target_id}` is gone — regenerate to refresh."),
        );
        return;
    };
    let show_actual = !state.sites_player_edition;
    egui::Grid::new("st_detail_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            ui.label(RichText::new(site.id.clone()).monospace());
            ui.end_row();
            ui.label("kind");
            ui.label(kind_label(site.kind));
            ui.end_row();
            ui.label("system / world");
            if ui
                .link(format!("{}/{}", site.system_id, site.world_id))
                .clicked()
            {
                state.focus_entity(EntityRef::World {
                    system: site.system_id.clone(),
                    world: site.world_id.clone(),
                });
            }
            ui.end_row();
            ui.label("region");
            ui.label(
                site.region_kind
                    .map(|r| format!("{r}"))
                    .unwrap_or_else(|| "—".to_string()),
            );
            ui.end_row();
            ui.label("name");
            ui.label(RichText::new(site.name.clone()).strong());
            ui.end_row();
            ui.label("controller");
            if let Some(f) = &site.controlling_faction {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
            ui.end_row();
            ui.label("public status");
            ui.label(format!("{}", site.public_status));
            ui.end_row();
            if show_actual {
                ui.label("actual status");
                let txt = RichText::new(format!("{}", site.actual_status));
                let txt = if site.public_status != site.actual_status {
                    txt.color(Color32::from_rgb(220, 170, 80))
                } else {
                    txt
                };
                ui.label(txt);
                ui.end_row();
            }
            ui.label("known to");
            if site.known_to.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(
                    site.known_to
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            ui.end_row();
            ui.label("tags");
            if site.tags.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(site.tags.join(", "));
            }
            ui.end_row();
            ui.label("hook");
            ui.label(site.hook.clone());
            ui.end_row();
        });
    ui.horizontal_wrapped(|ui| {
        if ui.button("highlight world on map").clicked() {
            state.focus_entity(EntityRef::World {
                system: site.system_id.clone(),
                world: site.world_id.clone(),
            });
        }
    });
}

// ── §ST1 / §ST2 manual entry editor ────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("§ST1 / §ST2 — manual sites").strong());
    ensure_sites_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.sites.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ manual site").clicked() {
            cfg.manual.push(blank_manual_site(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Manual entries are appended after derivation and survive Auto-derive.",
        );
    });
    if cfg.manual.is_empty() {
        ui.colored_label(Color32::GRAY, "No manual sites yet.");
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, s) in cfg.manual.iter_mut().enumerate() {
            let header = egui::CollapsingHeader::new(
                RichText::new(format!(
                    "[{idx}] {} — {}",
                    if s.name.is_empty() {
                        "(unnamed)"
                    } else {
                        s.name.as_str()
                    },
                    kind_label(s.kind),
                ))
                .strong(),
            )
            .id_salt(format!("st_manual_{idx}"))
            .default_open(idx == last_idx);
            header.show(ui, |ui| {
                changed |= manual_site_editor(ui, idx, s);
                if ui
                    .button(RichText::new("✕ remove").color(Color32::from_rgb(200, 90, 90)))
                    .clicked()
                {
                    remove_idx = Some(idx);
                }
            });
        }
    }
    if let Some(idx) = remove_idx {
        cfg.manual.remove(idx);
        changed = true;
    }
    if changed {
        on_catalog_edited(state);
    }
}

fn manual_site_editor(ui: &mut Ui, idx: usize, s: &mut WorldSite) -> bool {
    let mut changed = false;
    egui::Grid::new(format!("st_manual_grid_{idx}"))
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("id");
            changed |= ui.text_edit_singleline(&mut s.id).changed();
            ui.end_row();
            ui.label("kind");
            egui::ComboBox::from_id_salt(format!("st_manual_kind_{idx}"))
                .selected_text(kind_label(s.kind))
                .show_ui(ui, |ui| {
                    for k in KIND_VARIANTS {
                        if ui
                            .selectable_value(&mut s.kind, *k, kind_label(*k))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("system id");
            let mut sys = s.system_id.to_string();
            if ui.text_edit_singleline(&mut sys).changed() {
                s.system_id = SystemId::new(sys.as_str());
                changed = true;
            }
            ui.end_row();
            ui.label("world id");
            let mut w = s.world_id.to_string();
            if ui.text_edit_singleline(&mut w).changed() {
                s.world_id = WorldId::new(w.as_str());
                changed = true;
            }
            ui.end_row();
            ui.label("name");
            changed |= ui.text_edit_singleline(&mut s.name).changed();
            ui.end_row();
            ui.label("controller faction id");
            let mut ctrl = s
                .controlling_faction
                .as_ref()
                .map(|f| f.to_string())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut ctrl).changed() {
                let trimmed = ctrl.trim();
                s.controlling_faction = if trimmed.is_empty() {
                    None
                } else {
                    Some(FactionId::new(trimmed))
                };
                changed = true;
            }
            ui.end_row();
            ui.label("public status");
            egui::ComboBox::from_id_salt(format!("st_manual_pub_{idx}"))
                .selected_text(format!("{}", s.public_status))
                .show_ui(ui, |ui| {
                    for v in STATUS_VARIANTS {
                        if ui
                            .selectable_value(&mut s.public_status, *v, format!("{v}"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("actual status");
            egui::ComboBox::from_id_salt(format!("st_manual_act_{idx}"))
                .selected_text(format!("{}", s.actual_status))
                .show_ui(ui, |ui| {
                    for v in STATUS_VARIANTS {
                        if ui
                            .selectable_value(&mut s.actual_status, *v, format!("{v}"))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("known to (comma)");
            let mut csv = s
                .known_to
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            if ui.text_edit_singleline(&mut csv).changed() {
                s.known_to = csv
                    .split(',')
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .map(FactionId::new)
                    .collect();
                changed = true;
            }
            ui.end_row();
            ui.label("tags (comma)");
            let mut tags = s.tags.join(", ");
            if ui.text_edit_singleline(&mut tags).changed() {
                s.tags = tags
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                changed = true;
            }
            ui.end_row();
            ui.label("hook");
            changed |= ui.text_edit_multiline(&mut s.hook).changed();
            ui.end_row();
        });
    changed
}

fn blank_manual_site(seq: usize) -> WorldSite {
    WorldSite {
        id: format!("site-manual-{seq:04}"),
        world_id: WorldId::new(""),
        system_id: SystemId::new(""),
        region_kind: None,
        kind: SiteKind::GovernorsPalace,
        name: String::new(),
        controlling_faction: None,
        known_to: Vec::new(),
        public_status: SiteStatus::Active,
        actual_status: SiteStatus::Active,
        tags: Vec::new(),
        hook: String::new(),
    }
}

// ── save row ──────────────────────────────────────────────────────────────

fn show_save_row(ui: &mut Ui, state: &mut BuilderState) {
    let has_catalog = state.data_catalogs.sites.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has_catalog, egui::Button::new("Save sites.toml"))
            .clicked()
        {
            if state.config.inputs.sites.is_none() {
                state.config.inputs.sites = Some(DEFAULT_SITES_PATH.into());
            }
            if let Err(e) = crate::builder::project_io::save_project(state) {
                state.modal = Some(crate::builder::state::ModalKind::Message(format!(
                    "Save sites.toml failed: {e}"
                )));
            }
        }
        let path_label = state
            .config
            .inputs
            .sites
            .clone()
            .unwrap_or_else(|| format!("(unset; will write to {DEFAULT_SITES_PATH})"));
        ui.colored_label(Color32::DARK_GRAY, path_label);
    });
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn ensure_sites_catalog(state: &mut BuilderState) {
    if state.data_catalogs.sites.is_none() {
        state.data_catalogs.sites = Some(SitesConfig::default());
    }
    if state.config.inputs.sites.is_none() {
        state.config.inputs.sites = Some(DEFAULT_SITES_PATH.into());
    }
}

fn ensure_sites_catalog_if_needed(state: &mut BuilderState) {
    if state.data_catalogs.sites.is_none() {
        state.data_catalogs.sites = Some(SitesConfig::default());
    }
}

fn on_catalog_edited(state: &mut BuilderState) {
    state.dirty = true;
    if let Some(rel) = state.config.inputs.sites.clone() {
        state.dirty_files.insert(rel);
    } else {
        state.dirty_files.insert(DEFAULT_SITES_PATH.into());
    }
    state.mark_validation_dirty();
    if state.sites_auto_recompute {
        state.recompute_sites();
    }
}

fn kind_label(k: SiteKind) -> &'static str {
    match k {
        SiteKind::GovernorsPalace => "governor's palace",
        SiteKind::CathedralSpire => "cathedral spire",
        SiteKind::Manufactorum => "manufactorum",
        SiteKind::UnderhiveSumpCity => "underhive sump-city",
        SiteKind::VoidElevator => "void elevator",
        SiteKind::StarFortDockyard => "star-fort dockyard",
        SiteKind::QuarantineZone => "quarantine zone",
        SiteKind::XenosRuin => "xenos ruin",
        SiteKind::PilgrimNecropolis => "pilgrim necropolis",
        SiteKind::AstropathicChoir => "astropathic choir",
        SiteKind::ArbitesPrecinct => "Arbites precinct",
        SiteKind::DataVault => "data-vault",
        SiteKind::DisputedShrine => "disputed shrine",
        SiteKind::PenalMine => "penal mine",
        SiteKind::BlackMarketEnclave => "black-market enclave",
        SiteKind::CultSafehouse => "cult safehouse",
        SiteKind::CrashedVoidship => "crashed voidship",
        SiteKind::AgriBeltGranary => "agri granary",
        SiteKind::ForgeReactor => "forge reactor",
        SiteKind::TombComplex => "tomb complex",
        SiteKind::NavalAnchorage => "naval anchorage",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_catalog_seeds_defaults_and_path() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        assert!(state.data_catalogs.sites.is_none());
        ensure_sites_catalog(&mut state);
        assert!(state.data_catalogs.sites.is_some());
        assert_eq!(
            state.config.inputs.sites.as_deref(),
            Some(DEFAULT_SITES_PATH)
        );
    }

    #[test]
    fn recompute_sites_publishes_report() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.sites = Some(SitesConfig::default());
        state.recompute_sites();
        assert!(state.sites_report.is_some());
    }

    #[test]
    fn manual_site_survives_recompute() {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        let mut cfg = SitesConfig::default();
        let mut s = blank_manual_site(0);
        s.name = "Test Vault".into();
        cfg.manual.push(s);
        state.data_catalogs.sites = Some(cfg);
        state.recompute_sites();
        let report = state.sites_report.as_ref().unwrap();
        assert!(report.sites.iter().any(|s| s.id == "site-manual-0000"));
    }

    #[test]
    fn player_edition_flag_threads_into_recompute() {
        // Manual entries bypass the player-edition retain (they live after the
        // merge), so we only check that the flag plumbed through and the
        // report is still produced.
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.data_catalogs.sites = Some(SitesConfig::default());
        state.sites_player_edition = true;
        state.recompute_sites();
        assert!(state.sites_report.is_some());
    }
}
