//! Bitmap export: renders the sector as a PNG hex map with a legend.
//!
//! Pure pixel work — no external font files. A small 5x7 monospace bitmap
//! font is embedded below. Hex layout is pointy-top with odd-r offset rows,
//! matching the ASCII map in `export.rs`.
//!
//! All sizes are derived from a `Geom` struct built from an integer scale
//! factor so the same renderer can produce a small thumbnail or a ~4K poster.

use std::collections::HashMap;

use camino::Utf8Path;
use image::{Rgba, RgbaImage};

use crate::errors::SectorError;
use crate::sector_model::{GeneratedSector, RouteStability};

// ── Shared palette ──────────────────────────────────────────────────────────

pub(crate) const BG: Rgba<u8> = Rgba([14, 12, 20, 255]);
pub(crate) const PANEL_BG: Rgba<u8> = Rgba([22, 18, 30, 255]);
pub(crate) const HEX_EMPTY: Rgba<u8> = Rgba([28, 26, 38, 255]);
pub(crate) const HEX_OUTLINE: Rgba<u8> = Rgba([60, 55, 78, 255]);
pub(crate) const TEXT: Rgba<u8> = Rgba([232, 228, 240, 255]);
pub(crate) const TEXT_DIM: Rgba<u8> = Rgba([150, 145, 165, 255]);
pub(crate) const ROUTE_DIM: Rgba<u8> = Rgba([90, 88, 110, 255]);

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
) -> Result<(), SectorError> {
    write_image(sector, output_dir, "sector.png", scale)
}

pub fn write_bmp(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    scale: u32,
) -> Result<(), SectorError> {
    write_image(sector, output_dir, "sector.bmp", scale)
}

fn write_image(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    filename: &str,
    scale: u32,
) -> Result<(), SectorError> {
    let path = output_dir.join(filename);
    let img = render(sector, scale);
    img.save(path.as_std_path())
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    Ok(())
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn render(sector: &GeneratedSector, scale: u32) -> RgbaImage {
    let g = Geom::new(scale);
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;

    let map_w =
        (g.margin as f32 * 2.0 + sector.width as f32 * horiz_step + horiz_step / 2.0) as i32;
    let map_h = (g.margin as f32 * 2.0
        + (sector.height.saturating_sub(1)) as f32 * vert_step
        + 2.0 * g.hex_size) as i32;

    let legend_h = legend_height(sector, &g);
    let total_w = map_w + g.legend_width;
    let total_h = map_h.max(legend_h);

    let mut img = RgbaImage::from_pixel(total_w as u32, total_h as u32, BG);

    draw_hex_grid(&mut img, sector, &g);
    draw_routes(&mut img, sector, &g);
    draw_systems(&mut img, sector, &g);

    // Legend painted last so any overflow from the map gets clipped behind it.
    fill_rect(&mut img, map_w, 0, g.legend_width, total_h, PANEL_BG);
    draw_legend(&mut img, sector, map_w, &g);

    img
}

fn draw_hex_grid(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom) {
    for r in 0..sector.height as i32 {
        for q in 0..sector.width as i32 {
            let (cx, cy) = hex_center(q, r, g);
            draw_hex(img, cx, cy, g.hex_size, HEX_EMPTY, HEX_OUTLINE);
        }
    }
}

fn draw_routes(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom) {
    let mut centers: HashMap<&str, (i32, i32)> = HashMap::new();
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        centers.insert(sys.id.as_str(), (cx, cy));
    }
    let thickness = 2 * g.scale;
    for route in &sector.routes {
        let (Some(&a), Some(&b)) = (
            centers.get(route.from_system_id.as_str()),
            centers.get(route.to_system_id.as_str()),
        ) else {
            continue;
        };
        let color = stability_color(route.stability);
        draw_line_thick(img, a.0, a.1, b.0, b.1, color, thickness);
    }
}

fn draw_systems(img: &mut RgbaImage, sector: &GeneratedSector, g: &Geom) {
    for sys in &sector.systems {
        let (cx, cy) = hex_center(sys.coord.q, sys.coord.r, g);
        let fill = star_color(&sys.star.colour_code);
        // Slightly tinted hex fill behind the star.
        draw_hex(img, cx, cy, g.hex_size, tint(fill, 0.18), HEX_OUTLINE);
        // Star disk.
        fill_circle(img, cx, cy, (g.hex_size * 0.42) as i32, fill);
        // Outline ring.
        draw_circle(img, cx, cy, (g.hex_size * 0.42) as i32, darken(fill, 0.55));
        // Small world-count pip on the lower-right.
        let pip = sys.worlds.len().min(99);
        if pip > 0 {
            let label = format!("{}", pip);
            let scale = g.text_scale;
            let (tw, th) = text_size(&label, scale);
            let tx = cx + (g.hex_size * 0.55) as i32 - tw;
            let ty = cy + (g.hex_size * 0.55) as i32 - th;
            draw_text(img, tx, ty, &label, TEXT, scale);
        }
        // Short system label under the hex (numeric tail only, e.g. "0017").
        let label = short_system_label(&sys.id);
        let scale = g.text_scale;
        let (tw, _) = text_size(&label, scale);
        let tx = cx - tw / 2;
        let ty = cy + g.hex_size as i32 + 2 * g.scale;
        draw_text(img, tx, ty, &label, TEXT_DIM, scale);
    }
}

