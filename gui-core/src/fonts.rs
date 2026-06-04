//! §BEAUTY §5.5 — custom font registration for the builder + viewer.
//!
//! Typography is the cheapest 30% of beauty, and the stock apps ship egui's
//! `default_fonts`. This module registers three roles once at startup —
//!
//! - **display** face → titles only (a gothic / blackletter-adjacent face),
//!   exposed as the named family [`crate::design::display_family`];
//! - **body** face → a humanist sans, made the primary `Proportional` font so
//!   every ordinary label picks it up with no call-site change;
//! - **mono** face → the primary `Monospace` font for the tabular panels.
//!
//! **Shipping the binaries is opt-in (`bundled-fonts` feature).** egui needs the
//! font *bytes* at compile time (`FontData::from_static(include_bytes!(…))`), so
//! the OFL `.ttf`/`.otf` files must live in-repo under `gui-core/assets/fonts/`.
//! Until they are committed the feature stays off and [`install`] is a no-op:
//! the default build is byte-for-byte unchanged and never references a font
//! family that was not registered. See `gui-core/assets/fonts/FONTS.md` for the
//! exact filenames to drop in and the one-line command to enable the feature.

/// Register the bundled display / body / mono faces on `ctx`.
///
/// Call **once** at startup, *before* the first [`crate::theme::Theme::apply`],
/// from both apps (`builder` and `viewer`). With the `bundled-fonts` feature off
/// (the default) this does nothing, so the apps run on egui's `default_fonts`.
pub fn install(ctx: &egui::Context) {
    #[cfg(feature = "bundled-fonts")]
    {
        use egui::{FontData, FontDefinitions, FontFamily};

        // Start from egui's built-ins so the bundled faces keep emoji / CJK
        // fallbacks behind them rather than replacing the family wholesale.
        let mut fonts = FontDefinitions::default();

        fonts.font_data.insert(
            "sf-display".to_owned(),
            FontData::from_static(include_bytes!("../assets/fonts/display.ttf")),
        );
        fonts.font_data.insert(
            "sf-body".to_owned(),
            FontData::from_static(include_bytes!("../assets/fonts/body.ttf")),
        );
        fonts.font_data.insert(
            "sf-mono".to_owned(),
            FontData::from_static(include_bytes!("../assets/fonts/mono.ttf")),
        );

        // Body face = primary proportional (egui defaults remain as fallbacks).
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "sf-body".to_owned());
        // Mono face = primary monospace.
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "sf-mono".to_owned());
        // Display face = its own named family; titles opt in via
        // `design::display_family()`. Body is listed as its fallback.
        fonts.families.insert(
            FontFamily::Name(crate::design::FONT_DISPLAY.into()),
            vec!["sf-display".to_owned(), "sf-body".to_owned()],
        );

        ctx.set_fonts(fonts);
    }
    #[cfg(not(feature = "bundled-fonts"))]
    {
        let _ = ctx;
    }
}
