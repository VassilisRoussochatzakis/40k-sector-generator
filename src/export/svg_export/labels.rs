//! System name labels + subsector titles.

use std::collections::HashSet;

use crate::export::render_core::{
    labels::{subsector_label_backed, system_label_visible},
    RenderOptions,
};
use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::GeneratedSector;
use crate::subsectors::Subsector;

use super::geom::{hex_center, MapBounds};
use super::primitives::{rect, text};
use super::{star_radius_ratio, HEX_SIZE};

const SYS_LABEL_FONT: f32 = 10.0;
const SUB_LABEL_FONT: f32 = 13.0;

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Rect {
    fn intersects(self, o: Rect) -> bool {
        self.x0 < o.x1 && self.x1 > o.x0 && self.y0 < o.y1 && self.y1 > o.y0
    }
}

pub(super) fn draw_system_labels(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    opts: &RenderOptions,
) {
    if matches!(opts.theme.label_density, LabelDensity::None) {
        return;
    }
    let star_r = HEX_SIZE * star_radius_ratio();
    for sys in sector.systems.iter() {
        if !system_label_visible(sys, subsectors, &opts.theme) {
            continue;
        }
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        let label = sys.name.to_ascii_uppercase();
        let ty = cy + star_r + 3.0 + SYS_LABEL_FONT;
        let tw = label.chars().count() as f32 * SYS_LABEL_FONT * 0.6;
        let pad_x = 3.0;
        let pad_y = 1.0;
        rect(
            s,
            cx - tw * 0.5 - pad_x,
            ty - SYS_LABEL_FONT - pad_y,
            tw + pad_x * 2.0,
            SYS_LABEL_FONT + pad_y * 2.0,
            opts.theme.bg,
            None,
        );
        text(
            s,
            cx,
            ty,
            &label,
            opts.theme.text_dim,
            SYS_LABEL_FONT,
            "middle",
        );
    }
}

