//! §BEAUTY §6.6 — bespoke painted controls (a brass primary button + a sliding
//! toggle).
//!
//! egui's stock `Button` / `checkbox` are fine for the boring 80%; these are the
//! two controls that get screenshotted, so they are hand-painted to the painter
//! (BEAUTY.md §5.2) with eased motion (§5.3) and layered depth (§5.4). Both read
//! the accent from the active theme via [`crate::design`], so they recolor across
//! all 8 presets — amber under Grimdark, crimson under Light, blue under Slate.
//!
//! Same kit rule as [`crate::ui_kit`] / [`crate::card`]: `&mut Ui` + plain data,
//! **no `BuilderState`**, so the viewer can use them too.

use egui::{
    Align2, Color32, FontFamily, FontId, Pos2, Rect, Response, Rounding, Sense, Stroke, Ui, Vec2,
};

use crate::{design, palette};

/// A bespoke, hand-painted **primary action** button — a lit brass plate with a
/// two-tone sheen, soft accent glow on hover, a press depress, and a hairline
/// accent border, every state eased off `animate_bool_with_time`. Reserve it for
/// the *one* prominent action on a panel (Regenerate, Apply, Export, Compose…);
/// stock `ui.button` still carries the secondary actions.
///
/// Returns the click [`Response`] — use `.clicked()`. For a disabled look, gate
/// the call site (`ui.add_enabled_ui(enabled, |ui| primary_button(ui, …))`).
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let font = FontId::new(design::BODY, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
    let pad = Vec2::new(design::SPACE_LG, design::SPACE_SM);
    let size = Vec2::new(
        (galley.size().x + pad.x * 2.0).max(96.0),
        (galley.size().y + pad.y * 2.0).max(ui.spacing().interact_size.y + design::SPACE_XS),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let ctx = ui.ctx().clone();
    let id = response.id;
    let t_hover = design::ease_out_cubic(ctx.animate_bool_with_time(
        id.with("h"),
        response.hovered(),
        design::MOTION_BASE,
    ));
    let t_press = design::ease_out_cubic(ctx.animate_bool_with_time(
        id.with("p"),
        response.is_pointer_button_down_on(),
        design::MOTION_FAST,
    ));

    paint_primary(ui, rect, &font, label, t_hover, t_press);
    response
}

fn paint_primary(ui: &Ui, rect: Rect, font: &FontId, label: &str, t_hover: f32, t_press: f32) {
    let painter = ui.painter_at(rect);
    let accent = design::accent(ui);
    let r = design::RADIUS_SM;

    // Press sinks the plate ~1px and darkens it; the glow / sheen lead off hover.
    let face = rect.shrink(t_press * 1.0);

    // 1) Soft accent glow underlay, strongest on hover (no blur in 0.29 — an
    //    expanded low-alpha accent rect stands in for the halo).
    let glow_a = (60.0 * t_hover) as u8;
    if glow_a > 0 {
        painter.rect_filled(
            face.expand(2.0 + 3.0 * t_hover),
            r + 3.0,
            design::accent_glow(ui, glow_a),
        );
    }

    // 2) Painted brass face: a darker rounded base + a lighter top sheen (top
    //    corners only) reads as a vertical gradient without a square-cornered
    //    mesh poking past the radius. Press darkens both ends.
    let dark_shift = 0.16 * t_press;
    let base = design::lerp_color(accent, Color32::BLACK, 0.22 + dark_shift);
    let sheen = design::lerp_color(
        accent,
        Color32::WHITE,
        (0.22 + 0.16 * t_hover - dark_shift).clamp(0.0, 1.0),
    );
    painter.rect_filled(face, r, base);
    let sheen_rect = Rect::from_min_max(
        face.left_top(),
        Pos2::new(face.right(), face.top() + face.height() * 0.55),
    );
    painter.rect_filled(
        sheen_rect,
        Rounding {
            nw: r,
            ne: r,
            sw: 0.0,
            se: 0.0,
        },
        sheen,
    );

    // 3) Two-tone hairline depth edge — top highlight, bottom shadow.
    painter.line_segment(
        [
            Pos2::new(face.left() + r, face.top() + 0.5),
            Pos2::new(face.right() - r, face.top() + 0.5),
        ],
        Stroke::new(1.0, Color32::from_white_alpha(80)),
    );
    painter.line_segment(
        [
            Pos2::new(face.left() + r, face.bottom() - 0.5),
            Pos2::new(face.right() - r, face.bottom() - 0.5),
        ],
        Stroke::new(1.0, Color32::from_black_alpha(70)),
    );

    // 4) Hairline accent border, brighter on hover.
    painter.rect_stroke(
        face,
        r,
        Stroke::new(1.0, design::accent_bright(ui, 0.15 + 0.25 * t_hover)),
    );

    // 5) Label — dark ink on a light (amber) brass, light ink on a dark (crimson)
    //    accent, so it stays legible across presets. Perceptual luma on `accent`.
    let luma = 0.299 * accent.r() as f32 + 0.587 * accent.g() as f32 + 0.114 * accent.b() as f32;
    let ink = if luma > 140.0 {
        Color32::from_rgb(24, 18, 10)
    } else {
        Color32::from_rgb(244, 240, 236)
    };
    painter.text(
        face.center(),
        Align2::CENTER_CENTER,
        label,
        font.clone(),
        ink,
    );
}

/// A bespoke **sliding toggle** — a pill track whose knob slides between off and
/// on with an eased transition, the track filling with the theme accent when on.
/// Flips `*on` when clicked (and marks the response changed). A drop-in upgrade
/// over `ui.checkbox` / `ui.toggle_value` for prominent boolean settings.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = Vec2::new(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let ctx = ui.ctx().clone();
    let t =
        design::ease_out_cubic(ctx.animate_bool_with_time(response.id, *on, design::MOTION_BASE));

    let painter = ui.painter_at(rect);
    let radius = rect.height() / 2.0;
    let off_track = ui.visuals().widgets.inactive.bg_fill;
    let track = design::lerp_color(off_track, design::accent(ui), t);
    painter.rect_filled(rect, radius, track);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, Color32::from_black_alpha(60)),
    );

    // Knob slides left→right; a hairline ring + contact shadow lift it off the
    // track.
    let knob_r = radius - 3.0;
    let cx = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    let center = Pos2::new(cx, rect.center().y);
    painter.circle_filled(
        center + Vec2::new(0.0, 1.0),
        knob_r,
        Color32::from_black_alpha(50),
    );
    painter.circle_filled(center, knob_r, Color32::from_rgb(244, 242, 238));
    painter.circle_stroke(
        center,
        knob_r,
        Stroke::new(1.0, Color32::from_black_alpha(40)),
    );
    response
}

