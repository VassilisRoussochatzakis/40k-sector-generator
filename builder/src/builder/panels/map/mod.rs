//! MAP tab (§N3) — hex render + editor toolbox + transient dialogs.
//!
//! Renders the sector via [`sectorforge_gui_core::sector_view::SectorView`]
//! so the visual surface matches the main viewer (§N3 / §S2). Builder-only
//! interactions — tool dispatch, drag-move, rect-select, double-click rename,
//! pinned/multi-select overlays, the §S6 collision dialog — are layered on
//! top of the shared widget rather than reimplemented here.
//!
//! Submodules:
//! - [`cache`] — subsector + map-view cache rebuild on sector slice changes.
//! - [`interactions`] — hex render dispatcher + tool routing (click/drag).
//! - [`context_menu`] — §CTX1 right-click menu schemas + apply path.
//! - [`dialogs`] — modal Place / Rename / Bulk-Rename / Region-Rename /
//!   Collision windows.

mod cache;
mod context_menu;
mod dialogs;
mod interactions;
mod theme;

use crate::builder::state::{EntityRef, MapTool};
use crate::builder::BuilderState;

use sectorforge_gui_core::{palette, ui_kit::{self, labeled}};

pub(super) use context_menu::menu_anchor_pivot;

pub(crate) fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Map");
    ui.add_space(4.0);

    // §COLUMNS — move the editor controls (tool rail, zoom/selection status,
    // intel + theme/heatmap controls) into a resizable left `SidePanel` so the
    // hex canvas keeps the whole `CentralPanel` and never shrinks to a strip
    // while a tall toolbox steals its vertical budget. The canvas itself stays
    // inside the `CentralPanel`'s `ScrollArea::both()` and is allocated exactly
    // as before (`allocate_exact_size` → `rect.min` origin), so the pointer /
    // drag / rect-select coordinate math is unchanged — it just gets more room.
    egui::SidePanel::left("map_tools")
        .resizable(true)
        .default_width(280.0)
        .width_range(220.0..=460.0)
        .show_inside(ui, |ui| show_tool_rail(ui, state));

    // §COLUMNS §6.2 — pin the selected-entity inspector to a right rail so a
    // click on the map shows details in place instead of navigating away to the
    // SYSTEM / WORLD tab. Declared before the CentralPanel so it claims its edge
    // first; the canvas still allocates its own rect, so the pointer / drag math
    // is unchanged.
    egui::SidePanel::right("map_inspector")
        .resizable(true)
        .default_width(260.0)
        .width_range(200.0..=420.0)
        .show_inside(ui, |ui| show_map_inspector(ui, state));

    egui::CentralPanel::default().show_inside(ui, |ui| {
        egui::ScrollArea::both().show(ui, |ui| {
            interactions::show_hex_map(ui, state);
        });
    });

    // §CTX1 — Phase 1: floating right-click menu rendered as a free-standing
    // `egui::Area`. Sits above the canvas; dismissed on Escape / focus-loss /
    // outside primary click / item activation. Rendered at the tab root (not
    // inside a panel) so its viewport-relative anchoring is unaffected.
    context_menu::show_sector_context_menu(ui.ctx(), state);

    dialogs::show_place_dialog(ui.ctx(), state);
    dialogs::show_rename_dialog(ui.ctx(), state);
    dialogs::show_bulk_rename_dialog(ui.ctx(), state);
    dialogs::show_region_rename_dialog(ui.ctx(), state);
    dialogs::show_collision_dialog(ui.ctx(), state);
}

/// §COLUMNS — left rail holding every MAP editor control. Scrolls vertically so
/// a tall theme editor never competes with the canvas for the window's height.
fn show_tool_rail(ui: &mut egui::Ui, state: &mut BuilderState) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            show_toolbox(ui, state);
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Zoom:")
                    .on_hover_text("Hex size on screen — drag to zoom the map in or out.");
                ui.add(egui::Slider::new(&mut state.hex_size, 12.0..=64.0).text("hex"));
            });
            if !state.selected_systems.is_empty() {
                ui.label(format!(
                    "Selected: {} system(s)",
                    state.selected_systems.len()
                ));
            }
            if let Some(id) = &state.selected_system_id {
                ui.label(format!("Focused: {id}"));
            }
            if let Some(id) = &state.pending_route_start {
                ui.label(format!("Drawing route from: {id}"));
            }
            // Phase 4 — surface the live partial-regen anchor so the user can
            // tell why their next click on the map will be consumed.
            if let Some(anchor) = state.partial_regen_anchor {
                ui.colored_label(
                    egui::Color32::from_rgb(120, 200, 240),
                    format!(
                        "Regenerate region: pick the opposite corner (from {}, {}).",
                        anchor.q, anchor.r
                    ),
                );
                if ui
                    .small_button("Cancel")
                    .on_hover_text("Stop picking a region to regenerate.")
                    .clicked()
                {
                    state.partial_regen_anchor = None;
                }
            }
            ui.separator();
            crate::builder::panels::intel::show_map_intel_controls(ui, state);
            ui.separator();
            // §35 T1/T2/T3/T4 — theme picker, custom editor, heatmap selector, legend.
            theme::show(ui, state);
        });
}

/// Map editing tools: SELECT / ADD / DELETE / MOVE / ADD ROUTE / REGION PAINT.
pub(crate) fn show_toolbox(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Tool:");
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            let selected = state.map_tool == tool;
            if ui
                .selectable_label(selected, tool.label())
                .on_hover_text(tool_help(tool))
                .clicked()
            {
                state.map_tool = tool;
                if tool != MapTool::AddRoute {
                    state.pending_route_start = None;
                }
            }
        }
    });
}

