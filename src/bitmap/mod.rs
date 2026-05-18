//! PNG export: renders the sector as a PNG hex map with a legend.
//!
//! Pure pixel work — no external font files. A small 5x7 monospace bitmap
//! font is embedded below. Hex layout is pointy-top with odd-r offset rows,
//! matching the ASCII map in `export.rs`.
//!
//! All sizes are derived from a `Geom` struct built from an integer scale
//! factor so the same renderer can produce a small thumbnail or a ~4K poster.

use std::collections::{HashMap, HashSet};

use camino::Utf8Path;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage};

use crate::errors::SectorError;
use crate::faction_style::faction_style_rgb_by_id;
use crate::heatmap::{self, HeatCellRgb, HeatmapMode};
use crate::sector_model::{
    offset_r_neighbors, GeneratedSector, RoutePattern, RouteStability, RouteType,
};
use crate::subsectors::Subsector;

/// Per-render options independent of the project config. Mirrors the relevant
/// bits of [`crate::config::BitmapConfig`] so callers (CLI, GUI export, tests)
/// can override without touching the project's TOML.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Tint each system's hex by the dominant faction (§8).
    pub faction_fill: bool,
    /// Overlay a heatmap tint per system (§10). `Off` disables it.
    pub heatmap: HeatmapMode,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            faction_fill: true,
            heatmap: HeatmapMode::Off,
        }
    }
}

/// Save an RGBA image as PNG using fast (low-CPU) deflate. Lossless.
pub(crate) fn save_png_fast(img: &RgbaImage, path: &Utf8Path) -> Result<(), SectorError> {
    let file = std::fs::File::create(path.as_std_path())
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    let writer = std::io::BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::NoFilter);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    Ok(())
}

// ── Shared palette ──────────────────────────────────────────────────────────

pub(crate) const BG: Rgba<u8> = Rgba([14, 12, 20, 255]);
pub(crate) const PANEL_BG: Rgba<u8> = Rgba([22, 18, 30, 255]);
pub(crate) const HEX_EMPTY: Rgba<u8> = Rgba([28, 26, 38, 255]);
pub(crate) const HEX_OUTLINE: Rgba<u8> = Rgba([60, 55, 78, 255]);
pub(crate) const TEXT: Rgba<u8> = Rgba([232, 228, 240, 255]);
pub(crate) const TEXT_DIM: Rgba<u8> = Rgba([150, 145, 165, 255]);
pub(crate) const SUBSECTOR_BORDER: Rgba<u8> = Rgba([160, 160, 160, 255]);
pub(crate) const SUBSECTOR_LABEL: Rgba<u8> = Rgba([230, 195, 120, 255]);
pub(crate) const SUBSECTOR_LABEL_BG: Rgba<u8> = Rgba([20, 16, 28, 255]);
pub(crate) const CAPITAL_MARKER: Rgba<u8> = Rgba([255, 220, 100, 255]);
pub(crate) const CAPITAL_OUTLINE: Rgba<u8> = Rgba([60, 40, 10, 255]);

// ── Geometry ────────────────────────────────────────────────────────────────

/// Scaled geometry for the sector map. All pixel sizes are derived from
/// `scale` so callers can opt into higher resolution renders.
pub(crate) struct Geom {
    pub scale: i32,
    pub hex_size: f32,
    pub margin: i32,
    pub legend_width: i32,
    pub legend_pad: i32,
    pub line_h: i32,
    pub text_scale: i32,
    pub title_scale: i32,
}

impl Geom {
    fn new(scale: u32) -> Self {
        let s = scale.max(1) as i32;
        Self {
            scale: s,
            hex_size: 26.0 * s as f32,
            margin: 28 * s,
            legend_width: 280 * s,
            legend_pad: 16 * s,
            line_h: 18 * s,
            text_scale: s,
            title_scale: 2 * s,
        }
    }
}

// ── Public entry point ──────────────────────────────────────────────────────

pub fn write_bitmap(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
) -> Result<(), SectorError> {
    write_bitmap_with(
        sector,
        output_dir,
        scale,
        subsectors,
        RenderOptions::default(),
    )
}

