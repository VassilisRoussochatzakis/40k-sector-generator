//! System name labels + subsector label placement.

use std::collections::HashSet;

use image::RgbaImage;

use crate::export::render_core::labels::{subsector_label_backed, system_label_visible};
use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::geom::{hex_center, map_bounds, Geom, MapBounds, Rect};
use super::primitives::{draw_text, fill_rect, text_size, GLYPH_H};
use super::routes::star_radius_ratio;
use super::RenderOptions;

pub(super) fn system_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.28) / GLYPH_H as f32).round() as i32).max(1)
}

pub(super) fn subsector_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.36) / GLYPH_H as f32).round() as i32).max(1)
}

pub(super) fn draw_system_labels(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    opts: &RenderOptions,
) {
    if matches!(opts.theme.label_density, LabelDensity::None) {
        return;
    }
    let scale = system_label_scale(g);
    let pad_x = 3 * g.scale;
    let pad_y = g.scale;
    let star_r = (g.hex_size * star_radius_ratio()) as i32;
    for sys in sector.systems.iter() {
        if !system_label_visible(sys, subsectors, &opts.theme) {
            continue;
        }
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        let label = sys.name.to_ascii_uppercase();
        let (tw, th) = text_size(&label, scale);
        let tx = cx - tw / 2;
        let ty = cy + star_r + 3 * g.scale;
        // Pill background so the label stays readable when an adjacent
        // row's hex tip pokes through.
        fill_rect(
            img,
            tx - pad_x,
            ty - pad_y,
            tw + pad_x * 2,
            th + pad_y * 2,
            opts.theme.bg,
        );
        draw_text(img, tx, ty, &label, opts.theme.text_dim, scale);
    }
}

