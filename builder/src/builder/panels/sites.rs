//! SITES tab (§N1 / §N2). Phase D §ST1..§ST4 fills the site editor.
// TODO(docs/BUILDER_REQS.txt §ST1..§ST4): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Sites");
    super::placeholder::show(ui, state, "Phase D", "§ST1..§ST4");
}