/// Variant of [`write_bitmap`] that takes [`RenderOptions`] (faction fill +
/// heatmap mode).
pub fn write_bitmap_with(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> Result<(), SectorError> {
    let path = output_dir.join("sector.png");
    let img = render(sector, scale, subsectors, opts);
    save_png_fast(&img, &path)
}

/// Render the sector PNG to an explicit file path (caller chooses the name).
pub fn write_sector_png_to(
    sector: &GeneratedSector,
    path: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
) -> Result<(), SectorError> {
    write_sector_png_to_with(sector, path, scale, subsectors, RenderOptions::default())
}

/// Variant of [`write_sector_png_to`] that takes [`RenderOptions`].
pub fn write_sector_png_to_with(
    sector: &GeneratedSector,
    path: &Utf8Path,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> Result<(), SectorError> {
    let img = render(sector, scale, subsectors, opts);
    save_png_fast(&img, path)
}

// ── Rendering ───────────────────────────────────────────────────────────────

/// Map pixel bounds matching the GUI's `sector_view` layout. Includes the
/// bottom label band so system-name text fits under each hex.
struct MapBounds {
    w: i32,
    h: i32,
}

fn map_bounds(sector: &GeneratedSector, g: &Geom) -> MapBounds {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    // Pointy-top odd-r offset layout: odd rows shift right by half a step,
    // so the bounding rect is `width * horiz_step` wide plus a half-step
    // when height > 1 to cover the staggered odd rows.
    let odd_shift = if sector.height > 1 { 0.5 } else { 0.0 };
    let w = (g.margin as f32 * 2.0 + horiz_step * (sector.width as f32 + odd_shift)) as i32;
    let label_band = (g.hex_size * 0.55) as i32;
    let h = (g.margin as f32 * 2.0
        + (sector.height.saturating_sub(1)) as f32 * vert_step
        + 2.0 * g.hex_size) as i32
        + label_band;
    MapBounds { w, h }
}

fn render(
    sector: &GeneratedSector,
    scale: u32,
    subsectors: Option<&[Subsector]>,
    opts: RenderOptions,
) -> RgbaImage {
    let g = Geom::new(scale);
    let MapBounds { w: map_w, h: map_h } = map_bounds(sector, &g);

    let legend_h = legend_height(sector, &g, opts);
    let total_w = map_w + g.legend_width;
    let total_h = map_h.max(legend_h);

    let mut img = RgbaImage::from_pixel(total_w as u32, total_h as u32, BG);

    let subs = subsectors.unwrap_or(&[]);

    // Per-system fill tint for §8 (faction colour) / §10 (heatmap overlay).
    let heat = if matches!(opts.heatmap, HeatmapMode::Off) {
        HashMap::new()
    } else {
        heatmap::compute_rgb(sector, opts.heatmap)
    };
    let sys_tints = compute_system_tints(sector, opts, &heat);

    draw_hex_grid(&mut img, sector, &g, &sys_tints);
    if !subs.is_empty() {
        draw_subsector_borders(&mut img, sector, subs, &g);
    }
    draw_routes(&mut img, sector, &g);
    draw_systems(&mut img, sector, subs, &g);
    if !subs.is_empty() {
        draw_subsector_labels(&mut img, sector, subs, &g);
    }
    draw_system_labels(&mut img, sector, &g);

    // Legend painted last so any overflow from the map gets clipped behind it.
    fill_rect(&mut img, map_w, 0, g.legend_width, total_h, PANEL_BG);
    draw_legend(&mut img, sector, map_w, &g, opts);

    img
}

/// One hex tint per (q, r) where the system has a dominant faction (§8) or a
/// heatmap intensity > 0 (§10). Empty hexes stay at `HEX_EMPTY`.
fn compute_system_tints(
    sector: &GeneratedSector,
    opts: RenderOptions,
    heat: &HashMap<String, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for sys in &sector.systems {
        let key = (sys.coord.q, sys.coord.r);
        // Heatmap overrides faction fill for non-Control modes. For Control
        // mode the underlying score already drives `faction_style.fill`, so
        // both paths agree.
        if !matches!(opts.heatmap, HeatmapMode::Off) {
            if let Some(cell) = heat.get(&sys.id) {
                let strength = 0.18 + cell.intensity * 0.45;
                let color = rgba(cell.rgb);
                out.insert(key, tint(color, strength));
                continue;
            }
        }
        if opts.faction_fill {
            if let Some(dom) = sys.control.dominant.as_deref() {
                let style = faction_style_rgb_by_id(&sector.factions, dom);
                out.insert(key, tint(rgba(style.fill), 0.35));
            }
        }
    }
    out
}

fn rgba(t: (u8, u8, u8)) -> Rgba<u8> {
    Rgba([t.0, t.1, t.2, 255])
}

fn draw_hex_grid(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    g: &Geom,
    sys_tints: &HashMap<(i32, i32), Rgba<u8>>,
) {
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r, g);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(HEX_EMPTY);
            draw_hex(img, cx, cy, g.hex_size, fill, HEX_OUTLINE);
        }
    }
}

