//! Shared GUI chrome theming for the builder and viewer.
//!
//! A small set of dark presets plus the egui [`Style`]/[`Visuals`] wiring that
//! gives the apps their look. Both apps store a [`Theme`], call
//! [`Theme::apply`] when it changes, and render the runtime picker with
//! [`menu`]. This replaces the two duplicated `apply_theme` helpers that used
//! to live in `builder/src/app.rs` and `viewer/src/app/ui_helpers.rs`.
//!
//! Scope: a theme restyles the *chrome* — panels, windows, buttons, combos,
//! text, selection highlight, rounding/shadow/spacing. It deliberately does
//! **not** recolor the semantic map render (faction / hazard / route colors in
//! [`crate::palette`] and the custom painters); those carry meaning and must
//! stay stable across themes. The presets keep `panel_fill` close to the
//! violet base in [`crate::palette`] so themed standard panels remain coherent
//! with the viewer's custom-framed surfaces.

use egui::{
    Color32, Context, FontFamily, FontId, Margin, Rounding, Stroke, Style, TextStyle, Ui, Visuals,
};

use crate::{design, palette};

/// A selectable GUI chrome preset.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Theme {
    /// Imperial amber/gold on near-black warm violet (the default look).
    #[default]
    Grimdark,
    /// Cool blue-grey — preserves the original prototype builder palette.
    Void,
    /// Teal/cyan on deep blue-black.
    Abyssal,
    /// Neutral slate-grey with a blue accent — an IDE-style professional dark.
    Slate,
    /// Muted arctic blue-greys with a frost-cyan accent (the Nord palette).
    Nord,
    /// Schoonover's low-contrast teal base with a blue accent (Solarized Dark).
    Solarized,
    /// Near-monochrome grey chrome with a lone teal accent.
    Graphite,
    /// Parchment + crimson ink — the only light preset.
    Light,
}

impl Theme {
    /// Every preset, in picker order.
    pub const ALL: [Theme; 8] = [
        Theme::Grimdark,
        Theme::Void,
        Theme::Abyssal,
        Theme::Slate,
        Theme::Nord,
        Theme::Solarized,
        Theme::Graphite,
        Theme::Light,
    ];

