//! SEGMENTUM tab (§N1 / §N2). Phase E §SG1..§SG5 fills the compose editor.
// TODO(docs/BUILDER_REQS.txt §SG1..§SG5): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Segmentum");
    super::placeholder::show(ui, state, "Phase E", "§SG1..§SG5");
}