fn draw_routes(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom) {
    let mut centers: HashMap<&str, (i32, i32)> = HashMap::new();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        centers.insert(sys.id.as_str(), (cx, cy));
    }
    // Match GUI: thickness scales with hex size, not just scale factor.
    let thickness = ((g.hex_size * 0.08).max(2.0)) as i32;
    let star_r = g.hex_size * star_radius_ratio();
    for route in &sector.routes {
        let (Some(&a), Some(&b)) = (
            centers.get(route.from_system_id.as_str()),
            centers.get(route.to_system_id.as_str()),
        ) else {
            continue;
        };
        let Some(((sx, sy), (ex, ey))) = shorten_to_star(a, b, star_r) else {
            continue;
        };
        let color = stability_color(route.stability);
        draw_route_line_thick(
            img,
            sx,
            sy,
            ex,
            ey,
            color,
            thickness,
            route.route_type.pattern(),
        );
        draw_route_control_glyph(img, sector, route, (sx, sy), (ex, ey), thickness);
    }
}

/// §3: at the midpoint of a route, draw a symbol for the single strongest
/// `RouteControl` category (patrol / toll / interdiction / piracy) when its
/// score is >= 40. Colour comes from the controlling faction's
/// `FactionStyle.fill` so the reader can identify who is asserting along the
/// route.
fn draw_route_control_glyph(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    route: &crate::sector_model::GeneratedRoute,
    a: (i32, i32),
    b: (i32, i32),
    thickness: i32,
) {
    let Some((faction_id, kind, score)) = top_route_control(route) else {
        return;
    };
    if score < 40.0 {
        return;
    }
    let style = faction_style_rgb_by_id(&sector.factions, &faction_id);
    let color = Rgba([style.fill.0, style.fill.1, style.fill.2, 255]);
    let dark = darken(color, 0.5);
    let mx = (a.0 + b.0) / 2;
    let my = (a.1 + b.1) / 2;
    let size = (thickness * 3).max(6);
    match kind {
        ControlKind::Interdiction => {
            // Crossbar perpendicular to the line.
            let dx = (b.0 - a.0) as f32;
            let dy = (b.1 - a.1) as f32;
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let px = -dy / len;
            let py = dx / len;
            let half = size as f32;
            let x0 = (mx as f32 - px * half) as i32;
            let y0 = (my as f32 - py * half) as i32;
            let x1 = (mx as f32 + px * half) as i32;
            let y1 = (my as f32 + py * half) as i32;
            draw_line_thick(img, x0, y0, x1, y1, color, thickness.max(2));
            draw_line_thick(img, x0, y0, x1, y1, dark, 1);
        }
        ControlKind::Patrol => {
            // Filled disc.
            fill_circle(img, mx, my, size / 2, color);
            draw_circle(img, mx, my, size / 2, dark);
        }
        ControlKind::Toll => {
            // Filled square.
            let half = size / 2;
            fill_rect(img, mx - half, my - half, size, size, color);
            draw_rect_outline(img, mx - half, my - half, size, size, dark);
        }
        ControlKind::Piracy => {
            // X — two short diagonals.
            let half = size / 2;
            draw_line_thick(
                img,
                mx - half,
                my - half,
                mx + half,
                my + half,
                color,
                thickness.max(2),
            );
            draw_line_thick(
                img,
                mx - half,
                my + half,
                mx + half,
                my - half,
                color,
                thickness.max(2),
            );
        }
    }
}

#[derive(Clone, Copy)]
enum ControlKind {
    Patrol,
    Toll,
    Interdiction,
    Piracy,
}

