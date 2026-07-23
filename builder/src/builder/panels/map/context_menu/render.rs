//! §CTX1 — interactive menu builders (one `render_*` per schema) for the MAP
//! right-click menu. Each walks `ui`, emits the schema's rows, and forwards
//! activations to [`super::action::apply_sector_menu_action`]. Only transient
//! UI state (`bulk_delete_confirm`, the pending-* dialogs) is touched directly
//! here — every document edit goes through the action dispatch (§R4 carve-out).
//! Un-netted (needs a live egui loop), so these moved verbatim from the
//! pre-split file.

use sectorforge::ids::{FactionId, RouteId, SystemId};
use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{HexCoord, RouteStability, RouteType, SystemState};

use crate::builder::BuilderState;

use super::action::{apply_sector_menu_action, OpenInTarget, SectorMenuAction};

/// §CTX1 — Phase 2 §6.1: render the empty-hex schema. Returns `true` when
/// any item activated.
pub(super) fn render_empty_hex_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    coord: HexCoord,
) -> bool {
    let mut close = false;

    if ui.selectable_label(false, "PLACE SYSTEM HERE…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::PlaceSystem { coord });
    }

    let paint_enabled = state.selection.region_id.is_some();
    let paint_resp = ui.add_enabled(
        paint_enabled,
        egui::SelectableLabel::new(false, "PAINT REGION HERE"),
    );
    if paint_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::PaintRegion { coord });
    }
    if !paint_enabled {
        paint_resp.on_hover_text("Pick a region in the REGIONS tab first.");
    }

    let erase_enabled = state
        .sector
        .regions
        .iter()
        .any(|r| r.hexes.contains(&coord));
    let erase_resp = ui.add_enabled(
        erase_enabled,
        egui::SelectableLabel::new(false, "ERASE REGION HERE"),
    );
    if erase_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::EraseRegion { coord });
    }
    if !erase_enabled {
        erase_resp.on_hover_text("Hex is not part of any region.");
    }

    ui.separator();

    if ui
        .selectable_label(false, "START PARTIAL REGEN HERE")
        .clicked()
    {
        close |= apply_sector_menu_action(state, SectorMenuAction::StartPartialRegen { coord });
    }

    let label = format!("COPY COORD ({},{})", coord.q, coord.r);
    if ui.selectable_label(false, label).clicked() {
        ui.output_mut(|o| o.copied_text = format!("{},{}", coord.q, coord.r));
        close = true;
    }

    close
}

/// §CTX1 — Phase 2 §6.2: render the single-system schema.
pub(super) fn render_system_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    id: SystemId,
    coord: HexCoord,
) -> bool {
    // §CTX1 §10 row 12 — while ADD-ROUTE is half-armed, the system menu
    // collapses to "CANCEL ROUTE" + "Open in ROUTES" so the user can't
    // accidentally start a second pending route or run a destructive item.
    if state.drag.pending_route_start.is_some() {
        let mut close = false;
        if ui.selectable_label(false, "CANCEL ROUTE").clicked() {
            close |= apply_sector_menu_action(state, SectorMenuAction::CancelRoute);
        }
        if ui.selectable_label(false, "Open in ROUTES").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id,
                    target: OpenInTarget::Routes,
                },
            );
        }
        // §R4: `coord` is used on the main (non-early-return) path below
        // (REGENERATE SYSTEM, COPY COORD), so the parameter is genuinely
        // consumed — the stray `let _ = coord;` suppression is removed.
        return close;
    }

    let mut close = false;

    if ui.selectable_label(false, "FOCUS SYSTEM").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusSystem { id: id.clone() });
    }
    if ui.selectable_label(false, "RENAME…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::RenameSystem { id: id.clone() });
    }
    if ui.selectable_label(false, "DELETE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::DeleteSystem { id: id.clone() });
    }

    ui.separator();

    if ui.selectable_label(false, "ADD ROUTE FROM HERE…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::AddRouteFrom { id: id.clone() });
    }
    if ui.selectable_label(false, "ADD WORLD").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::AddWorld { id: id.clone() });
    }

    let pinned = state.pinned_systems.contains(&id);
    let regen_resp = ui.add_enabled(
        !pinned,
        egui::SelectableLabel::new(false, "REGENERATE SYSTEM"),
    );
    if regen_resp.clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::RegenerateSystem {
                id: id.clone(),
                coord,
            },
        );
    }
    if pinned {
        regen_resp.on_hover_text("Unpin first.");
    }

    let pin_label = if pinned { "UNPIN" } else { "TOGGLE PIN" };
    if ui.selectable_label(false, pin_label).clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::TogglePin { id: id.clone() });
    }

    ui.separator();

    ui.label(egui::RichText::new("Open in").italics());
    ui.indent("open_in_indent", |ui| {
        if ui.selectable_label(false, "SYSTEM").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::System,
                },
            );
        }

        let has_world = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| !s.worlds.is_empty())
            .unwrap_or(false);
        let world_resp = ui.add_enabled(has_world, egui::SelectableLabel::new(false, "WORLD"));
        if world_resp.clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::World,
                },
            );
        } else if !has_world {
            world_resp.on_hover_text("System has no worlds.");
        }

        if ui.selectable_label(false, "ROUTES").clicked() {
            close |= apply_sector_menu_action(
                state,
                SectorMenuAction::OpenIn {
                    id: id.clone(),
                    target: OpenInTarget::Routes,
                },
            );
        }
    });

    ui.separator();

    if ui
        .selectable_label(false, format!("COPY ID ({id})"))
        .clicked()
    {
        ui.output_mut(|o| o.copied_text = id.to_string());
        close = true;
    }
    if ui
        .selectable_label(false, format!("COPY COORD ({},{})", coord.q, coord.r))
        .clicked()
    {
        ui.output_mut(|o| o.copied_text = format!("{},{}", coord.q, coord.r));
        close = true;
    }

    close
}

