//! DIFF tab (§N1 / §N2). Phase E §DF1..§DF5 fills the diff + tick editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Diff");
    super::placeholder::show(ui, state, "Phase E", "§DF1..§DF5");
}
