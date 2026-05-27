//! §CTX1 right-click context menu schemas + apply path for the MAP tab.
//!
//! Resolves a right-click into a [`SectorMenuTarget`], routes the chosen item
//! to an action enum, and renders one of five schemas: §6.1 empty hex, §6.2
//! single system, §6.3 multi-selection, §6.4 route, §6.5 region hex. The pure
//! `apply_sector_menu_action` path is testable without an egui context.

use egui::Pos2;

use sectorforge::ids::{FactionId, RouteId, SystemId};
use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{HexCoord, RouteStability, RouteType, SystemState};
use sectorforge_gui_core::sector_view::SectorGeom;

use crate::builder::command::BuilderCommand;
use crate::builder::state::{
    BuilderTab, EntityRef, MapTool, PendingBulkRename, PendingPlace, PendingRegionRename,
    PendingRename, SectorMenuTarget,
};
use crate::builder::{BuilderState, ModalKind};

use super::interactions::paint_region_at;

/// §CTX1 — Phase 1: resolve what the right-click landed on. Pure read of
/// `state`, so unit tests can call it directly with a synthesised
/// [`SectorGeom`] + screen position. Returns `None` when the click should be
/// ignored (drag in progress / rect-select live / collision dialog already
/// open / RegionPaint mode without Ctrl).
pub(super) fn resolve_sector_context(
    state: &BuilderState,
    geom: &SectorGeom,
    pos: Pos2,
    sector_w: u32,
    sector_h: u32,
    ctrl_down: bool,
) -> Option<SectorMenuTarget> {
    // Suppression guards (§4.1).
    if state.drag_system.is_some() || state.rect_select.is_some() {
        return None;
    }
    if state.pending_collision.is_some() {
        return None;
    }
    if state.map_tool == MapTool::RegionPaint && !ctrl_down {
        return None;
    }

    if let Some(id) = geom.hit_system(&state.sector, pos) {
        let coord = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.coord)?;
        if state.selected_systems.contains(&id) && state.selected_systems.len() >= 2 {
            return Some(SectorMenuTarget::MultiSelection {
                ids: state.selected_systems.iter().cloned().collect(),
            });
        }
        return Some(SectorMenuTarget::System { id, coord });
    }

    // §CTX1 Phase 5 — route line hit-test runs before the hex pick so a
    // right-click on a route segment in an otherwise-empty hex opens the
    // route schema rather than the empty-hex one.
    if let Some(route_id) = geom.hit_route(&state.sector, pos) {
        let near_coord = geom
            .pick_hex(pos, sector_w, sector_h)
            .unwrap_or(HexCoord { q: 0, r: 0 });
        return Some(SectorMenuTarget::Route {
            id: route_id,
            near_coord,
        });
    }

    if let Some(coord) = geom.pick_hex(pos, sector_w, sector_h) {
        if let Some(region) = state
            .map_view_cache
            .as_ref()
            .and_then(|c| c.lookup.region_for_hex(coord))
        {
            return Some(SectorMenuTarget::RegionHex {
                region: region.to_string(),
                coord,
            });
        }
        return Some(SectorMenuTarget::EmptyHex { coord });
    }
    None
}

/// §CTX1 — Phase 2: per-item action types. Each variant maps 1:1 to a menu
/// row in the §6.1 / §6.2 schemas. Splitting the actions from the render path
/// lets unit tests assert state mutations without standing up an egui context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SectorMenuAction {
    PlaceSystem {
        coord: HexCoord,
    },
    PaintRegion {
        coord: HexCoord,
    },
    EraseRegion {
        coord: HexCoord,
    },
    FocusSystem {
        id: SystemId,
    },
    RenameSystem {
        id: SystemId,
    },
    DeleteSystem {
        id: SystemId,
    },
    AddRouteFrom {
        id: SystemId,
    },
    AddWorld {
        id: SystemId,
    },
    RegenerateSystem {
        id: SystemId,
        coord: HexCoord,
    },
    TogglePin {
        id: SystemId,
    },
    OpenIn {
        id: SystemId,
        target: OpenInTarget,
    },
    /// §CTX1 Phase 4 — arm the §G5 partial-regen anchor at `coord`. The next
    /// primary click on the map completes the rect anchor→click (see
    /// [`super::interactions::apply_partial_regen_anchor_click`]).
    StartPartialRegen {
        coord: HexCoord,
    },
    // ── §CTX1 Phase 3 — §6.3 multi-selection items ────────────────────────
    MultiFocusFirst,
    MultiBulkRenameOpen,
    MultiPinAll,
    MultiUnpinAll,
    MultiDeleteAllConfirmed,
    MultiAssignPrimaryFaction {
        fid: FactionId,
    },
    MultiFlipControlState {
        value: Option<SystemState>,
    },
    MultiReseedWorlds,
    MultiClearSelection,
    // ── §CTX1 Phase 5 — §6.4 route items ───────────────────────────────────
    FocusRoute {
        id: RouteId,
    },
    RemoveRoute {
        id: RouteId,
    },
    SetRouteType {
        id: RouteId,
        value: RouteType,
    },
    SetRouteStability {
        id: RouteId,
        value: RouteStability,
    },
    // ── §CTX1 Phase 5 — §6.5 region-hex items ──────────────────────────────
    FocusRegion {
        region: String,
    },
    EraseRegionHex {
        region: String,
        coord: HexCoord,
    },
    SetRegionKind {
        region: String,
        value: RegionConditionKind,
    },
    RenameRegionOpen {
        region: String,
    },
    /// §10 #12 — abort an in-flight ADD-ROUTE.
    CancelRoute,
}

