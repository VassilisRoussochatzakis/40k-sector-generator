//! §5 warp-region label overlay.

use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::GeneratedSector;

use super::colors::{darken, short, tint_against};
use super::geom::{hex_center, MapBounds};
use super::primitives::{rect, text};

const REGION_LABEL_FONT: f32 = 11.0;

pub(super) fn draw_region_labels(
    s: &mut String,
    sector: &GeneratedSector,
    theme: &MapTheme,
    bounds: MapBounds,
) {
    if sector.regions.is_empty() || matches!(theme.label_density, LabelDensity::None) {
        return;
    }
    let bg = tint_against(theme.panel_bg, 0.70, theme.hex_empty);
    let outline = darken(theme.text_dim, 0.45);
    for region in sector.regions.iter() {
        let Some((cx, cy)) = region_label_anchor(region) else {
            continue;
        };
        let label = short(&region.name.to_ascii_uppercase(), 18);
        let pad_x = 6.0;
        let pad_y = 3.0;
        let tw = label.chars().count() as f32 * REGION_LABEL_FONT * 0.6;
        let bw = tw + pad_x * 2.0;
        let bh = REGION_LABEL_FONT + pad_y * 2.0;
        let x = (cx - bw * 0.5).clamp(0.0, (bounds.w - bw).max(0.0));
        let y = (cy - bh * 0.5).clamp(0.0, (bounds.h - bh).max(0.0));
        rect(s, x, y, bw, bh, bg, Some(outline));
        text(
            s,
            x + bw * 0.5,
            REGION_LABEL_FONT.mul_add(0.35, y + bh * 0.5),
            &label,
            theme.text_dim,
            REGION_LABEL_FONT,
            "middle",
        );
    }
}

fn region_label_anchor(region: &crate::regions::WarpRegion) -> Option<(f32, f32)> {
    if region.hexes.is_empty() {
        return None;
    }
    let mut sx = 0.0_f32;
    let mut sy = 0.0_f32;
    for h in &region.hexes {
        let (cx, cy) = hex_center(h.q, h.r);
        sx += cx;
        sy += cy;
    }
    let n = region.hexes.len() as f32;
    Some((sx / n, sy / n))
}
