//! ROUTES tab (§N1 / §N2). Phase B §R1..§R7 fills the route editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Routes");
    super::placeholder::show(ui, state, "Phase B", "§R1..§R7");
}
