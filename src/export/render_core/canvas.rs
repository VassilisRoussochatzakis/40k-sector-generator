//! Backend-neutral drawing surface shared by `bitmap` and `svg_export`.
//!
//! REFACTOR_PLAN.md Task 3 Pass C/D: the [`Canvas`] trait captures the
//! minimum primitive set that the shared high-level drawing functions
//! (`render_core::routes`, `render_core::grid`, `render_core::systems`,
//! `render_core::regions`) need from each backend. Every primitive takes
//! `f32` world coordinates — bitmap quantises to `i32` inside its impl,
//! SVG emits the floats directly.
//!
//! The `quantize` hook lets shared code pre-round values that bitmap
//! traditionally rounds *before* deriving other geometry from them
//! (e.g. `dot_radius` feeding into `spacing = dot_radius * 2.5`). Without
//! it, byte-identical bitmap output would not be preserved.

use image::Rgba;

pub(crate) trait Canvas {
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba<u8>, thickness: f32);
    fn circle(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
        stroke_w: f32,
    );
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Rgba<u8>, stroke: Option<Rgba<u8>>);
    fn polygon(
        &mut self,
        pts: &[(f32, f32)],
        fill: Rgba<u8>,
        stroke: Option<Rgba<u8>>,
        stroke_w: f32,
    );

    /// Pre-quantise a value (radius, thickness, etc.) to the backend's
    /// resolution. Bitmap rounds to the nearest integer; SVG passes through.
    /// Shared code must call this on any value whose rounded form is then
    /// used to derive further geometry — preserves byte-identical PNG output.
    fn quantize(&self, value: f32) -> f32 {
        value
    }
}
