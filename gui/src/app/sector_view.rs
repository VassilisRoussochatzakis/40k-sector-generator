use std::collections::HashSet;
use std::sync::Arc;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use sectorforge::sector_model::{GeneratedSector, GeneratedSystem, SystemKind};
use sectorforge::subsectors::SubsectorConfig;

use super::{
    editor, info_panel, palette, App, PendingExport, SectorEditTool, View, TEXT, TEXT_DIM,
};
use crate::sector_view::{SectorClick, SectorView};

use crate::system_view::SystemSelection;

impl App {
    pub(super) fn system_by_id(&self, id: &str) -> Option<&GeneratedSystem> {
        self.sector.as_ref()?.systems.iter().find(|s| s.id == id)
    }

    pub(super) fn draw_sector_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("no sector loaded")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            return;
        };
        let overview_buckets = self.sector_overview_cache.buckets_for(&sector);
        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    if let Some(sel) = self.sector_selected_route.clone() {
                        if let Some(route) =
                            sector.routes.iter().find(|r| r.id.as_str() == sel.as_str())
                        {
                            let from = route.from_system_id.clone();
                            let to = route.to_system_id.clone();
                            info_panel::route_summary(ui, route, &sector, self.route_view_mode);
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("OPEN FROM").monospace()).clicked() {
                                    self.sector_selected = Some(from.clone());
                                    self.sector_selected_route = None;
                                    self.sector_selected_subsector = None;
                                    self.view = View::System {
                                        system_id: from.clone(),
                                        selection: SystemSelection::None,
                                    };
                                }
                                if ui.button(RichText::new("OPEN TO").monospace()).clicked() {
                                    self.sector_selected = Some(to.clone());
                                    self.sector_selected_route = None;
                                    self.sector_selected_subsector = None;
                                    self.view = View::System {
                                        system_id: to.clone(),
                                        selection: SystemSelection::None,
                                    };
                                }
                            });
                            if ui
                                .button(RichText::new("CLEAR ROUTE").monospace())
                                .clicked()
                            {
                                self.sector_selected_route = None;
                            }
                            ui.separator();
                        } else {
                            self.sector_selected_route = None;
                        }
                    }
                    if let Some(sel) = self.sector_selected.as_deref() {
                        if let (Some(sys), Some(sector)) =
                            (self.system_by_id(sel), self.sector.as_ref())
                        {
                            info_panel::system_summary(ui, sys, sector);
                            ui.add_space(10.0);
                            if ui
                                .button(RichText::new("OPEN SYSTEM →").monospace())
                                .clicked()
                            {
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
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| self.show_sector(ui));

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
        egui::Window::new(RichText::new(&title).monospace().strong())
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
        let Some(region) = sector
            .regions
            .iter()
            .find(|r| r.id == region_id)
            .cloned()
        else {
            self.sector_selected_region = None;
            return;
        };
        let mut open = true;
        let title = format!("REGION - {}", region.name);
        egui::Window::new(RichText::new(&title).monospace().strong())
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(340.0)
            .default_height(320.0)
            .anchor(egui::Align2::RIGHT_TOP, [-360.0, 60.0])
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new(&region.name).strong().monospace().size(18.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new(region.kind.label()).color(egui::Color32::from_rgb(220, 160, 60)).monospace());
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new(region.kind.description()).monospace().color(palette::TEXT_DIM)).wrap());
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("Hexes: {}", region.hexes.len())).monospace());
                    ui.label(RichText::new(format!("Centre: ({}, {})", region.centre.q, region.centre.r)).monospace());
                });
            });
        if !open {
            self.sector_selected_region = None;
        }
    }

    pub(super) fn show_sector(&mut self, ui: &mut egui::Ui) {
        let Some(sector) = self.sector.clone() else {
            return;
        };
        TopBottomPanel::bottom("sector_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.sector_hex_size, 20.0..=80.0).show_value(false),
                    );
                    ui.separator();
                    ui.label(RichText::new("HEATMAP").color(TEXT_DIM).monospace());
                    egui::ComboBox::from_id_salt("sector_heatmap")
                        .selected_text(
                            RichText::new(self.heatmap_mode.label())
                                .monospace()
                                .color(TEXT),
                        )
                        .show_ui(ui, |ui| {
                            for &m in super::HeatmapMode::ALL {
                                let sel = m == self.heatmap_mode;
                                if ui
                                    .selectable_label(sel, RichText::new(m.label()).monospace())
                                    .clicked()
                                    && !sel
                                {
                                    self.heatmap_mode = m;
                                }
                            }
                        });
                    ui.separator();
                    if ui
                        .selectable_label(self.map_edit_mode, RichText::new("EDIT MAP").monospace())
                        .clicked()
                    {
                        self.map_edit_mode = !self.map_edit_mode;
                        self.sector_edit_tool = SectorEditTool::Select;
                        self.pending_route_start = None;
                    }
                    if self.map_edit_mode {
                        if ui
                            .selectable_label(
                                self.sector_edit_tool == SectorEditTool::AddSystem,
                                RichText::new("ADD SYSTEM").monospace(),
                            )
                            .clicked()
                        {
                            self.sector_edit_tool = SectorEditTool::AddSystem;
                            self.pending_route_start = None;
                            self.sector_pick_export = false;
                        }
                        if ui
                            .selectable_label(
                                self.sector_edit_tool == SectorEditTool::AddRoute,
                                RichText::new("ADD WARP ROUTE").monospace(),
                            )
                            .clicked()
                        {
                            self.sector_edit_tool = SectorEditTool::AddRoute;
                            self.pending_route_start = None;
                            self.sector_pick_export = false;
                        }
                        if ui
                            .add_enabled(
                                self.sector_selected.is_some(),
                                egui::Button::new(RichText::new("REMOVE SYSTEM").monospace()),
                            )
                            .clicked()
                        {
                            self.remove_selected_system();
                        }
                        if ui
                            .add_enabled(
                                self.sector_selected_route.is_some(),
                                egui::Button::new(RichText::new("REMOVE WARP ROUTE").monospace()),
                            )
                            .clicked()
                        {
                            self.remove_selected_route();
                        }
                        if let Some(start) = self.pending_route_start.as_ref() {
                            ui.label(
                                RichText::new(format!("ROUTE FROM {}", start.to_uppercase()))
                                    .color(Color32::from_rgb(235, 200, 90))
                                    .monospace(),
                            );
                        }
                    }
                    if self.live_dirty {
                        ui.label(
                            RichText::new("UNSAVED")
                                .color(Color32::from_rgb(235, 200, 90))
                                .monospace(),
                        );
                    }
                    if self.sector_pick_export {
                        ui.label(
                            RichText::new("◉ click a system hex to pick for PNG export")
                                .color(Color32::from_rgb(235, 200, 90))
                                .monospace(),
                        );
                        if ui
                            .button(RichText::new("CANCEL PICK").monospace())
                            .clicked()
                        {
                            self.sector_pick_export = false;
                        }
                    }
                });
            });
        let heatmap = self
            .heatmap_cache
            .get_or_compute(&sector, self.heatmap_mode);
        ScrollArea::both().show(ui, |ui| {
            let (_resp, click) = SectorView {
                sector: &sector,
                selected_system: self.sector_selected.as_deref(),
                selected_route: self.sector_selected_route.as_deref(),
                hex_size: self.sector_hex_size,
                path_route_ids: None,
                path_waypoints: None,
                subsectors: Some(self.subsectors.as_slice()),
                selected_subsector: self.sector_selected_subsector.as_deref(),
                heatmap: heatmap.as_deref(),
                empty_hex_clicks: self.map_edit_mode
                    && self.sector_edit_tool == SectorEditTool::AddSystem,
                route_view_mode: self.route_view_mode,
            }
            .show(ui);
            match click {
                Some(SectorClick::System(id)) => {
                    if self.sector_pick_export {
                        self.sector_pick_export = false;
                        self.pending_export = Some(PendingExport::SystemPng(id));
                    } else if self.map_edit_mode
                        && self.sector_edit_tool == SectorEditTool::AddRoute
                    {
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
                Some(SectorClick::EmptyHex(coord)) => {
                    if self.map_edit_mode && self.sector_edit_tool == SectorEditTool::AddSystem {
                        self.add_system_at(coord);
                    }
                }
                None => {}
            }
        });
    }

    pub(super) fn add_system_at(&mut self, coord: sectorforge::sector_model::HexCoord) {
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        if sector.systems.iter().any(|s| s.coord == coord) {
            self.export_status = "hex already has a system".into();
            return;
        }
        let index = sector
            .systems
            .iter()
            .map(|sys| sys.index)
            .max()
            .unwrap_or(0)
            + 1;
        let id = sectorforge::ids::system_id(index);
        let sys = editor::state::empty_system(
            id.clone(),
            index,
            format!("System {index}"),
            coord,
            SystemKind::Star,
            None,
        );
        sector.systems.push(sys);
        self.sector_selected = Some(id.clone());
        self.sector_selected_route = None;
        self.sector_selected_subsector = None;
        self.sector_edit_tool = SectorEditTool::Select;
        self.pending_route_start = None;
        self.mark_live_sector_dirty(format!("added system {}", id));
    }

    pub(super) fn remove_selected_system(&mut self) {
        let Some(id) = self.sector_selected.clone() else {
            self.export_status = "select a system first".into();
            return;
        };
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        let world_ids: HashSet<_> = sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.worlds.iter().map(|w| w.id.clone()).collect())
            .unwrap_or_default();
        let before = sector.systems.len();
        sector.systems.retain(|s| s.id != id);
        if sector.systems.len() == before {
            self.export_status = "selected system not found".into();
            self.sector_selected = None;
            return;
        }
        sector
            .routes
            .retain(|r| r.from_system_id != id && r.to_system_id != id);
        for faction in &mut sector.factions {
            faction.system_presence.retain(|x| x != &id);
            faction.world_presence.retain(|x| !world_ids.contains(x));
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
            self.sector_edit_tool = SectorEditTool::Select;
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
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        let route_id = sectorforge::ids::route_id(&from, &to);
        if sector.routes.iter().any(|r| r.id == route_id) {
            self.export_status = format!("route {} already exists", route_id);
            return;
        }
        let from_coord = sector
            .systems
            .iter()
            .find(|s| s.id == from)
            .map(|s| s.coord);
        let to_coord = sector.systems.iter().find(|s| s.id == to).map(|s| s.coord);
        let (Some(a), Some(b)) = (from_coord, to_coord) else {
            self.export_status = "route endpoint missing".into();
            return;
        };
        let mut route = editor::state::empty_route(from, to);
        route.distance = sectorforge::sector_model::hex_distance(a, b);
        self.sector_selected = None;
        self.sector_selected_route = Some(route.id.clone());
        self.sector_selected_subsector = None;
        let status = format!("added route {}", route.id);
        sector.routes.push(route);
        self.mark_live_sector_dirty(status);
    }

    pub(super) fn remove_selected_route(&mut self) {
        let Some(id) = self.sector_selected_route.clone() else {
            self.export_status = "select a route first".into();
            return;
        };
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        let before = sector.routes.len();
        sector.routes.retain(|r| r.id != id);
        if sector.routes.len() == before {
            self.export_status = "selected route not found".into();
            self.sector_selected_route = None;
            return;
        }
        self.sector_selected_route = None;
        self.mark_live_sector_dirty(format!("removed route {}", id));
    }

    pub(super) fn mark_live_sector_dirty(&mut self, status: String) {
        self.live_dirty = true;
        self.dashboard.invalidate();
        self.heatmap_cache.invalidate();
        self.sector_overview_cache.invalidate();
        let source = self
            .sector_source_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        if let Some(sector) = self.sector.as_mut() {
            let sector = Arc::make_mut(sector);
            Self::refresh_live_manifest_counts(sector);
            self.subsectors =
                sectorforge::subsectors::build_subsectors(sector, SubsectorConfig::default())
                    .unwrap_or_default();
            if !self.editor.dirty {
                self.editor.set_sector(sector.clone(), source);
            }
        }
        self.export_status = status;
    }

    pub(super) fn refresh_live_manifest_counts(sector: &mut GeneratedSector) {
        sector.manifest.system_count = sector.systems.len();
        sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
        sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
        sector.manifest.route_count = sector.routes.len();
    }
}
