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

use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

use camino::Utf8PathBuf;
use sectorforge::config::OutputConfig;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::ExportProgress;
use sectorforge_gui_core::human_bytes;
use sectorforge_gui_core::jobs::{spawn_job, JobHandle};

/// Result the async bundle-export worker posts back over the channel.
pub enum ExportJobResult {
    /// Export succeeded; carries the success message for the completion modal.
    Done(String),
    /// Export failed; carries the error message.
    Failed(String),
}

impl sectorforge_gui_core::jobs::FromJobPanic for ExportJobResult {
    fn from_job_panic(message: String) -> Option<Self> {
        Some(Self::Failed(format!("export panicked: {message}")))
    }
}

/// Per-frame EXPORT state owned by [`crate::builder::BuilderState`]. In-memory
/// only — never serialised into `sector.json` or the `.sgforge` session.
///
/// The bundle / "everything" writers now run **off-thread** through
/// [`ExportState::spawn_bundle`] so a large `sector.json` write streams live
/// byte progress instead of freezing the UI (mirrors [`super::random_run`]).
/// The per-overlay and standalone-system writers stay synchronous (small).
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
    /// In-flight async bundle export. `Some` between dispatch and result drain.
    pub job: Option<JobHandle<ExportJobResult>>,
    /// Bumped on every dispatch / cancel; stale worker results are dropped.
    pub revision: u64,
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
            job: None,
            revision: 0,
        }
    }
}

impl ExportState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True while an async bundle export is in flight.
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Overall completion in `[0.0, 1.0]` for the progress bar.
    pub fn fraction(&self) -> f32 {
        self.job.as_ref().map_or(0.0, JobHandle::progress)
    }

    /// Latest live status line the worker posted (e.g. the `sector.json` byte
    /// counter), or `None` when idle / not yet reported.
    pub fn status_text(&self) -> Option<String> {
        self.job.as_ref().and_then(JobHandle::status)
    }

    /// Dispatch an async bundle export of `sector` under `outputs` into `dir`,
    /// streaming [`ExportProgress`] so the UI stays responsive during a large
    /// `sector.json` write. `summary` is the success message shown on
    /// completion. Cancels any prior job first.
    pub fn spawn_bundle(
        &mut self,
        ctx: &egui::Context,
        sector: Arc<GeneratedSector>,
        outputs: OutputConfig,
        dir: Utf8PathBuf,
        summary: String,
    ) {
        self.revision = self.revision.wrapping_add(1);
        self.error = None;
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        let revision = self.revision;
        self.job = Some(spawn_job(
            "builder-export",
            revision,
            "Exporting bundle…",
            ctx.clone(),
            move |job_ctx| {
                job_ctx.set_progress(0.02);
                job_ctx.set_status("writing sector.json…");
                let mut on_progress = |ev: ExportProgress| match ev {
                    ExportProgress::FormatStarted { format } => {
                        job_ctx.set_status(format!("writing {format}…"));
                    }
                    ExportProgress::JsonProgress { bytes_written } => {
                        job_ctx.set_progress(0.1);
                        job_ctx.set_status(format!(
                            "writing sector.json… {}",
                            human_bytes(bytes_written)
                        ));
                    }
                    ExportProgress::PerSystemJson { current, total } => {
                        job_ctx.set_status(format!("per-system json {current}/{total}"));
                    }
                    ExportProgress::FormatComplete { format, bytes } => {
                        job_ctx.set_status(format!("wrote {format} ({})", human_bytes(bytes)));
                    }
                    _ => {}
                };
                match sectorforge::export_sector_with_progress(
                    &sector,
                    &outputs,
                    &dir,
                    &mut on_progress,
                ) {
                    Ok(()) => {
                        job_ctx.set_progress(1.0);
                        ExportJobResult::Done(summary)
                    }
                    Err(e) => ExportJobResult::Failed(format!("Bundle export failed: {e}")),
                }
            },
        ));
    }

    /// Detach the in-flight job (the worker keeps running; its result is dropped
    /// on the next [`Self::pump`] because the revision no longer matches).
    pub fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        self.revision = self.revision.wrapping_add(1);
    }

    /// Per-frame drain. Returns a fresh, current-revision result the tick it
    /// lands (so the panel can pop the completion modal / surface the error), or
    /// `None` while the worker is still running or no job exists.
    pub fn pump(&mut self) -> Option<ExportJobResult> {
        let drained = self
            .job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some((job.revision, result)),
                Err(TryRecvError::Disconnected) => Some((
                    job.revision,
                    ExportJobResult::Failed("export worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            });
        let (revision, result) = drained?;
        self.job = None;
        if revision != self.revision {
            // A newer dispatch / cancel superseded this job; drop the result.
            return None;
        }
        if let ExportJobResult::Failed(message) = &result {
            self.error = Some(message.clone());
        }
        Some(result)
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
