//! SEARCH tab (§N1 / §N2). Phase E §SR1..§SR5 fills the wishes/search editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Search");
    super::placeholder::show(ui, state, "Phase E", "§SR1..§SR5");
}
