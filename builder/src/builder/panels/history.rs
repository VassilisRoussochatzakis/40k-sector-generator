//! HISTORY tab (§N1 / §N2). Phase C §H1..§H8 fills the chronicle editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("History");
    super::placeholder::show(ui, state, "Phase C", "§H1..§H8");
}
