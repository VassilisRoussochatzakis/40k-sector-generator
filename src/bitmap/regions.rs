//! Region label overlay (§5 warp phenomena).

use image::RgbaImage;

use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::GeneratedSector;

use super::colors::{darken, short, tint_against};
use super::geom::{hex_center, map_bounds, Geom, MapBounds};
use super::primitives::{draw_rect_outline, draw_text, fill_rect, text_size, GLYPH_H};

pub(super) fn region_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.31) / GLYPH_H as f32).round() as i32).max(1)
}

pub(super) fn draw_region_labels(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    g: &Geom,
    theme: &MapTheme,
) {
    if sector.regions.is_empty() || matches!(theme.label_density, LabelDensity::None) {
        return;
    }
    let scale = region_label_scale(g);
    let pad_x = 6 * g.scale;
    let pad_y = 3 * g.scale;
    let bg = tint_against(theme.panel_bg, 0.70, theme.hex_empty);
    let outline = darken(theme.text_dim, 0.45);
    let MapBounds { w: map_w, h: map_h } = map_bounds(sector, g);
    for region in sector.regions.iter() {
        let Some((cx, cy)) = region_label_anchor(region, g) else {
            continue;
        };
        let label = region_label_text(&region.name);
        let (tw, th) = text_size(&label, scale);
        let bw = tw + pad_x * 2;
        let bh = th + pad_y * 2;
        let max_x = (map_w - bw).max(0);
        let max_y = (map_h - bh).max(0);
        let x = (cx - bw / 2).clamp(0, max_x);
        let y = (cy - bh / 2).clamp(0, max_y);
        fill_rect(img, x, y, bw, bh, bg);
        draw_rect_outline(img, x, y, bw, bh, outline);
        draw_text(
            img,
            x + pad_x + (bw - pad_x * 2 - tw) / 2,
            y + pad_y,
            &label,
            theme.text_dim,
            scale,
        );
    }
}

pub(super) fn region_label_anchor(
    region: &crate::regions::WarpRegion,
    g: &Geom,
) -> Option<(i32, i32)> {
    if region.hexes.is_empty() {
        return None;
    }
    let mut sx: i64 = 0;
    let mut sy: i64 = 0;
    for h in &region.hexes {
        let (cx, cy) = hex_center(h.q, h.r, g);
        sx += cx as i64;
        sy += cy as i64;
    }
    let n = region.hexes.len() as i64;
    Some(((sx / n) as i32, (sy / n) as i32))
}

pub(super) fn region_label_text(name: &str) -> String {
    short(&name.to_ascii_uppercase(), 18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::{RegionConditionKind, WarpRegion};
    use crate::sector_model::HexCoord;

    #[test]
    fn region_label_anchor_uses_footprint_centroid() {
        let g = Geom::new(1, &MapTheme::gm_dark());
        let region = WarpRegion {
            id: "reg-0001".into(),
            name: "Aurelian Maelstrom".into(),
            kind: RegionConditionKind::WarpStorm,
            hexes: vec![HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 0 }],
            centre: HexCoord { q: 0, r: 0 },
        };
        let (x0, y0) = hex_center(0, 0, &g);
        let (x1, y1) = hex_center(2, 0, &g);
        assert_eq!(
            region_label_anchor(&region, &g),
            Some(((x0 + x1) / 2, (y0 + y1) / 2))
        );
        assert!(region_label_text(&region.name).len() <= 18);
    }
}