fn top_route_control(
    route: &crate::sector_model::GeneratedRoute,
) -> Option<(String, ControlKind, f32)> {
    let mut best: Option<(&str, ControlKind, f32)> = None;
    for c in &route.controls {
        for (kind, score) in [
            (ControlKind::Interdiction, c.interdiction),
            (ControlKind::Patrol, c.patrol),
            (ControlKind::Piracy, c.piracy),
            (ControlKind::Toll, c.toll),
        ] {
            if best.map(|(_, _, s)| score > s).unwrap_or(true) {
                best = Some((c.faction_id.as_str(), kind, score));
            }
        }
    }
    best.map(|(id, k, s)| (id.to_string(), k, s))
}

fn star_radius_ratio() -> f32 {
    0.2016
}

fn shorten_to_star(a: (i32, i32), b: (i32, i32), star_r: f32) -> Option<((i32, i32), (i32, i32))> {
    let dx = (b.0 - a.0) as f32;
    let dy = (b.1 - a.1) as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= star_r * 2.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let s = (
        (a.0 as f32 + ux * star_r).round() as i32,
        (a.1 as f32 + uy * star_r).round() as i32,
    );
    let e = (
        (b.0 as f32 - ux * star_r).round() as i32,
        (b.1 as f32 - uy * star_r).round() as i32,
    );
    Some((s, e))
}

/// Draws a line styled by `pattern`. For `Solid`, falls back to `draw_line_thick`.
/// Dashes are sized as multiples of a `unit` that scales with `thickness`, so the
/// pattern stays readable at any zoom. Short "dot" runs render as filled discs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_route_line_thick(
    img: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgba<u8>,
    thickness: i32,
    pattern: RoutePattern,
) {
    let strides = pattern.strides();
    if strides.is_empty() {
        draw_line_thick(img, x0, y0, x1, y1, color, thickness);
        return;
    }
    let unit = (thickness as f32).max(2.0);
    let dx = (x1 - x0) as f32;
    let dy = (y1 - y0) as f32;
    let total = (dx * dx + dy * dy).sqrt();
    if total <= 0.0 {
        return;
    }
    let ux = dx / total;
    let uy = dy / total;
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < total {
        let stride = strides[idx % strides.len()];
        let seg = stride * unit;
        let next_t = (t + seg).min(total);
        if idx.is_multiple_of(2) {
            let sx = (x0 as f32 + ux * t).round() as i32;
            let sy = (y0 as f32 + uy * t).round() as i32;
            let ex = (x0 as f32 + ux * next_t).round() as i32;
            let ey = (y0 as f32 + uy * next_t).round() as i32;
            if stride <= 1.5 {
                let mx = (sx + ex) / 2;
                let my = (sy + ey) / 2;
                let r = ((thickness as f32) * 0.6).round() as i32;
                fill_circle(img, mx, my, r.max(1), color);
            } else {
                draw_line_thick(img, sx, sy, ex, ey, color, thickness);
            }
        }
        t = next_t;
        idx += 1;
    }
}

fn draw_systems(img: &mut RgbaImage, sector: &GeneratedSector, subsectors: &[Subsector], g: &Geom) {
    let star_r = (g.hex_size * star_radius_ratio()) as i32;
    let pip_scale = pip_text_scale(g);
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        let fill = star_color(&sys.star.colour_code);
        // Star disk (no tinted hex fill; matches GUI sector view).
        fill_circle(img, cx, cy, star_r, fill);
        draw_circle(img, cx, cy, star_r, darken(fill, 0.55));

        // Subsector capital marker: gold diamond above the star.
        if subsectors
            .iter()
            .any(|s| s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str()))
        {
            draw_capital_marker(img, cx, cy, g.hex_size);
        }

        // World-count pip on the lower-right of the hex.
        let pip = sys.worlds.len();
        if pip > 0 {
            let label = format!("{pip}");
            let (tw, th) = text_size(&label, pip_scale);
            let tx = cx + (g.hex_size * 0.55) as i32 - tw;
            let ty = cy + (g.hex_size * 0.55) as i32 - th;
            draw_text(img, tx, ty, &label, TEXT, pip_scale);
        }
    }
}

fn pip_text_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.34) / GLYPH_H as f32).round() as i32).max(1)
}

fn system_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.28) / GLYPH_H as f32).round() as i32).max(1)
}

fn subsector_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.36) / GLYPH_H as f32).round() as i32).max(1)
}

fn draw_system_labels(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom) {
    let scale = system_label_scale(g);
    let pad_x = 3 * g.scale;
    let pad_y = g.scale;
    let star_r = (g.hex_size * star_radius_ratio()) as i32;
    for sys in &sector.systems {
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
            BG,
        );
        draw_text(img, tx, ty, &label, TEXT_DIM, scale);
    }
}