pub(super) fn draw_subsector_labels(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    theme: &MapTheme,
) {
    let scale = subsector_label_scale(g);
    let line_gap = 2 * g.scale;
    let pad_x = 6 * g.scale;
    let pad_y = 2 * g.scale;

    let MapBounds { w: map_w, h: map_h } = map_bounds(sector, g);

    // Static obstacles: every system marker bbox + every system name label rect.
    let sys_label_scale = system_label_scale(g);
    let sys_pad_x = 3 * g.scale;
    let sys_pad_y = g.scale;
    let hex_half_w = (g.hex_size * 3f32.sqrt() / 2.0) as i32;
    let hex_half_h = g.hex_size as i32;
    let star_r = (g.hex_size * star_radius_ratio()) as i32;
    let mut obstacles: Vec<Rect> = Vec::with_capacity(sector.systems.len() * 2);
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        obstacles.push(Rect {
            x0: cx - hex_half_w,
            y0: cy - hex_half_h,
            x1: cx + hex_half_w,
            y1: cy + hex_half_h,
        });
        let name = sys.name.to_ascii_uppercase();
        let (tw, th) = text_size(&name, sys_label_scale);
        let tx = cx - tw / 2;
        let ty = cy + star_r + 3 * g.scale;
        obstacles.push(Rect {
            x0: tx - sys_pad_x,
            y0: ty - sys_pad_y,
            x1: tx + tw + sys_pad_x,
            y1: ty + th + sys_pad_y,
        });
    }

    let mut placed_labels: Vec<Rect> = Vec::with_capacity(subsectors.len());

    for s in subsectors {
        if s.system_ids.is_empty() || s.hex_cells.is_empty() {
            continue;
        }

        let cells: HashSet<(i32, i32)> = s
            .hex_cells
            .iter()
            .map(|&(q, r)| (q as i32, r as i32))
            .collect();

        // Label dimensions.
        let top = "SUBSECTOR";
        let bot_owned: String;
        let bot: &str = {
            let raw = s
                .name
                .strip_prefix("Subsector ")
                .unwrap_or_else(|| s.name.as_ref());
            bot_owned = raw.to_ascii_uppercase();
            bot_owned.as_str()
        };
        let (tw_top, th_top) = text_size(top, scale);
        let (tw_bot, th_bot) = text_size(bot, scale);
        let block_w = tw_top.max(tw_bot);
        let block_h = th_top + line_gap + th_bot;

        // Centroid in pixel space — drives candidate ordering.
        let mut sx: i64 = 0;
        let mut sy: i64 = 0;
        for &(q, r) in &s.hex_cells {
            let (cx, cy) = hex_center(q as i32, r as i32, g);
            sx += cx as i64;
            sy += cy as i64;
        }
        let n = s.hex_cells.len() as i64;
        let cen_x = (sx / n) as i32;
        let cen_y = (sy / n) as i32;

        // Candidates: cells without systems, sorted by pixel distance to centroid.
        let sys_cells: HashSet<(i32, i32)> = sector
            .systems
            .iter()
            .map(|sys| (sys.coord.q, sys.coord.r))
            .collect();
        let mut cands: Vec<(i32, i32, i64)> = s
            .hex_cells
            .iter()
            .filter(|&&(q, r)| !sys_cells.contains(&(q as i32, r as i32)))
            .map(|&(q, r)| {
                let (cx, cy) = hex_center(q as i32, r as i32, g);
                let dx = (cx - cen_x) as i64;
                let dy = (cy - cen_y) as i64;
                (q as i32, r as i32, dx * dx + dy * dy)
            })
            .collect();
        cands.sort_by_key(|&(_, _, d)| d);

        let try_place = |q: i32, r: i32, above: bool, placed: &[Rect]| -> Option<(i32, i32)> {
            if !subsector_label_backed(q, r, above, &cells) {
                return None;
            }
            let (cx, cy) = hex_center(q, r, g);
            let block_top = if above {
                cy - g.hex_size as i32 - block_h - 2 * g.scale
            } else {
                cy + g.hex_size as i32 + 2 * g.scale
            };
            let block_min_x = cx - block_w / 2;
            let rect = Rect {
                x0: block_min_x - pad_x,
                y0: block_top - pad_y,
                x1: block_min_x + block_w + pad_x,
                y1: block_top + block_h + pad_y,
            };
            if rect.x0 < 0 || rect.y0 < 0 || rect.x1 > map_w || rect.y1 > map_h {
                return None;
            }
            for o in obstacles.iter().chain(placed.iter()) {
                if rect.intersects(*o) {
                    return None;
                }
            }
            Some((block_min_x, block_top))
        };

        // Try each candidate in centroid order, above first then below.
        let mut chosen: Option<(i32, i32)> = None;
        'outer: for &(q, r, _) in &cands {
            for above in [true, false] {
                if let Some(p) = try_place(q, r, above, &placed_labels) {
                    chosen = Some(p);
                    break 'outer;
                }
            }
        }

        // Fallback: anchor = cell nearest centroid (occupied or not), above,
        // clamped to map bounds. Visual overlap acceptable as last resort.
        let (block_min_x, block_top_y) = match chosen {
            Some(p) => p,
            None => {
                let Some(&(q0, r0)) = s.hex_cells.iter().min_by_key(|&&(q, r)| {
                    let (cx, cy) = hex_center(q as i32, r as i32, g);
                    let dx = (cx - cen_x) as i64;
                    let dy = (cy - cen_y) as i64;
                    dx * dx + dy * dy
                }) else {
                    continue;
                };
                let (cx, cy) = hex_center(q0 as i32, r0 as i32, g);
                let bt = cy - g.hex_size as i32 - block_h - 2 * g.scale;
                let bmx = (cx - block_w / 2).max(pad_x).min(map_w - block_w - pad_x);
                let bty = bt.max(pad_y).min(map_h - block_h - pad_y);
                (bmx, bty)
            }
        };

        fill_rect(
            img,
            block_min_x - pad_x,
            block_top_y - pad_y,
            block_w + pad_x * 2,
            block_h + pad_y * 2,
            theme.subsector_label_bg,
        );
        draw_text(
            img,
            block_min_x + (block_w - tw_top) / 2,
            block_top_y,
            top,
            theme.subsector_label,
            scale,
        );
        draw_text(
            img,
            block_min_x + (block_w - tw_bot) / 2,
            block_top_y + th_top + line_gap,
            bot,
            theme.subsector_label,
            scale,
        );

        placed_labels.push(Rect {
            x0: block_min_x - pad_x,
            y0: block_top_y - pad_y,
            x1: block_min_x + block_w + pad_x,
            y1: block_top_y + block_h + pad_y,
        });
    }
}
