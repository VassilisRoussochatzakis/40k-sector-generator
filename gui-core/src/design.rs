//! §DESIGN — the design-token foundation for showcase-grade GUI work.
//!
//! BEAUTY.md §4: beauty is *consistency in primitives*. Every margin, radius,
//! shadow, motion duration, and accent tint in a bespoke ("hero") component is
//! drawn from the small, disciplined set named here — never from a raw literal.
//! Mixed radii / off-grid spacing are the #1 tell of amateur UI; one family,
//! used everywhere, is what reads as premium.
//!
//! This module is the **single source of truth** for spacing / radius /
//! elevation / motion / type scale. [`crate::theme`] (global `Visuals`) and
//! [`crate::ui_kit`] (the shared widget kit) route through it; the type-scale
//! consts that used to live in `ui_kit` (`TITLE` / `SECTION` / `BODY` / `DIM`)
//! are now defined here and re-exported there, so there is one scale.
//!
//! **Scope rule (BEAUTY.md §8 / §UO8).** Tokens describe *form* — they never
//! carry semantic meaning. Faction / hazard / route / region colors live in
//! [`crate::palette`] + [`crate::visual_tokens`] and must not move here. The
//! accent helpers below *derive* hover/active/glow tints from whatever accent
//! the active [`crate::theme::Theme`] installed, so bespoke components recolor
//! correctly across all 8 presets instead of hardcoding amber.
//!
//! **egui 0.29 note.** This crate is pinned to egui 0.29, where corner radius is
//! [`egui::Rounding`] (renamed `CornerRadius` only in 0.31+) and
//! [`egui::epaint::Shadow`] carries `Vec2` / `f32` fields (they became `i8`
//! later). Color blending uses [`egui::Color32::lerp_to_gamma`] (there is no
//! `Color32::lerp` in 0.29). Re-check these on any egui bump.

use egui::epaint::Shadow;
use egui::{Color32, FontFamily, Mesh, Painter, Rect, Rounding, Ui, Vec2};

// ── spacing scale — a strict 4px base grid ───────────────────────────────────
//
// Every gap / pad / margin in bespoke work references one of these. When unsure
// which to use, step up one rung — generous, *consistent* whitespace reads as
// premium (BEAUTY.md §5.8).

/// 4px — hairline gaps, icon-to-label.
pub const SPACE_XS: f32 = 4.0;
/// 8px — the default gap between related controls.
pub const SPACE_SM: f32 = 8.0;
/// 12px — inner padding of cards / sections.
pub const SPACE_MD: f32 = 12.0;
/// 16px — between distinct groups.
pub const SPACE_LG: f32 = 16.0;
/// 24px — major section separation.
pub const SPACE_XL: f32 = 24.0;
/// 32px — page-level rhythm.
pub const SPACE_XXL: f32 = 32.0;

// ── radius scale — one family, used everywhere ───────────────────────────────

/// 4px — chips, swatches, small controls (egui widget rounding).
pub const RADIUS_SM: f32 = 4.0;
/// 8px — cards, section plates, windows.
pub const RADIUS_MD: f32 = 8.0;
/// 12px — large elevated surfaces, modals.
pub const RADIUS_LG: f32 = 12.0;

/// [`Rounding`] at [`RADIUS_SM`].
#[must_use]
pub fn rounding_sm() -> Rounding {
    Rounding::same(RADIUS_SM)
}
/// [`Rounding`] at [`RADIUS_MD`].
#[must_use]
pub fn rounding_md() -> Rounding {
    Rounding::same(RADIUS_MD)
}
/// [`Rounding`] at [`RADIUS_LG`].
#[must_use]
pub fn rounding_lg() -> Rounding {
    Rounding::same(RADIUS_LG)
}

// ── type scale — display / title / section / body / caption ──────────────────
//
// `TITLE`/`SECTION`/`BODY`/`DIM` keep their `ui_kit` names/values (re-exported
// there) so existing call sites and the §UO type scale are unchanged; `DISPLAY`
// is new, reserved for hero titles set in the display face (BEAUTY.md §5.5).

