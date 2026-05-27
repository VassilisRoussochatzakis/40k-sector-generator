//! `Canvas` impl for the PNG backend. Quantises `f32` world coordinates
//! to the pixel grid at the primitive boundary.
//!
//! All arithmetic mirrors what the per-call-site code used to do before
//! Pass C/D — `.round() as i32` at the leaf, integer thickness, integer
//! `fill_polygon` vertices. That's the contract `golden_png` enforces.

use image::{Rgba, RgbaImage};

use super::primitives::{
    draw_line_thick, draw_rect_outline, draw_ring, fill_circle, fill_polygon, fill_rect,
};
use crate::export::render_core::canvas::Canvas;

pub(crate) struct BitmapCanvas<'a> {
    pub(crate) img: &'a mut RgbaImage,
}

impl<'a> BitmapCanvas<'a> {
    pub(crate) fn new(img: &'a mut RgbaImage) -> Self {
        Self { img }
    }
}

#[inline]
fn r(v: f32) -> i32 {
    v.round() as i32
}

#[inline]
fn r_thick(v: f32) -> i32 {
    v.round().max(1.0) as i32
}

impl Canvas for BitmapCanvas<'_> {
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba<u8>, thickness: f32) {
        draw_line_thick(
            self.img,
            r(x0),
            r(y0),
            r(x1),
            r(y1),
            color,
            r_thick(thickness),
        );
    }

    fn circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
        stroke_w: f32,
    ) {
        let cxi = r(cx);
        let cyi = r(cy);
        let ri = radius.round().max(0.0) as i32;
        if fill.0[3] > 0 {
            fill_circle(self.img, cxi, cyi, ri, fill);
        }
        if let Some(sc) = stroke {
            let sw = r_thick(stroke_w);
            draw_ring(self.img, cxi, cyi, ri, sw, sc);
        }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Rgba<u8>, stroke: Option<Rgba<u8>>) {
        let xi = r(x);
        let yi = r(y);
        let wi = r(w);
        let hi = r(h);
        if fill.0[3] > 0 {
            fill_rect(self.img, xi, yi, wi, hi, fill);
        }
        if let Some(sc) = stroke {
            draw_rect_outline(self.img, xi, yi, wi, hi, sc);
        }
    }

    fn polygon(
        &mut self,
        pts: &[(f32, f32)],
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
        _stroke_w: f32,
    ) {
        if pts.is_empty() {
            return;
        }
        let ipts: Vec<(i32, i32)> = pts.iter().map(|&(x, y)| (r(x), r(y))).collect();
        if fill.0[3] > 0 {
            fill_polygon(self.img, &ipts, fill);
        }
        if let Some(sc) = stroke {
            for i in 0..ipts.len() {
                let (ax, ay) = ipts[i];
                let (bx, by) = ipts[(i + 1) % ipts.len()];
                super::primitives::draw_line(self.img, ax, ay, bx, by, sc);
            }
        }
    }

    fn quantize(&self, value: f32) -> f32 {
        value.round()
    }
}
