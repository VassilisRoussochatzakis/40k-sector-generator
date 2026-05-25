use egui::{RichText, ScrollArea, SidePanel};

use crate::segmentum_view::SegmentumAction;
use crate::system_view::SystemSelection;

use super::{palette, App, View, TEXT_DIM};

impl App {
    pub(super) fn draw_segmentum_layout(&mut self, ctx: &egui::Context) {
        let Some(bundle) = self.segmentum.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::BG))
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new("no segmentum loaded")
                            .color(TEXT_DIM)
                            .monospace(),
                    );
                });
            return;
        };

        let mut action = None;
        SidePanel::right("segmentum_info")
            .resizable(true)
            .default_width(380.0)
            .min_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    let next = crate::segmentum_view::show_side_panel(
                        ui,
                        &bundle,
                        self.segmentum_active_child.as_deref(),
                        &mut self.segmentum_selected_link,
                        self.route_view_mode,
                    );
                    if action.is_none() {
                        action = next;
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(14.0))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| {
                    let next = crate::segmentum_view::show_overview(
                        ui,
                        &bundle,
                        self.segmentum_active_child.as_deref(),
                        &mut self.segmentum_selected_link,
                        self.route_view_mode,
                    );
                    if action.is_none() {
                        action = next;
                    }
                });
            });

        if let Some(action) = action {
            self.handle_segmentum_action(action);
        }
    }

    pub(super) fn set_active_segmentum_child(&mut self, child_id: &str) -> bool {
        let Some(bundle) = self.segmentum.clone() else {
            return false;
        };
        let Some(child) = bundle.child(child_id) else {
            self.export_status = format!("segmentum child '{}' not found", child_id);
            return false;
        };
        self.segmentum_active_child = Some(child_id.into());
        self.set_loaded_sector(
            child.sector.clone(),
            Some(child.sector_path.as_str().to_string()),
        );
        true
    }

    pub(super) fn handle_segmentum_action(&mut self, action: SegmentumAction) {
        match action {
            SegmentumAction::OpenChild(child_id) => {
                if self.set_active_segmentum_child(&child_id) {
                    self.view = View::Sector;
                }
            }
            SegmentumAction::OpenSystem {
                child_id,
                system_id,
            } => {
                if self.set_active_segmentum_child(&child_id) {
                    self.sector_selected = Some(system_id.clone());
                    self.sector_selected_route = None;
                    self.view = View::System {
                        system_id,
                        selection: SystemSelection::None,
                    };
                }
            }
        }
    }
}
