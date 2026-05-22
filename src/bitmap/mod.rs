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
use crate::map_theme::{LabelDensity, LegendStyle, MapTheme, RouteLineMode, SymbolSet};
use crate::sector_model::{
    offset_r_neighbors, GeneratedSector, RoutePattern, RouteStability, RouteType,
};
use crate::subsectors::Subsector;

/// Per-render options independent of the project config. Mirrors the relevant
/// bits of [`crate::config::BitmapConfig`] so callers (CLI, GUI export, tests)
/// can override without touching the project's TOML.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Tint each system's hex by the dominant faction (§8).
    pub faction_fill: bool,
    /// Overlay a heatmap tint per system (§10). `Off` disables it.
    pub heatmap: HeatmapMode,
    /// §13 NEW2.md: presentation-only map theme.
    pub theme: MapTheme,
    pub route_view_mode: crate::sector_model::RouteViewMode,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            faction_fill: true,
            heatmap: HeatmapMode::Off,
            theme: MapTheme::gm_dark(),
            route_view_mode: crate::sector_model::RouteViewMode::default(),
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
    fn new(scale: u32, theme: &MapTheme) -> Self {
        let s = scale.max(1) as i32;
        let legend_width = match theme.legend {
            LegendStyle::Hidden => 0,
            LegendStyle::Compact => 220 * s,
            LegendStyle::Full => 280 * s,
        };
        Self {
            scale: s,
            hex_size: 26.0 * s as f32,
            margin: 28 * s,
            legend_width,
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
    let g = Geom::new(scale, &opts.theme);
    let MapBounds { w: map_w, h: map_h } = map_bounds(sector, &g);

    let legend_h = legend_height(sector, &g, &opts);
    let total_w = map_w + g.legend_width;
    let total_h = map_h.max(legend_h);

    let mut img = RgbaImage::from_pixel(total_w as u32, total_h as u32, opts.theme.bg);

    let subs = subsectors.unwrap_or(&[]);
    let draw_subsectors = !subs.is_empty() && opts.theme.show_subsector_borders;

    // Per-system fill tint for §8 (faction colour) / §10 (heatmap overlay).
    let heat = if matches!(opts.heatmap, HeatmapMode::Off) {
        HashMap::new()
    } else {
        heatmap::compute_rgb(sector, opts.heatmap)
    };
    let sys_tints = compute_system_tints(sector, &opts, &heat);

    draw_hex_grid(&mut img, sector, &g, &sys_tints, &opts.theme);
    if draw_subsectors {
        draw_subsector_borders(&mut img, sector, subs, &g, &opts.theme);
    }
    draw_routes(&mut img, sector, &g, &opts);
    draw_region_labels(&mut img, sector, &g, &opts.theme);
    draw_systems(&mut img, sector, subs, &g, &opts);
    if draw_subsectors && !matches!(opts.theme.label_density, LabelDensity::None) {
        draw_subsector_labels(&mut img, sector, subs, &g, &opts.theme);
    }
    draw_system_labels(&mut img, sector, subs, &g, &opts);

    // Legend painted last so any overflow from the map gets clipped behind it.
    if !matches!(opts.theme.legend, LegendStyle::Hidden) {
        fill_rect(
            &mut img,
            map_w,
            0,
            g.legend_width,
            total_h,
            opts.theme.panel_bg,
        );
        draw_legend(&mut img, sector, map_w, &g, &opts);
    }

    img
}

/// One hex tint per (q, r) where the system has a dominant faction (§8) or a
/// heatmap intensity > 0 (§10). Empty hexes stay at the theme's base fill.
fn compute_system_tints(
    sector: &GeneratedSector,
    opts: &RenderOptions,
    heat: &HashMap<crate::ids::SystemId, HeatCellRgb>,
) -> HashMap<(i32, i32), Rgba<u8>> {
    let mut out = HashMap::new();
    for sys in sector.systems.iter() {
        let key = (sys.coord.q, sys.coord.r);
        // Heatmap overrides faction fill for non-Control modes. For Control
        // mode the underlying score already drives `faction_style.fill`, so
        // both paths agree.
        if !matches!(opts.heatmap, HeatmapMode::Off) {
            if let Some(cell) = heat.get(&sys.id) {
                let strength =
                    opts.theme.heatmap_tint_min + cell.intensity * opts.theme.heatmap_tint_range;
                let color = rgba(cell.rgb);
                out.insert(key, tint_against(color, strength, opts.theme.hex_empty));
                continue;
            }
        }
        if opts.faction_fill {
            if let Some(dom) = sys.control.dominant.as_deref() {
                let style = faction_style_rgb_by_id(&sector.factions, dom);
                out.insert(
                    key,
                    tint_against(
                        rgba(style.fill),
                        opts.theme.faction_tint_strength,
                        opts.theme.hex_empty,
                    ),
                );
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
    theme: &MapTheme,
) {
    // §5 NEW.md: region tints underneath the system tint so the overlay reads
    // as background colour rather than overwriting faction fill.
    let region_tints = compute_region_tints(sector, theme);
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r, g);
            let base = region_tints
                .get(&(q, r))
                .copied()
                .unwrap_or(theme.hex_empty);
            let fill = sys_tints.get(&(q, r)).copied().unwrap_or(base);
            draw_hex(img, cx, cy, g.hex_size, fill, theme.hex_outline);
        }
    }
}

fn compute_region_tints(
    sector: &GeneratedSector,
    theme: &MapTheme,
) -> HashMap<(i32, i32), Rgba<u8>> {
    use crate::regions::RegionConditionKind;
    let mut out = HashMap::new();
    for region in sector.regions.iter() {
        let base = match region.kind {
            RegionConditionKind::WarpStorm => Rgba([120, 60, 180, 255]),
            RegionConditionKind::Turbulence => Rgba([110, 100, 160, 255]),
            RegionConditionKind::CalmCorridor => Rgba([80, 160, 170, 255]),
            RegionConditionKind::Blackout => Rgba([60, 60, 70, 255]),
            RegionConditionKind::Anomaly => Rgba([180, 130, 100, 255]),
        };
        let tinted = tint_against(base, theme.region_tint_strength, theme.hex_empty);
        for h in &region.hexes {
            out.insert((h.q, h.r), tinted);
        }
    }
    out
}

fn draw_routes(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom, opts: &RenderOptions) {
    let mut centers: HashMap<&str, (i32, i32)> = HashMap::new();
    for sys in sector.systems.iter() {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        centers.insert(sys.id.as_str(), (cx, cy));
    }
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
        let color = stability_color(&opts.theme, route.stability);
        let thickness = route_thickness(&opts.theme, route.stability, g);
        draw_route_line_thick(RouteLineParams {
            img,
            x0: sx,
            y0: sy,
            x1: ex,
            y1: ey,
            color,
            thickness,
            pattern: route.pattern_with_salt(&sector.seed, opts.route_view_mode),
        });
        draw_route_control_glyph(img, sector, route, (sx, sy), (ex, ey), thickness, &opts.theme);
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
    theme: &MapTheme,
) {
    let Some((faction_id, kind, score)) = top_route_control(route) else {
        return;
    };
    if score < 40.0 {
        return;
    }
    let style = faction_style_rgb_by_id(&sector.factions, &faction_id);
    let color = if matches!(theme.symbol_set, SymbolSet::Redacted) {
        theme.route_control_neutral
    } else {
        Rgba([style.fill.0, style.fill.1, style.fill.2, 255])
    };
    let dark = darken(color, 0.5);
    let mx = (a.0 + b.0) / 2;
    let my = (a.1 + b.1) / 2;
    let size = (thickness * 3).max(6);
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        let half = size;
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
        return;
    }
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

/// Draws a route styled by `pattern`. Motif-heavy patterns use rails, ladders,
/// ticks, chevrons, bursts, and triangles so the PNG map stays visually close
/// to the live GUI.
pub struct RouteLineParams<'a> {
    pub img: &'a mut RgbaImage,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub color: Rgba<u8>,
    pub thickness: i32,
    pub pattern: RoutePattern,
}

