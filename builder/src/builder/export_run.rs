//! §EX1..§EX8 EXPORT runtime: the chosen output folder, the standalone-system
//! export form, the cached `render-markdown` preview, and the last error.
//!
//! Unlike SEARCH / SEGMENTUM this runtime is fully **synchronous** — every
//! export is a direct `sectorforge::write_*` / `export_sector` call over the
//! live in-memory sector, so there is no off-thread job to track. The
//! per-format / bitmap / HTML knobs (§EX1..§EX4) are *not* duplicated here:
//! they edit the project's own [`sectorforge::config::OutputConfig`] in
//! `BuilderState::config.outputs` directly, so they round-trip to
//! `sectorforge.toml` on save and feed `export_sector` unchanged. This struct
//! only owns the transient bits the panel needs to remember between frames.

use camino::Utf8PathBuf;

/// Per-frame EXPORT state owned by [`crate::builder::BuilderState`]. In-memory
/// only — never serialised into `sector.json` or the `.sgforge` session.
pub struct ExportState {
    /// §EX1 / §EX7 / §EX8: folder every export writes into. `None` until the
    /// user picks one; all export buttons stay disabled while it is unset.
    pub output_dir: Option<Utf8PathBuf>,
    /// §EX5: seed override for the standalone single-system export. Empty =
    /// fall back to the project's `generation.seed` (CLI parity with
    /// `generate-system` omitting `--seed`).
    pub sys_seed: String,
    /// §EX5: axial hex coordinate the standalone system is stamped at.
    pub sys_coord_q: i32,
    pub sys_coord_r: i32,
    /// §EX5: 1-based system index (mixed into the stage RNG + the `sys-NNNN`
    /// id). Must be >= 1; the generator rejects 0.
    pub sys_index: usize,
    /// §EX5: also write `<id>.md` alongside `<id>.json`.
    pub sys_markdown: bool,
    /// §EX6: cached `render_sector_markdown` of the live sector. `None` until
    /// the user clicks "Refresh preview".
    pub md_preview: Option<String>,
    /// Human-readable error from the most recent export / preview attempt.
    /// Cleared on the next success.
    pub error: Option<String>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            output_dir: None,
            sys_seed: String::new(),
            sys_coord_q: 0,
            sys_coord_r: 0,
            sys_index: 1,
            sys_markdown: true,
            md_preview: None,
            error: None,
        }
    }
}

impl ExportState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_unset_with_index_one() {
        let s = ExportState::new();
        assert!(s.output_dir.is_none());
        assert!(s.sys_seed.is_empty());
        assert_eq!(s.sys_index, 1);
        assert!(s.sys_markdown);
        assert!(s.md_preview.is_none());
        assert!(s.error.is_none());
    }
}
