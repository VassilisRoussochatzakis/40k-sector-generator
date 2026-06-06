//! §CTX1 — right-click target resolution + pure menu predicates for the MAP
//! tab. The hit-test ([`resolve_sector_context`]), the dismiss / staleness
//! predicates, and the floating-menu anchor pivot live here. All are pure reads
//! of [`BuilderState`], so the `map/mod.rs` test module exercises them directly
//! without standing up an egui interaction loop.

use egui::Pos2;

use sectorforge::sector_model::HexCoord;
use sectorforge_gui_core::sector_view::SectorGeom;

use crate::builder::state::{MapTool, SectorMenuTarget};
use crate::builder::BuilderState;

/// §CTX1 — Phase 1: resolve what the right-click landed on. Pure read of
/// `state`, so unit tests can call it directly with a synthesised
/// [`SectorGeom`] + screen position. Returns `None` when the click should be
/// ignored (drag in progress / rect-select live / collision dialog already
/// open / RegionPaint mode without Ctrl).
pub(in crate::builder::panels::map) fn resolve_sector_context(
    state: &BuilderState,
    geom: &SectorGeom,
    pos: Pos2,
    sector_w: u32,
    sector_h: u32,
    ctrl_down: bool,
) -> Option<SectorMenuTarget> {
    // Suppression guards (§4.1).
    if state.drag.drag_system.is_some() || state.drag.rect_select.is_some() {
        return None;
    }
    if state.drag.pending_collision.is_some() {
        return None;
    }
    if state.map_view.tool == MapTool::RegionPaint && !ctrl_down {
        return None;
    }

    if let Some(id) = geom.hit_system(&state.sector, pos) {
        let coord = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.coord)?;
        if state.selection.systems.contains(&id) && state.selection.systems.len() >= 2 {
            return Some(SectorMenuTarget::MultiSelection {
                ids: state.selection.systems.iter().cloned().collect(),
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
            .map_view
            .cache
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

/// §CTX1 — Phase 1: pure dismiss predicate.
pub(in crate::builder::panels::map) fn should_dismiss_sector_context_menu(
    esc_pressed: bool,
    focused: bool,
    primary_click_outside: bool,
) -> bool {
    esc_pressed || !focused || primary_click_outside
}

/// §CTX1 §10 — returns `true` when the target referenced by an open menu no
/// longer exists in `state.sector`.
pub(in crate::builder::panels::map) fn sector_menu_target_is_stale(
    state: &BuilderState,
    target: &SectorMenuTarget,
) -> bool {
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
