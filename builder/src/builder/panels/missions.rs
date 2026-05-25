//! MISSIONS tab (§N1 / §N2). Phase D §M1..§M5 fills the mission editor.
// TODO(docs/BUILDER_REQS.txt §M1..§M5): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Missions");
    super::placeholder::show(ui, state, "Phase D", "§M1..§M5");
}