/// 28px — hero / display titles only (the registered display face).
pub const DISPLAY: f32 = 28.0;
/// 20px — panel / card titles (== egui `Heading`, == old `ui_kit::TITLE`).
pub const TITLE: f32 = 20.0;
/// 15px — section headers (== old `ui_kit::SECTION`).
pub const SECTION: f32 = 15.0;
/// 15px — body / value text (== old `ui_kit::BODY`).
pub const BODY: f32 = 15.0;
/// 14px — dimmed keys / secondary labels (== old `ui_kit::DIM`).
pub const DIM: f32 = 14.0;
/// 12px — captions / legends (== egui `Small`).
pub const CAPTION: f32 = 12.0;

// ── type *families* — the registered display face (BEAUTY.md §5.5) ────────────
//
// The body / mono faces (when bundled) are registered as the front of egui's
// `Proportional` / `Monospace` families by [`crate::fonts::install`], so ordinary
// text picks them up with no call-site change. The *display* face is a named
// family hero titles opt into via [`display_family`]; reserving it for titles
// keeps a gothic face where it reads well and out of body copy where it doesn't.

/// Family name the display face registers under (see [`crate::fonts`]).
pub const FONT_DISPLAY: &str = "display";

/// The [`FontFamily`] hero titles request. Resolves to the registered display
/// face when the `bundled-fonts` feature is on (the OFL binaries are present and
/// [`crate::fonts::install`] ran); otherwise it is the proportional default, so
/// the stock build is byte-for-byte unchanged and never references a missing
/// family.
#[must_use]
pub fn display_family() -> FontFamily {
    #[cfg(feature = "bundled-fonts")]
    {
        FontFamily::Name(FONT_DISPLAY.into())
    }
    #[cfg(not(feature = "bundled-fonts"))]
    {
        FontFamily::Proportional
    }
}

// ── elevation — soft layered shadows, never one harsh black drop ─────────────
//
// Real depth = a soft, large-blur ambient shadow (BEAUTY.md §5.4). Each preset
// takes `dark` so the alpha tracks the active theme (a heavy shadow under the
// `Light` parchment preset looks wrong). `elev_med` / `elev_high` reproduce the
// popup / window shadows `theme.rs` shipped before tokens, so routing the theme
// through them is pixel-stable; `elev_low` is the new card contact shadow.

/// Tight contact shadow for resting cards / chips.
#[must_use]
pub fn elev_low(dark: bool) -> Shadow {
    Shadow {
        offset: Vec2::new(0.0, 2.0),
        blur: 8.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 90 } else { 30 }),
    }
}

/// Lifted surface — hovered cards, popups (== the old `popup_shadow`).
#[must_use]
pub fn elev_med(dark: bool) -> Shadow {
    Shadow {
        offset: Vec2::new(0.0, 4.0),
        blur: 12.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 120 } else { 35 }),
    }
}

/// Floating surface — windows, modals (== the old `window_shadow`).
#[must_use]
pub fn elev_high(dark: bool) -> Shadow {
    Shadow {
        offset: Vec2::new(0.0, 6.0),
        blur: 20.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 140 } else { 45 }),
    }
}

// ── motion — subtle and fast beats slow and showy ────────────────────────────
//
// Drive interactive flourishes (hover lift, press, selection, entrance) off
// `ctx.animate_bool_with_time(id, on, dur)` — which returns `t` in `[0,1]` and
// auto-requests a repaint while animating — then shape `t` with one of the ease
// curves below before interpolating color / size / offset / shadow. Linear
// interpolation looks mechanical (BEAUTY.md §5.3).

/// 0.08s — press / depress, the snappiest transitions.
pub const MOTION_FAST: f32 = 0.08;
/// 0.14s — the default hover / selection transition.
pub const MOTION_BASE: f32 = 0.14;
/// 0.24s — entrance of newly shown content; the slowest we go.
pub const MOTION_SLOW: f32 = 0.24;

