//! WORLD tab (§N1 / §N2). Phase B §W1..§W7 fills the world inspector.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("World");
    super::placeholder::show(ui, state, "Phase B", "§W1..§W7");
}