fn draw_capital_marker(img: &mut RgbaImage, cx: i32, cy: i32, hex_size: f32) {
    let r = ((hex_size * 0.15).max(3.5)).round() as i32;
    let dy = -((hex_size * 0.55).round() as i32);
    let center_y = cy + dy;
    let pts = [
        (cx, center_y - r),
        (cx + r, center_y),
        (cx, center_y + r),
        (cx - r, center_y),
    ];
    fill_polygon(img, &pts, CAPITAL_MARKER);
    for i in 0..4 {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % 4];
        draw_line(img, ax, ay, bx, by, CAPITAL_OUTLINE);
    }
}

fn draw_subsector_borders(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
) {
    let mut owner: HashMap<(i32, i32), &str> = HashMap::new();
    for s in subsectors {
        for &(q, r) in &s.hex_cells {
            owner.insert((q as i32, r as i32), s.id.as_str());
        }
    }
    if owner.is_empty() {
        return;
    }
    let border_thick = (g.hex_size * 0.10).max(2.5);
    let dot_radius = ((border_thick * 0.8).max(2.0)).round() as i32;
    let spacing = dot_radius as f32 * 2.5;
    for r in 0..sector.height as i32 {
        let deltas = offset_r_neighbors(r);
        for q in 0..sector.width as i32 {
            let Some(here_id) = owner.get(&(q, r)).copied() else {
                continue;
            };
            let (cx, cy) = hex_center(q, r, g);
            let v = hex_vertices(cx, cy, g.hex_size);
            for (i, (dq, dr)) in deltas.iter().enumerate() {
                let other = owner.get(&(q + dq, r + dr)).copied();
                let differs = match other {
                    Some(id) => id != here_id,
                    None => true,
                };
                if !differs {
                    continue;
                }
                let a = v[i];
                let b = v[(i + 1) % 6];
                let edge_len = (((b.0 - a.0) as f32).powi(2) + ((b.1 - a.1) as f32).powi(2)).sqrt();
                let segments = (edge_len / spacing).ceil() as usize;
                for j in 0..=segments {
                    let t = j as f32 / segments as f32;
                    let mx = (a.0 as f32 + (b.0 - a.0) as f32 * t).round() as i32;
                    let my = (a.1 as f32 + (b.1 - a.1) as f32 * t).round() as i32;
                    fill_circle(img, mx, my, dot_radius, SUBSECTOR_BORDER);
                }
            }
        }
    }
}

/// Axis-aligned rect in pixel space. Used for collision tests when placing
/// subsector labels.
#[derive(Clone, Copy)]
struct Rect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl Rect {
    fn intersects(self, o: Rect) -> bool {
        self.x0 < o.x1 && self.x1 > o.x0 && self.y0 < o.y1 && self.y1 > o.y0
    }
}