    /// Human-readable name for menus and tooltips.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Theme::Grimdark => "Grimdark",
            Theme::Void => "Void",
            Theme::Abyssal => "Abyssal",
            Theme::Slate => "Slate",
            Theme::Nord => "Nord",
            Theme::Solarized => "Solarized",
            Theme::Graphite => "Graphite",
            Theme::Light => "Light",
        }
    }

    /// Push this theme's [`Style`] onto the egui context. Cheap, but allocates
    /// an `Arc<Style>`; callers apply only when the selection changes rather
    /// than every frame.
    pub fn apply(self, ctx: &Context) {
        let p = self.palette();
        // Push the chrome colors the custom-painted UI reads (panels + text).
        palette::set_chrome(palette::ChromeColors {
            bg: p.window,
            panel: p.panel,
            text: p.text,
            text_dim: p.text_weak,
        });
        let mut style = Style {
            visuals: build_visuals(&p),
            ..Style::default()
        };
        tune_spacing(&mut style);
        tune_typography(&mut style);
        ctx.set_style(style);
    }

    fn palette(self) -> Pal {
        match self {
            Theme::Grimdark => Pal {
                dark: true,
                window: rgb(16, 14, 18),
                panel: rgb(24, 20, 28),
                faint: rgb(30, 26, 34),
                extreme: rgb(12, 10, 14),
                weak: rgb(40, 34, 44),
                hover: rgb(58, 48, 42),
                active: rgb(80, 62, 44),
                accent: rgb(214, 158, 74),
                sel: rgb(92, 68, 30),
                text: rgb(232, 228, 240),
                text_weak: rgb(150, 145, 165),
                border: rgb(58, 50, 46),
            },
            Theme::Void => Pal {
                dark: true,
                window: rgb(20, 20, 25),
                panel: rgb(26, 26, 32),
                faint: rgb(32, 32, 40),
                extreme: rgb(16, 16, 20),
                weak: rgb(30, 30, 40),
                hover: rgb(45, 45, 60),
                active: rgb(60, 60, 80),
                accent: rgb(86, 150, 214),
                sel: rgb(60, 120, 180),
                text: rgb(228, 230, 238),
                text_weak: rgb(146, 150, 165),
                border: rgb(52, 54, 66),
            },
            Theme::Abyssal => Pal {
                dark: true,
                window: rgb(12, 16, 20),
                panel: rgb(18, 24, 30),
                faint: rgb(24, 32, 38),
                extreme: rgb(9, 12, 16),
                weak: rgb(28, 38, 46),
                hover: rgb(40, 56, 64),
                active: rgb(52, 76, 86),
                accent: rgb(86, 204, 196),
                sel: rgb(30, 78, 80),
                text: rgb(224, 234, 236),
                text_weak: rgb(140, 160, 164),
                border: rgb(44, 60, 66),
            },
            // Neutral cool-grey IDE chrome with a single blue accent (VS Code
            // Dark+ family). Lowest-personality dark preset.
            Theme::Slate => Pal {
                dark: true,
                window: rgb(30, 34, 39),
                panel: rgb(38, 43, 51),
                faint: rgb(44, 50, 59),
                extreme: rgb(22, 25, 29),
                weak: rgb(46, 52, 62),
                hover: rgb(58, 66, 79),
                active: rgb(72, 82, 98),
                accent: rgb(90, 156, 255),
                sel: rgb(46, 80, 130),
                text: rgb(216, 220, 227),
                text_weak: rgb(140, 148, 160),
                border: rgb(58, 64, 75),
            },
            // Nord: Polar Night greys (nord0..nord3) under Snow Storm text
            // (nord6) with a single Frost accent (nord8).
            Theme::Nord => Pal {
                dark: true,
                window: rgb(46, 52, 64),
                panel: rgb(59, 66, 82),
                faint: rgb(67, 76, 94),
                extreme: rgb(38, 43, 53),
                weak: rgb(64, 72, 90),
                hover: rgb(76, 86, 106),
                active: rgb(94, 109, 134),
                accent: rgb(136, 192, 208),
                sel: rgb(67, 97, 133),
                text: rgb(236, 239, 244),
                text_weak: rgb(146, 158, 178),
                border: rgb(76, 86, 106),
            },
            // Solarized Dark: base03 backdrop, base02 panels, base1 text — the
            // canonical low-contrast teal with a measured blue accent.
            Theme::Solarized => Pal {
                dark: true,
                window: rgb(0, 43, 54),
                panel: rgb(7, 54, 66),
                faint: rgb(13, 62, 74),
                extreme: rgb(0, 33, 42),
                weak: rgb(20, 68, 80),
                hover: rgb(33, 86, 98),
                active: rgb(48, 104, 116),
                accent: rgb(38, 139, 210),
                sel: rgb(18, 74, 108),
                text: rgb(147, 161, 161),
                text_weak: rgb(88, 110, 117),
                border: rgb(20, 68, 80),
            },
            // Graphite: near-colorless grey chrome so the map is the only color
            // on screen; one restrained teal accent.
            Theme::Graphite => Pal {
                dark: true,
                window: rgb(26, 26, 26),
                panel: rgb(36, 36, 36),
                faint: rgb(42, 42, 42),
                extreme: rgb(18, 18, 18),
                weak: rgb(46, 46, 46),
                hover: rgb(58, 58, 58),
                active: rgb(74, 74, 74),
                accent: rgb(77, 182, 172),
                sel: rgb(45, 72, 69),
                text: rgb(224, 224, 224),
                text_weak: rgb(140, 140, 140),
                border: rgb(58, 58, 58),
            },
            // Parchment app backdrop, near-white cards, crimson ink accent.
            Theme::Light => Pal {
                dark: false,
                window: rgb(224, 219, 209),
                panel: rgb(238, 234, 226),
                faint: rgb(231, 226, 217),
                extreme: rgb(248, 246, 241),
                weak: rgb(221, 215, 204),
                hover: rgb(208, 200, 187),
                active: rgb(194, 184, 168),
                accent: rgb(150, 42, 38),
                sel: rgb(228, 200, 194),
                text: rgb(32, 28, 26),
                text_weak: rgb(98, 90, 82),
                border: rgb(190, 183, 171),
            },
        }
    }
}

/// Renders a `Theme: <name>` menu button and applies the user's pick to
/// `current`. Returns `true` if the selection changed this frame.
pub fn menu(ui: &mut Ui, current: &mut Theme) -> bool {
    let before = *current;
    ui.menu_button(format!("Theme: {}", current.label()), |ui| {
        for theme in Theme::ALL {
            if ui.radio_value(current, theme, theme.label()).clicked() {
                ui.close_menu();
            }
        }
    });
    *current != before
}