pub(crate) fn draw_route_line_thick(params: RouteLineParams) {
    let RouteLineParams {
        img,
        x0,
        y0,
        x1,
        y1,
        color,
        thickness,
        pattern,
    } = params;
    let Some(geom) = BitmapRouteGeom::new(x0, y0, x1, y1, thickness) else {
        return;
    };
    match pattern {
        RoutePattern::Solid => draw_line_thick(img, x0, y0, x1, y1, color, thickness),
        RoutePattern::Dashed | RoutePattern::DotDash | RoutePattern::Dotted => {
            draw_bitmap_strided_route(img, geom, color, thickness, pattern.strides());
        }
        RoutePattern::Cracked => {
            draw_bitmap_jagged_route(
                img,
                geom,
                color,
                thickness,
                geom.unit * 3.0,
                thickness as f32 * 1.7,
            );
        }
        RoutePattern::Ghost => {
            draw_bitmap_strided_route(
                img,
                geom,
                dim_rgba(color, 0.62),
                thickness,
                pattern.strides(),
            );
        }
        RoutePattern::Burst => {
            draw_bitmap_bursts(img, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Staccato => {
            draw_bitmap_zigzag_route(
                img,
                geom,
                color,
                thickness,
                geom.unit * 3.2,
                thickness as f32 * 1.8,
            );
        }
        RoutePattern::Gravel => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 1.55, false, true);
        }
        RoutePattern::Twin => {
            draw_bitmap_parallel_routes(img, geom, color, thickness, thickness as f32);
        }
        RoutePattern::Tripod => {
            draw_bitmap_tripods(img, geom, color, thickness, geom.unit * 5.0);
        }
        RoutePattern::Tick => {
            draw_bitmap_base_spine(img, geom, color, thickness, 0.28);
            draw_bitmap_ticks(
                img,
                geom,
                color,
                thickness,
                geom.unit * 4.5,
                thickness as f32 * 2.2,
            );
        }
        RoutePattern::Bridge => {
            draw_bitmap_strided_route(
                img,
                geom,
                dim_rgba(color, 0.72),
                stroke_px(thickness, 0.8),
                &[3.0, 2.0],
            );
            draw_bitmap_ticks(
                img,
                geom,
                color,
                thickness,
                geom.unit * 5.0,
                thickness as f32 * 1.8,
            );
        }
        RoutePattern::Patter => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 2.2, true, false);
        }
        RoutePattern::Quartet => {
            draw_bitmap_dot_clusters(img, geom, color, thickness, geom.unit * 8.0, 4);
        }
        RoutePattern::Railroad => {
            let offset = thickness as f32 * 1.25;
            draw_bitmap_parallel_routes(img, geom, color, stroke_px(thickness, 0.8), offset);
            draw_bitmap_ticks(
                img,
                geom,
                color,
                stroke_px(thickness, 0.75),
                geom.unit * 5.5,
                offset * 1.15,
            );
        }
        RoutePattern::DoubleTap => {
            draw_bitmap_double_taps(img, geom, color, thickness, geom.unit * 7.0);
        }
        RoutePattern::Pebble => {
            draw_bitmap_disc_trail(img, geom, color, thickness, geom.unit * 2.6, false, true);
        }
        RoutePattern::Whisper => {
            draw_bitmap_disc_trail(
                img,
                geom,
                dim_rgba(color, 0.78),
                thickness,
                geom.unit * 7.0,
                false,
                false,
            );
        }
        RoutePattern::March => {
            draw_bitmap_chevrons(img, geom, color, thickness, geom.unit * 5.5);
        }
    }
}

