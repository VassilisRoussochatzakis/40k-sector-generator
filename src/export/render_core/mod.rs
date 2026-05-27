//! Shared rendering primitives reused by both `bitmap` (PNG) and `svg_export`
//! (SVG) backends.
//!
//! REFACTOR_PLAN.md Task 3 Pass C/D: the [`canvas::Canvas`] trait is the
//! backend-neutral drawing surface, and the per-shape modules ([`routes`],
//! [`grid`], [`systems`], [`regions`]) are generic over it. Bitmap quantises
//! `f32` coordinates to `i32` inside its impl; SVG passes through. See
//! [`canvas`] for the contract that keeps `golden_png` byte-identical.
//!
//! Labels and legend stay backend-specific: bitmap renders text via a 5×7
//! bitmap font, SVG via native `<text>` elements. The placement/layout math
//! is similar but the leaf rendering paradigms diverge enough that
//! deduplication would obscure rather than clarify.

pub(crate) mod canvas;
pub(crate) mod colors;
pub(crate) mod grid;
pub(crate) mod options;
pub(crate) mod regions;
pub(crate) mod routes;
pub(crate) mod systems;

pub use options::RenderOptions;
