//! SITES tab (§N1 / §N2) — §ST1..§ST4.
//!
//! §ST1  Per-world site editor. The list is the cached [`SitesReport`]
//!        published by [`BuilderState::recompute_sites`]; `derive_with`
//!        already groups by world id. The panel exposes the full 21-kind
//!        [`SiteKind`] picker (governor's palace … naval anchorage),
//!        controller, public/actual status, and one-line hook on every row.
//!        Selecting a row populates [`BuilderState::selected_site_id`] and
//!        [`BuilderState::sites_edit_target`] so cross-tab links land here
//!        first-class.
//! §ST2  "Regenerate sites" calls [`BuilderState::recompute_sites`] which
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
use sectorforge_gui_core::card;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};
use sectorforge_gui_core::widgets;

use sectorforge::ids::{FactionId, SystemId, WorldId};
use sectorforge::sites::{SiteKind, SiteStatus, SitesConfig, WorldSite};

use crate::builder::state::{ConfirmAction, EntityRef, ModalKind};
use crate::builder::BuilderState;

const DEFAULT_SITES_PATH: &str = "data/sites.toml";

/// Every [`SiteKind`] in panel-display order. Keep in sync with
/// `src/gen/sites.rs::SiteKind`.
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

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    ui.heading("Sites");
    ui.add_space(2.0);
    ui.label(
        RichText::new(
            "Notable places on each world — palaces, shrines, ruins. \
             Regenerate from the sector, hand-add your own, and choose what players are allowed to see.",
        )
        .color(Color32::DARK_GRAY),
    );

    // §COLUMNS — sector-wide controls stay full-width on top (regenerate,
    // player-edition toggle, sites.toml knobs, kind filter), then a master-detail
    // split: a persistent world roster on the left rail and the per-world site
    // table + detail + manual editor filling the rest. Replaces the single
    // horizontally-scrolling grid that stacked everything in one tall column.
    ui.separator();
    show_header_actions(ui, state);
    ui.separator();
    show_config_section(ui, state);
    ui.separator();
    show_filter_row(ui, state);
    ui.separator();

    egui::SidePanel::left("sites_world_roster")
        .resizable(true)
        .default_width(240.0)
        .width_range(180.0..=420.0)
        .show_inside(ui, |ui| show_world_roster(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                show_site_list(ui, state);
                ui.separator();
                show_detail_card(ui, state);
                ui.separator();
                show_manual_editor(ui, state);
                ui.separator();
                show_save_row(ui, state);
            });
    });
}

// ── §COLUMNS world roster (master pane) ─────────────────────────────────────

/// §COLUMNS — left-rail world roster, grouped by parent system. Selecting a
/// world sets [`BuilderState::selected_world_id`] (pure view state, written
/// directly) so the central site table filters to that world. Worlds that
/// currently have at least one site in the cached report are badged with the
/// count; the roster always lists every world so empty worlds stay selectable.
fn show_world_roster(ui: &mut Ui, state: &mut BuilderState) {
    ui.add_space(2.0);
    // Per-world site counts from the cached report (honours the kind filter so
    // the badge matches what the table will show).
    let filter = state.sites_filter_kind;
    let mut counts: std::collections::BTreeMap<WorldId, usize> = std::collections::BTreeMap::new();
    if let Some(report) = state.sites_report.as_ref() {
        for s in &report.sites {
            if filter.is_none_or(|k| s.kind == k) {
                *counts.entry(s.world_id.clone()).or_default() += 1;
            }
        }
    }

    let current = state.selected_world_id.clone();
    let mut pick: Option<(SystemId, WorldId)> = None;
    let total_worlds: usize = state.sector.systems.iter().map(|s| s.worlds.len()).sum();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if total_worlds == 0 {
                ui_kit::placeholder(
                    ui,
                    "No worlds in this sector yet — add systems and worlds in the Map tab first.",
                );
                return;
            }
            for sys in &state.sector.systems {
                if sys.worlds.is_empty() {
                    continue;
                }
                ui_kit::collapsing_section(
                    ui,
                    ("sites_roster_sys", sys.id.as_str()),
                    &format!("{} ({})", sys.name, sys.worlds.len()),
                    true,
                    |ui| {
                        for w in &sys.worlds {
                            let sel = current.as_ref() == Some(&w.id);
                            let n = counts.get(&w.id).copied().unwrap_or(0);
                            let label = if n > 0 {
                                format!("{} — {} [{n}]", w.name, w.id)
                            } else {
                                format!("{} — {}", w.name, w.id)
                            };
                            // §BEAUTY: animated selectable plate; the per-world
                            // site-count badge stays baked into the row label.
                            let (resp, _) =
                                card::selectable_plate(ui, ("site_row", &w.id), sel, |ui| {
                                    ui.label(RichText::new(label));
                                });
                            if resp
                                .on_hover_text("Show this world's sites in the table on the right")
                                .clicked()
                            {
                                pick = Some((sys.id.clone(), w.id.clone()));
                            }
                        }
                    },
                );
            }
        });
    if let Some((sid, wid)) = pick {
        state.selected_world_id = Some(wid);
        state.selected_system_id = Some(sid);
    }
}

