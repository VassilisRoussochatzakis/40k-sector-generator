//! PERSONAE tab (§N1 / §N2). Phase D §PER1..§PER5 fills the persona editor.
// TODO(docs/BUILDER_REQS.txt §PER1..§PER5): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Personae");
    super::placeholder::show(ui, state, "Phase D", "§PER1..§PER5");
}
