//! §CTX1 right-click context menu schemas + apply path for the MAP tab.
//!
//! Resolves a right-click into a [`SectorMenuTarget`], routes the chosen item
//! to an action enum, and renders one of five schemas: §6.1 empty hex, §6.2
//! single system, §6.3 multi-selection, §6.4 route, §6.5 region hex. The pure
//! `apply_sector_menu_action` path is testable without an egui context.
//!
//! Submodules:
//! - [`resolve`] — right-click hit-test + the pure dismiss/staleness predicates
//!   + the floating-menu anchor pivot.
//! - [`action`] — the `SectorMenuAction` model and `apply_sector_menu_action`
//!   dispatch (the netted core).
//! - [`render`] — the five interactive schema builders (un-netted).
//!
//! `show_sector_context_menu` (below) is the glue: it gates on the resolve-side
//! predicates, dispatches to a `render::*` builder by target, and anchors the
//! floating menu via [`menu_anchor_pivot`]. Keeping it here lets it reach the
//! private `render::*` builders and the re-exported `resolve::*` helpers without
//! widening anything further.

mod action;
mod render;
mod resolve;

use crate::builder::state::SectorMenuTarget;
use crate::builder::BuilderState;

use render::{
    render_empty_hex_menu, render_multi_selection_menu, render_region_hex_menu, render_route_menu,
    render_system_menu,
};

// §CTX1 — re-export the map-visible surface so external consumers keep
// resolving at `context_menu::` after the dir-module split. These are
// equal-visibility re-exports (each item is declared `pub(in …::map)`), so they
// do not widen visibility (no E0364).
//
// The action facade is consumed only by the `map/mod.rs` `#[cfg(test)]`
// round-trip tests — the `render::*` builders reach `apply_sector_menu_action`
// & friends via the internal `action::` path — so it is gated to the test build
// to avoid an unused-import warning in the non-test lib.
#[cfg(test)]
pub(super) use action::{
    apply_sector_menu_action, sector_menu_action_label, OpenInTarget, SectorMenuAction,
};
pub(super) use resolve::{
    resolve_sector_context, sector_menu_target_is_stale, should_dismiss_sector_context_menu,
};
// `menu_anchor_pivot` is also consumed by `panels::system_map` (via the
// `map/mod.rs` re-export), so its surface stays at panels-wide visibility.
pub(in crate::builder::panels) use resolve::menu_anchor_pivot;

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