/// §CTX1 — Phase 2 "Open in ▸" targets wired in Phase 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OpenInTarget {
    System,
    World,
    Routes,
}

/// §CTX1 Phase 7 polish — short human label for the telemetry tail rendered
/// in the status bar. Mirrors the spec's "ctx_menu: <schema> :: <item>"
/// format.
pub(super) fn sector_menu_action_label(action: &SectorMenuAction) -> &'static str {
    match action {
        SectorMenuAction::PlaceSystem { .. } => "sector :: PLACE SYSTEM",
        SectorMenuAction::PaintRegion { .. } => "sector :: PAINT REGION",
        SectorMenuAction::EraseRegion { .. } => "sector :: ERASE REGION",
        SectorMenuAction::FocusSystem { .. } => "sector :: FOCUS SYSTEM",
        SectorMenuAction::RenameSystem { .. } => "sector :: RENAME",
        SectorMenuAction::DeleteSystem { .. } => "sector :: DELETE",
        SectorMenuAction::AddRouteFrom { .. } => "sector :: ADD ROUTE FROM",
        SectorMenuAction::AddWorld { .. } => "sector :: ADD WORLD",
        SectorMenuAction::RegenerateSystem { .. } => "sector :: REGENERATE SYSTEM",
        SectorMenuAction::TogglePin { .. } => "sector :: TOGGLE PIN",
        SectorMenuAction::OpenIn { .. } => "sector :: OPEN IN",
        SectorMenuAction::StartPartialRegen { .. } => "sector :: START PARTIAL REGEN",
        SectorMenuAction::MultiFocusFirst => "multi :: FOCUS FIRST",
        SectorMenuAction::MultiBulkRenameOpen => "multi :: BULK RENAME",
        SectorMenuAction::MultiPinAll => "multi :: PIN ALL",
        SectorMenuAction::MultiUnpinAll => "multi :: UNPIN ALL",
        SectorMenuAction::MultiDeleteAllConfirmed => "multi :: DELETE ALL",
        SectorMenuAction::MultiAssignPrimaryFaction { .. } => "multi :: ASSIGN PRIMARY FACTION",
        SectorMenuAction::MultiFlipControlState { .. } => "multi :: FLIP CONTROL STATE",
        SectorMenuAction::MultiReseedWorlds => "multi :: RESEED WORLDS",
        SectorMenuAction::MultiClearSelection => "multi :: CLEAR SELECTION",
        SectorMenuAction::FocusRoute { .. } => "route :: FOCUS",
        SectorMenuAction::RemoveRoute { .. } => "route :: REMOVE",
        SectorMenuAction::SetRouteType { .. } => "route :: SET TYPE",
        SectorMenuAction::SetRouteStability { .. } => "route :: SET STABILITY",
        SectorMenuAction::FocusRegion { .. } => "region :: FOCUS",
        SectorMenuAction::EraseRegionHex { .. } => "region :: ERASE HEX",
        SectorMenuAction::SetRegionKind { .. } => "region :: RECOLOR",
        SectorMenuAction::RenameRegionOpen { .. } => "region :: RENAME",
        SectorMenuAction::CancelRoute => "route :: CANCEL",
    }
}

