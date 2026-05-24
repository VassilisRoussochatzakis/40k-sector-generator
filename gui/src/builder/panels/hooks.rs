//! HOOKS tab (§N1 / §N2). Phase D §HK1..§HK6 fills the hook editor.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Hooks");
    super::placeholder::show(ui, state, "Phase D", "§HK1..§HK6");
}