pub(super) fn draw_subsector_labels(
    s: &mut String,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    theme: &MapTheme,
    bounds: MapBounds,
) {
    let line_gap = 2.0;
    let pad_x = 6.0;
    let pad_y = 2.0;

    let sys_pad_x = 3.0;
    let sys_pad_y = 1.0;
    let hex_half_w = HEX_SIZE * 3f32.sqrt() / 2.0;
    let hex_half_h = HEX_SIZE;
    let star_r = HEX_SIZE * star_radius_ratio();

    let mut obstacles: Vec<Rect> = Vec::with_capacity(sector.systems.len() * 2);
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r);
        obstacles.push(Rect {
            x0: cx - hex_half_w,
            y0: cy - hex_half_h,
            x1: cx + hex_half_w,
            y1: cy + hex_half_h,
        });
        let name = sys.name.to_ascii_uppercase();
        let tw = name.chars().count() as f32 * SYS_LABEL_FONT * 0.6;
        let ty = cy + star_r + 3.0 + SYS_LABEL_FONT;
        obstacles.push(Rect {
            x0: cx - tw * 0.5 - sys_pad_x,
            y0: ty - SYS_LABEL_FONT - sys_pad_y,
            x1: cx + tw * 0.5 + sys_pad_x,
            y1: ty + sys_pad_y,
        });
    }

    let mut placed: Vec<Rect> = Vec::with_capacity(subsectors.len());
    for sub in subsectors {
        if sub.system_ids.is_empty() || sub.hex_cells.is_empty() {
            continue;
        }
        let cells: HashSet<(i32, i32)> = sub
            .hex_cells
            .iter()
            .map(|&(q, r)| (q as i32, r as i32))
            .collect();

        let top = "SUBSECTOR";
        let bot_owned = sub
            .name
            .strip_prefix("Subsector ")
            .unwrap_or_else(|| sub.name.as_ref())
            .to_ascii_uppercase();

        let tw_top = top.chars().count() as f32 * SUB_LABEL_FONT * 0.6;
        let tw_bot = bot_owned.chars().count() as f32 * SUB_LABEL_FONT * 0.6;
        let block_w = tw_top.max(tw_bot);
        let block_h = SUB_LABEL_FONT.mul_add(2.0, line_gap);

        let mut sx = 0.0_f32;
        let mut sy = 0.0_f32;
        for &(q, r) in &sub.hex_cells {
            let (cx, cy) = hex_center(q as i32, r as i32);
            sx += cx;
            sy += cy;
        }
        let n = sub.hex_cells.len() as f32;
        let cen_x = sx / n;
        let cen_y = sy / n;

        let sys_cells: HashSet<(i32, i32)> = sector
            .systems
            .iter()
            .map(|sys| (sys.coord.q, sys.coord.r))
            .collect();
        let mut cands: Vec<(i32, i32, f32)> = sub
            .hex_cells
            .iter()
            .filter(|&&(q, r)| !sys_cells.contains(&(q as i32, r as i32)))
            .map(|&(q, r)| {
                let (cx, cy) = hex_center(q as i32, r as i32);
                let dx = cx - cen_x;
                let dy = cy - cen_y;
                (q as i32, r as i32, dx.mul_add(dx, dy * dy))
            })
            .collect();
        cands.sort_by(|a, b| {
            a.2.partial_cmp(&b.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                // Determinism: break distance ties on the stable unique (q, r)
                // cell key so the order does not depend on the (stable-sort)
                // input ordering or FMA rounding of the squared distance.
                // `sub.hex_cells` is already `(q, r)`-sorted upstream, so this
                // only makes today's ordering explicit.
                .then_with(|| (a.0, a.1).cmp(&(b.0, b.1)))
        });

        let try_place = |q: i32, r: i32, above: bool, placed: &[Rect]| -> Option<(f32, f32)> {
            if !subsector_label_backed(q, r, above, &cells) {
                return None;
            }
            let (cx, cy) = hex_center(q, r);
            let block_top = if above {
                cy - HEX_SIZE - block_h - 2.0
            } else {
                cy + HEX_SIZE + 2.0
            };
            let block_min_x = cx - block_w * 0.5;
            let rect_b = Rect {
                x0: block_min_x - pad_x,
                y0: block_top - pad_y,
                x1: block_min_x + block_w + pad_x,
                y1: block_top + block_h + pad_y,
            };
            if rect_b.x0 < 0.0 || rect_b.y0 < 0.0 || rect_b.x1 > bounds.w || rect_b.y1 > bounds.h {
                return None;
            }
            for o in obstacles.iter().chain(placed.iter()) {
                if rect_b.intersects(*o) {
                    return None;
                }
            }
            Some((block_min_x, block_top))
        };

        let mut chosen: Option<(f32, f32)> = None;
        'outer: for &(q, r, _) in &cands {
            for above in [true, false] {
                if let Some(p) = try_place(q, r, above, &placed) {
                    chosen = Some(p);
                    break 'outer;
                }
            }
        }

        let (block_min_x, block_top_y) = match chosen {
            Some(p) => p,
            None => {
                let Some(&(q0, r0)) = sub.hex_cells.iter().min_by(|&&(q1, r1), &&(q2, r2)| {
                    let (cx1, cy1) = hex_center(q1 as i32, r1 as i32);
                    let (cx2, cy2) = hex_center(q2 as i32, r2 as i32);
                    let d1 = (cy1 - cen_y).mul_add(cy1 - cen_y, (cx1 - cen_x).powi(2));
                    let d2 = (cy2 - cen_y).mul_add(cy2 - cen_y, (cx2 - cen_x).powi(2));
                    d1.partial_cmp(&d2).unwrap_or(std::cmp::Ordering::Equal)
                }) else {
                    continue;
                };
                let (cx, cy) = hex_center(q0 as i32, r0 as i32);
                let bt = cy - HEX_SIZE - block_h - 2.0;
                let bmx = (cx - block_w * 0.5)
                    .max(pad_x)
                    .min(bounds.w - block_w - pad_x);
                let bty = bt.max(pad_y).min(bounds.h - block_h - pad_y);
                (bmx, bty)
            }
        };

        rect(
            s,
            block_min_x - pad_x,
            block_top_y - pad_y,
            block_w + pad_x * 2.0,
            block_h + pad_y * 2.0,
            theme.subsector_label_bg,
            None,
        );
        text(
            s,
            block_min_x + block_w * 0.5,
            block_top_y + SUB_LABEL_FONT,
            top,
            theme.subsector_label,
            SUB_LABEL_FONT,
            "middle",
        );
        text(
            s,
            block_min_x + block_w * 0.5,
            SUB_LABEL_FONT.mul_add(2.0, block_top_y) + line_gap,
            &bot_owned,
            theme.subsector_label,
            SUB_LABEL_FONT,
            "middle",
        );

        placed.push(Rect {
            x0: block_min_x - pad_x,
            y0: block_top_y - pad_y,
            x1: block_min_x + block_w + pad_x,
            y1: block_top_y + block_h + pad_y,
        });
    }
}
