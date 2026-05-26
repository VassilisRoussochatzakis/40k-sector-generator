//! §LINK4 — shared cross-tab link widget. Lives in `gui-core` because it has
//! no dependency on `BuilderState`; the caller checks `.clicked()` and
//! dispatches `state.focus_entity(target)`.
//!
//! The arrow glyph `→` is rendered as a small prefix; pass `with_arrow=false`
//! for inline links inside prose.

/// Render a cross-tab entity link. Returns the egui `Response` so the caller
/// can attach tooltips, context menus, etc.
pub fn entity_link(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    with_arrow: bool,
) -> egui::Response {
    let widget: egui::WidgetText = label.into();
    if with_arrow {
        let text = format!("→ {}", widget.text());
        ui.link(text)
    } else {
        ui.link(widget)
    }
}
