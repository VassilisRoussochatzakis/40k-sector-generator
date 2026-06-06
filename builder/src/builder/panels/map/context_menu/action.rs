//! §CTX1 — Phase 2 action model + dispatch for the MAP right-click menu.
//! [`SectorMenuAction`] enumerates one variant per menu row;
//! [`apply_sector_menu_action`] applies the chosen item to [`BuilderState`]
//! (document edits route through the command bus / `edit_*` wrappers, never a
//! direct field write). The whole path is testable without an egui context, so
//! the `map/mod.rs` test module round-trips ~every variant.

use sectorforge::ids::{FactionId, RouteId, SystemId};
use sectorforge::regions::RegionConditionKind;
use sectorforge::sector_model::{HexCoord, RouteStability, RouteType, SystemState};

use crate::builder::command::BuilderCommand;
use crate::builder::panels::map::interactions::paint_region_at;
use crate::builder::state::{
    BuilderTab, EntityRef, MapTool, PendingBulkRename, PendingPlace, PendingRegionRename,
    PendingRename,
};
use crate::builder::{BuilderState, ModalKind};

/// §CTX1 — Phase 2: per-item action types. Each variant maps 1:1 to a menu
/// row in the §6.1 / §6.2 schemas. Splitting the actions from the render path
/// lets unit tests assert state mutations without standing up an egui context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::builder::panels::map) enum SectorMenuAction {
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
pub(in crate::builder::panels::map) enum OpenInTarget {
    System,
    World,
    Routes,
}

