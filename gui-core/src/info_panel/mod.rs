//! Right-side info panel. One pure render fn per entity kind so layout is easy
//! to tweak in isolation.
//!
//! Split (AREA_F F8, verbatim) into entity-section submodules; the shared
//! text primitives (`title`/`section`/`body`/`dim`/`kv`/`short`), legend rows,
//! and `stability_block` stay here as parent-private helpers visible to every
//! submodule. The public render fns are re-exported so `info_panel::*` paths
//! in the builder and viewer stay unchanged.

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
pub use overview::{sector_overview, sector_overview_with_buckets, SectorOverviewCache};
pub use route::route_summary;
pub use subsector::subsector_summary;
pub use system::{star_detail, system_summary};
pub use world::world_detail;

// These delegate to the shared `ui_kit` text helpers (§UO P1 dogfood) so the
// info panel follows the one type scale defined there.
fn title(ui: &mut Ui, s: &str) {
    crate::ui_kit::mono_title(ui, s);
}

fn section(ui: &mut Ui, s: &str) {
    crate::ui_kit::mono_section(ui, s);
}

fn body(ui: &mut Ui, s: &str) {
    crate::ui_kit::mono_body(ui, s);
}

fn dim(ui: &mut Ui, s: &str) {
    crate::ui_kit::mono_dim(ui, s);
}

fn kv(ui: &mut Ui, k: &str, v: &str) {
    crate::ui_kit::kv(ui, k, v);
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
    use palette::RouteControlKind;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 12.0), egui::Sense::hover());
        let center = rect.center();
        let size = 10.0;
        let half = size / 2.0;
        let color = palette::chrome_text_dim();
        let painter = ui.painter();

        match kind {
            RouteControlKind::Patrol => {
                painter.circle_filled(center, half, color);
            }
            RouteControlKind::Toll => {
                painter.rect_filled(
                    egui::Rect::from_center_size(center, Vec2::splat(size)),
                    0.0,
                    color,
                );
            }
            RouteControlKind::Interdiction => {
                painter.line_segment(
                    [center - Vec2::new(0.0, half), center + Vec2::new(0.0, half)],
                    egui::Stroke::new(2.5, color),
                );
            }
            RouteControlKind::Piracy => {
                painter.line_segment(
                    [
                        center - Vec2::new(half, half),
                        center + Vec2::new(half, half),
                    ],
                    egui::Stroke::new(2.5, color),
                );
                painter.line_segment(
                    [
                        center - Vec2::new(half, -half),
                        center + Vec2::new(half, -half),
                    ],
                    egui::Stroke::new(2.5, color),
                );
            }
        }

        ui.label(
            RichText::new(kind.label())
                .color(palette::chrome_text())
                .size(12.0),
        );
    });
}

fn short(s: &str, max: usize) -> String {
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