/// Ease-out-cubic — decelerating; the default for hover/selection grow-ins.
#[must_use]
pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Ease-in-out-cubic — symmetric; for entrances and value sweeps.
#[must_use]
pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - (u * u * u) / 2.0
    }
}

// ── accent ramp — derived from the active theme, not hardcoded ───────────────

/// The active theme's accent color. [`crate::theme::Theme::apply`] installs the
/// preset accent as `visuals.hyperlink_color`; reading it here means a bespoke
/// component's glow / hover tint recolors automatically across all 8 presets.
#[must_use]
pub fn accent(ui: &Ui) -> Color32 {
    ui.visuals().hyperlink_color
}

/// Blend two colors by `t` in `[0,1]` (gamma-correct). Wraps
/// [`Color32::lerp_to_gamma`] — the 0.29 blend helper (no `Color32::lerp`).
#[must_use]
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// Accent lightened toward white by `t` — for hover / active highlights.
#[must_use]
pub fn accent_bright(ui: &Ui, t: f32) -> Color32 {
    lerp_color(accent(ui), Color32::WHITE, t)
}

/// The accent at a low alpha, for a soft glow underlay behind a primary action
/// or the selected element (use sparingly — BEAUTY.md §5.9).
#[must_use]
pub fn accent_glow(ui: &Ui, alpha: u8) -> Color32 {
    let a = accent(ui);
    Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), alpha)
}

// ── void plate — the nav rail's "cold void of space" selection backdrop ───────
//
// A *form* token (like the accent ramp / elevation): the fixed deep-space fill +
// starlight the nav void-plate paints for its selected tab (see
// `card::selectable_void_plate`). Deliberately theme-independent — the void reads
// as the void under every preset — and carries no semantic meaning, so it stays
// here rather than in `palette`.

/// Deep indigo — the lit top edge of the nav void-plate's faux gradient.
pub const VOID_TOP: Color32 = Color32::from_rgb(22, 26, 48);
/// Near-black blue — the bottom of the nav void-plate's faux gradient.
pub const VOID_BOTTOM: Color32 = Color32::from_rgb(7, 8, 18);
/// Cool starlight — the nav void-plate's stars and hairline border (cold
/// white-blue, never the warm accent).
pub const STARLIGHT: Color32 = Color32::from_rgb(198, 214, 255);

// ── gradients — egui has no gradient-fill primitive (BEAUTY.md §5.6) ──────────

/// Paint `rect` with a vertical two-stop gradient (`top` → `bottom`) as a single
/// `Mesh` quad. egui 0.29 has no gradient primitive, so a per-vertex-colored mesh
/// is the cheap stand-in for card sheen / a painted brass button face. Corners
/// are left square — clip or overlay a hairline-rounded border on top if needed.
pub fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    // Two triangles: (LT, RT, LB) and (RT, RB, LB).
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(mesh);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_curves_are_clamped_and_anchored() {
        // Endpoints pinned; curve stays inside the unit square; out-of-range t
        // clamps rather than diverging.
        for f in [ease_out_cubic, ease_in_out_cubic] {
            assert!((f(0.0) - 0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert_eq!(f(-1.0), f(0.0));
            assert_eq!(f(2.0), f(1.0));
            for i in 0..=10 {
                let t = i as f32 / 10.0;
                let v = f(t);
                assert!((0.0..=1.0).contains(&v), "f({t}) = {v} out of [0,1]");
            }
        }
    }

    #[test]
    fn elevation_alpha_tracks_dark_mode() {
        // Dark presets carry a heavier shadow than the Light parchment preset.
        assert!(elev_high(true).color.a() > elev_high(false).color.a());
        assert!(elev_low(true).color.a() > elev_low(false).color.a());
    }

    #[test]
    fn lerp_color_clamps_t() {
        let a = Color32::BLACK;
        let b = Color32::WHITE;
        assert_eq!(lerp_color(a, b, -1.0), lerp_color(a, b, 0.0));
        assert_eq!(lerp_color(a, b, 2.0), lerp_color(a, b, 1.0));
    }
}
