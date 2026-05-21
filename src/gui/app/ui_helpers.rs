use egui::Color32;

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(20, 20, 25);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 30, 40);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 45, 60);
    visuals.widgets.active.bg_fill = Color32::from_rgb(60, 60, 80);
    visuals.selection.bg_fill = Color32::from_rgb(60, 120, 180);
    ctx.set_visuals(visuals);
}
