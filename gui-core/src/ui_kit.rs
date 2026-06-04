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
//! Plus [`combo`] (a pre-sized dropdown) and a set of text helpers
//! ([`mono_title`], …) for tabular panels like
//! [`crate::info_panel`].
//!
//! Everything reads the active theme — `Frame::group` paints the themed
//! `faint_bg`/border for free, and the text helpers pull
//! [`crate::palette::chrome_text`] / [`chrome_text_dim`](crate::palette::chrome_text_dim).
//! **No dependency on `BuilderState`** — same rule as [`crate::nav`]; these
//! take `&mut Ui` and plain data only.
//!
//! [docs/UI_OVERHAUL.md]: the UI overhaul playbook.

use egui::{Color32, Frame, Margin, RichText, Ui, WidgetText};

use crate::{design, palette};

/// Proportional text scale (`§UO3.1`), now sourced from [`crate::design`] (the
/// §DESIGN token module) so there is a single scale across the global chrome and
/// bespoke components. Re-exported here so the existing `ui_kit::TITLE` call
/// sites (e.g. [`crate::info_panel`]) keep working unchanged.
pub use crate::design::{BODY, DIM, SECTION, TITLE};

// ── tier-2: section containers ──────────────────────────────────────────────

/// A titled, framed, bordered section box (always open). Tier-2 container.
///
/// Returns the closure's value. The frame fill + border come from the active
/// theme via `Frame::group`.
pub fn section<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let dark = ui.visuals().dark_mode;
    Frame::group(ui.style())
        .inner_margin(Margin::same(design::SPACE_MD))
        .rounding(design::rounding_md())
        .shadow(design::elev_low(dark))
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(palette::chrome_text()));
            ui.add_space(design::SPACE_XS);
            gilt_rule(ui);
            ui.add_space(design::SPACE_SM);
            add(ui)
        })
        .inner
}

/// §BEAUTY — a hairline brass "gilt" rule beneath a section title, in place of the
/// flat themed `separator()`, so a section header reads like a ruled plate. The
/// accent is the active theme's (via [`design::accent`]), so it recolors across
/// all presets. `&mut Ui` + paint only — no `BuilderState`, same kit rule.
fn gilt_rule(ui: &mut Ui) {
    let accent = design::accent(ui);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        0.0,
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 130),
    );
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
    let dark = ui.visuals().dark_mode;
    Frame::group(ui.style())
        .inner_margin(Margin::same(design::SPACE_SM))
        .rounding(design::rounding_md())
        .shadow(design::elev_low(dark))
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

// ── tier-2.5: responsive multi-column + reading-width (§COLUMNS) ─────────────

/// Like [`egui::Ui::columns`] but chooses the column count from the available
/// width: up to `want` columns while each keeps ≥ `min_col_w`, otherwise fewer,
/// down to 1. The closure receives a slice of the chosen length — it MUST
/// handle `cols.len() == 1` (everything stacked on a narrow window).
///
/// This is the core fix for the one-column-stack panels: a panel whose sections
/// are independent framed boxes flows them side-by-side on a wide window and
/// collapses cleanly to a single column on a laptop, instead of painting a tall
/// skinny ribbon down the left edge with a vast empty gutter on the right.
pub fn columns_responsive<R>(
    ui: &mut Ui,
    want: usize,
    min_col_w: f32,
    add: impl FnOnce(&mut [Ui]) -> R,
) -> R {
    let spacing = ui.spacing().item_spacing.x;
    let avail = ui.available_width();
    let fit = ((avail + spacing) / (min_col_w + spacing)).floor() as usize;
    let n = fit.clamp(1, want.max(1));
    ui.columns(n, add)
}

/// Constrain `add` to at most `max_w` and left-align it, so prose / markdown /
/// help text keep a readable line length on a wide window instead of running
/// edge-to-edge. Callers typically pass `720.0`.
pub fn reading_column<R>(ui: &mut Ui, max_w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = ui.available_width().min(max_w);
    ui.allocate_ui(egui::vec2(w, 0.0), |ui| {
        ui.set_width(w);
        add(ui)
    })
    .inner
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

// ── text helpers (tabular panels) ─────────────────────────────────

/// Title row (size [`TITLE`]), primary text color, + a little space.
pub fn mono_title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text()).size(TITLE));
    ui.add_space(2.0);
}

/// Bold section header (size [`SECTION`]), primary text color.
pub fn mono_section(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text())
            .size(SECTION)
            .strong(),
    );
}

/// Body line (size [`BODY`]), primary text color.
pub fn mono_body(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text()).size(BODY));
}

/// Dimmed line (size [`DIM`]), secondary text color.
pub fn mono_dim(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text_dim()).size(DIM));
}

/// A consistent empty-state line — dimmed + italic, theme-aware (§UO P5). Use
/// in place of a bare `ui.colored_label(Color32::GRAY, …)` so "nothing here yet"
/// messages read uniformly and follow the active preset (notably the `Light`
/// theme, where a hardcoded grey reads wrong).
pub fn placeholder(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .italics()
            .color(palette::chrome_text_dim()),
    );
}

/// A `key: value` row — dimmed key, primary value.
pub fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{k}:"))
                .color(palette::chrome_text_dim())
                .size(DIM),
        );
        ui.label(RichText::new(v).color(palette::chrome_text()).size(DIM));
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
                placeholder(ui, "No systems yet");
            });
            collapsing_section(ui, "sys_star", "Star", true, |ui| {
                combo("star_class", "G").show_ui(ui, |ui| {
                    let _ = ui.selectable_label(false, "O");
                });
            });
            mono_title(ui, "SECTOR");
            mono_section(ui, "ROUTES (3)");
            mono_body(ui, "→ macragge");
            // §COLUMNS helpers — both collapse paths exercised by the headless
            // width; `cols.len()` may be 1, so the closure must not index [1].
            columns_responsive(ui, 3, 200.0, |cols| {
                for c in cols.iter_mut() {
                    c.label("metric");
                }
            });
            reading_column(ui, 720.0, |ui| {
                ui.label("a width-capped paragraph of readable prose");
            });
        });
    }
}