#[derive(Clone, Copy)]
struct BitmapRouteGeom {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    ux: f32,
    uy: f32,
    nx: f32,
    ny: f32,
    total: f32,
    unit: f32,
}

impl BitmapRouteGeom {
    fn new(x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32) -> Option<Self> {
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let total = (dx * dx + dy * dy).sqrt();
        if total <= 0.0 {
            return None;
        }
        let ux = dx / total;
        let uy = dy / total;
        Some(Self {
            x0,
            y0,
            x1,
            y1,
            ux,
            uy,
            nx: -uy,
            ny: ux,
            total,
            unit: (thickness as f32).max(2.0),
        })
    }

    fn at(self, t: f32, offset: f32) -> (i32, i32) {
        let t = t.clamp(0.0, self.total);
        (
            (self.x0 as f32 + self.ux * t + self.nx * offset).round() as i32,
            (self.y0 as f32 + self.uy * t + self.ny * offset).round() as i32,
        )
    }
}

fn draw_bitmap_strided_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    strides: &[f32],
) {
    if strides.is_empty() {
        draw_line_thick(img, geom.x0, geom.y0, geom.x1, geom.y1, color, thickness);
        return;
    }
    let mut t = 0.0_f32;
    let mut idx: usize = 0;
    while t < geom.total {
        let stride = strides[idx % strides.len()];
        let seg = stride * geom.unit;
        let next_t = (t + seg).min(geom.total);
        if idx.is_multiple_of(2) {
            let (sx, sy) = geom.at(t, 0.0);
            let (ex, ey) = geom.at(next_t, 0.0);
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

fn draw_bitmap_parallel_routes(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    offset: f32,
) {
    for side in [-offset, offset] {
        let (sx, sy) = geom.at(0.0, side);
        let (ex, ey) = geom.at(geom.total, side);
        draw_line_thick(img, sx, sy, ex, ey, color, thickness);
    }
}

fn draw_bitmap_base_spine(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    dim: f32,
) {
    draw_line_thick(
        img,
        geom.x0,
        geom.y0,
        geom.x1,
        geom.y1,
        dim_rgba(color, dim),
        stroke_px(thickness, 0.7),
    );
}

fn draw_bitmap_ticks(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    half_len: f32,
) {
    let mut t = spacing * 0.5;
    while t < geom.total {
        let (mx, my) = geom.at(t, 0.0);
        let sx = (mx as f32 - geom.nx * half_len).round() as i32;
        let sy = (my as f32 - geom.ny * half_len).round() as i32;
        let ex = (mx as f32 + geom.nx * half_len).round() as i32;
        let ey = (my as f32 + geom.ny * half_len).round() as i32;
        draw_line_thick(img, sx, sy, ex, ey, color, stroke_px(thickness, 0.75));
        t += spacing;
    }
}

fn draw_bitmap_jagged_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = (geom.x0, geom.y0);
    let mut t = spacing;
    let mut sign = 1.0;
    while t < geom.total {
        let next = geom.at(t, amplitude * sign);
        draw_line_thick(img, prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        t += spacing;
        sign = -sign;
    }
    draw_line_thick(img, prev.0, prev.1, geom.x1, geom.y1, color, thickness);
}

fn draw_bitmap_zigzag_route(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    amplitude: f32,
) {
    let mut prev = geom.at(0.0, -amplitude);
    let mut t = spacing * 0.5;
    let mut sign = 1.0;
    while t < geom.total {
        let next = geom.at(t, amplitude * sign);
        draw_line_thick(img, prev.0, prev.1, next.0, next.1, color, thickness);
        prev = next;
        t += spacing;
        sign = -sign;
    }
    let end = geom.at(geom.total, -amplitude * sign);
    draw_line_thick(img, prev.0, prev.1, end.0, end.1, color, thickness);
}

fn draw_bitmap_disc_trail(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    hollow: bool,
    alternating: bool,
) {
    let mut t = spacing * 0.5;
    let mut i = 0usize;
    while t < geom.total {
        let radius = if alternating && i.is_multiple_of(2) {
            thickness as f32 * 0.85
        } else {
            thickness as f32 * 0.55
        }
        .round() as i32;
        let (mx, my) = geom.at(t, 0.0);
        if hollow {
            draw_circle(img, mx, my, radius.max(1), color);
        } else {
            fill_circle(img, mx, my, radius.max(1), color);
        }
        t += spacing;
        i += 1;
    }
}

fn draw_bitmap_dot_clusters(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
    count: usize,
) {
    let dot_gap = geom.unit * 1.25;
    let radius = ((thickness as f32) * 0.55).round() as i32;
    let mut t = spacing * 0.5;
    while t < geom.total {
        let center = (count as f32 - 1.0) * 0.5;
        for i in 0..count {
            let local = (i as f32 - center) * dot_gap;
            let (mx, my) = geom.at(t + local, 0.0);
            fill_circle(img, mx, my, radius.max(1), color);
        }
        t += spacing;
    }
}

fn draw_bitmap_double_taps(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let pair_gap = geom.unit * 1.3;
    let half_len = thickness as f32 * 1.8;
    let mut t = spacing * 0.5;
    while t < geom.total {
        for local in [-pair_gap * 0.5, pair_gap * 0.5] {
            let (mx, my) = geom.at(t + local, 0.0);
            let sx = (mx as f32 - geom.nx * half_len).round() as i32;
            let sy = (my as f32 - geom.ny * half_len).round() as i32;
            let ex = (mx as f32 + geom.nx * half_len).round() as i32;
            let ey = (my as f32 + geom.ny * half_len).round() as i32;
            draw_line_thick(img, sx, sy, ex, ey, color, stroke_px(thickness, 0.8));
        }
        t += spacing;
    }
}

fn draw_bitmap_chevrons(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let size = geom.unit * 1.8;
    let mut t = spacing * 0.5;
    while t < geom.total {
        let tip = geom.at(t + size * 0.35, 0.0);
        let back = geom.at(t - size * 0.35, 0.0);
        let left = (
            (back.0 as f32 + geom.nx * size * 0.35).round() as i32,
            (back.1 as f32 + geom.ny * size * 0.35).round() as i32,
        );
        let right = (
            (back.0 as f32 - geom.nx * size * 0.35).round() as i32,
            (back.1 as f32 - geom.ny * size * 0.35).round() as i32,
        );
        draw_line_thick(img, left.0, left.1, tip.0, tip.1, color, thickness);
        draw_line_thick(img, right.0, right.1, tip.0, tip.1, color, thickness);
        t += spacing;
    }
}

fn draw_bitmap_tripods(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let size = (geom.unit * 1.8).max(thickness as f32 * 2.5);
    let mut t = spacing * 0.5;
    while t < geom.total {
        let mid = geom.at(t, 0.0);
        let mid_p = (mid.0 as f32, mid.1 as f32);

        // Forward leg
        let fwd = geom.at(t + size * 0.4, 0.0);
        draw_line_thick(img, mid.0, mid.1, fwd.0, fwd.1, color, thickness);

        // Lateral legs
        let l_pos = (
            (mid_p.0 + geom.nx * size * 0.4).round() as i32,
            (mid_p.1 + geom.ny * size * 0.4).round() as i32,
        );
        let r_pos = (
            (mid_p.0 - geom.nx * size * 0.4).round() as i32,
            (mid_p.1 - geom.ny * size * 0.4).round() as i32,
        );
        draw_line_thick(img, mid.0, mid.1, l_pos.0, l_pos.1, color, thickness);
        draw_line_thick(img, mid.0, mid.1, r_pos.0, r_pos.1, color, thickness);

        t += spacing;
    }
}

fn draw_bitmap_bursts(
    img: &mut RgbaImage,
    geom: BitmapRouteGeom,
    color: Rgba<u8>,
    thickness: i32,
    spacing: f32,
) {
    let radius = (thickness as f32 * 1.6).max(2.0);
    let mut t = spacing * 0.5;
    while t < geom.total {
        let mid = geom.at(t, 0.0);
        let a = geom.at(t - radius, 0.0);
        let b = geom.at(t + radius, 0.0);
        let c = (
            (mid.0 as f32 - geom.nx * radius).round() as i32,
            (mid.1 as f32 - geom.ny * radius).round() as i32,
        );
        let d = (
            (mid.0 as f32 + geom.nx * radius).round() as i32,
            (mid.1 as f32 + geom.ny * radius).round() as i32,
        );
        draw_line_thick(img, a.0, a.1, b.0, b.1, color, stroke_px(thickness, 0.65));
        draw_line_thick(img, c.0, c.1, d.0, d.1, color, stroke_px(thickness, 0.65));
        fill_circle(img, mid.0, mid.1, stroke_px(thickness, 0.45), color);
        t += spacing;
    }
}

fn stroke_px(thickness: i32, factor: f32) -> i32 {
    ((thickness as f32) * factor).round().max(1.0) as i32
}

fn dim_rgba(color: Rgba<u8>, factor: f32) -> Rgba<u8> {
    Rgba([
        ((color.0[0] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((color.0[1] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        ((color.0[2] as f32) * factor).round().clamp(0.0, 255.0) as u8,
        color.0[3],
    ])
}

fn draw_systems(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    opts: &RenderOptions,
) {
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
            draw_capital_marker(img, cx, cy, g.hex_size, &opts.theme);
        }

        // World-count pip on the lower-right of the hex.
        let pip = sector.get_worlds_for_system(sys).len();
        if pip > 0 {
            let label = format!("{pip}");
            let (tw, th) = text_size(&label, pip_scale);
            let tx = cx + (g.hex_size * 0.55) as i32 - tw;
            let ty = cy + (g.hex_size * 0.55) as i32 - th;
            draw_text(img, tx, ty, &label, opts.theme.text, pip_scale);
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

fn region_label_scale(g: &Geom) -> i32 {
    (((g.hex_size * 0.31) / GLYPH_H as f32).round() as i32).max(1)
}

fn draw_region_labels(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom, theme: &MapTheme) {
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

fn region_label_anchor(region: &crate::regions::WarpRegion, g: &Geom) -> Option<(i32, i32)> {
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

fn region_label_text(name: &str) -> String {
    short(&name.to_ascii_uppercase(), 18)
}

fn draw_system_labels(
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
        if !system_label_visible(sys, subsectors, &opts.theme, sector) {
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

fn system_label_visible(
    sys: &crate::sector_model::GeneratedSystem,
    subsectors: &[Subsector],
    theme: &MapTheme,
    sector: &GeneratedSector,
) -> bool {
    match theme.label_density {
        LabelDensity::All => true,
        LabelDensity::None => false,
        LabelDensity::ImportantOnly => {
            sector.get_worlds_for_system(sys).len() >= 4
                || !sys.primary_factions.is_empty()
                || subsectors.iter().any(|s| {
                    s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str())
                })
        }
    }
}

fn draw_capital_marker(img: &mut RgbaImage, cx: i32, cy: i32, hex_size: f32, theme: &MapTheme) {
    let r = ((hex_size * 0.15).max(3.5)).round() as i32;
    let dy = -((hex_size * 0.55).round() as i32);
    let center_y = cy + dy;
    if matches!(theme.symbol_set, SymbolSet::Redacted) {
        fill_rect(
            img,
            cx - r,
            center_y - r / 2,
            r * 2,
            r,
            theme.capital_marker,
        );
        draw_rect_outline(
            img,
            cx - r,
            center_y - r / 2,
            r * 2,
            r,
            theme.capital_outline,
        );
        return;
    }
    if matches!(theme.symbol_set, SymbolSet::Tactical) {
        draw_line_thick(
            img,
            cx - r,
            center_y,
            cx + r,
            center_y,
            theme.capital_marker,
            2,
        );
        draw_line_thick(
            img,
            cx,
            center_y - r,
            cx,
            center_y + r,
            theme.capital_marker,
            2,
        );
        draw_circle(img, cx, center_y, r, theme.capital_outline);
        return;
    }
    let pts = [
        (cx, center_y - r),
        (cx + r, center_y),
        (cx, center_y + r),
        (cx - r, center_y),
    ];
    fill_polygon(img, &pts, theme.capital_marker);
    for i in 0..4 {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % 4];
        draw_line(img, ax, ay, bx, by, theme.capital_outline);
    }
}

fn draw_subsector_borders(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    subsectors: &[Subsector],
    g: &Geom,
    theme: &MapTheme,
) {
    let mut owner: HashMap<(i32, i32), &str> = HashMap::new();
    for s in subsectors {
        for &(q, r) in &s.hex_cells {
            owner.insert((q as i32, r as i32), s.id.as_ref());
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
                    fill_circle(img, mx, my, dot_radius, theme.subsector_border);
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

fn legend_height(sector: &GeneratedSector, g: &Geom, opts: &RenderOptions) -> i32 {
    if matches!(opts.theme.legend, LegendStyle::Hidden) {
        return 0;
    }
    if matches!(opts.theme.legend, LegendStyle::Compact) {
        let heatmap_lines = if matches!(opts.heatmap, HeatmapMode::Off) {
            0
        } else {
            2
        };
        let lines = 4 + 1 + 5 + 1 + factions_visible(sector) + heatmap_lines;
        return g.legend_pad * 2 + lines as i32 * g.line_h;
    }
    // title block (4) + spacer
    // + ROUTE TYPE header + N type rows + spacer
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
    let route_type_rows = match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => RouteType::ALL.len() as i32,
        crate::sector_model::RouteViewMode::TopLevel => {
            crate::sector_model::RouteKind::ALL.len() as i32
        }
    };
    let lines = 4
        + 1
        + route_type_rows
        + 1
        + 1
        + 4
        + 1
        + route_control_lines
        + 1
        + factions_visible(sector) as i32
        + heatmap_lines;
    g.legend_pad * 2 + lines * g.line_h
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
    opts: &RenderOptions,
) {
    let x0 = map_w + g.legend_pad;
    let mut y = g.legend_pad;
    let line_h = g.line_h;
    let body = g.text_scale;
    let title = g.title_scale;
    let swatch = 12 * g.scale;

    let title_text = format!("SECTOR: {}", sector.id.to_uppercase());
    draw_text(img, x0, y, &title_text, opts.theme.text, title);
    y += line_h + 4 * g.scale;
    draw_text(
        img,
        x0,
        y,
        &format!("SEED: {}", short(&sector.seed, 20)),
        opts.theme.text_dim,
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
            sector.all_worlds().count(),
        ),
        opts.theme.text_dim,
        body,
    );
    y += line_h + 4 * g.scale;

    draw_text(
        img,
        x0,
        y,
        &format!("THEME: {}", short(&opts.theme.name.to_uppercase(), 20)),
        opts.theme.text_dim,
        body,
    );
    y += line_h;

    if matches!(opts.theme.legend, LegendStyle::Compact) {
        y += 4 * g.scale;
        draw_compact_legend_body(img, sector, x0, y, g, opts);
        return;
    }

    draw_text(img, x0, y, "ROUTE TYPE", opts.theme.text, body);
    y += line_h;
    match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => {
            for rtype in RouteType::ALL {
                draw_route_line_thick(RouteLineParams {
                    img,
                    x0,
                    y0: y + 8 * g.scale,
                    x1: x0 + 30 * g.scale,
                    y1: y + 8 * g.scale,
                    color: opts.theme.route_type,
                    thickness: 3 * g.scale,
                    pattern: rtype.pattern(opts.route_view_mode),
                });
                draw_text(img, x0 + 38 * g.scale, y, rtype.label(), opts.theme.text, body);
                y += line_h;
            }
        }
        crate::sector_model::RouteViewMode::TopLevel => {
            for kind in crate::sector_model::RouteKind::ALL {
                draw_route_line_thick(RouteLineParams {
                    img,
                    x0,
                    y0: y + 8 * g.scale,
                    x1: x0 + 30 * g.scale,
                    y1: y + 8 * g.scale,
                    color: opts.theme.route_type,
                    thickness: 3 * g.scale,
                    pattern: kind.patterns()[0],
                });
                draw_text(img, x0 + 38 * g.scale, y, kind.label(), opts.theme.text, body);
                y += line_h;
            }
        }
    }
    y += 4 * g.scale;

    draw_text(img, x0, y, "ROUTE STABILITY", opts.theme.text, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Perilous, "PERILOUS"),
    ] {
        let color = stability_color(&opts.theme, stab);
        draw_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 22 * g.scale,
            y + 8 * g.scale,
            color,
            3 * g.scale,
        );
        draw_text(img, x0 + 30 * g.scale, y, name, opts.theme.text, body);
        y += line_h;
    }
    y += 4 * g.scale;

    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        draw_text(img, x0, y, "ROUTE CONTROL", opts.theme.text, body);
        y += line_h;
        let glyph_cx = x0 + 8 * g.scale;
        let glyph_size = 10 * g.scale;
        let half = glyph_size / 2;
        let neutral = opts.theme.route_control_neutral;
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
            draw_text(img, x0 + 22 * g.scale, y, name, opts.theme.text, body);
            y += line_h;
        }
        y += 4 * g.scale;
    }

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", opts.theme.text, body);
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
                opts.theme.text_dim,
                body,
            );
            y += line_h;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4 * g.scale;
        draw_text(img, x0, y, "HEATMAP", opts.theme.text, body);
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
            opts.theme.text,
            body,
        );
    }
}

fn draw_compact_legend_body(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    x0: i32,
    mut y: i32,
    g: &Geom,
    opts: &RenderOptions,
) {
    let body = g.text_scale;
    let line_h = g.line_h;
    let swatch = 12 * g.scale;

    draw_text(img, x0, y, "ROUTES", opts.theme.text, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARD"),
        (RouteStability::Perilous, "PERIL"),
    ] {
        let color = stability_color(&opts.theme, stab);
        draw_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 22 * g.scale,
            y + 8 * g.scale,
            color,
            route_thickness(&opts.theme, stab, g),
        );
        draw_text(img, x0 + 30 * g.scale, y, name, opts.theme.text, body);
        y += line_h;
    }
    y += 4 * g.scale;

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", opts.theme.text, body);
        y += line_h;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            FACTION_MINOR_FRACTION,
            FACTION_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    ..
                } => (label.to_uppercase(), *system_count, (140, 140, 150)),
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
                &format!("{} ({} SYS)", short(&label, 14), sys_n),
                opts.theme.text_dim,
                body,
            );
            y += line_h;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4 * g.scale;
        draw_text(img, x0, y, "HEATMAP", opts.theme.text, body);
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
            opts.theme.text,
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

fn stability_color(theme: &MapTheme, s: RouteStability) -> Rgba<u8> {
    match s {
        RouteStability::Stable => theme.route_stable,
        RouteStability::Unstable => theme.route_unstable,
        RouteStability::Hazardous => theme.route_hazardous,
        RouteStability::Perilous => theme.route_perilous,
    }
}

pub(crate) fn tint_against(c: Rgba<u8>, amount: f32, base: Rgba<u8>) -> Rgba<u8> {
    let mix = |v: u8, base: u8| {
        let f = f32::from(v) * amount + f32::from(base) * (1.0 - amount);
        f.round().clamp(0.0, 255.0) as u8
    };
    Rgba([
        mix(c.0[0], base.0[0]),
        mix(c.0[1], base.0[1]),
        mix(c.0[2], base.0[2]),
        255,
    ])
}

fn route_thickness(theme: &MapTheme, stability: RouteStability, g: &Geom) -> i32 {
    let base = (g.hex_size * 0.08).max(2.0) * theme.route_thickness;
    let mode = if matches!(theme.route_line_mode, RouteLineMode::HazardWeighted) {
        match stability {
            RouteStability::Stable => 0.9,
            RouteStability::Unstable => 1.05,
            RouteStability::Hazardous => 1.25,
            RouteStability::Perilous => 1.45,
        }
    } else {
        1.0
    };
    (base * mode).round().max(1.0) as i32
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
    use crate::regions::{RegionConditionKind, WarpRegion};
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
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
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