/// One-line, plain-language description of what each map tool does, shown on
/// hover over the tool button.
fn tool_help(tool: MapTool) -> &'static str {
    match tool {
        MapTool::Select => {
            "Select systems. Click to focus one, Shift-click to add to the selection, \
             or drag a box to select several."
        }
        MapTool::AddSystem => "Add a system. Click an empty hex to place a new system there.",
        MapTool::DeleteSystem => "Delete a system. Click a system to remove it from the map.",
        MapTool::MoveSystem => "Move a system. Drag a system to a new hex.",
        MapTool::AddRoute => "Add a route. Click one system, then another, to link them.",
        MapTool::RegionPaint => {
            "Paint a warp region. Brush hexes to add them to the selected region; \
             Ctrl-click for the right-click menu."
        }
    }
}

/// §COLUMNS §6.2 — right-rail inspector for the entity selected on the map.
/// Shows a compact read-only summary of the focused system (and the selected
/// world, when it belongs to that system) plus explicit "Open in … tab"
/// deep-links, so a map click reveals details in place rather than forcing a
/// jump to the SYSTEM / WORLD tab. Selecting a world here is pure view state
/// (`selected_world_id`); only the explicit buttons navigate.
fn show_map_inspector(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.label(
        egui::RichText::new("INSPECTOR")
            .small()
            .color(palette::chrome_text_dim()),
    );
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let multi = state.selected_systems.len();
            let Some(sys_id) = state.selected_system_id.clone() else {
                ui_kit::placeholder(ui, "Click a system on the map to inspect it here.");
                if multi > 1 {
                    ui.add_space(4.0);
                    ui.label(format!(
                        "{multi} systems selected. Use the SYSTEM tab's bulk operations to edit them together."
                    ));
                }
                return;
            };
            let Some(sys_idx) = state.sector.systems.iter().position(|s| s.id == sys_id) else {
                return;
            };

            // Snapshot system facts up-front so the deep-link buttons below can
            // mutate `state` without holding a `sector` borrow across the click.
            let (sys_name, coord, kind, star, worlds, primary, control_state) = {
                let sys = &state.sector.systems[sys_idx];
                (
                    sys.name.to_string(),
                    sys.coord,
                    sys.kind,
                    sys.star
                        .as_ref()
                        .map(|s| format!("{} ({})", s.colour_code, s.colour_name)),
                    sys.worlds
                        .iter()
                        .map(|w| (w.id.clone(), w.name.to_string()))
                        .collect::<Vec<_>>(),
                    sys.primary_factions.to_vec(),
                    sys.control.state,
                )
            };

            let mut open_system = false;
            let mut pick_world = None;
            let mut open_world = None;
            let mut open_faction = None;

            ui_kit::section(ui, &format!("{sys_name}  ·  {sys_id}"), |ui| {
                labeled(
                    ui,
                    "Coordinates",
                    "Hex grid position of this system (schema: coord), as column, row.",
                    |ui| {
                        ui.label(format!("({}, {})", coord.q, coord.r));
                    },
                );
                labeled(
                    ui,
                    "Kind",
                    "What sits at this hex — a star system, deep-space anomaly, and so on (schema: kind).",
                    |ui| {
                        ui.label(kind.to_string());
                    },
                );
                if let Some(star) = &star {
                    labeled(
                        ui,
                        "Star",
                        "Colour class of the system's star (schema: star), shown as code and name.",
                        |ui| {
                            ui.label(star);
                        },
                    );
                }
                if let Some(cs) = control_state {
                    labeled(
                        ui,
                        "Control",
                        "Who holds this system and how contested it is (schema: control.state).",
                        |ui| {
                            ui.label(cs.to_string());
                        },
                    );
                }
                if !primary.is_empty() {
                    ui.label("Primary factions:")
                        .on_hover_text("Factions that dominate this system (schema: primary_factions). Click one to open it.");
                    for fid in &primary {
                        if sectorforge_gui_core::entity_link(ui, fid.to_string(), true).clicked() {
                            open_faction = Some(fid.clone());
                        }
                    }
                }
                if ui
                    .button("Open in SYSTEM tab")
                    .on_hover_text("Edit this system's full details in the SYSTEM tab.")
                    .clicked()
                {
                    open_system = true;
                }
            });

            ui.add_space(4.0);
            ui_kit::section(ui, &format!("Worlds ({})", worlds.len()), |ui| {
                if worlds.is_empty() {
                    ui_kit::placeholder(ui, "No worlds in this system yet.");
                }
                for (wid, wname) in &worlds {
                    let sel = state.selected_world_id.as_ref() == Some(wid);
                    if ui
                        .selectable_label(sel, format!("{wname}  ·  {wid}"))
                        .clicked()
                    {
                        pick_world = Some(wid.clone());
                    }
                }
            });

            // World detail card — only when the selected world is in this system.
            if let Some(wid) = state.selected_world_id.clone() {
                if let Some(w_idx) = state.sector.systems[sys_idx]
                    .worlds
                    .iter()
                    .position(|w| w.id == wid)
                {
                    let (wname, wtype, pop, gov, fac_n) = {
                        let w = &state.sector.systems[sys_idx].worlds[w_idx];
                        (
                            w.name.to_string(),
                            w.world.world_type.to_string(),
                            w.world.population.to_string(),
                            w.world.government.to_string(),
                            w.factions.len(),
                        )
                    };
                    ui.add_space(4.0);
                    ui_kit::section(ui, &format!("World · {wname}"), |ui| {
                        labeled(
                            ui,
                            "Type",
                            "World classification, e.g. hive, agri, forge (schema: world.world_type).",
                            |ui| {
                                ui.label(&wtype);
                            },
                        );
                        labeled(
                            ui,
                            "Population",
                            "Rough population band of the world (schema: world.population).",
                            |ui| {
                                ui.label(&pop);
                            },
                        );
                        labeled(
                            ui,
                            "Government",
                            "How the world is governed (schema: world.government).",
                            |ui| {
                                ui.label(&gov);
                            },
                        );
                        labeled(
                            ui,
                            "Factions present",
                            "How many factions hold a presence on this world (schema: factions).",
                            |ui| {
                                ui.label(fac_n.to_string());
                            },
                        );
                        if ui
                            .button("Open in WORLD tab")
                            .on_hover_text("Edit this world's full details in the WORLD tab.")
                            .clicked()
                        {
                            open_world = Some(wid.clone());
                        }
                    });
                }
            }

            // Apply deferred selection / navigation after the borrows above end.
            if let Some(wid) = pick_world {
                state.selected_world_id = Some(wid);
            }
            if open_system {
                state.focus_entity(EntityRef::System(sys_id.clone()));
            }
            if let Some(wid) = open_world {
                state.focus_entity(EntityRef::World {
                    system: sys_id.clone(),
                    world: wid,
                });
            }
            if let Some(fid) = open_faction {
                state.focus_entity(EntityRef::Faction(fid));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::cache::refresh_map_cache;
    use super::context_menu::{
        apply_sector_menu_action, menu_anchor_pivot, resolve_sector_context,
        sector_menu_action_label, sector_menu_target_is_stale, should_dismiss_sector_context_menu,
        OpenInTarget, SectorMenuAction,
    };
    use super::interactions::{
        apply_partial_regen_anchor_click, apply_rect_select, handle_click, handle_drag_drop,
    };
    use crate::builder::command::BuilderCommand;
    use crate::builder::state::{
        BuilderTab, MapTool, PendingCollision, SectorContextMenu, SectorMenuTarget,
    };
    use crate::builder::{BuilderState, ModalKind};
    use egui::Pos2;
    use sectorforge::ids::SystemId;
    use sectorforge::regions::RegionConditionKind;
    use sectorforge::sector_model::{HexCoord, RouteStability, RouteType, SystemState};
    use sectorforge_gui_core::sector_view::SectorGeom;

    #[test]
    fn map_inspector_paints_headless_with_selection() {
        // §COLUMNS §6.2: the right-rail inspector must paint without panicking
        // when a system + world are focused (exercises the deferred-mutation
        // borrow pattern: snapshot, render links, apply after the borrows end).
        let mut state = BuilderState::new_blank("t", "T", "seed", 8, 8);
        let sid = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let wid = state.sector.add_world_to_system(&sid, "W").unwrap();
        state.selected_system_id = Some(sid);
        state.selected_world_id = Some(wid);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::right("map_inspector_test")
                .show(ctx, |ui| super::show_map_inspector(ui, &mut state));
        });
    }

    #[test]
    fn map_tool_labels_are_non_empty() {
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            assert!(!tool.label().is_empty());
        }
    }

    fn blank(width: u32, height: u32) -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", width, height)
    }

    #[test]
    fn handle_click_select_focuses_system() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        handle_click(
            &mut state,
            Some(id.clone()),
            Some(HexCoord { q: 1, r: 1 }),
            false,
        );
        assert_eq!(state.selected_system_id, Some(id.clone()));
        assert!(state.selected_systems.contains(&id));
    }

    #[test]
    fn handle_click_shift_adds_to_selection() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 0 }, "B")
            .unwrap();
        handle_click(
            &mut state,
            Some(a.clone()),
            Some(HexCoord { q: 0, r: 0 }),
            false,
        );
        handle_click(
            &mut state,
            Some(b.clone()),
            Some(HexCoord { q: 1, r: 0 }),
            true,
        );
        assert!(state.selected_systems.contains(&a));
        assert!(state.selected_systems.contains(&b));
    }

    #[test]
    fn handle_drag_drop_move_succeeds() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        handle_drag_drop(&mut state, id.clone(), HexCoord { q: 3, r: 3 });
        let sys = state.sector.systems.iter().find(|s| s.id == id).unwrap();
        assert_eq!(sys.coord, HexCoord { q: 3, r: 3 });
    }

    #[test]
    fn handle_drag_drop_collision_arms_dialog() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        handle_drag_drop(&mut state, a.clone(), HexCoord { q: 2, r: 2 });
        let pending = state.pending_collision.expect("collision dialog armed");
        assert_eq!(pending.dragging, a);
        assert_eq!(pending.occupant, b);
    }

    #[test]
    fn handle_drag_drop_out_of_bounds_rejected() {
        let mut state = blank(4, 4);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        handle_drag_drop(&mut state, id.clone(), HexCoord { q: 9, r: 9 });
        let sys = state.sector.systems.iter().find(|s| s.id == id).unwrap();
        assert_eq!(sys.coord, HexCoord { q: 1, r: 1 });
        assert!(matches!(state.modal, Some(ModalKind::Message(_))));
    }

    #[test]
    fn apply_rect_select_picks_systems_in_box() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        let _outside = state
            .sector
            .add_system(HexCoord { q: 6, r: 6 }, "C")
            .unwrap();
        apply_rect_select(
            &mut state,
            HexCoord { q: 0, r: 0 },
            HexCoord { q: 3, r: 3 },
            false,
        );
        assert!(state.selected_systems.contains(&a));
        assert!(state.selected_systems.contains(&b));
        assert_eq!(state.selected_systems.len(), 2);
    }

    #[test]
    fn map_cache_refresh_populates_subsectors() {
        let mut state = blank(8, 8);
        state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        state
            .sector
            .add_system(HexCoord { q: 3, r: 3 }, "B")
            .unwrap();
        refresh_map_cache(&mut state);
        let cache = state.map_view_cache.as_ref().expect("cache populated");
        assert!(!cache.subsectors.is_empty());
        assert_eq!(
            cache.lookup.hex_system.len(),
            state.sector.systems.len(),
            "lookup table contains every system"
        );
    }

    // ── §CTX1 Phase 1 tests ────────────────────────────────────────────────

    #[test]
    fn secondary_click_on_system_opens_menu() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false)
            .expect("right-click on a system resolves");
        match target {
            SectorMenuTarget::System {
                id: hit_id,
                coord: hit_coord,
            } => {
                assert_eq!(hit_id, id);
                assert_eq!(hit_coord, HexCoord { q: 2, r: 2 });
            }
            other => panic!("expected System target, got {other:?}"),
        }
    }

    #[test]
    fn secondary_click_on_empty_hex_returns_empty_hex_target() {
        let state = blank(8, 8);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(3, 3);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false)
            .expect("right-click inside sector resolves");
        assert!(matches!(
            target,
            SectorMenuTarget::EmptyHex { coord } if coord == HexCoord { q: 3, r: 3 }
        ));
    }

    #[test]
    fn secondary_click_dismissed_during_drag() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.drag_system = Some(id);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none(),
            "drag in progress suppresses the menu"
        );
    }

    #[test]
    fn secondary_click_dismissed_during_rect_select() {
        let mut state = blank(8, 8);
        state.rect_select = Some((HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 4 }));
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none());
    }

    #[test]
    fn secondary_click_in_region_paint_needs_ctrl() {
        let mut state = blank(8, 8);
        state.map_tool = MapTool::RegionPaint;
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(1, 1);
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none(),
            "RegionPaint without Ctrl yields to paint-erase"
        );
        assert!(
            resolve_sector_context(&state, &geom, centre, 8, 8, true).is_some(),
            "Ctrl modifier opens the menu even in RegionPaint mode"
        );
    }

    #[test]
    fn multi_selection_target_when_two_selected() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(1, 1);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false).unwrap();
        match target {
            SectorMenuTarget::MultiSelection { ids } => {
                assert!(ids.contains(&a) && ids.contains(&b));
                assert_eq!(ids.len(), 2);
            }
            other => panic!("expected MultiSelection, got {other:?}"),
        }
    }

    #[test]
    fn escape_closes_menu() {
        assert!(should_dismiss_sector_context_menu(true, true, false));
        assert!(
            should_dismiss_sector_context_menu(false, false, false),
            "focus loss dismisses"
        );
        assert!(
            should_dismiss_sector_context_menu(false, true, true),
            "outside primary click dismisses"
        );
        assert!(!should_dismiss_sector_context_menu(false, true, false));
    }

    #[test]
    fn context_menu_field_default_none() {
        let state = blank(4, 4);
        assert!(state.sector_context_menu.is_none());
    }

    // ── §CTX1 Phase 2 tests — per-item action assertions ──────────────────

    fn add_region(state: &mut BuilderState, id: &str, hex: HexCoord) {
        let mut regions = (*state.sector.regions).clone();
        regions.push(sectorforge::regions::WarpRegion {
            id: id.to_string(),
            kind: sectorforge::regions::RegionConditionKind::WarpStorm,
            name: format!("Region {id}"),
            hexes: vec![hex],
            centre: hex,
        });
        state.sector.regions = std::sync::Arc::new(regions);
    }

    #[test]
    fn ctx_action_place_system_arms_pending_place() {
        let mut state = blank(8, 8);
        let closed = apply_sector_menu_action(
            &mut state,
            SectorMenuAction::PlaceSystem {
                coord: HexCoord { q: 2, r: 3 },
            },
        );
        assert!(closed);
        let pending = state.pending_place.expect("pending_place armed");
        assert_eq!(pending.coord, HexCoord { q: 2, r: 3 });
        assert!(pending.name.starts_with("Sys-"));
    }

    #[test]
    fn ctx_action_paint_region_paints_when_region_selected() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-a", HexCoord { q: 0, r: 0 });
        state.selected_region_id = Some("reg-a".into());
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::PaintRegion {
                coord: HexCoord { q: 4, r: 4 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-a")
            .unwrap();
        assert!(region.hexes.contains(&HexCoord { q: 4, r: 4 }));
    }

    #[test]
    fn ctx_action_erase_region_removes_hex() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-a", HexCoord { q: 1, r: 1 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::EraseRegion {
                coord: HexCoord { q: 1, r: 1 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-a")
            .unwrap();
        assert!(!region.hexes.contains(&HexCoord { q: 1, r: 1 }));
    }

    #[test]
    fn ctx_action_focus_system_switches_tab_and_selection() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id: id.clone() });
        assert_eq!(state.selected_system_id, Some(id));
        assert_eq!(state.active_tab, BuilderTab::System);
    }

    #[test]
    fn ctx_action_rename_arms_pending_rename_with_current_name() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RenameSystem { id: id.clone() },
        );
        let pending = state.pending_rename.expect("rename armed");
        assert_eq!(pending.id, id);
        assert_eq!(pending.text, "Alpha");
    }

    #[test]
    fn ctx_action_delete_removes_system() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::DeleteSystem { id: id.clone() },
        );
        assert!(state.sector.systems.iter().all(|s| s.id != id));
    }

    #[test]
    fn ctx_action_add_route_from_arms_tool() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::AddRouteFrom { id: id.clone() },
        );
        assert_eq!(state.map_tool, MapTool::AddRoute);
        assert_eq!(state.pending_route_start, Some(id));
    }

    #[test]
    fn ctx_action_add_world_appends_world() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let before = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .len();
        apply_sector_menu_action(&mut state, SectorMenuAction::AddWorld { id: id.clone() });
        let after = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .len();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn ctx_action_regenerate_pinned_is_noop() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        state.pinned_systems.insert(id.clone());
        let before_modal = state.modal.is_some();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RegenerateSystem {
                id,
                coord: HexCoord { q: 2, r: 2 },
            },
        );
        assert_eq!(state.modal.is_some(), before_modal);
    }

    #[test]
    fn ctx_action_toggle_pin_flips_membership() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        assert!(!state.pinned_systems.contains(&id));
        apply_sector_menu_action(&mut state, SectorMenuAction::TogglePin { id: id.clone() });
        assert!(state.pinned_systems.contains(&id));
        apply_sector_menu_action(&mut state, SectorMenuAction::TogglePin { id: id.clone() });
        assert!(!state.pinned_systems.contains(&id));
    }

    #[test]
    fn ctx_action_open_in_routes_switches_to_routes_tab() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::OpenIn {
                id: id.clone(),
                target: OpenInTarget::Routes,
            },
        );
        assert_eq!(state.active_tab, BuilderTab::Routes);
        assert_eq!(state.selected_system_id, Some(id));
    }

    #[test]
    fn ctx_action_open_in_world_selects_first_world() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "Alpha")
            .unwrap();
        let cmd = BuilderCommand::AddWorld {
            system: id.clone(),
            name: "World-1".into(),
            result_id: None,
        };
        state.run(cmd).unwrap();
        let first_world = state
            .sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .worlds
            .first()
            .unwrap()
            .id
            .clone();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::OpenIn {
                id: id.clone(),
                target: OpenInTarget::World,
            },
        );
        assert_eq!(state.active_tab, BuilderTab::World);
        assert_eq!(state.selected_system_id, Some(id));
        assert_eq!(state.selected_world_id, Some(first_world));
    }

    // ── §CTX1 Phase 3 tests — multi-selection menu ────────────────────────

    fn multi_state(width: u32, height: u32) -> (BuilderState, SystemId, SystemId) {
        let mut state = blank(width, height);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.selected_systems.insert(a.clone());
        state.selected_systems.insert(b.clone());
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::MultiSelection {
                ids: vec![a.clone(), b.clone()],
            },
            bulk_delete_confirm: false,
        });
        (state, a, b)
    }

    #[test]
    fn ctx_multi_focus_first_focuses_first_id() {
        let (mut state, a, _b) = multi_state(8, 8);
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiFocusFirst);
        assert_eq!(state.selected_system_id, Some(a));
        assert_eq!(state.active_tab, BuilderTab::System);
    }

    #[test]
    fn ctx_multi_bulk_rename_open_arms_pending_dialog() {
        let (mut state, _a, _b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiBulkRenameOpen);
        let pending = state.pending_bulk_rename.expect("dialog armed");
        assert_eq!(pending.pattern, "Sys-{n}");
    }

    #[test]
    fn ctx_multi_pin_all_pins_every_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiPinAll);
        assert!(state.pinned_systems.contains(&a));
        assert!(state.pinned_systems.contains(&b));
    }

    #[test]
    fn ctx_multi_unpin_all_clears_every_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        state.pinned_systems.insert(a.clone());
        state.pinned_systems.insert(b.clone());
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiUnpinAll);
        assert!(!state.pinned_systems.contains(&a));
        assert!(!state.pinned_systems.contains(&b));
    }

    #[test]
    fn ctx_multi_delete_all_confirmed_removes_and_clears_selection() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiDeleteAllConfirmed);
        assert!(state.sector.systems.iter().all(|s| s.id != a && s.id != b));
        assert!(state.selected_systems.is_empty());
        assert!(state.selected_system_id.is_none());
    }

    #[test]
    fn ctx_multi_delete_requires_confirm_gate() {
        let (mut state, a, b) = multi_state(8, 8);
        let confirming = state
            .sector_context_menu
            .as_ref()
            .map(|m| m.bulk_delete_confirm)
            .unwrap();
        assert!(!confirming, "fresh menu starts with confirm unarmed");
        state
            .sector_context_menu
            .as_mut()
            .unwrap()
            .bulk_delete_confirm = true;
        assert!(state.sector.systems.iter().any(|s| s.id == a));
        assert!(state.sector.systems.iter().any(|s| s.id == b));
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiDeleteAllConfirmed);
        assert!(state.sector.systems.iter().all(|s| s.id != a && s.id != b));
    }

    #[test]
    fn ctx_multi_assign_primary_faction_writes_each() {
        let (mut state, a, b) = multi_state(8, 8);
        let fid = sectorforge::ids::FactionId::from("imperium");
        state
            .sector
            .factions
            .push(sectorforge::sector_model::GeneratedFaction {
                id: fid.clone(),
                name: std::sync::Arc::from("Imperium"),
                kind: std::sync::Arc::from("Imperium"),
                disposition: std::sync::Arc::from("Order"),
                subfactions: Vec::new(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: Default::default(),
            });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::MultiAssignPrimaryFaction { fid: fid.clone() },
        );
        for id in [&a, &b] {
            let sys = state.sector.systems.iter().find(|s| s.id == **id).unwrap();
            assert!(
                sys.primary_factions.contains(&fid),
                "{id} should carry the new primary faction"
            );
        }
    }

    #[test]
    fn ctx_multi_flip_control_state_writes_each() {
        let (mut state, a, b) = multi_state(8, 8);
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::MultiFlipControlState {
                value: Some(SystemState::Warzone),
            },
        );
        for id in [&a, &b] {
            let sys = state.sector.systems.iter().find(|s| s.id == **id).unwrap();
            assert_eq!(sys.control.state, Some(SystemState::Warzone));
        }
    }

    #[test]
    fn ctx_multi_clear_selection_drops_selected_systems() {
        let (mut state, _a, _b) = multi_state(8, 8);
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiClearSelection);
        assert!(state.selected_systems.is_empty());
        assert!(state.selected_system_id.is_none());
    }

    #[test]
    fn ctx_multi_reseed_skips_when_all_pinned() {
        let (mut state, a, b) = multi_state(8, 8);
        state.pinned_systems.insert(a.clone());
        state.pinned_systems.insert(b.clone());
        let before_modal = state.modal.is_some();
        apply_sector_menu_action(&mut state, SectorMenuAction::MultiReseedWorlds);
        assert_eq!(state.modal.is_some(), before_modal);
    }

    // ── §CTX1 Phase 4 tests — partial-regen anchor ────────────────────────

    #[test]
    fn ctx_partial_regen_anchor_defaults_none() {
        let state = blank(4, 4);
        assert!(state.partial_regen_anchor.is_none());
    }

    #[test]
    fn ctx_action_start_partial_regen_arms_anchor() {
        let mut state = blank(8, 8);
        let coord = HexCoord { q: 2, r: 3 };
        let closed =
            apply_sector_menu_action(&mut state, SectorMenuAction::StartPartialRegen { coord });
        assert!(closed, "menu dismisses after arming the anchor");
        assert_eq!(state.partial_regen_anchor, Some(coord));
        assert!(state.partial_regen_rect.is_none());
    }

    #[test]
    fn ctx_partial_regen_anchor_click_completes_rect() {
        let mut state = blank(8, 8);
        state.partial_regen_anchor = Some(HexCoord { q: 1, r: 5 });
        let consumed = apply_partial_regen_anchor_click(&mut state, HexCoord { q: 4, r: 2 });
        assert!(consumed, "click consumed while anchor was armed");
        assert!(state.partial_regen_anchor.is_none(), "anchor cleared");
        let rect = state.partial_regen_rect.expect("rect populated");
        assert_eq!(rect.min_q, 1);
        assert_eq!(rect.max_q, 4);
        assert_eq!(rect.min_r, 2);
        assert_eq!(rect.max_r, 5);
    }

    #[test]
    fn ctx_partial_regen_anchor_click_noop_without_anchor() {
        let mut state = blank(8, 8);
        let consumed = apply_partial_regen_anchor_click(&mut state, HexCoord { q: 0, r: 0 });
        assert!(!consumed);
        assert!(state.partial_regen_rect.is_none());
    }

    #[test]
    fn ctx_partial_regen_anchor_not_in_session_file() {
        use crate::builder::session::SessionFile;
        let mut state = blank(4, 4);
        state.partial_regen_anchor = Some(HexCoord { q: 2, r: 2 });
        let file = SessionFile::from_state(&state, Vec::new());
        let round_tripped = file.into_state();
        assert!(round_tripped.partial_regen_anchor.is_none());
    }

    // ── §CTX1 Phase 5 tests — route + region-hex menus ───────────────────

    fn add_route(state: &mut BuilderState, a: HexCoord, b: HexCoord) -> sectorforge::ids::RouteId {
        let sa = state.sector.add_system(a, "A").unwrap();
        let sb = state.sector.add_system(b, "B").unwrap();
        state
            .sector
            .add_route(&sa, &sb, RouteType::StableWarpLane, RouteStability::Stable)
            .unwrap()
    }

    #[test]
    fn resolve_returns_route_target_when_clicking_segment() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 0 });
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let a = geom.hex_center(0, 0);
        let b = geom.hex_center(4, 0);
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let target = resolve_sector_context(&state, &geom, mid, 8, 8, false)
            .expect("midpoint resolves to route");
        assert!(matches!(
            target,
            SectorMenuTarget::Route { id: hit, .. } if hit == id
        ));
    }

    #[test]
    fn resolve_returns_region_hex_when_cache_has_region() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-z", HexCoord { q: 3, r: 4 });
        refresh_map_cache(&mut state);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(3, 4);
        let target = resolve_sector_context(&state, &geom, centre, 8, 8, false).unwrap();
        assert!(matches!(
            target,
            SectorMenuTarget::RegionHex { ref region, coord }
                if region == "reg-z" && coord == HexCoord { q: 3, r: 4 }
        ));
    }

    #[test]
    fn ctx_action_set_route_type_runs_command_and_undoes() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteType {
                id: id.clone(),
                value: RouteType::SmugglingLane,
            },
        );
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .route_type,
            RouteType::SmugglingLane
        );
        state.undo().unwrap();
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .route_type,
            RouteType::StableWarpLane
        );
    }

    #[test]
    fn ctx_action_set_route_stability_runs_command() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteStability {
                id: id.clone(),
                value: RouteStability::Perilous,
            },
        );
        assert_eq!(
            state
                .sector
                .routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .stability,
            RouteStability::Perilous
        );
    }

    #[test]
    fn ctx_action_remove_route_drops_route() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        apply_sector_menu_action(&mut state, SectorMenuAction::RemoveRoute { id: id.clone() });
        assert!(state.sector.routes.iter().all(|r| r.id != id));
    }

    #[test]
    fn ctx_action_set_route_type_noop_when_unchanged() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        let log_before = state.command_log.len();
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRouteType {
                id,
                value: RouteType::StableWarpLane,
            },
        );
        assert_eq!(
            state.command_log.len(),
            log_before,
            "same-value cycle should not push a command"
        );
    }

    #[test]
    fn ctx_action_focus_route_switches_to_routes_tab() {
        let mut state = blank(8, 8);
        let id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusRoute { id: id.clone() });
        assert_eq!(state.selected_route_id, Some(id));
        assert_eq!(state.active_tab, BuilderTab::Routes);
    }

    #[test]
    fn ctx_action_set_region_kind_runs_command() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::SetRegionKind {
                region: "reg-x".into(),
                value: RegionConditionKind::CalmCorridor,
            },
        );
        assert_eq!(
            state
                .sector
                .regions
                .iter()
                .find(|r| r.id == "reg-x")
                .unwrap()
                .kind,
            RegionConditionKind::CalmCorridor
        );
    }

    #[test]
    fn ctx_action_focus_region_switches_tab() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        state.active_tab = BuilderTab::Map;
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::FocusRegion {
                region: "reg-x".into(),
            },
        );
        assert_eq!(state.selected_region_id.as_deref(), Some("reg-x"));
        assert_eq!(state.active_tab, BuilderTab::Regions);
    }

    #[test]
    fn ctx_action_erase_region_hex_drops_hex() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 2, r: 2 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::EraseRegionHex {
                region: "reg-x".into(),
                coord: HexCoord { q: 2, r: 2 },
            },
        );
        let region = state
            .sector
            .regions
            .iter()
            .find(|r| r.id == "reg-x")
            .unwrap();
        assert!(!region.hexes.contains(&HexCoord { q: 2, r: 2 }));
    }

    #[test]
    fn ctx_action_rename_region_open_arms_dialog() {
        let mut state = blank(8, 8);
        add_region(&mut state, "reg-x", HexCoord { q: 0, r: 0 });
        apply_sector_menu_action(
            &mut state,
            SectorMenuAction::RenameRegionOpen {
                region: "reg-x".into(),
            },
        );
        let pending = state.pending_region_rename.expect("dialog armed");
        assert_eq!(pending.region, "reg-x");
        assert_eq!(pending.text, "Region reg-x");
    }

    #[test]
    fn map_cache_stable_across_idempotent_calls() {
        let mut state = blank(8, 8);
        state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        refresh_map_cache(&mut state);
        let digest = state
            .map_view_cache
            .as_ref()
            .map(|c| c.digest.clone())
            .unwrap();
        refresh_map_cache(&mut state);
        assert_eq!(
            digest,
            state.map_view_cache.as_ref().unwrap().digest,
            "digest unchanged when sector slice unchanged"
        );
    }

    // ── §CTX1 Phase 7 polish tests ────────────────────────────────────────

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::new(1000.0, 800.0))
    }

    #[test]
    fn menu_anchor_pivot_top_left_when_cursor_top_left() {
        assert_eq!(
            menu_anchor_pivot(Pos2::new(100.0, 100.0), screen()),
            egui::Align2::LEFT_TOP,
        );
    }

    #[test]
    fn menu_anchor_pivot_flips_horizontally_on_right_half() {
        assert_eq!(
            menu_anchor_pivot(Pos2::new(950.0, 100.0), screen()),
            egui::Align2::RIGHT_TOP,
        );
    }

    #[test]
    fn menu_anchor_pivot_flips_vertically_on_bottom_half() {
        assert_eq!(
            menu_anchor_pivot(Pos2::new(100.0, 780.0), screen()),
            egui::Align2::LEFT_BOTTOM,
        );
    }

    #[test]
    fn menu_anchor_pivot_clamps_to_viewport_in_corner() {
        assert_eq!(
            menu_anchor_pivot(Pos2::new(990.0, 790.0), screen()),
            egui::Align2::RIGHT_BOTTOM,
        );
    }

    #[test]
    fn ctx_menu_telemetry_records_last_action_label() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        assert!(state.last_menu_action.is_none());
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id });
        assert_eq!(
            state.last_menu_action.as_deref(),
            Some("sector :: FOCUS SYSTEM"),
        );
    }

    #[test]
    fn ctx_menu_telemetry_label_covers_every_sector_variant() {
        for (action, label) in [
            (
                SectorMenuAction::PlaceSystem {
                    coord: HexCoord { q: 0, r: 0 },
                },
                "sector :: PLACE SYSTEM",
            ),
            (
                SectorMenuAction::MultiClearSelection,
                "multi :: CLEAR SELECTION",
            ),
            (
                SectorMenuAction::FocusRoute {
                    id: sectorforge::ids::route_id(
                        &sectorforge::ids::system_id(1),
                        &sectorforge::ids::system_id(2),
                    ),
                },
                "route :: FOCUS",
            ),
            (
                SectorMenuAction::FocusRegion {
                    region: "reg-a".into(),
                },
                "region :: FOCUS",
            ),
        ] {
            assert_eq!(sector_menu_action_label(&action), label);
        }
    }

    #[test]
    fn ctx_menu_telemetry_resets_through_session_round_trip() {
        let mut state = blank(8, 8);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        apply_sector_menu_action(&mut state, SectorMenuAction::FocusSystem { id });
        assert!(state.last_menu_action.is_some());
        let file = crate::builder::session::SessionFile::from_state(&state, Vec::new());
        let restored = file.into_state();
        assert!(restored.last_menu_action.is_none());
    }

    // ── §CTX1 §10 edge-case tests ─────────────────────────────────────────

    #[test]
    fn ctx_resolve_returns_none_outside_sector_bounds() {
        let state = blank(4, 4);
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let outside = Pos2::new(-10_000.0, -10_000.0);
        assert!(resolve_sector_context(&state, &geom, outside, 4, 4, false).is_none());
    }

    #[test]
    fn ctx_resolve_suppressed_when_pending_collision() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 2, r: 2 }, "B")
            .unwrap();
        state.pending_collision = Some(PendingCollision {
            dragging: a,
            target: HexCoord { q: 2, r: 2 },
            occupant: b,
        });
        let geom = SectorGeom::new(28.0, Pos2::ZERO);
        let centre = geom.hex_center(2, 2);
        assert!(resolve_sector_context(&state, &geom, centre, 8, 8, false).is_none());
    }

    #[test]
    fn sector_menu_target_stale_when_system_removed() {
        let mut state = blank(4, 4);
        let id = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "Alpha")
            .unwrap();
        let target = SectorMenuTarget::System {
            id: id.clone(),
            coord: HexCoord { q: 1, r: 1 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.systems.retain(|s| s.id != id);
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_stale_when_route_removed() {
        let mut state = blank(8, 8);
        let route_id = add_route(&mut state, HexCoord { q: 0, r: 0 }, HexCoord { q: 4, r: 0 });
        let target = SectorMenuTarget::Route {
            id: route_id,
            near_coord: HexCoord { q: 0, r: 0 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.routes.clear();
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_stale_when_region_removed() {
        let mut state = blank(4, 4);
        add_region(&mut state, "reg-x", HexCoord { q: 0, r: 0 });
        let target = SectorMenuTarget::RegionHex {
            region: "reg-x".into(),
            coord: HexCoord { q: 0, r: 0 },
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.regions = std::sync::Arc::new(vec![]);
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn sector_menu_target_not_stale_for_empty_hex_or_subsector_border() {
        let state = blank(4, 4);
        assert!(!sector_menu_target_is_stale(
            &state,
            &SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 1, r: 1 }
            }
        ));
        assert!(!sector_menu_target_is_stale(
            &state,
            &SectorMenuTarget::SubsectorBorder {
                subsector: "sub-A".into(),
                coord: HexCoord { q: 1, r: 1 },
            }
        ));
    }

    #[test]
    fn sector_menu_target_stale_when_every_multi_id_removed() {
        let mut state = blank(4, 4);
        let a = state
            .sector
            .add_system(HexCoord { q: 0, r: 0 }, "A")
            .unwrap();
        let b = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "B")
            .unwrap();
        let target = SectorMenuTarget::MultiSelection {
            ids: vec![a.clone(), b.clone()],
        };
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.systems.retain(|s| s.id != a);
        assert!(!sector_menu_target_is_stale(&state, &target));
        state.sector.systems.clear();
        assert!(sector_menu_target_is_stale(&state, &target));
    }

    #[test]
    fn set_active_tab_drops_sector_context_menu() {
        let mut state = blank(4, 4);
        state.active_tab = BuilderTab::Map;
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        state.set_active_tab(BuilderTab::Routes);
        assert!(state.sector_context_menu.is_none());
        assert_eq!(state.active_tab, BuilderTab::Routes);
    }

    #[test]
    fn set_active_tab_keeps_menu_when_tab_unchanged() {
        let mut state = blank(4, 4);
        state.active_tab = BuilderTab::Map;
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        state.set_active_tab(BuilderTab::Map);
        assert!(state.sector_context_menu.is_some());
    }

    #[test]
    fn ctx_action_cancel_route_clears_pending_start_and_disarms_tool() {
        let mut state = blank(8, 8);
        let a = state
            .sector
            .add_system(HexCoord { q: 1, r: 1 }, "A")
            .unwrap();
        state.map_tool = MapTool::AddRoute;
        state.pending_route_start = Some(a);
        let closed = apply_sector_menu_action(&mut state, SectorMenuAction::CancelRoute);
        assert!(closed);
        assert!(state.pending_route_start.is_none());
        assert_eq!(state.map_tool, MapTool::Select);
        assert_eq!(state.last_menu_action.as_deref(), Some("route :: CANCEL"));
    }

    #[test]
    fn ctx_menu_dropped_through_session_round_trip() {
        let mut state = blank(4, 4);
        state.sector_context_menu = Some(SectorContextMenu {
            screen_pos: Pos2::ZERO,
            target: SectorMenuTarget::EmptyHex {
                coord: HexCoord { q: 0, r: 0 },
            },
            bulk_delete_confirm: false,
        });
        let file = crate::builder::session::SessionFile::from_state(&state, Vec::new());
        let restored = file.into_state();
        assert!(restored.sector_context_menu.is_none());
    }
}