// ── §ST2 / §ST3 header actions ─────────────────────────────────────────────

fn show_header_actions(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Regenerate sites")
            .on_hover_text("Rebuild every world's sites from the current sector. Your hand-added sites are kept.")
            .clicked()
        {
            ensure_sites_catalog(state);
            state.recompute_sites();
        }
        ui.checkbox(&mut state.sites_auto_recompute, "Regenerate after every edit")
            .on_hover_text("Re-run the generator automatically whenever you change a setting or a manual site");
        if ui
            .checkbox(&mut state.sites_player_edition, "Players' view")
            .on_hover_text(
                "Hide each site's true status, showing only what's publicly known — \
                 the same as the --player export flag",
            )
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
        ui.label(format!("{total} site(s)  ({manual} hand-added)"));
        if state.data_catalogs.sites.is_none() {
            ui.colored_label(
                palette::warning(),
                "● using default settings (nothing saved yet)",
            );
        }
    });
}

// ── §ST4 config knobs ──────────────────────────────────────────────────────

fn show_config_section(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Generation settings").strong());
    ensure_sites_catalog_if_needed(state);
    let Some(cfg) = state.data_catalogs.sites.as_mut() else {
        return;
    };
    let mut changed = false;
    labeled(
        ui,
        "Max per world",
        "Most sites the generator will place on any single world (schema: max_per_world). Higher means busier worlds.",
        |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.max_per_world).range(0..=32))
                .changed();
        },
    );
    labeled(
        ui,
        "Skip empty worlds",
        "Don't generate sites on uninhabited worlds (schema: skip_uninhabited). Tomb, Dead, Warp-Lost and Daemon worlds still get sites.",
        |ui| {
            changed |= ui.checkbox(&mut cfg.skip_uninhabited, "").changed();
        },
    );
    if changed {
        on_catalog_edited(state);
    }
}

// ── §ST1 kind filter ───────────────────────────────────────────────────────

fn show_filter_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Show only").strong());
        let label = match state.sites_filter_kind {
            None => "all kinds".to_string(),
            Some(k) => kind_label(k).to_string(),
        };
        ui_kit::combo("st1_kind", label).show_ui(ui, |ui| {
            ui.selectable_value(&mut state.sites_filter_kind, None, "all kinds");
            for k in KIND_VARIANTS {
                ui.selectable_value(&mut state.sites_filter_kind, Some(*k), kind_label(*k))
                    .on_hover_text(format!("schema key: {}", k.as_slug()));
            }
        });
    });
}

// ── §ST1 ranked list grouped by world ──────────────────────────────────────

