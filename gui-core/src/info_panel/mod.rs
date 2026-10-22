//! Right-side info panel. One pure render fn per entity kind so layout is easy
//! to tweak in isolation.
//!
//! Split (AREA_F F8, verbatim) into entity-section submodules; the shared
//! text primitives (`title`/`section`/`body`/`dim`/`short`, plus the imported
//! `ui_kit::kv`), the `list_block`/`opt_kv_block` sections, legend rows, and
//! `stability_block` stay here as parent-private helpers visible to every
//! submodule. The public render fns are re-exported so `info_panel::*` paths
//! in the builder and viewer stay unchanged.

use std::sync::Arc;

use egui::{Color32, Pos2, RichText, Ui, Vec2};

use sectorforge::sector_model::RoutePattern;

use crate::palette::{self, darken, draw_route_line};

mod history;
mod overview;
mod route;
mod subsector;
mod system;
mod world;

pub use history::world_history;
pub use overview::{sector_overview_with_buckets, SectorOverviewCache};
pub use route::route_summary;
pub use subsector::subsector_summary;
pub use system::{star_detail, system_summary};
pub use world::world_detail;

// Text primitives following the one type scale defined in `ui_kit` (§UO P1
// dogfood) — inlined here since `ui_kit` itself had no other caller for them.
use crate::ui_kit::{kv, BODY, DIM, SECTION, TITLE};

/// Title row (size [`TITLE`]), primary text color, + a little space.
fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text()).size(TITLE));
    ui.add_space(2.0);
}

/// Bold section header (size [`SECTION`]), primary text color.
fn section(ui: &mut Ui, s: &str) {
    ui.label(
        RichText::new(s)
            .color(palette::chrome_text())
            .size(SECTION)
            .strong(),
    );
}

/// Body line (size [`BODY`]), primary text color.
fn body(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text()).size(BODY));
}

/// Dimmed line (size [`DIM`]), secondary text color.
fn dim(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(palette::chrome_text_dim()).size(DIM));
}

/// Gap + `heading` + one dim line per item (uppercased when `upper`); draws
/// nothing for an empty list. The shared TAGS / NOTES block.
fn list_block(ui: &mut Ui, heading: &str, items: &[Arc<str>], upper: bool) {
    if items.is_empty() {
        return;
    }
    ui.add_space(8.0);
    section(ui, heading);
    for s in items {
        if upper {
            dim(ui, &s.to_uppercase());
        } else {
            dim(ui, s);
        }
    }
}

/// Gap + `heading` + a `kv` row per present value (uppercased, cut to `max`
/// chars); draws nothing and returns `false` when every value is `None`.
fn opt_kv_block(ui: &mut Ui, heading: &str, max: usize, rows: &[(&str, Option<&str>)]) -> bool {
    if rows.iter().all(|(_, v)| v.is_none()) {
        return false;
    }
    ui.add_space(8.0);
    section(ui, heading);
    for (k, v) in rows {
        if let Some(v) = v {
            kv(ui, k, &short(&v.to_uppercase(), max));
        }
    }
    true
}

fn stability_block(ui: &mut Ui, st: &sectorforge::stability::StabilityState) {
    if *st == sectorforge::stability::StabilityState::default() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "STABILITY");
    kv(ui, "PUBLIC ORDER", &format!("{:.0}", st.public_order));
    kv(ui, "CORRUPTION", &format!("{:.0}", st.corruption));
    kv(ui, "FEAR", &format!("{:.0}", st.fear));
    kv(ui, "REBELLION", &format!("{:.0}", st.rebellion_risk));
    kv(ui, "XENOS THREAT", &format!("{:.0}", st.xenos_threat));
    kv(ui, "WARP INSTAB.", &format!("{:.0}", st.warp_instability));
    kv(
        ui,
        "FAMINE/STRESS",
        &format!("{:.0}", st.famine_or_resource_stress),
    );
}

fn legend_row(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, color);
        ui.painter()
            .rect_stroke(rect, 1.0, egui::Stroke::new(1.0, darken(color, 0.5)));
        ui.label(RichText::new(text).color(palette::chrome_text()).size(12.0));
    });
}

fn legend_route_row(ui: &mut Ui, color: Color32, pattern: RoutePattern, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 12.0), egui::Sense::hover());
        let y = rect.center().y;
        let a = Pos2::new(rect.left(), y);
        let b = Pos2::new(rect.right(), y);
        draw_route_line(ui.painter(), a, b, 2.5, color, pattern);
        ui.label(RichText::new(text).color(palette::chrome_text()).size(12.0));
    });
}

fn legend_control_row(ui: &mut Ui, kind: palette::RouteControlKind) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 12.0), egui::Sense::hover());
        // The map glyph at a fixed 10px in the chrome colour, unringed, with a
        // vertical crossbar.
        palette::paint_route_control_shape(
            ui.painter(),
            kind,
            rect.center(),
            10.0,
            Vec2::new(0.0, 5.0),
            egui::Stroke::new(2.5, palette::chrome_text_dim()),
            egui::Stroke::NONE,
        );
        ui.label(
            RichText::new(kind.label())
                .color(palette::chrome_text())
                .size(12.0),
        );
    });
}

/// Truncate `s` to at most `max` chars, appending `.` when it doesn't fit.
/// Char-counted (not byte-sliced) so multibyte input never panics. Callers
/// that need an uppercased label pass `s.to_ascii_uppercase()` in.
pub(crate) fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::short;

    #[test]
    fn short_keeps_string_at_or_below_max() {
        // len == max: returned unchanged.
        assert_eq!(short("hello", 5), "hello");
        // len < max: also unchanged.
        assert_eq!(short("hi", 5), "hi");
    }

    #[test]
    fn short_truncates_to_take_max_minus_one_plus_dot() {
        // 5 chars > max 4 → take(3) + '.'.
        assert_eq!(short("hello", 4), "hel.");
    }

    #[test]
    fn short_counts_chars_not_bytes_no_panic() {
        // "héllo" is 5 chars (é is multibyte) ≤ 6 → unchanged, no panic.
        assert_eq!(short("héllo", 6), "héllo");
        // 5 chars > max 3 → take(2) chars = "hé" + '.'. Byte-slicing at 2
        // would split `é`; char-boundary slicing must not panic.
        assert_eq!(short("héllo", 3), "hé.");
    }

    #[test]
    fn short_max_zero_saturates_without_underflow() {
        // max.saturating_sub(1) == 0 → take(0) == "" then push '.'.
        assert_eq!(short("abc", 0), ".");
    }
}
