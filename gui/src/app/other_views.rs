use super::App;
use egui::{RichText, Window};
use sectorforge::examples;

impl App {
    pub(super) fn draw_example_gallery(&mut self, ctx: &egui::Context) {
        let mut open = self.show_examples;
        Window::new("BUNDLED EXAMPLES")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("Select an example project to load. These are bundled into the binary and will be extracted to a temporary directory.").monospace());
                ui.add_space(8.0);

                let examples = examples::list_bundled_examples();
                for name in examples {
                    if ui.button(RichText::new(format!("LOAD {}", name.to_uppercase())).monospace()).clicked() {
                        match examples::extract_example(&name) {
                            Ok((tmp, path)) => {
                                self.example_temp_dir = Some(tmp);
                                self.load_project_path(path);
                                self.show_examples = false;
                                self.export_status = format!("Loaded example '{}'", name);
                            }
                            Err(e) => {
                                self.export_status = format!("Failed to extract example: {}", e);
                            }
                        }
                    }
                }
            });
        self.show_examples = open;
    }
}
