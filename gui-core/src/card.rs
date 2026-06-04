//! §BEAUTY — bespoke, hand-painted showcase surfaces (depth + motion).
//!
//! Where [`crate::ui_kit`] gives coherent *containers*, this module gives the
//! showcase **plate**: a custom-painted, hover-/selection-animated row that
//! reads as a physical surface rather than a flat rectangle. It is the recipe
//! the §6 "faction card" hero yields, kept deliberately reusable — `&mut Ui` +
//! plain data, **no `BuilderState`** (same rule as [`crate::ui_kit`] /
//! [`crate::nav`]) — so every roster rail, and the viewer, can adopt it.
//!
//! All depth / motion / accent values come from [`crate::design`] tokens, and
//! the accent is read from the active theme, so the plate recolors correctly
//! across all 8 presets instead of hardcoding amber. egui 0.29 specifics:
//! motion via `Context::animate_bool_with_time` (which auto-requests repaints
//! while animating), color blends via `Color32::lerp_to_gamma` (wrapped in
//! [`crate::design`]).

use egui::{Align, Color32, Layout, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::design;

/// A selectable, hover-animated plate row — the showcase faction-card recipe.
///
/// Paints a layered background (soft accent glow → hover/selection wash →
/// two-tone hairline depth edge → a growing brass selection bar → a hairline
/// accent border), every layer eased off `animate_bool_with_time`, then runs
/// `content` *inside* the plate so the caller adds ordinary widgets (a swatch,
/// labels, a delete button). Rows share a fixed height for a clean list rhythm.
///
/// Returns the plate's click [`Response`] (use `.clicked()` for selection) and
/// the value `content` returns — e.g. whether an inner delete button was
/// clicked. Check that flag *before* the row's `.clicked()`, so a delete press
/// doesn't also select the row.
pub fn selectable_plate<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    selected: bool,
    content: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let id = ui.make_persistent_id(id_salt);
    let row_h = (ui.spacing().interact_size.y + design::SPACE_XS).max(28.0);
    let full_w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(full_w, row_h), Sense::click());

    // Eased motion: hover lift + selection. Both auto-request a repaint while in
    // flight, so the row settles smoothly without us driving an animation loop.
    let ctx = ui.ctx().clone();
    let t_h = design::ease_out_cubic(ctx.animate_bool_with_time(
        id.with("hover"),
        response.hovered(),
        design::MOTION_BASE,
    ));
    let t_s = design::ease_out_cubic(ctx.animate_bool_with_time(
        id.with("sel"),
        selected,
        design::MOTION_BASE,
    ));

    paint_plate(ui, rect, t_h, t_s);

    // Content sits inside the plate, clear of the selection bar on the left.
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + design::SPACE_SM, rect.top()),
        Pos2::new(rect.right() - design::SPACE_XS, rect.bottom()),
    );
    let r = ui
        .allocate_new_ui(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), content)
                .inner
        })
        .inner;

    (response, r)
}

/// Paint the layered plate background for hover factor `t_h` and selection
/// factor `t_s`, both in `[0,1]`. Draw order matters — glow behind, then
/// washes, then the depth edge / bar / border on top; the caller's content is
/// painted after this returns.
fn paint_plate(ui: &Ui, rect: Rect, t_h: f32, t_s: f32) {
    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    let accent = design::accent(ui);
    let r = design::RADIUS_SM;

    // 1) Soft accent glow underlay when selected. egui 0.29 has no blur, so an
    //    expanded, low-alpha accent rect stands in for a halo.
    if t_s > 0.01 {
        painter.rect_filled(
            rect.expand(3.0 * t_s),
            r + 3.0,
            design::accent_glow(ui, (36.0 * t_s) as u8),
        );
    }

    // 2) Hover lift (neutral) + selection wash (accent). Translucent overlays,
    //    so the plate is invisible at rest and layers over any backdrop.
    let hover_a = (26.0 * t_h) as u8;
    if hover_a > 0 {
        let lift = if dark {
            Color32::from_white_alpha(hover_a)
        } else {
            Color32::from_black_alpha(hover_a / 2)
        };
        painter.rect_filled(rect, r, lift);
    }
    let sel_a = (26.0 * t_s) as u8;
    if sel_a > 0 {
        painter.rect_filled(rect, r, design::accent_glow(ui, sel_a));
    }

    // 3) Two-tone hairline edge — top highlight, bottom shadow — the "physical
    //    plate" tell. Strength follows whichever of hover / selection leads.
    let edge = t_h.max(t_s);
    if edge > 0.01 {
        let x0 = rect.left() + design::SPACE_XS;
        let x1 = rect.right() - design::SPACE_XS;
        painter.line_segment(
            [
                Pos2::new(x0, rect.top() + 0.5),
                Pos2::new(x1, rect.top() + 0.5),
            ],
            Stroke::new(1.0, Color32::from_white_alpha((30.0 * edge) as u8)),
        );
        painter.line_segment(
            [
                Pos2::new(x0, rect.bottom() - 0.5),
                Pos2::new(x1, rect.bottom() - 0.5),
            ],
            Stroke::new(1.0, Color32::from_black_alpha((42.0 * edge) as u8)),
        );
    }

    // 4) Brass selection bar on the left, growing from the centre.
    if t_s > 0.01 {
        let bar_h = (rect.height() * 0.6 * t_s).max(2.0);
        let bar = Rect::from_center_size(
            Pos2::new(rect.left() + 3.0, rect.center().y),
            Vec2::new(3.0, bar_h),
        );
        painter.rect_filled(bar, 1.5, accent);
    }

    // 5) Hairline accent border when selected.
    if t_s > 0.01 {
        painter.rect_stroke(
            rect,
            r,
            Stroke::new(1.0, design::accent_glow(ui, (140.0 * t_s) as u8)),
        );
    }
}
