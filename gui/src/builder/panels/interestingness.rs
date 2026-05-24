//! INTERESTINGNESS tab (§N1 / §N2). Phase D §INT1..§INT4 fills the scorer.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Interestingness");
    super::placeholder::show(ui, state, "Phase D", "§INT1..§INT4");
}
