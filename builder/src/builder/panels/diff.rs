//! DIFF tab (§N1 / §N2). Phase E §DF1..§DF5 fills the diff + tick editor.
// TODO(docs/BUILDER_REQS.txt §DF1..§DF5): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Diff");
    super::placeholder::show(ui, state, "Phase E");
}
