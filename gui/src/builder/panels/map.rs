//! MAP tab (§N3). Hosts the hex map plus the editor toolbox.
//!
//! Phase A ships the toolbox + state plumbing only — the actual map render is
//! the existing [`crate::sector_view`] surface and will be migrated to read
//! [`BuilderState`] in Phase B (§S1..§S6). The toolbox arms a
//! [`crate::builder::state::MapTool`] on [`BuilderState::map_tool`] so the
//! Phase B click handlers can branch on the active tool.

use crate::builder::state::MapTool;
use crate::builder::BuilderState;

pub fn show(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.heading("Map");
    ui.add_space(4.0);
    show_toolbox(ui, state);
    ui.separator();
    ui.colored_label(
        egui::Color32::GRAY,
        "Hex map render is wired in Phase B (§S1..§S6).",
    );
    ui.label(format!(
        "active tool: {}    armed via the toolbox above",
        state.map_tool.label()
    ));
    ui.label(format!(
        "sector: {} system(s), {} route(s), {} region(s)",
        state.sector.systems.len(),
        state.sector.routes.len(),
        state.sector.regions.len(),
    ));
}

/// §N3 toolbox: ADD / DELETE / MOVE / ROUTE / REGION-PAINT.
pub fn show_toolbox(ui: &mut egui::Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        ui.label("tool:");
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            let selected = state.map_tool == tool;
            if ui.selectable_label(selected, tool.label()).clicked() {
                state.map_tool = tool;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_tool_labels_are_non_empty() {
        for tool in [
            MapTool::Select,
            MapTool::AddSystem,
            MapTool::DeleteSystem,
            MapTool::MoveSystem,
            MapTool::AddRoute,
            MapTool::RegionPaint,
        ] {
            assert!(!tool.label().is_empty());
        }
    }
}