/// §CTX1 — apply one menu item. Returns the menu's close intent (always
/// `true`: every action dismisses the menu).
pub(super) fn apply_sector_menu_action(state: &mut BuilderState, action: SectorMenuAction) -> bool {
    // §CTX1 Phase 7 — capture the activation label up-front so we record
    // *what* the user clicked even when the action errors out below.
    state.last_menu_action = Some(sector_menu_action_label(&action).to_string());
    match action {
        SectorMenuAction::PlaceSystem { coord } => {
            let default_name = format!("Sys-{}", state.sector.systems.len() + 1);
            state.pending_place = Some(PendingPlace {
                coord,
                name: default_name,
            });
        }
        SectorMenuAction::PaintRegion { coord } => {
            paint_region_at(state, coord, false);
        }
        SectorMenuAction::EraseRegion { coord } => {
            let owning = state
                .sector
                .regions
                .iter()
                .find(|r| r.hexes.iter().any(|h| *h == coord))
                .map(|r| r.id.clone());
            if let Some(rid) = owning {
                if let Err(e) = state.erase_region_hex(&rid, coord) {
                    state.modal = Some(ModalKind::Message(format!("Region erase failed: {e}")));
                }
            }
        }
        SectorMenuAction::FocusSystem { id } => {
            state.focus_entity(EntityRef::System(id));
        }
        SectorMenuAction::RenameSystem { id } => {
            let name = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.to_string())
                .unwrap_or_default();
            state.pending_rename = Some(PendingRename { id, text: name });
        }
        SectorMenuAction::DeleteSystem { id } => {
            let cmd = BuilderCommand::RemoveSystem {
                id,
                before: None,
                removed_routes: Vec::new(),
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Delete failed: {e}")));
            }
        }
        SectorMenuAction::AddRouteFrom { id } => {
            state.map_tool = MapTool::AddRoute;
            state.pending_route_start = Some(id);
        }
        SectorMenuAction::AddWorld { id } => {
            let n = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.worlds.len() + 1)
                .unwrap_or(1);
            let cmd = BuilderCommand::AddWorld {
                system: id,
                name: format!("World-{n}"),
                result_id: None,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Add world failed: {e}")));
            }
        }
        SectorMenuAction::RegenerateSystem { id, coord } => {
            if state.pinned_systems.contains(&id) {
                return true;
            }
            let index = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.index)
                .unwrap_or(0);
            match state.generate_system_here(coord, index, None) {
                Ok(new_id) => state.focus_system(new_id),
                Err(e) => {
                    state.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
                }
            }
        }
        SectorMenuAction::TogglePin { id } => {
            if state.pinned_systems.contains(&id) {
                state.pinned_systems.remove(&id);
            } else {
                state.pinned_systems.insert(id);
            }
        }
        SectorMenuAction::StartPartialRegen { coord } => {
            state.partial_regen_anchor = Some(coord);
        }
        SectorMenuAction::OpenIn { id, target } => match target {
            OpenInTarget::System => {
                state.focus_entity(EntityRef::System(id));
            }
            OpenInTarget::World => {
                let first_world = state
                    .sector
                    .systems
                    .iter()
                    .find(|s| s.id == id)
                    .and_then(|s| s.worlds.first().map(|w| w.id.clone()));
                if let Some(world) = first_world {
                    state.focus_entity(EntityRef::World { system: id, world });
                }
            }
            OpenInTarget::Routes => {
                state.selected_system_id = Some(id);
                state.focus_entity(EntityRef::Tab(BuilderTab::Routes));
            }
        },
        SectorMenuAction::MultiFocusFirst => {
            if let Some(first) = state.selected_systems.iter().next().cloned() {
                state.focus_entity(EntityRef::System(first));
            }
        }
        SectorMenuAction::MultiBulkRenameOpen => {
            state.pending_bulk_rename = Some(PendingBulkRename {
                pattern: "Sys-{n}".to_string(),
            });
        }
        SectorMenuAction::MultiPinAll => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.insert(id);
            }
        }
        SectorMenuAction::MultiUnpinAll => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.remove(&id);
            }
        }
        SectorMenuAction::MultiDeleteAllConfirmed => {
            let ids: Vec<SystemId> = state.selected_systems.iter().cloned().collect();
            for id in ids {
                let cmd = BuilderCommand::RemoveSystem {
                    id: id.clone(),
                    before: None,
                    removed_routes: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.modal = Some(ModalKind::Message(format!(
                        "Bulk delete failed at {id}: {e}"
                    )));
                    break;
                }
            }
            state.selected_systems.clear();
            state.selected_system_id = None;
        }
        SectorMenuAction::MultiAssignPrimaryFaction { fid } => {
            crate::builder::panels::system::apply_bulk_primary_faction(state, fid);
        }
        SectorMenuAction::MultiFlipControlState { value } => {
            crate::builder::panels::system::apply_bulk_control_state(state, value);
        }
        SectorMenuAction::MultiReseedWorlds => {
            crate::builder::panels::system::apply_bulk_reseed(state);
        }
        SectorMenuAction::MultiClearSelection => {
            state.selected_systems.clear();
            state.selected_system_id = None;
        }
        SectorMenuAction::FocusRoute { id } => {
            state.focus_entity(EntityRef::Route(id));
        }
        SectorMenuAction::RemoveRoute { id } => {
            let cmd = BuilderCommand::RemoveRoute { id, before: None };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Remove route failed: {e}")));
            }
        }
        SectorMenuAction::SetRouteType { id, value } => {
            let before = state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.route_type);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRouteType {
                id,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Set route type failed: {e}")));
            }
        }
        SectorMenuAction::SetRouteStability { id, value } => {
            let before = state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.stability);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRouteStability {
                id,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Set stability failed: {e}")));
            }
        }
        SectorMenuAction::FocusRegion { region } => {
            state.focus_entity(EntityRef::Region(region));
        }
        SectorMenuAction::EraseRegionHex { region, coord } => {
            if let Err(e) = state.erase_region_hex(&region, coord) {
                state.modal = Some(ModalKind::Message(format!("Region erase failed: {e}")));
            }
        }
        SectorMenuAction::SetRegionKind { region, value } => {
            let before = state
                .sector
                .regions
                .iter()
                .find(|r| r.id == region)
                .map(|r| r.kind);
            let Some(before) = before else { return true };
            if before == value {
                return true;
            }
            let cmd = BuilderCommand::SetRegionKind {
                region,
                before,
                after: value,
            };
            if let Err(e) = state.run(cmd) {
                state.modal = Some(ModalKind::Message(format!("Recolor region failed: {e}")));
            }
        }
        SectorMenuAction::RenameRegionOpen { region } => {
            let text = state
                .sector
                .regions
                .iter()
                .find(|r| r.id == region)
                .map(|r| r.name.clone())
                .unwrap_or_default();
            state.pending_region_rename = Some(PendingRegionRename { region, text });
        }
        SectorMenuAction::CancelRoute => {
            state.pending_route_start = None;
            state.map_tool = MapTool::Select;
        }
    }
    true
}