/// [`toggle`] followed by a clickable label, as one row (the label also flips the
/// value). Returns the toggle's [`Response`]; check `.changed()`.
pub fn toggle_with_label(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    ui.horizontal(|ui| {
        let mut resp = toggle(ui, on);
        if ui
            .add(egui::Label::new(label).sense(Sense::click()))
            .clicked()
        {
            *on = !*on;
            resp.mark_changed();
        }
        resp
    })
    .inner
}

/// A **destructive-action** button (§SPRUCE D3 / §5.2): the danger color as a
/// label + hairline border over a transparent rest fill, filling with danger on
/// hover (the ink flips to a light ash), every state eased. Reads clearly as
/// dangerous without shouting at rest the way [`primary_button`] does — reserve
/// it for delete / remove confirms. Theme-aware via [`palette::danger`].
pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let font = FontId::new(design::BODY, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
    let pad = Vec2::new(design::SPACE_MD, design::SPACE_SM);
    let size = Vec2::new(
        galley.size().x + pad.x * 2.0,
        (galley.size().y + pad.y * 2.0).max(ui.spacing().interact_size.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let t_hover = design::ease_out_cubic(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        design::MOTION_BASE,
    ));

    let danger = palette::danger();
    let r = design::RADIUS_SM;
    let painter = ui.painter_at(rect);
    let fill = Color32::from_rgba_unmultiplied(
        danger.r(),
        danger.g(),
        danger.b(),
        (200.0 * t_hover) as u8,
    );
    painter.rect_filled(rect, r, fill);
    painter.rect_stroke(rect, r, Stroke::new(1.0, danger));
    let ink = design::lerp_color(danger, Color32::from_rgb(244, 240, 236), t_hover);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, ink);
    response
}

/// A low-emphasis **ghost** button (§SPRUCE D3): no border or fill at rest — just
/// dimmed text — with a faint neutral hover wash and the ink lifting to primary.
/// For tertiary actions (an add `+`, an inline dismiss) that should recede until
/// pointed at. Theme-aware via [`palette::chrome_text_dim`] / [`palette::chrome_text`].
pub fn ghost_button(ui: &mut Ui, label: &str) -> Response {
    let font = FontId::new(design::BODY, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE);
    let pad = Vec2::new(design::SPACE_SM, design::SPACE_XS);
    let size = Vec2::new(
        galley.size().x + pad.x * 2.0,
        (galley.size().y + pad.y * 2.0).max(ui.spacing().interact_size.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let t_hover = design::ease_out_cubic(ui.ctx().animate_bool_with_time(
        response.id,
        response.hovered(),
        design::MOTION_BASE,
    ));

    let r = design::RADIUS_SM;
    let hover_bg = ui.visuals().widgets.hovered.bg_fill;
    let painter = ui.painter_at(rect);
    let fill = Color32::from_rgba_unmultiplied(
        hover_bg.r(),
        hover_bg.g(),
        hover_bg.b(),
        (160.0 * t_hover) as u8,
    );
    painter.rect_filled(rect, r, fill);
    let ink = design::lerp_color(palette::chrome_text_dim(), palette::chrome_text(), t_hover);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, ink);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_paint_headless() {
        egui::__run_test_ui(|ui| {
            let _ = primary_button(ui, "Regenerate");
            let _ = danger_button(ui, "🗑 Delete");
            let _ = ghost_button(ui, "+");
            let mut on = false;
            let _ = toggle(ui, &mut on);
            let _ = toggle_with_label(ui, &mut on, "Faction fill");
        });
    }
}