fn draw_subsector_labels(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
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
            let raw = s.name.strip_prefix("Subsector ").unwrap_or(s.name.as_str());
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
            // Need same-subsector hexes covering label rect: above → NW + NE
            // neighbors; below → SW + SE neighbors. Index per offset_r_neighbors:
            // 0:E 1:SE 2:SW 3:W 4:NW 5:NE.
            let nbrs = offset_r_neighbors(r);
            if above {
                let nw = (q + nbrs[4].0, r + nbrs[4].1);
                let ne = (q + nbrs[5].0, r + nbrs[5].1);
                if !cells.contains(&nw) || !cells.contains(&ne) {
                    return None;
                }
            } else {
                let se = (q + nbrs[1].0, r + nbrs[1].1);
                let sw = (q + nbrs[2].0, r + nbrs[2].1);
                if !cells.contains(&se) || !cells.contains(&sw) {
                    return None;
                }
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
        let (block_min_x, block_top_y) = chosen.unwrap_or_else(|| {
            let &(q0, r0) = s
                .hex_cells
                .iter()
                .min_by_key(|&&(q, r)| {
                    let (cx, cy) = hex_center(q as i32, r as i32, g);
                    let dx = (cx - cen_x) as i64;
                    let dy = (cy - cen_y) as i64;
                    dx * dx + dy * dy
                })
                .expect("non-empty");
            let (cx, cy) = hex_center(q0 as i32, r0 as i32, g);
            let bt = cy - g.hex_size as i32 - block_h - 2 * g.scale;
            let bmx = (cx - block_w / 2).max(pad_x).min(map_w - block_w - pad_x);
            let bty = bt.max(pad_y).min(map_h - block_h - pad_y);
            (bmx, bty)
        });

        fill_rect(
            img,
            block_min_x - pad_x,
            block_top_y - pad_y,
            block_w + pad_x * 2,
            block_h + pad_y * 2,
            SUBSECTOR_LABEL_BG,
        );
        draw_text(
            img,
            block_min_x + (block_w - tw_top) / 2,
            block_top_y,
            top,
            SUBSECTOR_LABEL,
            scale,
        );
        draw_text(
            img,
            block_min_x + (block_w - tw_bot) / 2,
            block_top_y + th_top + line_gap,
            bot,
            SUBSECTOR_LABEL,
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

fn legend_height(sector: &GeneratedSector, g: &Geom, opts: RenderOptions) -> i32 {
    // title block (3) + spacer + 7 star rows + spacer
    // + ROUTE TYPE header + 4 type rows + spacer
    // + ROUTE STABILITY header + 4 stab rows + spacer
    // + optional ROUTE CONTROL header + 4 rows + spacer
    // + factions block + optional heatmap row + footer pad.
    let heatmap_lines = if matches!(opts.heatmap, HeatmapMode::Off) {
        0
    } else {
        2
    };
    let route_control_lines = if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        5
    } else {
        0
    };
    let lines = 3
        + 1
        + 7
        + 1
        + 1
        + 4
        + 1
        + 1
        + 4
        + 1
        + route_control_lines
        + 1
        + factions_visible(sector)
        + heatmap_lines;
    g.legend_pad * 2 + lines as i32 * g.line_h
}

use crate::importance::{
    DEFAULT_DISPLAY_CAP as FACTION_DISPLAY_CAP, DEFAULT_MINOR_FRACTION as FACTION_MINOR_FRACTION,
};

fn factions_visible(sector: &GeneratedSector) -> usize {
    if sector.factions.is_empty() {
        return 0;
    }
    let buckets = crate::importance::compute_display_buckets(
        sector,
        FACTION_MINOR_FRACTION,
        FACTION_DISPLAY_CAP,
    );
    1 + buckets.len()
}

fn draw_legend(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    map_w: i32,
    g: &Geom,
    opts: RenderOptions,
) {
    let x0 = map_w + g.legend_pad;
    let mut y = g.legend_pad;
    let line_h = g.line_h;
    let body = g.text_scale;
    let title = g.title_scale;
    let swatch = 12 * g.scale;

    let title_text = format!("SECTOR: {}", sector.id.to_uppercase());
    draw_text(img, x0, y, &title_text, TEXT, title);
    y += line_h + 4 * g.scale;
    draw_text(
        img,
        x0,
        y,
        &format!("SEED: {}", short(&sector.seed, 20)),
        TEXT_DIM,
        body,
    );
    y += line_h - 4 * g.scale;
    draw_text(
        img,
        x0,
        y,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.manifest.world_count,
        ),
        TEXT_DIM,
        body,
    );
    y += line_h + 4 * g.scale;

    draw_text(img, x0, y, "STAR COLOURS", TEXT, body);
    y += line_h;
    for (code, name) in STAR_LEGEND {
        let color = star_color(code);
        fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, color);
        draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(color, 0.5));
        draw_text(
            img,
            x0 + swatch + 8 * g.scale,
            y,
            &format!("{code} - {name}"),
            TEXT,
            body,
        );
        y += line_h;
    }
    y += 4 * g.scale;

    draw_text(img, x0, y, "ROUTE TYPE", TEXT, body);
    y += line_h;
    for (rtype, name) in [
        (RouteType::StableWarpLane, "STABLE WARP LANE"),
        (RouteType::ChartedPassage, "CHARTED PASSAGE"),
        (RouteType::DangerousPassage, "DANGEROUS PASSAGE"),
        (RouteType::SecretPassage, "SECRET PASSAGE"),
        (RouteType::Webway, "WEBWAY THREAD"),
        (RouteType::BlackShip, "BLACK-SHIP LANE"),
        (RouteType::SmugglingLane, "SMUGGLING LANE"),
    ] {
        draw_route_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 30 * g.scale,
            y + 8 * g.scale,
            TEXT,
            3 * g.scale,
            rtype.pattern(),
        );
        draw_text(img, x0 + 38 * g.scale, y, name, TEXT, body);
        y += line_h;
    }
    y += 4 * g.scale;

    draw_text(img, x0, y, "ROUTE STABILITY", TEXT, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Perilous, "PERILOUS"),
    ] {
        let color = stability_color(stab);
        draw_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 22 * g.scale,
            y + 8 * g.scale,
            color,
            3 * g.scale,
        );
        draw_text(img, x0 + 30 * g.scale, y, name, TEXT, body);
        y += line_h;
    }
    y += 4 * g.scale;

    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        draw_text(img, x0, y, "ROUTE CONTROL", TEXT, body);
        y += line_h;
        let glyph_cx = x0 + 8 * g.scale;
        let glyph_size = 10 * g.scale;
        let half = glyph_size / 2;
        let neutral = Rgba([170, 170, 180, 255]);
        for (name, kind) in [
            ("PATROL", ControlKind::Patrol),
            ("TOLL", ControlKind::Toll),
            ("INTERDICTION", ControlKind::Interdiction),
            ("PIRACY", ControlKind::Piracy),
        ] {
            let cy_y = y + 8 * g.scale;
            match kind {
                ControlKind::Patrol => {
                    fill_circle(img, glyph_cx, cy_y, half, neutral);
                    draw_circle(img, glyph_cx, cy_y, half, darken(neutral, 0.5));
                }
                ControlKind::Toll => {
                    fill_rect(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        neutral,
                    );
                    draw_rect_outline(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        darken(neutral, 0.5),
                    );
                }
                ControlKind::Interdiction => {
                    draw_line_thick(
                        img,
                        glyph_cx,
                        cy_y - half,
                        glyph_cx,
                        cy_y + half,
                        neutral,
                        2 * g.scale,
                    );
                }
                ControlKind::Piracy => {
                    draw_line_thick(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_cx + half,
                        cy_y + half,
                        neutral,
                        2 * g.scale,
                    );
                    draw_line_thick(
                        img,
                        glyph_cx - half,
                        cy_y + half,
                        glyph_cx + half,
                        cy_y - half,
                        neutral,
                        2 * g.scale,
                    );
                }
            }
            draw_text(img, x0 + 22 * g.scale, y, name, TEXT, body);
            y += line_h;
        }
        y += 4 * g.scale;
    }

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", TEXT, body);
        y += line_h;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            FACTION_MINOR_FRACTION,
            FACTION_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, world_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    world_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, *world_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    world_count,
                    ..
                } => (
                    label.to_uppercase(),
                    *system_count,
                    *world_count,
                    (140, 140, 150),
                ),
            };
            let swatch_color = rgba(swatch_rgb);
            fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, swatch_color);
            draw_rect_outline(
                img,
                x0,
                y + 2 * g.scale,
                swatch,
                swatch,
                darken(swatch_color, 0.5),
            );
            draw_text(
                img,
                x0 + swatch + 8 * g.scale,
                y,
                &format!("{} ({} SYS, {} W)", short(&label, 16), sys_n, world_n),
                TEXT_DIM,
                body,
            );
            y += line_h;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4 * g.scale;
        draw_text(img, x0, y, "HEATMAP", TEXT, body);
        y += line_h;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, chip);
        draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(chip, 0.5));
        draw_text(
            img,
            x0 + swatch + 8 * g.scale,
            y,
            opts.heatmap.label(),
            TEXT,
            body,
        );
    }
}