/// §CTX1 — Phase 3 §6.3: render the multi-selection schema.
pub(super) fn render_multi_selection_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    ids: &[SystemId],
) -> bool {
    let mut close = false;
    ui.label(format!("{} systems selected", ids.len()));
    ui.separator();

    if ui.selectable_label(false, "FOCUS FIRST").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiFocusFirst);
    }
    if ui.selectable_label(false, "BULK RENAME…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiBulkRenameOpen);
    }

    let any_unpinned = ids.iter().any(|id| !state.pinned_systems.contains(id));
    let any_pinned = ids.iter().any(|id| state.pinned_systems.contains(id));
    let pin_resp = ui.add_enabled(any_unpinned, egui::SelectableLabel::new(false, "PIN ALL"));
    if pin_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiPinAll);
    }
    if !any_unpinned {
        pin_resp.on_hover_text("Every selected system is already pinned.");
    }
    let unpin_resp = ui.add_enabled(any_pinned, egui::SelectableLabel::new(false, "UNPIN ALL"));
    if unpin_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiUnpinAll);
    }
    if !any_pinned {
        unpin_resp.on_hover_text("Nothing in the selection is pinned.");
    }

    ui.separator();

    let confirming = state
        .map_view
        .sector_context_menu
        .as_ref()
        .map(|m| m.bulk_delete_confirm)
        .unwrap_or(false);
    if confirming {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Confirm DELETE ALL?").strong());
            if ui.button("Yes").clicked() {
                close |= apply_sector_menu_action(state, SectorMenuAction::MultiDeleteAllConfirmed);
            }
            if ui.button("No").clicked() {
                if let Some(menu) = state.map_view.sector_context_menu.as_mut() {
                    menu.bulk_delete_confirm = false;
                }
            }
        });
    } else if ui
        .selectable_label(false, format!("✕ DELETE ALL ({})", ids.len()))
        .clicked()
    {
        if let Some(menu) = state.map_view.sector_context_menu.as_mut() {
            menu.bulk_delete_confirm = true;
        }
    }

    ui.separator();

    let factions: Vec<(FactionId, String)> = state
        .sector
        .factions
        .iter()
        .map(|f| (f.id.clone(), f.name.to_string()))
        .collect();
    if factions.is_empty() {
        ui.add_enabled(
            false,
            egui::SelectableLabel::new(false, "ASSIGN PRIMARY FACTION ▸"),
        )
        .on_disabled_hover_text("Sector has no factions — add one in the FACTIONS tab.");
    } else {
        ui.menu_button("ASSIGN PRIMARY FACTION ▸", |ui| {
            for (fid, name) in &factions {
                if ui
                    .selectable_label(false, format!("→ {name} ({fid})"))
                    .clicked()
                {
                    close |= apply_sector_menu_action(
                        state,
                        SectorMenuAction::MultiAssignPrimaryFaction { fid: fid.clone() },
                    );
                    ui.close_menu();
                }
            }
        });
    }

    ui.menu_button("FLIP CONTROL STATE ▸", |ui| {
        for value in [
            None,
            Some(SystemState::Pacified),
            Some(SystemState::Fragmented),
            Some(SystemState::Blockaded),
            Some(SystemState::Warzone),
            Some(SystemState::Infiltrated),
            Some(SystemState::Quarantined),
            Some(SystemState::Uncharted),
        ] {
            let label = match value {
                None => "(none)".to_string(),
                Some(v) => format!("{v}"),
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::MultiFlipControlState { value },
                );
                ui.close_menu();
            }
        }
    });

    let reseed_enabled = ids.iter().any(|id| !state.pinned_systems.contains(id));
    let reseed_resp = ui.add_enabled(
        reseed_enabled,
        egui::SelectableLabel::new(false, "RESEED WORLDS"),
    );
    if reseed_resp.clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiReseedWorlds);
    }
    if !reseed_enabled {
        reseed_resp.on_hover_text("All selected systems are pinned — unpin first.");
    }

    ui.separator();

    if ui.selectable_label(false, "CLEAR SELECTION").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiClearSelection);
    }

    close
}

