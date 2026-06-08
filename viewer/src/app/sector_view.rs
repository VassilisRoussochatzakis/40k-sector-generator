use std::sync::Arc;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel, Ui};

use sectorforge::ids::SystemId;
use sectorforge::sector_model::{GeneratedSector, GeneratedSystem};

use super::{info_panel, palette, App, PendingExport, View};
use crate::editor::state::SectorEditTool;
use crate::sector_view::{SectorClick, SectorView};

use crate::system_view::SystemSelection;

impl App {
    pub(super) fn system_by_id(&self, id: &str) -> Option<&GeneratedSystem> {
        self.sector.as_ref()?.systems.iter().find(|s| s.id == id)
    }

    pub(super) fn draw_sector_layout(&mut self, ctx: &egui::Context) {
        let preview_mode = self.editor.preview_sector.is_some();
        let sector = if let Some(preview) = &self.editor.preview_sector {
            Arc::new(preview.clone())
        } else if let Some(sector) = self.sector.clone() {
            sector
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::chrome_bg()))
                .show(ctx, |ui| {
                    ui.label(RichText::new("no sector loaded").color(palette::chrome_text_dim()));
                });
            return;
        };

        if preview_mode {
            egui::TopBottomPanel::top("preview_banner")
                .frame(
                    egui::Frame::none()
                        // Preview-mode banner background fill, not a status color (AREA_F F5).
                        .fill(egui::Color32::from_rgb(0, 80, 0))
                        .inner_margin(4.0),
                )
                .show(ctx, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("PREVIEW MODE - APPLY CHANGES TO COMMIT")
                                .strong()
                                .color(Color32::WHITE),
                        );
                    });
                });
        }

        let overview_buckets = self.sector_overview_cache.buckets_for(&sector);
        if self.info_panel_open {
            SidePanel::right("info")
                .resizable(true)
                .default_width(320.0)
                .min_width(260.0)
                .frame(
                    egui::Frame::none()
                        .fill(palette::chrome_panel())
                        .inner_margin(14.0),
                )
                .show(ctx, |ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        // TF-P-4: hand info_panel the prebuilt faction-style index so its
                        // per-route / per-faction loops skip the O(N) `faction_style_by_id`
                        // scan. `None` in preview mode — the cache reflects `self.sector`,
                        // not the (possibly edited) preview — so the panel falls back to the
                        // exact free-fn lookup against the preview sector (byte-identical).
                        let info_cache = if preview_mode {
                            None
                        } else {
                            self.sector_map_cache.as_ref()
                        };
                        if let Some(sel) = self.sector_selected_route.clone() {
                            if let Some(route) =
                                sector.routes.iter().find(|r| r.id.as_str() == sel.as_str())
                            {
                                let from = route.from_system_id.clone();
                                let to = route.to_system_id.clone();
                                info_panel::route_summary(
                                    ui,
                                    route,
                                    &sector,
                                    self.route_view_mode,
                                    info_cache,
                                );
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    if ui.button(RichText::new("OPEN FROM")).clicked() {
                                        self.sector_selected = Some(from.clone());
                                        self.sector_selected_route = None;
                                        self.sector_selected_subsector = None;
                                        self.view = View::System {
                                            system_id: from.clone(),
                                            selection: SystemSelection::None,
                                        };
                                    }
                                    if ui.button(RichText::new("OPEN TO")).clicked() {
                                        self.sector_selected = Some(to.clone());
                                        self.sector_selected_route = None;
                                        self.sector_selected_subsector = None;
                                        self.view = View::System {
                                            system_id: to.clone(),
                                            selection: SystemSelection::None,
                                        };
                                    }
                                });
                                if ui.button(RichText::new("CLEAR ROUTE")).clicked() {
                                    self.sector_selected_route = None;
                                }
                                ui.separator();
                            } else {
                                self.sector_selected_route = None;
                            }
                        }
                        if let Some(sel) = self.sector_selected.as_deref() {
                            if let Some(sys) = sector.systems.iter().find(|s| s.id == sel) {
                                info_panel::system_summary(ui, sys, &sector, info_cache);
                                ui.add_space(10.0);
                                if ui.button(RichText::new("OPEN SYSTEM →")).clicked() {
                                    self.view = View::System {
                                        system_id: sys.id.clone(),
                                        selection: SystemSelection::None,
                                    };
                                }
                                ui.separator();
                            }
                        }
                        info_panel::sector_overview_with_buckets(
                            ui,
                            &sector,
                            overview_buckets.as_slice(),
                            self.route_view_mode,
                            info_cache,
                        );
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::chrome_bg()))
            .show(ctx, |ui| self.show_sector_with(ui, &sector));

        self.draw_subsector_popup(ctx, &sector);
        self.draw_region_popup(ctx, &sector);
    }

    pub(super) fn draw_subsector_popup(&mut self, ctx: &egui::Context, sector: &GeneratedSector) {
        let Some(sub_id) = self.sector_selected_subsector.clone() else {
            return;
        };
        let Some(sub) = self
            .subsectors
            .iter()
            .find(|s| s.id.as_ref() == sub_id.as_ref())
            .cloned()
        else {
            self.sector_selected_subsector = None;
            return;
        };
        let mut open = true;
        let title = format!("SUBSECTOR {} - {}", sub.label, sub.name);
        egui::Window::new(RichText::new(&title).strong())
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(340.0)
            .default_height(520.0)
            .anchor(egui::Align2::RIGHT_TOP, [-360.0, 60.0])
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    info_panel::subsector_summary(ui, &sub, sector);
                });
            });
        if !open {
            self.sector_selected_subsector = None;
        }
    }

    pub(super) fn draw_region_popup(&mut self, ctx: &egui::Context, sector: &GeneratedSector) {
        let Some(region_id) = self.sector_selected_region.clone() else {
            return;
        };
        let Some(region) = sector.regions.iter().find(|r| r.id == region_id).cloned() else {
            self.sector_selected_region = None;
            return;
        };
        let mut open = true;
        let title = format!("REGION - {}", region.name);
        egui::Window::new(RichText::new(&title).strong())
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(340.0)
            .default_height(320.0)
            .anchor(egui::Align2::RIGHT_TOP, [-360.0, 60.0])
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new(&region.name).strong().size(18.0));
                    ui.add_space(4.0);
                    ui.label(
                        // Data-viz: region-kind hue, not a UI status color (AREA_F F5).
                        RichText::new(region.kind.label())
                            .color(egui::Color32::from_rgb(220, 160, 60)),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(region.kind.description())
                                .color(palette::chrome_text_dim()),
                        )
                        .wrap(),
                    );
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("Hexes: {}", region.hexes.len())));
                    ui.label(RichText::new(format!(
                        "Centre: ({}, {})",
                        region.centre.q, region.centre.r
                    )));
                });
            });
        if !open {
            self.sector_selected_region = None;
        }
    }

    pub(super) fn show_sector_with(&mut self, ui: &mut Ui, sector: &GeneratedSector) {
        TopBottomPanel::bottom("sector_controls")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(6.0),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(RichText::new(if self.info_panel_open {
                            "◀ PANEL"
                        } else {
                            "PANEL ▶"
                        }))
                        .clicked()
                    {
                        self.info_panel_open = !self.info_panel_open;
                    }
                    ui.separator();
                    if ui.button(RichText::new("ZOOM TO FIT")).clicked() {
                        self.zoom_to_fit();
                    }
                    ui.separator();
                    ui.label(RichText::new("HEX SIZE").color(palette::chrome_text_dim()));
                    ui.add(
                        egui::Slider::new(&mut self.sector_hex_size, 5.0..=250.0).show_value(false),
                    );
                    ui.separator();
                    ui.label(RichText::new("HEATMAP").color(palette::chrome_text_dim()));
                    crate::ui_kit::combo(
                        "sector_heatmap",
                        RichText::new(self.heatmap_mode.label()).color(palette::chrome_text()),
                    )
                    .show_ui(ui, |ui| {
                        for &m in super::HeatmapMode::ALL {
                            let sel = m == self.heatmap_mode;
                            if ui.selectable_label(sel, RichText::new(m.label())).clicked() && !sel
                            {
                                self.heatmap_mode = m;
                            }
                        }
                    });
                    ui.separator();
                    if ui
                        .selectable_label(self.map_edit_mode, RichText::new("EDIT MAP"))
                        .clicked()
                    {
                        self.map_edit_mode = !self.map_edit_mode;
                        self.editor.tool = SectorEditTool::Select;
                        self.pending_route_start = None;
                    }
                    if self.map_edit_mode {
                        if ui
                            .selectable_label(
                                self.editor.tool == SectorEditTool::AddSystem,
                                RichText::new("ADD SYSTEM"),
                            )
                            .clicked()
                        {
                            self.editor.tool = SectorEditTool::AddSystem;
                            self.pending_route_start = None;
                            self.sector_pick_export = false;
                        }
                        if ui
                            .selectable_label(
                                self.editor.tool == SectorEditTool::AddRoute,
                                RichText::new("ADD WARP ROUTE"),
                            )
                            .clicked()
                        {
                            self.editor.tool = SectorEditTool::AddRoute;
                            self.pending_route_start = None;
                            self.sector_pick_export = false;
                        }
                        if ui
                            .add_enabled(
                                self.sector_selected.is_some(),
                                egui::Button::new(RichText::new("REMOVE SYSTEM")),
                            )
                            .clicked()
                        {
                            self.remove_selected_system();
                        }
                        if ui
                            .add_enabled(
                                self.sector_selected_route.is_some(),
                                egui::Button::new(RichText::new("REMOVE WARP ROUTE")),
                            )
                            .clicked()
                        {
                            self.remove_selected_route();
                        }
                        if let Some(start) = self.pending_route_start.as_ref() {
                            ui.label(
                                RichText::new(format!("ROUTE FROM {}", start.to_uppercase()))
                                    .color(palette::warning()),
                            );
                        }
                    }
                    if self.editor.dirty {
                        ui.label(RichText::new("UNSAVED").color(palette::warning()));
                    }
                    if self.sector_pick_export {
                        ui.label(
                            RichText::new("◉ click a system hex to pick for PNG export")
                                .color(palette::warning()),
                        );
                        if ui.button(RichText::new("CANCEL PICK")).clicked() {
                            self.sector_pick_export = false;
                        }
                    }
                });
            });

        let heatmap = self.heatmap_cache.get_or_compute(sector, self.heatmap_mode);

        let (rect, response) = ui.allocate_at_least(ui.available_size(), egui::Sense::drag());

        // Handle zooming
        let mut zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta == 1.0 && response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                zoom_delta = (scroll / 400.0).exp();
            }
        }

        if zoom_delta != 1.0 {
            if let Some(mouse_pos) = response.hover_pos() {
                // Zoom relative to mouse position
                let old_zoom = self.sector_hex_size;
                self.sector_hex_size = (self.sector_hex_size * zoom_delta).clamp(5.0, 250.0);
                let actual_delta = self.sector_hex_size / old_zoom;

                // Adjust pan to keep mouse over the same map point
                let map_origin = rect.min + self.sector_pan;
                self.sector_pan = (map_origin - mouse_pos) * actual_delta + (mouse_pos - rect.min);
            } else {
                self.sector_hex_size = (self.sector_hex_size * zoom_delta).clamp(5.0, 250.0);
            }
        }

        // Handle panning
        if response.dragged() {
            self.sector_pan += response.drag_delta();
        }

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            let (_resp, click) = SectorView {
                selected_system: self.sector_selected.as_deref(),
                selected_route: self.sector_selected_route.as_deref(),
                hex_size: self.sector_hex_size,
                subsectors: Some(self.subsectors.as_slice()),
                cache: self.sector_map_cache.as_ref(),
                selected_subsector: self.sector_selected_subsector.as_deref(),
                heatmap: heatmap.as_deref(),
                empty_hex_clicks: self.map_edit_mode
                    && self.editor.tool == SectorEditTool::AddSystem,
                route_view_mode: self.route_view_mode,
                origin: rect.min + self.sector_pan,
                sense: egui::Sense::click(),
                show_hover_coord: true,
                ..SectorView::new(sector)
            }
            .show(ui);
            match click {
                Some(SectorClick::System(id)) => {
                    if self.sector_pick_export {
                        self.sector_pick_export = false;
                        self.pending_export = Some(PendingExport::SystemPng(id));
                    } else if self.map_edit_mode && self.editor.tool == SectorEditTool::AddRoute {
                        self.pick_route_endpoint(id);
                    } else if self.sector_selected.as_deref() == Some(id.as_str()) {
                        self.sector_selected_route = None;
                        self.view = View::System {
                            system_id: id,
                            selection: SystemSelection::None,
                        };
                    } else {
                        self.sector_selected = Some(id);
                        self.sector_selected_route = None;
                        self.sector_selected_subsector = None;
                        self.sector_selected_region = None;
                    }
                }
                Some(SectorClick::Route(id)) => {
                    if self.sector_pick_export {
                        // routes are not valid export targets
                    } else if self.sector_selected_route.as_deref() == Some(id.as_str()) {
                        self.sector_selected_route = None;
                    } else {
                        self.sector_selected_route = Some(id);
                        self.sector_selected = None;
                        self.sector_selected_subsector = None;
                        self.sector_selected_region = None;
                    }
                }
                Some(SectorClick::Subsector(id)) => {
                    if self.sector_pick_export {
                        // empty hexes are not valid export targets
                    } else if self.sector_selected_subsector.as_deref() == Some(id.as_str()) {
                        self.sector_selected_subsector = None;
                    } else {
                        self.sector_selected_subsector = Some(id.into());
                        self.sector_selected = None;
                        self.sector_selected_route = None;
                        self.sector_selected_region = None;
                    }
                }
                Some(SectorClick::Region(id)) => {
                    if self.sector_pick_export {
                        // empty hexes are not valid export targets
                    } else if self.sector_selected_region.as_deref() == Some(id.as_str()) {
                        self.sector_selected_region = None;
                    } else {
                        self.sector_selected_region = Some(id);
                        self.sector_selected = None;
                        self.sector_selected_route = None;
                        self.sector_selected_subsector = None;
                    }
                }
                Some(SectorClick::EmptyHex(coord))
                    if self.map_edit_mode && self.editor.tool == SectorEditTool::AddSystem =>
                {
                    self.add_system_at(coord);
                }
                Some(SectorClick::EmptyHex(_)) => {}
                None => {}
                Some(_) => {}
            }
        });
    }

    pub(super) fn add_system_at(&mut self, coord: sectorforge::sector_model::HexCoord) {
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        if sector.systems.iter().any(|s| s.coord == coord) {
            self.export_status = "hex already has a system".into();
            return;
        }
        // F11: the system insert (index/id assignment, manifest bump) lives once in
        // `GeneratedSector::add_system`. The display name still mirrors the assigned
        // index, which `add_system` derives as `max(index)+1` — the same value.
        let index = sector
            .systems
            .iter()
            .map(|sys| sys.index)
            .max()
            .unwrap_or(0)
            + 1;
        let id = match sector.add_system(coord, &format!("System {index}")) {
            Ok(id) => id,
            Err(e) => {
                self.export_status = e.to_string();
                return;
            }
        };
        self.sector_selected = Some(id.clone());
        self.sector_selected_route = None;
        self.sector_selected_subsector = None;
        self.editor.tool = SectorEditTool::Select;
        self.pending_route_start = None;
        self.mark_live_sector_dirty(format!("added system {}", id));
    }

    pub(super) fn remove_selected_system(&mut self) {
        let Some(id) = self.sector_selected.clone() else {
            self.export_status = "select a system first".into();
            return;
        };
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        // F11: the cascade (drop routes touching the system + scrub it and its
        // worlds from every faction's system_presence/world_presence + refresh
        // manifest counts) lives once in `GeneratedSector::remove_system`.
        if sector.remove_system(&id).is_err() {
            self.export_status = "selected system not found".into();
            self.sector_selected = None;
            return;
        }
        self.sector_selected = None;
        self.sector_selected_route = None;
        self.sector_selected_subsector = None;
        self.pending_route_start = None;
        self.mark_live_sector_dirty(format!("removed system {}", id));
    }

    pub(super) fn pick_route_endpoint(&mut self, id: sectorforge::ids::SystemId) {
        if let Some(from) = self.pending_route_start.clone() {
            if from == id {
                self.export_status = "choose a different destination system".into();
                return;
            }
            self.add_route_between(from, id);
            self.pending_route_start = None;
            self.editor.tool = SectorEditTool::Select;
        } else {
            self.pending_route_start = Some(id.clone());
            self.sector_selected = Some(id.clone());
            self.sector_selected_route = None;
            self.sector_selected_subsector = None;
            self.export_status = format!("route start {}", id);
        }
    }

    pub(super) fn add_route_between(
        &mut self,
        from: sectorforge::ids::SystemId,
        to: sectorforge::ids::SystemId,
    ) {
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let route_id = sectorforge::ids::route_id(&from, &to);
        if sector.routes.iter().any(|r| r.id == route_id) {
            self.export_status = format!("route {} already exists", route_id);
            return;
        }
        // F11: route construction (canonical id, endpoint validation, distance from
        // endpoint coords, manifest bump) lives once in `GeneratedSector::add_route`
        // — a default StableWarpLane/Stable lane, matching the old `empty_route`.
        let new_id = match sector.add_route(
            &from,
            &to,
            sectorforge::sector_model::RouteType::StableWarpLane,
            sectorforge::sector_model::RouteStability::Stable,
        ) {
            Ok(id) => id,
            Err(e) => {
                self.export_status = e.to_string();
                return;
            }
        };
        self.sector_selected = None;
        self.sector_selected_route = Some(new_id.clone());
        self.sector_selected_subsector = None;
        self.mark_live_sector_dirty(format!("added route {}", new_id));
    }

    pub(super) fn remove_selected_route(&mut self) {
        let Some(id) = self.sector_selected_route.clone() else {
            self.export_status = "select a route first".into();
            return;
        };
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        // F11: shared route removal (drops the route + refreshes the manifest count).
        if sector.remove_route(&id).is_err() {
            self.export_status = "selected route not found".into();
            self.sector_selected_route = None;
            return;
        }
        self.sector_selected_route = None;
        self.mark_live_sector_dirty(format!("removed route {}", id));
    }

    /// Finalize an App-side live map edit: reindex IDs on the source-of-truth
    /// `editor.sector`, follow the rename through the current selection, then mark
    /// the editor dirty. The frame bridge (app/mod.rs) re-derives the read snapshot
    /// (`self.sector`), display subsectors and caches from here — this no longer
    /// touches `self.sector` directly (F-S1).
    pub(super) fn mark_live_sector_dirty(&mut self, status: String) {
        let stable_ids = self.editor.stable_ids_on_rename;
        if let Some(sector) = self.editor.sector.as_mut() {
            let (sys_map, _world_map) = sector.reindex_ids(stable_ids);
            Self::refresh_live_manifest_counts(sector);

            // Follow the reindex through the current selection / open system view.
            if let Some(sel) = self.sector_selected.as_ref() {
                if let Some(new_id) = sys_map.get(sel.as_str()) {
                    self.sector_selected = Some(SystemId::new(new_id.clone()));
                }
            }
            if let View::System { system_id, .. } = &mut self.view {
                if let Some(new_id) = sys_map.get(system_id.as_str()) {
                    *system_id = SystemId::new(new_id.clone());
                }
            }
            // Route IDs are derived from endpoints and already rewritten in-place by
            // reindex_ids; the route selection (if any) needs no separate fixup.
        }
        self.editor.mark_dirty();
        self.export_status = status;
    }

    pub(super) fn refresh_live_manifest_counts(sector: &mut GeneratedSector) {
        sector.manifest.system_count = sector.systems.len();
        sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
        sector.manifest.route_count = sector.routes.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::ids::{route_id, system_id, world_id, FactionId};
    use sectorforge::sector_model::{
        empty_faction, empty_route, empty_sector, empty_system, empty_world, hex_distance,
        HexCoord, SystemKind,
    };

    /// Canonical viewer fixture: two `Star` systems (`sys-0001` at (0,0) with one
    /// world `sys-0001-w01`, `sys-0002` at (3,0)) and one faction present on both
    /// `sys-0001` and `sys-0001-w01`. `App::new` seeds the derived snapshot; the
    /// App-method tests then mutate `editor.sector` (the source of truth) directly
    /// and assert against it (`app.sector` stays stale until `sync_derived_sector`).
    fn app_with_two_systems() -> App {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        let sector = app.editor.sector.as_mut().unwrap();
        let mut s1 = empty_system(
            system_id(1),
            1,
            "S1".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            None,
        );
        s1.worlds.push(empty_world(1, 1, "W1".into()));
        let s2 = empty_system(
            system_id(2),
            2,
            "S2".into(),
            HexCoord { q: 3, r: 0 },
            SystemKind::Star,
            None,
        );
        sector.systems.push(s1);
        sector.systems.push(s2);
        let mut f = empty_faction(&FactionId::new("imperium"));
        f.system_presence.push(system_id(1));
        f.world_presence.push(world_id(1, 1));
        sector.factions.push(f);
        app
    }

    // ── Gap 218: remove_selected_system ─────────────────────────────────────

    /// Removing the selected system cascade-removes routes touching it, scrubs the
    /// system from every faction's `system_presence` and its worlds from
    /// `world_presence`, clears the selection, and marks the editor dirty.
    #[test]
    fn remove_selected_system_cascades_routes_and_presence() {
        let mut app = app_with_two_systems();
        // route between sys-0001 and sys-0002 (touches the system being removed)
        app.editor
            .sector
            .as_mut()
            .unwrap()
            .routes
            .push(empty_route(system_id(1), system_id(2)));
        app.sector_selected = Some(system_id(1));

        app.remove_selected_system();

        let sector = app.editor.sector.as_ref().unwrap();
        // sys-0001 gone, sys-0002 remains
        assert_eq!(sector.systems.len(), 1);
        assert!(!sector.systems.iter().any(|s| s.id == system_id(1)));
        // the only route touched sys-0001 → cascade-removed
        assert!(sector.routes.is_empty());
        // faction presence scrubbed of sys-0001 and its world
        assert!(sector.factions[0]
            .system_presence
            .iter()
            .all(|x| *x != system_id(1)));
        assert!(sector.factions[0].system_presence.is_empty());
        assert!(sector.factions[0].world_presence.is_empty());
        // selection cleared, dirty set
        assert!(app.sector_selected.is_none());
        assert!(app.editor.dirty);
    }

    /// With no system selected, `remove_selected_system` is a no-op: it sets a
    /// status and does NOT mark the editor dirty.
    #[test]
    fn remove_selected_system_without_selection_is_noop() {
        let mut app = app_with_two_systems();
        app.sector_selected = None;

        app.remove_selected_system();

        assert_eq!(app.export_status, "select a system first");
        assert!(!app.editor.dirty);
        assert_eq!(app.editor.sector.as_ref().unwrap().systems.len(), 2);
    }

    // ── Gap 219: add_route_between ──────────────────────────────────────────

    /// Adding a route selects it, recomputes its distance from endpoint coords
    /// (not the `empty_route` default of 1), and dedups on the canonical
    /// `route_id` — including the reversed endpoint pair (route_id sorts).
    #[test]
    fn add_route_between_dedups_recomputes_distance_and_selects() {
        let mut app = app_with_two_systems();

        app.add_route_between(system_id(1), system_id(2));

        {
            let sector = app.editor.sector.as_ref().unwrap();
            assert_eq!(sector.routes.len(), 1);
            let route = &sector.routes[0];
            assert_eq!(route.id, route_id(&system_id(1), &system_id(2)));
            assert_eq!(route.id.as_str(), "route-sys-0001-sys-0002");
            // distance recomputed from coords (0,0)→(3,0), provably not the default 1
            let d = hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 3, r: 0 });
            assert_eq!(route.distance, d);
            assert!(route.distance > 1);
        }
        // selects the new route, clears system selection, dirty
        assert_eq!(
            app.sector_selected_route,
            Some(route_id(&system_id(1), &system_id(2)))
        );
        assert!(app.sector_selected.is_none());
        assert!(app.editor.dirty);

        // dedup on the REVERSED pair — route_id sorts endpoints, so the id matches
        app.editor.dirty = false;
        app.add_route_between(system_id(2), system_id(1));
        assert_eq!(app.editor.sector.as_ref().unwrap().routes.len(), 1);
        assert!(app.export_status.contains("already exists"));
    }

    // ── Gap 224: mark_live_sector_dirty selection remap ─────────────────────

    /// With `stable_ids_on_rename = false`, the sequential reindex renumbers systems
    /// by vec order; `mark_live_sector_dirty` follows the rename through both the
    /// selection and the open `View::System`, and marks the editor dirty.
    #[test]
    fn mark_live_sector_dirty_sequential_remaps_selection() {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        app.editor.stable_ids_on_rename = false;
        // two systems whose current ids do NOT match their sequential position
        let a = empty_system(
            SystemId::new("sys-9990"),
            99,
            "A".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            None,
        );
        let b = empty_system(
            SystemId::new("sys-9991"),
            98,
            "B".into(),
            HexCoord { q: 1, r: 0 },
            SystemKind::Star,
            None,
        );
        app.editor.sector.as_mut().unwrap().systems.push(a);
        app.editor.sector.as_mut().unwrap().systems.push(b);
        app.sector_selected = Some(SystemId::new("sys-9990"));
        app.view = View::System {
            system_id: SystemId::new("sys-9990"),
            selection: SystemSelection::None,
        };

        app.mark_live_sector_dirty("edit".into());

        // selection followed the rename: 1st pushed system (vec idx 0) → sys-0001
        assert_eq!(app.sector_selected, Some(system_id(1)));
        match &app.view {
            View::System {
                system_id: open_id, ..
            } => assert_eq!(*open_id, system_id(1)),
            other => panic!("expected View::System, got {other:?}"),
        }
        let sector = app.editor.sector.as_ref().unwrap();
        assert_eq!(sector.systems[0].id, system_id(1));
        assert_eq!(sector.systems[1].id, system_id(2));
        assert!(app.editor.dirty);
    }

    /// In the default stable mode with systems already at valid sequential ids,
    /// `mark_live_sector_dirty` leaves the selection untouched (the reindex map is
    /// empty — no id churns) while still marking dirty.
    #[test]
    fn mark_live_sector_dirty_stable_leaves_selection_unchanged() {
        let mut app = app_with_two_systems();
        assert!(app.editor.stable_ids_on_rename); // default
        app.sector_selected = Some(system_id(1));

        app.mark_live_sector_dirty("edit".into());

        assert_eq!(app.sector_selected, Some(system_id(1)));
        let sector = app.editor.sector.as_ref().unwrap();
        assert_eq!(sector.systems[0].id, system_id(1));
        assert_eq!(sector.systems[1].id, system_id(2));
        assert!(app.editor.dirty);
    }
}