/// §CTX1 — Phase 2 §6.1: render the empty-hex schema. Returns `true` when
/// any item activated.
fn render_empty_hex_menu(ui: &mut egui::Ui, state: &mut BuilderState, coord: HexCoord) -> bool {
    let mut close = false;

    if ui.selectable_label(false, "PLACE SYSTEM HERE…").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::PlaceSystem { coord });
    }

    let paint_enabled = state.selected_region_id.is_some();
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
        .any(|r| r.hexes.iter().any(|h| *h == coord));
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
fn render_system_menu(
    ui: &mut egui::Ui,
    state: &mut BuilderState,
    id: SystemId,
    coord: HexCoord,
) -> bool {
    // §CTX1 §10 row 12 — while ADD-ROUTE is half-armed, the system menu
    // collapses to "CANCEL ROUTE" + "Open in ROUTES" so the user can't
    // accidentally start a second pending route or run a destructive item.
    if state.pending_route_start.is_some() {
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
        let _ = coord;
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
        regen_resp.on_hover_text("Unpin first (§S3).");
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
fn render_multi_selection_menu(
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
                if let Some(menu) = state.sector_context_menu.as_mut() {
                    menu.bulk_delete_confirm = false;
                }
            }
        });
    } else if ui
        .selectable_label(false, format!("✕ DELETE ALL ({})", ids.len()))
        .clicked()
    {
        if let Some(menu) = state.sector_context_menu.as_mut() {
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
                Some(v) => format!("{v:?}"),
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
        reseed_resp.on_hover_text("All selected systems are pinned — unpin first (§S3).");
    }

    ui.separator();

    if ui.selectable_label(false, "CLEAR SELECTION").clicked() {
        close |= apply_sector_menu_action(state, SectorMenuAction::MultiClearSelection);
    }

    close
}