/// Internal flat color set a preset expands into a full [`Visuals`].
struct Pal {
    /// Whether to base on [`Visuals::dark`] (vs [`Visuals::light`]).
    dark: bool,
    window: Color32,
    panel: Color32,
    faint: Color32,
    extreme: Color32,
    weak: Color32,
    hover: Color32,
    active: Color32,
    accent: Color32,
    sel: Color32,
    text: Color32,
    text_weak: Color32,
    border: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn build_visuals(p: &Pal) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    // §DESIGN: one radius family + soft layered elevation, derived from tokens.
    let round = Rounding::same(design::RADIUS_SM);

    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.faint_bg_color = p.faint;
    v.extreme_bg_color = p.extreme;
    v.code_bg_color = p.extreme;

    v.window_fill = p.window;
    v.panel_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_rounding = design::rounding_md();
    v.window_shadow = design::elev_high(p.dark);
    v.popup_shadow = design::elev_med(p.dark);

    v.selection.bg_fill = p.sel;
    v.selection.stroke = Stroke::new(1.0, p.accent);

    let w = &mut v.widgets;
    // Backdrop surfaces (labels, separators, panel frames).
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_weak);
    w.noninteractive.rounding = round;

    // Resting interactive widgets (buttons, combos).
    w.inactive.bg_fill = p.weak;
    w.inactive.weak_bg_fill = p.weak;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.rounding = round;

    // Hover: accent outline so the cursor target reads clearly.
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.rounding = round;

    // Pressed / active.
    w.active.bg_fill = p.active;
    w.active.weak_bg_fill = p.active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.rounding = round;

    // Open combo / menu root.
    w.open.bg_fill = p.weak;
    w.open.weak_bg_fill = p.weak;
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.rounding = round;

    v
}

/// Spacing/padding tweaks shared by every preset — looser than egui's dense
/// defaults, which is most of what separates "themed" from "prototype".
fn tune_spacing(style: &mut Style) {
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.window_margin = Margin::same(10.0);
    // §UO3.2: taller click targets and a wider combo floor so enum names stop
    // clipping. These pair with the larger `Button` text style below.
    style.spacing.interact_size.y = 26.0;
    style.spacing.combo_width = 190.0;
}

/// App-wide type scale (§UO3.1). egui's default text styles are dense; this is
/// the single place that enlarges every default-styled label / button / combo /
/// tab across both apps. Bumping `Button` is the high-leverage move — every tab
/// in the builder strip and every combo's selected text is `Button`-styled.
fn tune_typography(style: &mut Style) {
    style.text_styles = [
        (
            // §BEAUTY §5.5: headings request the registered display face. With the
            // `bundled-fonts` feature off this resolves to `Proportional`, so the
            // stock type scale is unchanged.
            TextStyle::Heading,
            FontId::new(20.0, design::display_family()),
        ),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        ),
    ]
    .into();
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Theme::apply` writes the process-wide `palette::CHROME` static, so the
    // two tests that call it must not run concurrently or they clobber each
    // other's chrome snapshot. Serialize them.
    static CHROME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn every_theme_applies_and_flips_chrome() {
        let _guard = CHROME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        for theme in Theme::ALL {
            theme.apply(&ctx);

            // The chrome store the custom painters read must match the preset.
            let p = theme.palette();
            let chrome = palette::chrome();
            assert_eq!(chrome.bg, p.window, "{}: chrome bg", theme.label());
            assert_eq!(chrome.panel, p.panel, "{}: chrome panel", theme.label());
            assert_eq!(chrome.text, p.text, "{}: chrome text", theme.label());
            assert_eq!(
                chrome.text_dim,
                p.text_weak,
                "{}: chrome text_dim",
                theme.label()
            );

            // egui's own widgets must agree on dark vs light. Light is the
            // only non-dark preset.
            assert_eq!(
                ctx.style().visuals.dark_mode,
                theme != Theme::Light,
                "{}: dark_mode",
                theme.label()
            );
        }
    }

    #[test]
    fn type_scale_and_combo_sizing_applied() {
        // §UO3.1/§UO3.2: the theme owns the app-wide type scale and control
        // sizing. Lock the load-bearing values so a stray egui default can't
        // silently shrink the chrome again.
        let _guard = CHROME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = egui::Context::default();
        Theme::default().apply(&ctx);
        let style = ctx.style();
        assert_eq!(style.text_styles[&TextStyle::Body].size, 15.0);
        assert_eq!(style.text_styles[&TextStyle::Button].size, 15.0);
        assert_eq!(style.text_styles[&TextStyle::Heading].size, 20.0);
        assert_eq!(style.spacing.combo_width, 190.0);
        assert_eq!(style.spacing.interact_size.y, 26.0);
    }
}
