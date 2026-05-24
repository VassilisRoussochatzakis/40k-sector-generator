//! CONTROL tab (§N1 / §N2). Phase B §C1..§C8 + §CL1..§CL4 fill presence,
//! dominance, control state, and claims editors.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Control");
    super::placeholder::show(ui, state, "Phase B", "§C1..§C8 + §CL1..§CL4");
}
