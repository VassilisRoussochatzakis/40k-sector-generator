//! EXPORT tab (§N1 / §N2). Phase E §EX1..§EX8 fills the export bundle editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Export");
    super::placeholder::show(ui, state, "Phase E", "§EX1..§EX8");
}