// ── Hex geometry ────────────────────────────────────────────────────────────

fn hex_center(q: i32, r: i32, g: &Geom) -> (i32, i32) {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let row_shift = if r & 1 == 0 { 0.0 } else { 0.5 };
    let x = g.margin as f32 + horiz_step * (q as f32 + row_shift) + horiz_step / 2.0;
    let y = g.margin as f32 + vert_step * r as f32 + g.hex_size;
    (x.round() as i32, y.round() as i32)
}

fn hex_vertices(cx: i32, cy: i32, size: f32) -> [(i32, i32); 6] {
    let mut out = [(0i32, 0i32); 6];
    for (i, slot) in out.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 30.0);
        let x = cx as f32 + size * angle.cos();
        let y = cy as f32 + size * angle.sin();
        *slot = (x.round() as i32, y.round() as i32);
    }
    out
}

fn draw_hex(img: &mut RgbaImage, cx: i32, cy: i32, size: f32, fill: Rgba<u8>, outline: Rgba<u8>) {
    let pts = hex_vertices(cx, cy, size);
    fill_polygon(img, &pts, fill);
    for i in 0..6 {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % 6];
        draw_line(img, ax, ay, bx, by, outline);
    }
}

mod primitives;
#[cfg(test)]
use primitives::glyph;
pub(crate) use primitives::{
    draw_circle, draw_line_thick, draw_rect_outline, draw_ring, draw_text, fill_circle, fill_rect,
    text_size, GLYPH_H,
};
use primitives::{draw_line, fill_polygon};

