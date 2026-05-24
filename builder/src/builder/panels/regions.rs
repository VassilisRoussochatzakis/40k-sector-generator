//! REGIONS tab (§N1 / §N2). Phase C §REG1..§REG7 fills the region editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Regions");
    super::placeholder::show(ui, state, "Phase C", "§REG1..§REG7");
}
