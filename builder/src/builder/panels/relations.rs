//! RELATIONS tab (§N1 / §N2). Phase C §REL1..§REL9 fills the diplomacy matrix.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Relations");
    super::placeholder::show(ui, state, "Phase C", "§REL1..§REL9");
}
