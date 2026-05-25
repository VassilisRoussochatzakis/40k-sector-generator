//! Shared helper for stub panels (§N2). Every Phase B+ tab ships an empty
//! module under `gui/src/builder/panels/` so the §N1 router can wire it; the
//! body is filled when the matching phase lands. The helper renders a single
//! grey label naming the phase and the docs/BUILDER_REQS section that fills it in.

use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, _state: &mut BuilderState, phase: &str, section: &str) {
    ui.colored_label(
        egui::Color32::GRAY,
        format!("Placeholder — {phase} fills in this tab (see docs/BUILDER_REQS.txt {section})."),
    );
}
