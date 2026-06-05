//! Shared helpers for the catalog **roster** panels (the list-on-the-left +
//! detail-on-the-right manual-entry editors).
//!
//! §E-S2 proportionate dedup. The review's full master-detail shell did **not**
//! hold across the five catalog panels: RELATIONS is a matrix/cell editor,
//! ECONOMY's override editors key off the *shared* `selected_world_id` /
//! `selected_system_id`, the roster panels' list rendering is panel-specific
//! (`card::selectable_plate` rails sourced from a derived report), and there is
//! no add-row scratch buffer (rows are appended blank via `cfg.manual.push`).
//! What *is* byte-identical across MISSIONS and HOOKS is folded here; PERSONAE's
//! detail (selection-only, no edit-target) and its edit-target-scroll id field
//! diverge and stay hand-written.

use std::fmt::Display;

/// Resolve which roster entry the detail card should show: the explicit
/// edit-target (set by a focus / "edit" deep-link) when present, otherwise the
/// current selection. Both are transient view-state ids.
pub(crate) fn detail_target(
    edit_target: &Option<String>,
    selected_id: &Option<String>,
) -> Option<String> {
    edit_target.clone().or_else(|| selected_id.clone())
}

/// Inline single-line editor for a roster entry's id field. Returns `true` when
/// the text changed, so the caller can fold it into its own `changed` flag.
///
/// Generic over the id newtype (`MissionId` / `HookId` / …) via the same
/// `Display` + `From<String>` the hand-written sites already relied on.
pub(crate) fn id_edit_field<I>(ui: &mut egui::Ui, id: &mut I) -> bool
where
    I: Display + From<String>,
{
    let mut buf = id.to_string();
    if ui.text_edit_singleline(&mut buf).changed() {
        *id = buf.into();
        true
    } else {
        false
    }
}
