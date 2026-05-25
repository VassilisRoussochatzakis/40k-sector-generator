//! BRIEFING tab (§N1 / §N2). Phase D §BR1..§BR5 fills the briefing editor.
// TODO(docs/BUILDER_REQS.txt §BR1..§BR5): implement — tracked in §41 Outstanding panels.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Briefing");
    super::placeholder::show(ui, state, "Phase D", "§BR1..§BR5");
}