fn show_site_list(ui: &mut Ui, state: &mut BuilderState) {
    // §COLUMNS — the right pane is the selected world's site table. Without a
    // world selected, prompt the user to pick one from the roster on the left.
    let Some(world_id) = state.selected_world_id.clone() else {
        ui.label(RichText::new("Sites on this world").strong());
        ui_kit::placeholder(
            ui,
            "Pick a world from the list on the left to see its sites.",
        );
        return;
    };
    let world_name = state
        .sector
        .systems
        .iter()
        .flat_map(|s| s.worlds.iter())
        .find(|w| w.id == world_id)
        .map(|w| w.name.to_string())
        .unwrap_or_else(|| world_id.to_string());
    ui.label(RichText::new(format!("Sites on {world_name} ({world_id})")).strong());
    let Some(report) = state.sites_report.clone() else {
        ui_kit::placeholder(
            ui,
            "No sites yet — click Regenerate sites at the top to build them.",
        );
        return;
    };
    let filter = state.sites_filter_kind;
    let rows: Vec<&WorldSite> = report
        .sites
        .iter()
        .filter(|s| s.world_id == world_id)
        .filter(|s| filter.is_none_or(|k| s.kind == k))
        .collect();
    if rows.is_empty() {
        ui_kit::placeholder(
            ui,
            "No sites on this world match the current filter and view — try \"all kinds\" or turn off Players' view.",
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
                    // §COLUMNS — the table is scoped to the selected world, so the
                    // old leading "World" column (which repeated the same id on
                    // every row) is replaced by the per-row site id selector.
                    ui.label(RichText::new("ID").strong())
                        .on_hover_text("Stable site identifier (schema: id). Click to select.");
                    ui.label(RichText::new("Kind").strong());
                    ui.label(RichText::new("Name").strong());
                    ui.label(RichText::new("Controller").strong())
                        .on_hover_text(
                            "Faction in charge of the site (schema: controlling_faction)",
                        );
                    ui.label(RichText::new("Known status").strong())
                        .on_hover_text("Status everyone can see (schema: public_status)");
                    if show_actual {
                        ui.label(RichText::new("True status").strong())
                            .on_hover_text(
                                "Real status, hidden in Players' view (schema: actual_status)",
                            );
                    }
                    ui.label(RichText::new("").strong());
                    ui.end_row();

                    for s in &rows {
                        let is_selected = selected.as_deref() == Some(s.id.as_str());
                        if ui
                            .selectable_label(is_selected, RichText::new(s.id.clone()).monospace())
                            .clicked()
                        {
                            state.selected_site_id = Some(s.id.clone());
                            state.sites_edit_target = Some(s.id.clone());
                            state.focus_entity(EntityRef::World {
                                system: s.system_id.clone(),
                                world: s.world_id.clone(),
                            });
                        }
                        ui.label(kind_label(s.kind))
                            .on_hover_text(format!("schema key: {}", s.kind.as_slug()));
                        ui.label(RichText::new(s.name.clone()).strong());
                        ui.label(
                            s.controlling_faction
                                .as_ref()
                                .map(|f| f.to_string())
                                .unwrap_or_else(|| "—".to_string()),
                        );
                        let public_diff = s.public_status != s.actual_status;
                        let public_text = RichText::new(status_label(s.public_status));
                        let public_text = if public_diff && show_actual {
                            public_text.color(palette::warning())
                        } else {
                            public_text
                        };
                        ui.label(public_text)
                            .on_hover_text(format!("schema key: {}", s.public_status.as_slug()));
                        if show_actual {
                            ui.label(status_label(s.actual_status))
                                .on_hover_text(format!(
                                    "schema key: {}",
                                    s.actual_status.as_slug()
                                ));
                        }
                        if ui
                            .button("Show on map")
                            .on_hover_text("Select this site and highlight its world on the map")
                            .clicked()
                        {
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
    ui.label(RichText::new("Selected site").strong());
    let target = state
        .sites_edit_target
        .clone()
        .or_else(|| state.selected_site_id.clone());
    let Some(target_id) = target else {
        ui_kit::placeholder(
            ui,
            "Select a site in the table above to see its full details.",
        );
        return;
    };
    let Some(site) = state
        .sites_report
        .as_ref()
        .and_then(|r| r.sites.iter().find(|s| s.id == target_id))
        .cloned()
    else {
        ui_kit::placeholder(
            ui,
            "That site is no longer in the report — click Regenerate sites to refresh.",
        );
        return;
    };
    let show_actual = !state.sites_player_edition;
    labeled(ui, "ID", "Stable site identifier (schema: id).", |ui| {
        ui.label(RichText::new(site.id.clone()).monospace());
    });
    labeled(
        ui,
        "Kind",
        "What sort of site this is (schema: kind).",
        |ui| {
            ui.label(kind_label(site.kind))
                .on_hover_text(format!("schema key: {}", site.kind.as_slug()));
        },
    );
    labeled(
        ui,
        "System / world",
        "Where the site sits (schema: system_id / world_id). Click to jump to it.",
        |ui| {
            if ui
                .link(format!("{}/{}", site.system_id, site.world_id))
                .clicked()
            {
                state.focus_entity(EntityRef::World {
                    system: site.system_id.clone(),
                    world: site.world_id.clone(),
                });
            }
        },
    );
    labeled(
        ui,
        "Region",
        "Map region the site falls in, if any (schema: region_kind).",
        |ui| {
            ui.label(
                site.region_kind
                    .map(|r| format!("{r}"))
                    .unwrap_or_else(|| "—".to_string()),
            );
        },
    );
    labeled(
        ui,
        "Name",
        "Display name of the site (schema: name).",
        |ui| {
            ui.label(RichText::new(site.name.clone()).strong());
        },
    );
    labeled(
        ui,
        "Controller",
        "Faction in charge of the site (schema: controlling_faction). Click to jump to it.",
        |ui| {
            if let Some(f) = &site.controlling_faction {
                if ui.link(f.to_string()).clicked() {
                    state.focus_entity(EntityRef::Faction(f.clone()));
                }
            } else {
                ui.colored_label(Color32::DARK_GRAY, "—");
            }
        },
    );
    labeled(
        ui,
        "Known status",
        "Status everyone can see (schema: public_status).",
        |ui| {
            ui.label(status_label(site.public_status))
                .on_hover_text(format!("schema key: {}", site.public_status.as_slug()));
        },
    );
    if show_actual {
        labeled(
            ui,
            "True status",
            "Real status, hidden in Players' view (schema: actual_status).",
            |ui| {
                let txt = RichText::new(status_label(site.actual_status));
                let txt = if site.public_status != site.actual_status {
                    txt.color(palette::warning())
                } else {
                    txt
                };
                ui.label(txt)
                    .on_hover_text(format!("schema key: {}", site.actual_status.as_slug()));
            },
        );
    }
    labeled(
        ui,
        "Known to",
        "Factions that secretly know the true status (schema: known_to).",
        |ui| {
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
        },
    );
    labeled(
        ui,
        "Tags",
        "Free-form labels for filtering and flavour (schema: tags).",
        |ui| {
            if site.tags.is_empty() {
                ui.colored_label(Color32::DARK_GRAY, "—");
            } else {
                ui.label(site.tags.join(", "));
            }
        },
    );
    labeled(
        ui,
        "Adventure hook",
        "One-line story seed for the site (schema: hook).",
        |ui| {
            ui.label(site.hook.clone());
        },
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Show world on map")
            .on_hover_text("Highlight this site's world on the map")
            .clicked()
        {
            state.focus_entity(EntityRef::World {
                system: site.system_id.clone(),
                world: site.world_id.clone(),
            });
        }
    });
}

// ── §ST1 / §ST2 manual entry editor ────────────────────────────────────────

fn show_manual_editor(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Hand-added sites").strong());
    ensure_sites_catalog_if_needed(state);
    // Faction ids already present in the sector, for the controller dropdown.
    let faction_ids: Vec<FactionId> = state.sector.factions.iter().map(|f| f.id.clone()).collect();
    // World / system ids present in the sector, for the location dropdowns.
    let mut world_choices: Vec<(SystemId, WorldId, String)> = Vec::new();
    for sys in &state.sector.systems {
        for w in &sys.worlds {
            world_choices.push((sys.id.clone(), w.id.clone(), w.name.to_string()));
        }
    }
    let Some(cfg) = state.data_catalogs.sites.as_mut() else {
        return;
    };
    let mut changed = false;
    let mut remove_idx: Option<usize> = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("➕  Add site")
            .on_hover_text("Add a blank hand-made site you can fill in below")
            .clicked()
        {
            cfg.manual.push(blank_manual_site(cfg.manual.len()));
            changed = true;
        }
        ui.colored_label(
            Color32::DARK_GRAY,
            "Hand-added sites are kept whenever you regenerate.",
        );
    });
    if cfg.manual.is_empty() {
        ui_kit::placeholder(ui, "No hand-added sites yet — click Add site to make one.");
    } else {
        let last_idx = cfg.manual.len().saturating_sub(1);
        for (idx, s) in cfg.manual.iter_mut().enumerate() {
            ui_kit::collapsing_section(
                ui,
                format!("site_manual_{idx}"),
                &format!(
                    "{} — {}",
                    if s.name.is_empty() {
                        "(unnamed)"
                    } else {
                        s.name.as_str()
                    },
                    kind_label(s.kind),
                ),
                idx == last_idx,
                |ui| {
                    changed |= manual_site_editor(ui, idx, s, &faction_ids, &world_choices);
                    if ui
                        .button(RichText::new("🗑  Delete site").color(palette::danger()))
                        .on_hover_text("Remove this hand-added site")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                },
            );
        }
    }
    // §FRIENDLY_PANEL_PASS transform #7: a hand-added site bypasses the undo bus,
    // so confirm the delete the in-card 🗑 requested (label read while `cfg` is
    // still borrowed; dispatch happens after it is released).
    let pending_delete = remove_idx.and_then(|idx| {
        cfg.manual.get(idx).map(|s| {
            let label = if s.name.is_empty() {
                format!("site #{}", idx + 1)
            } else {
                s.name.clone()
            };
            (idx, label)
        })
    });
    if changed {
        on_catalog_edited(state);
    }
    if let Some((idx, label)) = pending_delete {
        state.modal = Some(ModalKind::ConfirmDestructive {
            title: "Delete site?".into(),
            body: format!("Remove the hand-added site “{label}”."),
            action: ConfirmAction::DeleteManualSite(idx),
        });
    }
}

/// §FRIENDLY_PANEL_PASS transform #7: delete the manual site at `idx` (confirmed
/// payload of [`ModalKind::ConfirmDestructive`]) and run the catalogue-edited
/// bookkeeping. Manual sites bypass the undo bus.
pub(crate) fn delete_manual(state: &mut BuilderState, idx: usize) {
    {
        let Some(cfg) = state.data_catalogs.sites.as_mut() else {
            return;
        };
        if idx >= cfg.manual.len() {
            return;
        }
        cfg.manual.remove(idx);
    }
    on_catalog_edited(state);
}

fn manual_site_editor(
    ui: &mut Ui,
    idx: usize,
    s: &mut WorldSite,
    faction_ids: &[FactionId],
    world_choices: &[(SystemId, WorldId, String)],
) -> bool {
    let mut changed = false;
    labeled(
        ui,
        "ID",
        "Stable identifier for this site (schema: id). Lowercase, no spaces.",
        |ui| {
            changed |= ui.text_edit_singleline(&mut s.id).changed();
        },
    );
    labeled(
        ui,
        "Kind",
        "What sort of site this is (schema: kind).",
        |ui| {
            ui_kit::combo(format!("st_manual_kind_{idx}"), kind_label(s.kind)).show_ui(ui, |ui| {
                for k in KIND_VARIANTS {
                    if ui
                        .selectable_value(&mut s.kind, *k, kind_label(*k))
                        .on_hover_text(format!("schema key: {}", k.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "Location",
        "Which system and world the site sits on (schema: system_id / world_id). Pick an existing world, or type ids for one not yet in the sector.",
        |ui| {
            changed |= location_combo(ui, idx, s, world_choices);
        },
    );
    labeled(
        ui,
        "Name",
        "Display name of the site (schema: name).",
        |ui| {
            changed |= ui.text_edit_singleline(&mut s.name).changed();
        },
    );
    labeled(
        ui,
        "Controller",
        "Faction in charge of the site (schema: controlling_faction). Leave blank for none.",
        |ui| {
            changed |= faction_combo(
                ui,
                format!("st_manual_ctrl_{idx}"),
                &mut s.controlling_faction,
                faction_ids,
            );
        },
    );
    labeled(
        ui,
        "Known status",
        "Status everyone can see (schema: public_status).",
        |ui| {
            ui_kit::combo(
                format!("st_manual_pub_{idx}"),
                status_label(s.public_status),
            )
            .show_ui(ui, |ui| {
                for v in STATUS_VARIANTS {
                    if ui
                        .selectable_value(&mut s.public_status, *v, status_label(*v))
                        .on_hover_text(format!("schema key: {}", v.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "True status",
        "Real status (schema: actual_status). Make it differ from the known status to hide something from players.",
        |ui| {
            ui_kit::combo(
                format!("st_manual_act_{idx}"),
                status_label(s.actual_status),
            )
            .show_ui(ui, |ui| {
                for v in STATUS_VARIANTS {
                    if ui
                        .selectable_value(&mut s.actual_status, *v, status_label(*v))
                        .on_hover_text(format!("schema key: {}", v.as_slug()))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        },
    );
    labeled(
        ui,
        "Known to",
        "Factions that secretly know the true status, separated by commas (schema: known_to).",
        |ui| {
            let mut csv = s
                .known_to
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if ui
                .add(egui::TextEdit::singleline(&mut csv).hint_text("faction ids, comma-separated"))
                .changed()
            {
                s.known_to = csv
                    .split(',')
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .map(FactionId::new)
                    .collect();
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Tags",
        "Free-form labels for filtering and flavour, separated by commas (schema: tags).",
        |ui| {
            let mut tags = s.tags.join(",");
            if ui
                .add(egui::TextEdit::singleline(&mut tags).hint_text("tags, comma-separated"))
                .changed()
            {
                s.tags = tags
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                changed = true;
            }
        },
    );
    labeled(
        ui,
        "Adventure hook",
        "One-line story seed for the site (schema: hook).",
        |ui| {
            changed |= ui.text_edit_multiline(&mut s.hook).changed();
        },
    );
    changed
}

/// Dropdown over the sector's worlds (+ "(custom…)" rows for ids not yet in the
/// sector). Friendlier than two raw id boxes: a manual site usually targets a
/// world that already exists, so it becomes one click instead of retyping the
/// system + world id pair. The custom rows keep off-sector targets reachable.
fn location_combo(
    ui: &mut Ui,
    idx: usize,
    s: &mut WorldSite,
    world_choices: &[(SystemId, WorldId, String)],
) -> bool {
    let mut changed = false;
    let current = if s.world_id.as_str().is_empty() && s.system_id.as_str().is_empty() {
        "(choose a world)".to_string()
    } else {
        let name = world_choices
            .iter()
            .find(|(sid, wid, _)| *sid == s.system_id && *wid == s.world_id)
            .map(|(_, _, name)| name.clone());
        match name {
            Some(name) => format!("{name} — {}/{}", s.system_id, s.world_id),
            None => format!("{}/{}", s.system_id, s.world_id),
        }
    };
    ui.vertical(|ui| {
        ui_kit::combo(format!("st_manual_loc_{idx}"), current).show_ui(ui, |ui| {
            for (sid, wid, name) in world_choices {
                let selected = *sid == s.system_id && *wid == s.world_id;
                if ui
                    .selectable_label(selected, format!("{name} — {sid}/{wid}"))
                    .clicked()
                {
                    s.system_id = sid.clone();
                    s.world_id = wid.clone();
                    changed = true;
                }
            }
        });
        // Free-text fall-through for a world / system not in the sector yet.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("or type ids:")
                    .small()
                    .color(Color32::DARK_GRAY),
            );
            let mut sys = s.system_id.to_string();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut sys)
                        .hint_text("system id")
                        .desired_width(90.0),
                )
                .changed()
            {
                s.system_id = SystemId::new(sys.as_str());
                changed = true;
            }
            let mut w = s.world_id.to_string();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut w)
                        .hint_text("world id")
                        .desired_width(90.0),
                )
                .changed()
            {
                s.world_id = WorldId::new(w.as_str());
                changed = true;
            }
        });
    });
    changed
}

/// Dropdown over the faction ids present in the sector, plus "(none)" and an
/// in-popup custom row. Friendlier than free-typing a raw id: the controller is
/// almost always a faction that already exists. Mirrors the factions panel's
/// `id_combo`.
fn faction_combo(
    ui: &mut Ui,
    salt: String,
    slot: &mut Option<FactionId>,
    faction_ids: &[FactionId],
) -> bool {
    let mut changed = false;
    let current = slot.as_ref().map(|f| f.to_string());
    let label = current.clone().unwrap_or_else(|| "(none)".to_owned());
    ui.horizontal(|ui| {
        ui_kit::combo(salt, label).show_ui(ui, |ui| {
            if ui.selectable_label(slot.is_none(), "(none)").clicked() {
                *slot = None;
                changed = true;
            }
            for fid in faction_ids {
                let selected = current.as_deref() == Some(fid.as_str());
                if ui.selectable_label(selected, fid.as_str()).clicked() {
                    *slot = Some(fid.clone());
                    changed = true;
                }
            }
            ui.separator();
            ui.label(RichText::new("custom…").small().color(Color32::DARK_GRAY));
            let mut buf = current.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut buf)
                        .hint_text("faction id not in sector")
                        .desired_width(160.0),
                )
                .changed()
            {
                let trimmed = buf.trim();
                *slot = if trimmed.is_empty() {
                    None
                } else {
                    Some(FactionId::new(trimmed))
                };
                changed = true;
            }
        });
        if slot.is_some() && ui.small_button("×").on_hover_text("clear").clicked() {
            *slot = None;
            changed = true;
        }
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
            .add_enabled(has_catalog, egui::Button::new("💾  Save sites"))
            .on_hover_text("Write all site settings and hand-added sites back to sites.toml")
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
            .map(|p| format!("file: {p}"))
            .unwrap_or_else(|| format!("file: {DEFAULT_SITES_PATH} (will be created on save)"));
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
    state.mark_catalog_dirty(state.config.inputs.sites.clone(), DEFAULT_SITES_PATH);
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

/// Human label for a status. The `SiteStatus` `Display` impl emits the raw
/// snake_case slug (e.g. `under_construction`); this gives the friendlier text
/// for visible widgets while the slug stays reachable via `as_slug()` tooltips.
fn status_label(s: SiteStatus) -> &'static str {
    match s {
        SiteStatus::Active => "active",
        SiteStatus::Restricted => "restricted",
        SiteStatus::Abandoned => "abandoned",
        SiteStatus::Quarantined => "quarantined",
        SiteStatus::Sealed => "sealed",
        SiteStatus::Contested => "contested",
        SiteStatus::UnderConstruction => "under construction",
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