fn legend_height(sector: &GeneratedSector, g: &Geom) -> i32 {
    // Header (3 lines) + spacer + 7 star rows + spacer + header + 4 stab rows
    // + spacer + 1 footer line, with vertical pad on each side.
    let lines = 3 + 1 + 7 + 1 + 1 + 4 + 1 + 1 + factions_visible(sector);
    g.legend_pad * 2 + lines as i32 * g.line_h
}

fn factions_visible(sector: &GeneratedSector) -> usize {
    // Show up to 6 factions in legend (+1 header line if any).
    if sector.factions.is_empty() {
        0
    } else {
        1 + sector.factions.len().min(6)
    }
}

fn draw_legend(img: &mut RgbaImage, sector: &GeneratedSector, map_w: i32, g: &Geom) {
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
            &format!("{} - {}", code, name),
            TEXT,
            body,
        );
        y += line_h;
    }
    y += 4 * g.scale;

    draw_text(img, x0, y, "ROUTE STABILITY", TEXT, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Lost, "LOST"),
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

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", TEXT, body);
        y += line_h;
        for f in sector.factions.iter().take(6) {
            draw_text(
                img,
                x0,
                y,
                &format!(
                    "{} ({} SYS, {} WORLDS)",
                    short(&f.name.to_uppercase(), 18),
                    f.system_presence.len(),
                    f.world_presence.len(),
                ),
                TEXT_DIM,
                body,
            );
            y += line_h;
        }
    }
}

// ── Hex geometry ────────────────────────────────────────────────────────────

fn hex_center(q: i32, r: i32, g: &Geom) -> (i32, i32) {
    let horiz_step = g.hex_size * 3f32.sqrt();
    let vert_step = g.hex_size * 1.5;
    let x = g.margin as f32 + horiz_step * (q as f32 + 0.5 * r as f32) + horiz_step / 2.0;
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

// ── Drawing primitives (shared with system_map) ─────────────────────────────

pub(crate) fn put_pixel(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (w, h) = (img.width() as i32, img.height() as i32);
    if x >= w || y >= h {
        return;
    }
    img.put_pixel(x as u32, y as u32, color);
}

pub(crate) fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    for yy in y..y + h {
        for xx in x..x + w {
            put_pixel(img, xx, yy, color);
        }
    }
}

pub(crate) fn draw_rect_outline(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: Rgba<u8>,
) {
    draw_line(img, x, y, x + w - 1, y, color);
    draw_line(img, x, y + h - 1, x + w - 1, y + h - 1, color);
    draw_line(img, x, y, x, y + h - 1, color);
    draw_line(img, x + w - 1, y, x + w - 1, y + h - 1, color);
}

pub(crate) fn draw_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        put_pixel(img, x, y, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

pub(crate) fn draw_line_thick(
    img: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgba<u8>,
    thickness: i32,
) {
    let half = thickness / 2;
    for ox in -half..=half {
        for oy in -half..=half {
            draw_line(img, x0 + ox, y0 + oy, x1 + ox, y1 + oy, color);
        }
    }
}

pub(crate) fn fill_circle(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    let r2 = radius * radius;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                put_pixel(img, cx + dx, cy + dy, color);
            }
        }
    }
}

pub(crate) fn draw_circle(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    let r2 = radius * radius;
    let r2_in = (radius - 1).max(0) * (radius - 1).max(0);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d = dx * dx + dy * dy;
            if d <= r2 && d >= r2_in {
                put_pixel(img, cx + dx, cy + dy, color);
            }
        }
    }
}

/// Draw a single-pixel circle stroke of the given thickness.
pub(crate) fn draw_ring(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    color: Rgba<u8>,
) {
    let outer = radius + thickness / 2;
    let inner = (radius - (thickness - thickness / 2)).max(0);
    let outer2 = outer * outer;
    let inner2 = inner * inner;
    for dy in -outer..=outer {
        for dx in -outer..=outer {
            let d = dx * dx + dy * dy;
            if d <= outer2 && d >= inner2 {
                put_pixel(img, cx + dx, cy + dy, color);
            }
        }
    }
}

