//! SYSTEM tab (§N1 / §N2). Phase B §S1..§S6 fills the system inspector.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("System");
    super::placeholder::show(ui, state, "Phase B", "§S1..§S6");
}
