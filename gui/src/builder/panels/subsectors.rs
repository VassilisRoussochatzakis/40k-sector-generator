//! SUBSECTORS tab (§N1 / §N2). Phase C §SUB1..§SUB5 fills subsector edit.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Subsectors");
    super::placeholder::show(ui, state, "Phase C", "§SUB1..§SUB5");
}
