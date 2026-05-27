//! `Canvas` impl for the SVG backend. Emits XML primitives via the
//! existing `primitives` helpers; coordinates pass through unchanged.

use image::Rgba;

use super::primitives::{circle, line, polygon, rect};
use crate::export::render_core::canvas::Canvas;

pub(crate) struct SvgCanvas<'a> {
    pub(crate) out: &'a mut String,
}

impl<'a> SvgCanvas<'a> {
    pub(crate) fn new(out: &'a mut String) -> Self {
        Self { out }
    }
}

impl Canvas for SvgCanvas<'_> {
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba<u8>, thickness: f32) {
        line(self.out, x0, y0, x1, y1, color, thickness, None);
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
        circle(self.out, cx, cy, radius, fill, stroke, stroke_w);
    }

    fn rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
    ) {
        rect(self.out, x, y, w, h, fill, stroke);
    }

    fn polygon(
        &mut self,
        pts: &[(f32, f32)],
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
        stroke_w: f32,
    ) {
        polygon(self.out, pts, fill, stroke, stroke_w);
    }
}