// ── Color helpers ───────────────────────────────────────────────────────────

pub(crate) const STAR_LEGEND: &[(&str, &str)] = &[
    ("O", "ORANGE DWARF"),
    ("B", "BLUE-WHITE"),
    ("A", "AMBER"),
    ("F", "FUCHSIA"),
    ("G", "GREEN"),
    ("K", "KHAKI"),
    ("M", "MAROON"),
];

pub(crate) fn star_color(code: &str) -> Rgba<u8> {
    match code.trim().to_ascii_uppercase().as_str() {
        "O" => Rgba([255, 150, 70, 255]),
        "B" => Rgba([180, 210, 255, 255]),
        "A" => Rgba([255, 200, 90, 255]),
        "F" => Rgba([220, 90, 200, 255]),
        "G" => Rgba([110, 210, 130, 255]),
        "K" => Rgba([200, 190, 130, 255]),
        "M" => Rgba([200, 60, 70, 255]),
        _ => Rgba([180, 180, 180, 255]),
    }
}

fn stability_color(s: RouteStability) -> Rgba<u8> {
    match s {
        RouteStability::Stable => Rgba([110, 210, 130, 255]),
        RouteStability::Unstable => Rgba([240, 200, 90, 255]),
        RouteStability::Hazardous => Rgba([235, 90, 90, 255]),
        RouteStability::Perilous => Rgba([165, 100, 215, 255]),
    }
}

pub(crate) fn tint(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let mix = |v: u8, base: u8| {
        let f = f32::from(v) * amount + f32::from(base) * (1.0 - amount);
        f.round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        mix(c.0[0], HEX_EMPTY.0[0]),
        mix(c.0[1], HEX_EMPTY.0[1]),
        mix(c.0[2], HEX_EMPTY.0[2]),
        255,
    ])
}

pub(crate) fn darken(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let scale = (1.0 - amount).clamp(0.0, 1.0);
    Rgba([
        (f32::from(c.0[0]) * scale) as u8,
        (f32::from(c.0[1]) * scale) as u8,
        (f32::from(c.0[2]) * scale) as u8,
        c.0[3],
    ])
}

pub(crate) fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::*;
    use std::collections::BTreeMap;

    fn empty_manifest() -> GenerationManifest {
        GenerationManifest {
            project_id: "p".into(),
            generated_at_policy: "x".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: "abc".into(),
            seed_hash: "h".into(),
            profile: None,
            input_digests: BTreeMap::new(),
            settings_digest: "s".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        }
    }

    fn sample_sector() -> GeneratedSector {
        let sys = GeneratedSystem {
            id: "s1".into(),
            index: 0,
            name: "Test".into(),
            coord: HexCoord { q: 1, r: 1 },
            star: GeneratedStar {
                colour_code: "O".into(),
                colour_name: "orange dwarf".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: Default::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        };
        GeneratedSector {
            id: "demo".into(),
            title: "Demo".into(),
            seed: "abc".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            width: 4,
            height: 4,
            systems: vec![sys],
            routes: vec![],
            factions: vec![],
            manifest: empty_manifest(),
            influence_field: Default::default(),
            power_projection: Default::default(),
        }
    }

    #[test]
    fn renders_without_panicking() {
        let s = sample_sector();
        let img = render(&s, 1, None, RenderOptions::default());
        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }

    #[test]
    fn scaled_render_is_larger() {
        let s = sample_sector();
        let small = render(&s, 1, None, RenderOptions::default());
        let big = render(&s, 4, None, RenderOptions::default());
        assert!(big.width() >= small.width() * 3);
        assert!(big.height() >= small.height() * 3);
    }

    #[test]
    fn glyph_returns_blank_for_space() {
        assert_eq!(glyph(' '), [0; 7]);
    }
}
