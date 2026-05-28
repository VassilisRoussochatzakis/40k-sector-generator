//! ANALYTICS tab (§N1 / §N2). Phase E §A1..§A4 fills the analytics dashboard.
// TODO(docs/BUILDER_REQS.txt §A1..§A4): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Analytics");
    super::placeholder::show(ui, state, "Phase E");
}
