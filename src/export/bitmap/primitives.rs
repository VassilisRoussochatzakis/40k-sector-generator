//! Low-level drawing primitives: pixel I/O, lines, rects, circles, polygon
//! fill, and the embedded 5×7 monospace font.
//!
//! Shared with [`super::super::system_map`] via `pub(crate)` re-exports in
//! [`super`].

use image::{Rgba, RgbaImage};

#[inline]
pub(crate) fn put_pixel(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let stride = w as usize * 4;
    let idx = y as usize * stride + x as usize * 4;
    let buf = img.as_mut();
    buf[idx] = color.0[0];
    buf[idx + 1] = color.0[1];
    buf[idx + 2] = color.0[2];
    buf[idx + 3] = color.0[3];
}

/// Fast horizontal span fill (inclusive end x1). Clipped, single row slice write.
#[inline]
pub(crate) fn fill_row(img: &mut RgbaImage, x0: i32, x1: i32, y: i32, color: Rgba<u8>) {
    let iw = img.width() as i32;
    let ih = img.height() as i32;
    if y < 0 || y >= ih {
        return;
    }
    let xs = x0.max(0);
    let xe = (x1 + 1).min(iw);
    if xs >= xe {
        return;
    }
    let stride = iw as usize * 4;
    let row_start = y as usize * stride + xs as usize * 4;
    let row_end = y as usize * stride + xe as usize * 4;
    let c = color.0;
    let buf = img.as_mut();
    for px in buf[row_start..row_end].chunks_exact_mut(4) {
        px[0] = c[0];
        px[1] = c[1];
        px[2] = c[2];
        px[3] = c[3];
    }
}

pub(crate) fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    if w <= 0 || h <= 0 {
        return;
    }
    let iw = img.width() as i32;
    let ih = img.height() as i32;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(iw);
    let y1 = (y + h).min(ih);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let stride = iw as usize * 4;
    let row_bytes = (x1 - x0) as usize * 4;
    let c = color.0;
    let buf = img.as_mut();
    let first_start = y0 as usize * stride + x0 as usize * 4;
    {
        let row = &mut buf[first_start..first_start + row_bytes];
        for px in row.chunks_exact_mut(4) {
            px[0] = c[0];
            px[1] = c[1];
            px[2] = c[2];
            px[3] = c[3];
        }
    }
    for yy in (y0 + 1)..y1 {
        let dst_start = yy as usize * stride + x0 as usize * 4;
        buf.copy_within(first_start..first_start + row_bytes, dst_start);
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
    if thickness <= 1 {
        draw_line(img, x0, y0, x1, y1, color);
        return;
    }
    let half = thickness / 2;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        fill_rect(img, x - half, y - half, thickness, thickness, color);
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

pub(crate) fn fill_circle(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    if radius < 0 {
        return;
    }
    let r2 = radius * radius;
    for dy in -radius..=radius {
        let max_dx2 = r2 - dy * dy;
        if max_dx2 < 0 {
            continue;
        }
        let dx = (max_dx2 as f32).sqrt() as i32;
        fill_row(img, cx - dx, cx + dx, cy + dy, color);
    }
}

pub(crate) fn draw_circle(img: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    draw_ring(img, cx, cy, radius, 1, color);
}

/// Draw a circle stroke of the given thickness.
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
        let dy2 = dy * dy;
        if dy2 > outer2 {
            continue;
        }
        let outer_dx = ((outer2 - dy2) as f32).sqrt() as i32;
        let y = cy + dy;
        if dy2 >= inner2 {
            fill_row(img, cx - outer_dx, cx + outer_dx, y, color);
        } else {
            let inner_dx = ((inner2 - dy2) as f32).sqrt() as i32;
            fill_row(img, cx - outer_dx, cx - inner_dx - 1, y, color);
            fill_row(img, cx + inner_dx + 1, cx + outer_dx, y, color);
        }
    }
}

pub(super) fn fill_polygon(img: &mut RgbaImage, pts: &[(i32, i32)], color: Rgba<u8>) {
    if pts.is_empty() {
        return;
    }
    let ymin = pts
        .iter()
        .map(|p| p.1)
        .min()
        .expect("invariant: pts non-empty checked above");
    let ymax = pts
        .iter()
        .map(|p| p.1)
        .max()
        .expect("invariant: pts non-empty checked above");
    let mut xs: Vec<i32> = Vec::with_capacity(pts.len());
    for y in ymin..=ymax {
        xs.clear();
        for i in 0..pts.len() {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % pts.len()];
            if (ay <= y && by > y) || (by <= y && ay > y) {
                let t = (y - ay) as f32 / (by - ay) as f32;
                let x = t.mul_add((bx - ax) as f32, ax as f32);
                xs.push(x.round() as i32);
            }
        }
        xs.sort_unstable();
        let mut i = 0;
        while i + 1 < xs.len() {
            fill_row(img, xs[i], xs[i + 1], y, color);
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
                if scale == 1 {
                    put_pixel(img, px, py, color);
                } else {
                    fill_rect(img, px, py, scale, scale, color);
                }
            }
        }
    }
}

pub(super) fn glyph(c: char) -> [u8; 7] {
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