fn fill_polygon(img: &mut RgbaImage, pts: &[(i32, i32)], color: Rgba<u8>) {
    if pts.is_empty() {
        return;
    }
    let ymin = pts.iter().map(|p| p.1).min().unwrap();
    let ymax = pts.iter().map(|p| p.1).max().unwrap();
    for y in ymin..=ymax {
        let mut xs: Vec<i32> = Vec::with_capacity(pts.len());
        for i in 0..pts.len() {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % pts.len()];
            if (ay <= y && by > y) || (by <= y && ay > y) {
                let t = (y - ay) as f32 / (by - ay) as f32;
                let x = ax as f32 + t * (bx - ax) as f32;
                xs.push(x.round() as i32);
            }
        }
        xs.sort_unstable();
        let mut i = 0;
        while i + 1 < xs.len() {
            for x in xs[i]..=xs[i + 1] {
                put_pixel(img, x, y, color);
            }
            i += 2;
        }
    }
}

// ── Text rendering (embedded 5x7 monospace font) ────────────────────────────

pub(crate) const GLYPH_W: i32 = 5;
pub(crate) const GLYPH_H: i32 = 7;
pub(crate) const GLYPH_SPACE: i32 = 1;

pub(crate) fn text_size(s: &str, scale: i32) -> (i32, i32) {
    let n = s.chars().count() as i32;
    let w = n * (GLYPH_W + GLYPH_SPACE) * scale - GLYPH_SPACE * scale;
    (w.max(0), GLYPH_H * scale)
}

pub(crate) fn draw_text(img: &mut RgbaImage, x: i32, y: i32, s: &str, color: Rgba<u8>, scale: i32) {
    let mut cx = x;
    for c in s.chars() {
        draw_glyph(img, cx, y, c, color, scale);
        cx += (GLYPH_W + GLYPH_SPACE) * scale;
    }
}

fn draw_glyph(img: &mut RgbaImage, x: i32, y: i32, c: char, color: Rgba<u8>, scale: i32) {
    let rows = glyph(c);
    for (row, bits) in rows.iter().enumerate() {
        for col in 0..GLYPH_W {
            let mask = 1u8 << (GLYPH_W - 1 - col);
            if bits & mask != 0 {
                let px = x + col * scale;
                let py = y + row as i32 * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        put_pixel(img, px + dx, py + dy, color);
                    }
                }
            }
        }
    }
}

fn glyph(c: char) -> [u8; 7] {
    let up = c.to_ascii_uppercase();
    match up {
        ' ' => [0; 7],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10011, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b11111, 0b00001, 0b00001, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '.' => [0, 0, 0, 0, 0, 0, 0b00100],
        ',' => [0, 0, 0, 0, 0, 0b00100, 0b01000],
        ':' => [0, 0, 0b00100, 0, 0b00100, 0, 0],
        ';' => [0, 0, 0b00100, 0, 0b00100, 0b00100, 0b01000],
        '-' => [0, 0, 0, 0b01110, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '\'' => [0b00100, 0b00100, 0b01000, 0, 0, 0, 0],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '#' => [
            0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010,
        ],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '*' => [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0],
        _ => [
            0b00000, 0b11111, 0b10001, 0b10001, 0b10001, 0b11111, 0b00000,
        ],
    }
}

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
        RouteStability::Lost => ROUTE_DIM,
    }
}

pub(crate) fn tint(c: Rgba<u8>, amount: f32) -> Rgba<u8> {
    let mix = |v: u8, base: u8| {
        let f = v as f32 * amount + base as f32 * (1.0 - amount);
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
        (c.0[0] as f32 * scale) as u8,
        (c.0[1] as f32 * scale) as u8,
        (c.0[2] as f32 * scale) as u8,
        c.0[3],
    ])
}

fn short_system_label(id: &str) -> String {
    // Use the tail after the last '-' if present, else the full id, uppercased.
    let tail = id.rsplit('-').next().unwrap_or(id);
    tail.to_ascii_uppercase()
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
        }
    }

    #[test]
    fn renders_without_panicking() {
        let s = sample_sector();
        let img = render(&s, 1);
        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }

    #[test]
    fn scaled_render_is_larger() {
        let s = sample_sector();
        let small = render(&s, 1);
        let big = render(&s, 4);
        assert!(big.width() >= small.width() * 3);
        assert!(big.height() >= small.height() * 3);
    }

    #[test]
    fn glyph_returns_blank_for_space() {
        assert_eq!(glyph(' '), [0; 7]);
    }
}