/// §CTX1 Phase 5: human-readable label for a [`RouteStability`].
fn stability_label(value: RouteStability) -> &'static str {
    match value {
        RouteStability::Stable => "stable",
        RouteStability::Unstable => "unstable",
        RouteStability::Hazardous => "hazardous",
        RouteStability::Perilous => "perilous",
    }
}

/// §CTX1 — Phase 5 §6.4: render the route schema.
fn render_route_menu(ui: &mut egui::Ui, state: &mut BuilderState, id: RouteId) -> bool {
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
            stability_label(cur_stab)
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
                format!("• {}", stability_label(value))
            } else {
                stability_label(value).to_string()
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
fn render_region_hex_menu(
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
                format!("  {} {}", value.glyph(), value.label())
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

/// §CTX1 — Phase 1: pure dismiss predicate.
pub(super) fn should_dismiss_sector_context_menu(
    esc_pressed: bool,
    focused: bool,
    primary_click_outside: bool,
) -> bool {
    esc_pressed || !focused || primary_click_outside
}

/// §CTX1 §10 — returns `true` when the target referenced by an open menu no
/// longer exists in `state.sector`.
pub(super) fn sector_menu_target_is_stale(state: &BuilderState, target: &SectorMenuTarget) -> bool {
    match target {
        SectorMenuTarget::System { id, .. } => !state.sector.systems.iter().any(|s| &s.id == id),
        SectorMenuTarget::Route { id, .. } => !state.sector.routes.iter().any(|r| &r.id == id),
        SectorMenuTarget::RegionHex { region, .. } => {
            !state.sector.regions.iter().any(|r| r.id == *region)
        }
        SectorMenuTarget::MultiSelection { ids } => ids
            .iter()
            .all(|id| !state.sector.systems.iter().any(|s| &s.id == id)),
        SectorMenuTarget::EmptyHex { .. } | SectorMenuTarget::SubsectorBorder { .. } => false,
    }
}

/// §CTX1 — Phase 7 polish: pick the [`Align2`] pivot that should anchor a
/// floating right-click menu at `cursor`.
pub(in crate::builder::panels) fn menu_anchor_pivot(
    cursor: egui::Pos2,
    screen: egui::Rect,
) -> egui::Align2 {
    let centre = screen.center();
    let right = cursor.x > centre.x;
    let bottom = cursor.y > centre.y;
    match (right, bottom) {
        (false, false) => egui::Align2::LEFT_TOP,
        (true, false) => egui::Align2::RIGHT_TOP,
        (false, true) => egui::Align2::LEFT_BOTTOM,
        (true, true) => egui::Align2::RIGHT_BOTTOM,
    }
}

/// §CTX1 — render the floating right-click menu.
pub(super) fn show_sector_context_menu(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(menu) = state.sector_context_menu.as_ref() else {
        return;
    };
    if sector_menu_target_is_stale(state, &menu.target) {
        state.sector_context_menu = None;
        return;
    }
    let screen_pos = menu.screen_pos;
    let target = menu.target.clone();
    let mut close = false;
    let screen_rect = ctx.input(|i| i.screen_rect());
    let pivot = menu_anchor_pivot(screen_pos, screen_rect);
    let area_resp = egui::Area::new(egui::Id::new("sector_context_menu"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(screen_pos)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                match &target {
                    SectorMenuTarget::EmptyHex { coord } => {
                        close |= render_empty_hex_menu(ui, state, *coord);
                    }
                    SectorMenuTarget::System { id, coord } => {
                        close |= render_system_menu(ui, state, id.clone(), *coord);
                    }
                    SectorMenuTarget::MultiSelection { ids } => {
                        close |= render_multi_selection_menu(ui, state, ids);
                    }
                    SectorMenuTarget::Route { id, .. } => {
                        close |= render_route_menu(ui, state, id.clone());
                    }
                    SectorMenuTarget::RegionHex { region, coord } => {
                        close |= render_region_hex_menu(ui, state, region, *coord);
                    }
                    SectorMenuTarget::SubsectorBorder { .. } => {
                        if ui.selectable_label(false, "CLOSE").clicked() {
                            close = true;
                        }
                    }
                }
            });
        });

    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let focused = ctx.input(|i| i.focused);
    let area_rect = area_resp.response.rect;
    let primary_click_outside = ctx.input(|i| {
        i.pointer.primary_clicked()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !area_rect.contains(p))
    });

    if close || should_dismiss_sector_context_menu(esc, focused, primary_click_outside) {
        state.sector_context_menu = None;
    }
}
