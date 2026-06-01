//! §UO — shared chrome widgets for the builder and viewer.
//!
//! Two tiers the panels were missing (see [docs/UI_OVERHAUL.md] `§UO3.4`):
//!
//! - **Section containers** — [`section`] / [`collapsing_section`]: a titled,
//!   framed, bordered box that groups related controls. Replaces the bare
//!   `CollapsingHeader` + `ui.separator()` pattern that left panels reading as
//!   one flat wall of widgets.
//! - **Field rows** — [`field`]: an aligned label-left / control-right row.
//!
//! Plus [`combo`] (a pre-sized dropdown) and a set of monospace text helpers
//! ([`mono`], [`mono_title`], …) for tabular panels like
//! [`crate::info_panel`].
//!
//! Everything reads the active theme — `Frame::group` paints the themed
//! `faint_bg`/border for free, and the text helpers pull
//! [`crate::palette::chrome_text`] / [`chrome_text_dim`](crate::palette::chrome_text_dim).
//! **No dependency on `BuilderState`** — same rule as [`crate::nav`]; these
//! take `&mut Ui` and plain data only.
//!
//! [docs/UI_OVERHAUL.md]: the UI overhaul playbook.

use egui::{FontId, Frame, Margin, RichText, Rounding, Ui, WidgetText};

use crate::palette;

/// Monospace text scale, aligned with the theme type scale (`§UO3.1`). Tabular
/// panels use these explicit sizes so columns line up; everything else should
/// just use plain `ui.label(..)` and inherit the theme's proportional `Body`.
pub const TITLE: f32 = 20.0;
/// Section-header monospace size.
pub const SECTION: f32 = 15.0;
/// Body / value monospace size.
pub const BODY: f32 = 15.0;
/// Dimmed / key monospace size.
pub const DIM: f32 = 14.0;

/// A monospace [`FontId`] at `size`.
#[must_use]
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

// ── tier-2: section containers ──────────────────────────────────────────────

/// A titled, framed, bordered section box (always open). Tier-2 container.
///
/// Returns the closure's value. The frame fill + border come from the active
/// theme via `Frame::group`.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::group(ui.style())
        .inner_margin(Margin::same(10.0))
        .rounding(Rounding::same(6.0))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(palette::chrome_text()));
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

/// Same framed box as [`section`], but the body collapses. Drop-in replacement
/// for the bare `CollapsingHeader` pattern. `id_source` disambiguates headers
/// that share a title within a panel. Returns the body's value when open.
pub fn collapsing_section<R>(
    ui: &mut Ui,
    id_source: impl std::hash::Hash,
    title: &str,
    default_open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    Frame::group(ui.style())
        .inner_margin(Margin::same(8.0))
        .rounding(Rounding::same(6.0))
        .show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(title).strong())
                .id_salt(id_source)
                .default_open(default_open)
                .show(ui, add)
                .body_returned
        })
        .inner
}

// ── tier-3: field row ───────────────────────────────────────────────────────

/// An aligned label-left / control-right row. The label occupies a fixed
/// column so stacked fields line up; `add` paints the control(s).
pub fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let h = ui.spacing().interact_size.y;
        ui.add_sized(
            [150.0, h],
            egui::Label::new(RichText::new(label).color(palette::chrome_text_dim())),
        );
        add(ui);
    });
}

// ── dropdowns ───────────────────────────────────────────────────────────────

/// A pre-sized [`egui::ComboBox`]. Caller chains `.show_ui(ui, |ui| { … })`.
/// Width and height also come from theme spacing (`§UO3.2`); this pins a floor
/// and a consistent id-salt entry point.
pub fn combo(id_source: impl std::hash::Hash, selected: impl Into<WidgetText>) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(id_source)
        .selected_text(selected)
        .width(190.0)
}

// ── monospace text helpers (tabular panels) ─────────────────────────────────

/// Monospace title row (size [`TITLE`]), primary text color, + a little space.
pub fn mono_title(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text())
            .font(mono(TITLE)),
    );
    ui.add_space(2.0);
}

/// Bold monospace section header (size [`SECTION`]), primary text color.
pub fn mono_section(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text())
            .font(mono(SECTION))
            .strong(),
    );
}

/// Monospace body line (size [`BODY`]), primary text color.
pub fn mono_body(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text())
            .font(mono(BODY)),
    );
}

/// Dimmed monospace line (size [`DIM`]), secondary text color.
pub fn mono_dim(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text_dim())
            .font(mono(DIM)),
    );
}

/// A `key: value` row — dimmed monospace key, primary monospace value.
pub fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{k}:"))
                .color(palette::chrome_text_dim())
                .font(mono(DIM)),
        );
        ui.label(
            RichText::new(v)
                .color(palette::chrome_text())
                .font(mono(DIM)),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Headless smoke test: every widget builds and paints without panicking.
    #[test]
    fn widgets_paint_headless() {
        egui::__run_test_ui(|ui| {
            section(ui, "Identity", |ui| {
                field(ui, "Name", |ui| {
                    ui.label("Cadia");
                });
                kv(ui, "id", "cadia-01");
                mono_dim(ui, "subsector A");
            });
            collapsing_section(ui, "sys_star", "Star", true, |ui| {
                combo("star_class", "G").show_ui(ui, |ui| {
                    let _ = ui.selectable_label(false, "O");
                });
            });
            mono_title(ui, "SECTOR");
            mono_section(ui, "ROUTES (3)");
            mono_body(ui, "→ macragge");
        });
    }
}
