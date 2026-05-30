//! RANDOM.md §7.4 random-sector generation runtime: an off-thread
//! synthesise-generate-derive job with a live progress snapshot, mirroring
//! [`super::search_run`] / [`super::segmentum_run`].
//!
//! The worker runs [`sectorforge::random_sector::generate_random_sector_with_progress`],
//! forwarding every [`RandomProgress`] snapshot into the shared cell, then
//! persists the generated sector to `out/sector.json` (so the project can be
//! reopened fully wired). It posts a [`RandomJobResult`] back over the channel;
//! the §7.3 wizard panel pumps the channel each frame and — on success —
//! replaces the active [`crate::builder::BuilderState`] by reopening `dest` (a
//! session boundary, like the new-project / new-from-preset wizards).
//!
//! Cancellation detaches the in-flight job (the worker runs to completion in the
//! background and its result is dropped on revision mismatch); generation has no
//! early-out hook.

use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use sectorforge::config::OutputFormat;
use sectorforge::random_sector::{
    generate_random_sector_with_progress, RandomPhase, RandomProgress, SectorSize,
};
use sectorforge_gui_core::jobs::{spawn_job, JobHandle};

/// Result the random-generation worker posts back over the channel.
pub enum RandomJobResult {
    /// Generation + export succeeded; `dest` is the project root the host
    /// reopens to install the fully-wired sector.
    Done { dest: Utf8PathBuf },
    /// Generation, export, or scaffolding failed.
    Failed(String),
}

/// Per-frame random-generation state owned by [`crate::builder::BuilderState`].
#[derive(Default)]
pub struct RandomGenState {
    /// In-flight job. `Some` between dispatch and result drain.
    pub job: Option<JobHandle<RandomJobResult>>,
    /// Live progress snapshot, written by the worker, read by the popup.
    pub progress: Arc<Mutex<Option<RandomProgress>>>,
    /// Human-readable error from the most recent failed run.
    pub error: Option<String>,
    /// Bumped on every dispatch / cancel; stale worker results are dropped.
    pub revision: u64,
}

impl RandomGenState {
    /// True while a generation worker is in flight.
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Overall completion in `[0.0, 1.0]` for the progress bar.
    pub fn fraction(&self) -> f32 {
        self.job.as_ref().map_or(0.0, JobHandle::progress)
    }

    /// Latest progress snapshot (cloned out of the shared cell).
    pub fn progress_snapshot(&self) -> Option<RandomProgress> {
        self.progress.lock().unwrap().clone()
    }

    /// Dispatch a generation. Cancels any prior job, resets progress, then
    /// spawns the synthesise→generate→derive→export pipeline on a worker thread,
    /// forwarding every [`RandomProgress`] snapshot into [`Self::progress`].
    pub fn spawn(
        &mut self,
        ctx: &egui::Context,
        size: SectorSize,
        seed: String,
        dest: Utf8PathBuf,
    ) {
        self.revision = self.revision.wrapping_add(1);
        self.error = None;
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        *self.progress.lock().unwrap() = None;

        let revision = self.revision;
        let progress_cell = Arc::clone(&self.progress);
        self.job = Some(spawn_job(
            "builder-random",
            revision,
            "Generating random sector…",
            ctx.clone(),
            move |job_ctx| {
                let presets_dir = sectorforge::presets::default_presets_dir();
                let result = generate_random_sector_with_progress(
                    size,
                    Some(seed.clone()),
                    &presets_dir,
                    &dest,
                    &mut |p| {
                        let frac = p.fraction;
                        *progress_cell.lock().unwrap() = Some(p);
                        job_ctx.set_progress(frac);
                    },
                );
                let report = match result {
                    Ok(report) => report,
                    Err(e) => {
                        return RandomJobResult::Failed(format!("Random generation failed: {e}"))
                    }
                };

                // Persist the generated sector to out/sector.json so the host can
                // reopen `dest` as a fully-wired project (the synthesised config
                // turns on every format; here we only need JSON to reopen).
                let mut out_cfg = report.input.config.outputs.clone();
                out_cfg.formats = vec![OutputFormat::Json];
                let out_dir = dest.join(&out_cfg.directory);
                *progress_cell.lock().unwrap() = Some(RandomProgress {
                    phase: RandomPhase::Prose,
                    fraction: 0.98,
                    detail: Some("Writing sector.json".to_string()),
                });
                job_ctx.set_progress(0.98);
                if let Err(e) = sectorforge::export_sector(&report.sector, &out_cfg, &out_dir) {
                    return RandomJobResult::Failed(format!("Writing sector.json failed: {e}"));
                }
                job_ctx.set_progress(1.0);

                RandomJobResult::Done { dest: dest.clone() }
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
        *self.progress.lock().unwrap() = None;
    }

    /// Per-frame drain. Returns a fresh, current-revision result when one landed
    /// this tick (so the host can reopen the project / surface the error), or
    /// `None` while the worker is still running or no job exists.
    pub fn pump(&mut self) -> Option<RandomJobResult> {
        let drained = self
            .job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some((job.revision, result)),
                Err(TryRecvError::Disconnected) => Some((
                    job.revision,
                    RandomJobResult::Failed("random worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            });

        let (revision, result) = drained?;
        self.job = None;
        *self.progress.lock().unwrap() = None;
        if revision != self.revision {
            // A newer dispatch / cancel superseded this job; drop the result.
            return None;
        }
        if let RandomJobResult::Failed(message) = &result {
            self.error = Some(message.clone());
        }
        Some(result)
    }
}
