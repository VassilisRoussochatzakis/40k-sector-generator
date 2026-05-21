use egui::{RichText, ScrollArea, SidePanel, TopBottomPanel};

use super::{editor, palette, preset_gallery, App, TEXT_DIM};

impl App {
    pub(super) fn draw_edit_layout(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("edit_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                editor::editor_toolbar(ui, &mut self.editor);
            });

        SidePanel::right("edit_inspector")
            .resizable(true)
            .default_width(360.0)
            .min_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    use editor::state::Selection as Sel;
                    let sel = self.editor.selection.clone();
                    match self.editor.tab {
                        editor::state::Tab::Map => match sel {
                            Sel::World { .. } => editor::show_world_inspector(ui, &mut self.editor),
                            Sel::System(_) => editor::show_system_inspector(ui, &mut self.editor),
                            Sel::None => {
                                ui.label(
                                    RichText::new("click a hex to add or select a system")
                                        .color(TEXT_DIM)
                                        .monospace(),
                                );
                            }
                        },
                        editor::state::Tab::Routes => editor::show_routes(ui, &mut self.editor),
                        editor::state::Tab::Factions => editor::show_factions(ui, &mut self.editor),
                        editor::state::Tab::Settings => editor::show_settings(ui, &mut self.editor),
                    }
                });
            });

        TopBottomPanel::bottom("edit_controls")
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(6.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("HEX SIZE").color(TEXT_DIM).monospace());
                    ui.add(
                        egui::Slider::new(&mut self.editor.hex_size, 20.0..=80.0).show_value(false),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG))
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| match self.editor.tab {
                    editor::state::Tab::Map | editor::state::Tab::Routes => {
                        editor::show_map(ui, &mut self.editor)
                    }
                    _ => {
                        ui.label(
                            RichText::new("(switch to MAP tab to view the hex grid)")
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    }
                });
            });

        editor::draw_dialog(ctx, &mut self.editor);
        self.route_view_mode = self.editor.route_view_mode;
    }

    pub(super) fn draw_preset_gallery(&mut self, ctx: &egui::Context) {
        if !self.preset_gallery.open {
            return;
        }
        let mut open = true;
        egui::Window::new(
            RichText::new("NEW PROJECT FROM PRESET")
                .monospace()
                .strong(),
        )
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(620.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            preset_gallery::show(ui, &mut self.preset_gallery);
        });
        if !open {
            self.preset_gallery.open = false;
        }
    }

    pub(super) fn draw_data_layout(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("data_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::PANEL_BG)
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let can_reload = self.project_dir.is_some();
                    if ui
                        .add_enabled(
                            can_reload,
                            egui::Button::new(RichText::new("RELOAD").monospace()),
                        )
                        .on_hover_text("re-read CSVs from disk (discards unsaved edits)")
                        .clicked()
                    {
                        if let Some(dir) = self.project_dir.clone() {
                            if let Err(e) = self.data_editor.load_from_project(&dir) {
                                self.data_editor.status = format!("load failed: {e}");
                            }
                        }
                    }
                    let can_save = self.data_editor.key_path.is_some();
                    if ui
                        .add_enabled(
                            can_save,
                            egui::Button::new(RichText::new("SAVE").monospace()),
                        )
                        .clicked()
                    {
                        if let Err(e) = self.data_editor.save() {
                            self.data_editor.status = format!("save failed: {e}");
                        }
                    }
                    if let Some(dir) = &self.project_dir {
                        ui.label(
                            RichText::new(dir.display().to_string())
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                "no project loaded — pass --project <dir> when launching",
                            )
                            .color(TEXT_DIM)
                            .monospace(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(&self.data_editor.status)
                                .color(TEXT_DIM)
                                .monospace(),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::BG).inner_margin(8.0))
            .show(ctx, |ui| {
                crate::gui::data_editor::show(ui, &mut self.data_editor);
            });
    }
}
