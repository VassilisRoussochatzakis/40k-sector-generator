//! Top-level eframe app: holds loaded sector + navigation state, dispatches
//! between sector view and system view.

use egui::{Color32, RichText, ScrollArea, SidePanel, TopBottomPanel};

use crate::sector_model::{GeneratedSector, GeneratedSystem};

use super::info_panel;
use super::palette::{self, TEXT, TEXT_DIM};
use super::sector_view::SectorView;
use super::system_view::{SystemClick, SystemSelection, SystemView};

pub struct App {
    sector: GeneratedSector,
    view: View,
    sector_selected: Option<String>,
    sector_hex_size: f32,
    system_side: f32,
}

#[derive(Debug, Clone)]
enum View {
    Sector,
    System {
        system_id: String,
        selection: SystemSelection,
    },
}

impl App {
    pub fn new(sector: GeneratedSector) -> Self {
        Self {
            sector,
            view: View::Sector,
            sector_selected: None,
            sector_hex_size: 44.0,
            system_side: 700.0,
        }
    }

    fn system_by_id(&self, id: &str) -> Option<&GeneratedSystem> {
        self.sector.systems.iter().find(|s| s.id == id)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx);

        TopBottomPanel::top("nav")
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let on_sector = matches!(self.view, View::Sector);
                    if ui
                        .selectable_label(on_sector, RichText::new("SECTOR MAP").color(TEXT).monospace())
                        .clicked()
                    {
                        self.view = View::Sector;
                    }
                    if let View::System { system_id, .. } = &self.view {
                        ui.label(RichText::new("›").color(TEXT_DIM).monospace());
                        ui.label(
                            RichText::new(format!("SYSTEM {}", system_id.to_uppercase()))
                                .color(TEXT)
                                .monospace(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} - {} sys, {} worlds",
                                self.sector.id.to_uppercase(),
                                self.sector.systems.len(),
                                self.sector.manifest.world_count
                            ))
                            .color(TEXT_DIM)
                            .monospace(),
                        );
                    });
                });
            });

        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(egui::Frame::none().fill(palette::PANEL_BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    match &self.view {
                        View::Sector => {
                            if let Some(sel) = self.sector_selected.as_deref() {
                                if let Some(sys) = self.system_by_id(sel) {
                                    info_panel::system_summary(ui, sys);
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
                            info_panel::sector_overview(ui, &self.sector);
                        }
                        View::System { system_id, selection } => {
                            let sys_opt = self.system_by_id(system_id).cloned();
                            if let Some(sys) = sys_opt.as_ref() {
                                match *selection {
                                    SystemSelection::World(idx) => {
                                        if let Some(w) =
                                            sys.worlds.iter().find(|w| w.index == idx)
                                        {
                                            info_panel::world_detail(ui, w);
                                            ui.add_space(10.0);
                                        }
                                    }
                                    SystemSelection::Star => {
                                        info_panel::star_detail(ui, sys);
                                        ui.add_space(10.0);
                                    }
                                    SystemSelection::None => {}
                                }
                                ui.separator();
                                info_panel::system_summary(ui, sys);
                                ui.add_space(10.0);
                                if ui
                                    .button(RichText::new("← BACK TO SECTOR").monospace())
                                    .clicked()
                                {
                                    self.view = View::Sector;
                                }
                            }
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                match self.view.clone() {
                    View::Sector => self.show_sector(ui),
                    View::System { system_id, selection } => {
                        self.show_system(ui, &system_id, selection)
                    }
                }
            });
    }
}

impl App {
    fn show_sector(&mut self, ui: &mut egui::Ui) {
        TopBottomPanel::bottom("sector_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.sector_hex_size, 20.0..=80.0)
                            .show_value(false),
                    );
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let (_resp, click) = SectorView {
                sector: &self.sector,
                selected_system: self.sector_selected.as_deref(),
                hex_size: self.sector_hex_size,
            }
            .show(ui);
            if let Some(c) = click {
                if self.sector_selected.as_deref() == Some(c.system_id.as_str()) {
                    // Second click on already-selected system → drill in.
                    self.view = View::System {
                        system_id: c.system_id,
                        selection: SystemSelection::None,
                    };
                } else {
                    self.sector_selected = Some(c.system_id);
                }
            }
        });
    }

    fn show_system(&mut self, ui: &mut egui::Ui, system_id: &str, selection: SystemSelection) {
        TopBottomPanel::bottom("system_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0)
                            .show_value(false),
                    );
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let sys_clone = self.system_by_id(system_id).cloned();
            let Some(sys) = sys_clone else {
                ui.label(RichText::new("system not found").color(Color32::RED));
                return;
            };
            let (_resp, click) = SystemView {
                system: &sys,
                selected: selection,
                side: self.system_side,
            }
            .show(ui);
            if let Some(c) = click {
                let new_sel = match c {
                    SystemClick::Star => SystemSelection::Star,
                    SystemClick::World(i) => SystemSelection::World(i),
                };
                self.view = View::System {
                    system_id: system_id.to_string(),
                    selection: new_sel,
                };
            }
        });
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = palette::PANEL_BG;
    visuals.window_fill = palette::PANEL_BG;
    visuals.extreme_bg_color = palette::BG;
    visuals.widgets.noninteractive.bg_fill = palette::PANEL_BG;
    visuals.widgets.inactive.bg_fill = palette::HEX_EMPTY;
    visuals.widgets.hovered.bg_fill = palette::HEX_OUTLINE;
    visuals.widgets.active.bg_fill = palette::SELECTION;
    visuals.override_text_color = Some(TEXT);
    ctx.set_visuals(visuals);
}
