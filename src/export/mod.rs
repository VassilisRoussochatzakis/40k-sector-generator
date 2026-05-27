//! Output writers + render backends.
//!
//! `writers` (originally `src/export.rs`) is the top-level dispatcher that
//! `export_all` runs over the configured outputs. The other submodules are
//! per-format renderers: PNG (`bitmap`), SVG (`svg_export`), interactive HTML
//! (`html_export`), Markdown (`render`), the per-system map / theme
//! (`system_map` / `map_theme`), the heatmap colour scale (`heatmap`),
//! subsector clustering (`subsectors`), and multi-sector composition
//! (`segmentum`).
//!
//! The original module `src/export.rs` was renamed to `writers.rs` to free up
//! the `export` name for this parent directory. Everything that lived under
//! `crate::export::*` (e.g. `export_all`, `write_md_and_json`) is hoisted
//! back via `pub use writers::*` below, so callers see no path change.

pub mod bitmap;
pub mod heatmap;
pub mod html_export;
pub mod map_theme;
pub mod render;
pub mod render_core;
pub mod segmentum;
pub mod subsectors;
pub mod svg_export;
pub mod system_map;
pub mod writers;

pub use writers::*;
