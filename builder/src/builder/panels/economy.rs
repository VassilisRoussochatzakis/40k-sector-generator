//! ECONOMY tab (§N1 / §N2). Phase C §E1..§E7 fills the economy editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Economy");
    super::placeholder::show(ui, state, "Phase C", "§E1..§E7");
}
