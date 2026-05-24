//! PROSE tab (§N1 / §N2). Phase D §PR1..§PR4 fills the prose editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Prose");
    super::placeholder::show(ui, state, "Phase D", "§PR1..§PR4");
}