/// §CTX1 — Phase 5 §6.4: render the route schema.
pub(super) fn render_route_menu(ui: &mut egui::Ui, state: &mut BuilderState, id: RouteId) -> bool {
    let mut close = false;
    let route_summary = state
        .sector
        .routes
        .iter()
        .find(|r| r.id == id)
        .map(|r| (r.route_type, r.stability));
    let Some((cur_type, cur_stab)) = route_summary else {
        if ui.selectable_label(false, "CLOSE").clicked() {
            close = true;
        }
        return close;
    };

    ui.label(
        egui::RichText::new(format!(
            "ROUTE {id} — {} / {}",
            cur_type.editor_label(),
            cur_stab.as_slug()
        ))
        .italics(),
    );
    ui.separator();

    if ui.selectable_label(false, "FOCUS ROUTE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusRoute { id: id.clone() });
    }
    if ui.selectable_label(false, "✕ REMOVE ROUTE").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::RemoveRoute { id: id.clone() });
    }

    ui.menu_button("CYCLE ROUTE TYPE ▸", |ui| {
        for value in RouteType::ALL {
            let label = if value == cur_type {
                format!("• {}", value.editor_label())
            } else {
                value.editor_label().to_string()
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRouteType {
                        id: id.clone(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    ui.menu_button("CYCLE STABILITY ▸", |ui| {
        for value in [
            RouteStability::Stable,
            RouteStability::Unstable,
            RouteStability::Hazardous,
            RouteStability::Perilous,
        ] {
            let label = if value == cur_stab {
                format!("• {}", value.as_slug())
            } else {
                value.as_slug().to_string()
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRouteStability {
                        id: id.clone(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    ui.separator();
    if ui.selectable_label(false, "Open in ROUTES ▸").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::FocusRoute { id });
    }

    close
}

/// §CTX1 — Phase 5 §6.5: render the region-hex schema.
pub(super) fn render_region_hex_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    region: &str,
    coord: HexCoord,
) -> bool {
    let mut close = false;
    let summary = state
        .sector
        .regions
        .iter()
        .find(|r| r.id == region)
        .map(|r| (r.name.clone(), r.kind));
    let Some((name, cur_kind)) = summary else {
        if ui.selectable_label(false, "CLOSE").clicked() {
            close = true;
        }
        return close;
    };

    ui.label(
        egui::RichText::new(format!("REGION {region} — {name} [{}]", cur_kind.label())).italics(),
    );
    ui.separator();

    if ui.selectable_label(false, "FOCUS REGION").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::FocusRegion {
                region: region.to_string(),
            },
        );
    }
    if ui.selectable_label(false, "ERASE FROM REGION").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::EraseRegionHex {
                region: region.to_string(),
                coord,
            },
        );
    }

    ui.menu_button("RECOLOR ▸", |ui| {
        for value in RegionConditionKind::ALL.iter().copied() {
            let label = if value == cur_kind {
                format!("• {} {}", value.glyph(), value.label())
            } else {
                format!("{} {}", value.glyph(), value.label())
            };
            if ui.selectable_label(false, label).clicked() {
                close |= apply_sector_menu_action(
                    state,
                    SectorMenuAction::SetRegionKind {
                        region: region.to_string(),
                        value,
                    },
                );
                ui.close_menu();
            }
        }
    });

    if ui.selectable_label(false, "RENAME REGION…").clicked() {
        close |= apply_sector_menu_action(
            state,
            SectorMenuAction::RenameRegionOpen {
                region: region.to_string(),
            },
        );
    }

    close
}