/// §CTX1 Phase 7 polish — short human label for the telemetry tail rendered
/// in the status bar. Mirrors the spec's "ctx_menu: <schema> :: <item>"
/// format.
pub(in crate::builder::panels::map) fn sector_menu_action_label(
    action: &SectorMenuAction,
) -> &'static str {
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
pub(in crate::builder::panels::map) fn apply_sector_menu_action(
    state: &mut BuilderState,
    action: SectorMenuAction,
) -> bool {
    // §CTX1 Phase 7 — capture the activation label up-front so we record
    // *what* the user clicked even when the action errors out below.
    state.feedback.last_menu_action = Some(sector_menu_action_label(&action).to_string());
    match action {
        SectorMenuAction::PlaceSystem { coord } => {
            let default_name = format!("Sys-{}", state.sector.systems.len() + 1);
            state.drag.pending_place = Some(PendingPlace {
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
                .find(|r| r.hexes.contains(&coord))
                .map(|r| r.id.clone());
            if let Some(rid) = owning {
                // D1: discrete erase → one undoable EditRegion.
                state.begin_region_stroke(&rid);
                if let Err(e) = state.erase_region_hex(&rid, coord) {
                    state.drag.region_stroke_before = None;
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("Region erase failed: {e}")));
                } else {
                    state.commit_region_stroke();
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
            state.drag.pending_rename = Some(PendingRename { id, text: name });
        }
        SectorMenuAction::DeleteSystem { id } => {
            let cmd = BuilderCommand::RemoveSystem {
                id,
                before: None,
                removed_routes: Vec::new(),
            };
            if let Err(e) = state.run(cmd) {
                state.feedback.modal = Some(ModalKind::Message(format!("Delete failed: {e}")));
            }
        }
        SectorMenuAction::AddRouteFrom { id } => {
            state.map_view.tool = MapTool::AddRoute;
            state.drag.pending_route_start = Some(id);
        }
        SectorMenuAction::AddWorld { id } => {
            let (sys_name, next_orbit, next_index) = state
                .sector
                .systems
                .iter()
                .find(|s| s.id == id)
                .map(|s| {
                    (
                        s.name.to_string(),
                        s.worlds
                            .iter()
                            .map(|w| w.orbit)
                            .max()
                            .unwrap_or(0)
                            .saturating_add(1),
                        s.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1,
                    )
                })
                .unwrap_or((String::new(), 1, 1));
            let name = format!(
                "{sys_name} {}",
                sectorforge::names::roman_numeral(next_orbit as usize)
            );
            let cmd = BuilderCommand::AddWorld {
                system: id.clone(),
                name,
                result_id: None,
            };
            match state.run(cmd) {
                Err(e) => {
                    state.feedback.modal =
                        Some(ModalKind::Message(format!("Add world failed: {e}")));
                }
                Ok(()) => {
                    // §R4: pin the new world's orbit through the command bus
                    // rather than writing `w.orbit` directly. Resolve the new
                    // world's id (found via `next_index`) into an owned value
                    // first so no `state.sector` borrow is held across the
                    // dispatch. `SetWorldOrbit::apply` captures `before`.
                    let new_world = state
                        .sector
                        .systems
                        .iter()
                        .find(|s| s.id == id)
                        .and_then(|s| s.worlds.iter().find(|w| w.index == next_index))
                        .map(|w| w.id.clone());
                    if let Some(world) = new_world {
                        let orbit_cmd = BuilderCommand::SetWorldOrbit {
                            world,
                            before: 0,
                            after: next_orbit,
                        };
                        if let Err(e) = state.run(orbit_cmd) {
                            state.feedback.modal =
                                Some(ModalKind::Message(format!("Set world orbit failed: {e}")));
                        }
                    }
                }
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
                    state.feedback.modal = Some(ModalKind::Message(format!("Regen failed: {e}")));
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
            state.map_view.partial_regen_anchor = Some(coord);
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
                state.selection.system_id = Some(id);
                state.focus_entity(EntityRef::Tab(BuilderTab::Routes));
            }
        },
        SectorMenuAction::MultiFocusFirst => {
            if let Some(first) = state.selection.systems.iter().next().cloned() {
                state.focus_entity(EntityRef::System(first));
            }
        }
        SectorMenuAction::MultiBulkRenameOpen => {
            state.drag.pending_bulk_rename = Some(PendingBulkRename {
                pattern: "Sys-{n}".to_string(),
            });
        }
        SectorMenuAction::MultiPinAll => {
            let ids: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.insert(id);
            }
        }
        SectorMenuAction::MultiUnpinAll => {
            let ids: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
            for id in ids {
                state.pinned_systems.remove(&id);
            }
        }
        SectorMenuAction::MultiDeleteAllConfirmed => {
            let ids: Vec<SystemId> = state.selection.systems.iter().cloned().collect();
            for id in ids {
                let cmd = BuilderCommand::RemoveSystem {
                    id: id.clone(),
                    before: None,
                    removed_routes: Vec::new(),
                };
                if let Err(e) = state.run(cmd) {
                    state.feedback.modal = Some(ModalKind::Message(format!(
                        "Bulk delete failed at {id}: {e}"
                    )));
                    break;
                }
            }
            state.selection.systems.clear();
            state.selection.system_id = None;
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
            state.selection.systems.clear();
            state.selection.system_id = None;
        }
        SectorMenuAction::FocusRoute { id } => {
            state.focus_entity(EntityRef::Route(id));
        }
        SectorMenuAction::RemoveRoute { id } => {
            let cmd = BuilderCommand::RemoveRoute { id, before: None };
            if let Err(e) = state.run(cmd) {
                state.feedback.modal =
                    Some(ModalKind::Message(format!("Remove route failed: {e}")));
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
                state.feedback.modal =
                    Some(ModalKind::Message(format!("Set route type failed: {e}")));
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
                state.feedback.modal =
                    Some(ModalKind::Message(format!("Set stability failed: {e}")));
            }
        }
        SectorMenuAction::FocusRegion { region } => {
            state.focus_entity(EntityRef::Region(region));
        }
        SectorMenuAction::EraseRegionHex { region, coord } => {
            // D1: discrete erase → one undoable EditRegion.
            state.begin_region_stroke(&region);
            if let Err(e) = state.erase_region_hex(&region, coord) {
                state.drag.region_stroke_before = None;
                state.feedback.modal =
                    Some(ModalKind::Message(format!("Region erase failed: {e}")));
            } else {
                state.commit_region_stroke();
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
                state.feedback.modal =
                    Some(ModalKind::Message(format!("Recolor region failed: {e}")));
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
            state.drag.pending_region_rename = Some(PendingRegionRename { region, text });
        }
        SectorMenuAction::CancelRoute => {
            state.drag.pending_route_start = None;
            state.map_view.tool = MapTool::Select;
        }
    }
    true
}
