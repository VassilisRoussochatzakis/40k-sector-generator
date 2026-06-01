use std::sync::Arc;

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use sectorforge::ids::SystemId;

use super::{editor, info_panel, palette, App, PendingExport, View};
use crate::editor::state::SectorEditTool;
use crate::system_view::{SystemClick, SystemLayout, SystemSelection, SystemView};

impl App {
    pub(super) fn draw_system_layout(
        &mut self,
        ctx: &egui::Context,
        system_id: &SystemId,
        selection: SystemSelection,
    ) {
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
                    let sys_opt = self.system_by_id(system_id.as_str()).cloned();
                    if let Some(sys) = sys_opt.as_ref() {
                        let sector = self.sector.as_ref().expect("sector loaded");
                        match selection {
                            SystemSelection::World(idx) => {
                                if let Some(w) = sys.worlds.iter().find(|w| w.index == idx) {
                                    info_panel::world_detail(ui, w);
                                    info_panel::world_history(ui, sector, w.id.as_str());
                                    ui.add_space(10.0);
                                }
                            }
                            SystemSelection::Star => {
                                info_panel::star_detail(ui, sys);
                                ui.add_space(10.0);
                            }
                            SystemSelection::None => {}
                            _ => {}
                        }
                        ui.separator();
                        info_panel::system_summary(ui, sys, sector);
                        ui.add_space(10.0);
                        if ui.button(RichText::new("← BACK TO SECTOR")).clicked() {
                            self.view = View::Sector;
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::chrome_bg()))
            .show(ctx, |ui| self.show_system(ui, system_id, selection));
    }

    pub(super) fn show_system(
        &mut self,
        ui: &mut egui::Ui,
        system_id: &SystemId,
        selection: SystemSelection,
    ) {
        let sys_id_owned = system_id.clone();
        TopBottomPanel::bottom("system_controls")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(6.0),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("SIZE").color(palette::chrome_text_dim()));
                    ui.add(
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0).show_value(false),
                    );
                    ui.separator();
                    ui.label(RichText::new("LAYOUT").color(palette::chrome_text_dim()));
                    let mut horiz = matches!(self.system_layout, SystemLayout::Horizontal);
                    if ui
                        .selectable_label(horiz, RichText::new("HORIZ"))
                        .on_hover_text("star left, planets arrayed right in orbit order")
                        .clicked()
                    {
                        horiz = true;
                    }
                    if ui
                        .selectable_label(!horiz, RichText::new("ORBITAL"))
                        .on_hover_text("concentric orbit rings")
                        .clicked()
                    {
                        horiz = false;
                    }
                    self.system_layout = if horiz {
                        SystemLayout::Horizontal
                    } else {
                        SystemLayout::Orbital
                    };
                    if ui
                        .button(RichText::new("EXPORT MAP PNG"))
                        .on_hover_text("export this system's map to a PNG")
                        .clicked()
                    {
                        self.pending_export = Some(PendingExport::SystemPng(sys_id_owned.clone()));
                    }
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
                        if ui.button(RichText::new("ADD PLANET")).clicked() {
                            if let Some(world_index) = self.add_planet_to_system(&sys_id_owned) {
                                self.view = View::System {
                                    system_id: sys_id_owned.clone(),
                                    selection: SystemSelection::World(world_index),
                                };
                            }
                        }
                        let selected_world = match selection {
                            SystemSelection::World(idx) => Some(idx),
                            SystemSelection::None | SystemSelection::Star => None,
                            _ => None,
                        };
                        if ui
                            .add_enabled(
                                selected_world.is_some(),
                                egui::Button::new(RichText::new("REMOVE PLANET")),
                            )
                            .clicked()
                        {
                            if let Some(idx) = selected_world {
                                self.remove_planet_from_system(&sys_id_owned, idx);
                            }
                        }
                    }
                    if self.live_dirty {
                        ui.label(RichText::new("UNSAVED").color(Color32::from_rgb(235, 200, 90)));
                    }
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let sys_clone = self.system_by_id(system_id.as_str()).cloned();
            let Some(sys) = sys_clone else {
                ui.label(RichText::new("system not found").color(Color32::RED));
                return;
            };
            let (_resp, click) = SystemView {
                system: &sys,
                selected: selection,
                side: self.system_side,
                height: self.system_side,
                layout: self.system_layout,
            }
            .show(ui);
            if let Some(c) = click {
                let new_sel = match c {
                    SystemClick::Star => SystemSelection::Star,
                    SystemClick::World(i) => SystemSelection::World(i),
                    _ => SystemSelection::None,
                };
                self.view = View::System {
                    system_id: system_id.clone(),
                    selection: new_sel,
                };
            }
        });
    }

    pub(super) fn add_planet_to_system(
        &mut self,
        system_id: &sectorforge::ids::SystemId,
    ) -> Option<usize> {
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return None;
        };
        let sector = Arc::make_mut(sector);
        let Some(sys) = sector.systems.iter_mut().find(|s| &s.id == system_id) else {
            self.export_status = "system not found".into();
            return None;
        };
        let next = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1;
        let mut world = editor::state::empty_world(sys.index, next, format!("Planet {next}"));
        if let Some(star) = &sys.star {
            world.world.star_colour = star.colour_name.clone();
            world.world.star_colour_code = star.colour_code.clone();
        } else {
            world.world.star_colour = "white".into();
            world.world.star_colour_code = "W".into();
        }
        sys.worlds.push(world);
        self.mark_live_sector_dirty(format!("added planet {}:{}", system_id, next));
        Some(next)
    }

    pub(super) fn remove_planet_from_system(
        &mut self,
        system_id: &sectorforge::ids::SystemId,
        world_index: usize,
    ) {
        let Some(sector) = self.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let sector = Arc::make_mut(sector);
        let Some(sys) = sector.systems.iter_mut().find(|s| &s.id == system_id) else {
            self.export_status = "system not found".into();
            return;
        };
        let removed_world_id = sys
            .worlds
            .iter()
            .find(|w| w.index == world_index)
            .map(|w| w.id.clone());
        let before = sys.worlds.len();
        sys.worlds.retain(|w| w.index != world_index);
        if sys.worlds.len() == before {
            self.export_status = "selected planet not found".into();
            return;
        }
        if let Some(world_id) = removed_world_id {
            for faction in &mut sector.factions {
                faction.world_presence.retain(|x| x != &world_id);
            }
        }
        self.view = View::System {
            system_id: system_id.clone(),
            selection: SystemSelection::None,
        };
        self.mark_live_sector_dirty(format!("removed planet {}:{}", system_id, world_index));
    }
}
