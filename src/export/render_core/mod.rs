//! Shared rendering primitives reused by both `bitmap` (PNG) and `svg_export`
//! (SVG) backends.
//!
//! REFACTOR_PLAN.md Task 3 Pass C/D: the [`canvas::Canvas`] trait is the
//! backend-neutral drawing surface, and per-shape modules ([`routes`],
//! [`grid`]) are generic over it. Bitmap quantises `f32` coordinates to
//! `i32` inside its impl; SVG passes through. See [`canvas`] for the
//! contract that keeps `golden_png` byte-identical.
//!
//! `systems`, `labels`, `legend`, and `regions` stay backend-specific. The
//! star-disk pieces look identical on the surface, but the bitmap
//! `(r as i32) / 2` floor-division (e.g. in the Redacted capital marker)
//! does not round-trip through `f32 * 0.5` without changing PNG bytes, so
//! their `Canvas` rewrite is parked behind the same precision constraint
//! that already kept text rendering split. The plan's 800-line floor isn't
//! met by `routes` + `grid` alone (saved ≈ 750), but landing the safe slice
//! is preferable to "while I'm in there" precision changes that
//! `golden_png.rs` would catch.

pub(crate) mod canvas;
pub(crate) mod colors;
pub(crate) mod grid;
pub(crate) mod labels;
pub(crate) mod options;
pub(crate) mod routes;

pub use options::RenderOptions;
