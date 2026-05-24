//! FACTIONS tab (§N1 / §N2). Phase B §F1..§F7 fills the faction editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Factions");
    super::placeholder::show(ui, state, "Phase B", "§F1..§F7");
}
