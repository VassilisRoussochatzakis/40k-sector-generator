//! §G3 live preview pipeline.
//!
//! Owns the scratch [`GeneratedSector`] that the generation panel produces from
//! the active `[generation]` config plus the off-thread job that actually runs
//! [`sectorforge::generate_sector_with_progress_and_cancel`]. The panel
//! schedules a preview via [`PreviewState::schedule`] when any generation field
//! changes; the host pumps both the debounce timer and the job channel each
//! frame through [`PreviewState::pump`].
//!
//! Determinism: every schedule bumps `revision`, the in-flight job is
//! cancelled, and the receiving side discards stale revisions. Same config +
//! same catalogs => same preview bytes.

use std::sync::mpsc::TryRecvError;

use sectorforge::sector_model::GeneratedSector;
use sectorforge::{SectorError, SectorProgress};

use sectorforge_gui_core::jobs::{spawn_job, JobHandle};

/// Outcome the worker posts back. Mirrors the legacy editor's variant.
#[allow(clippy::large_enum_variant)]
pub enum PreviewJobResult {
    Ready(GeneratedSector),
    Cancelled,
    Failed(String),
}

impl sectorforge_gui_core::jobs::FromJobPanic for PreviewJobResult {
    fn from_job_panic(message: String) -> Option<Self> {
        Some(Self::Failed(format!("preview panicked: {message}")))
    }
}

/// §G3 debounce window (ms) between a config edit and the worker dispatch.
pub const DEFAULT_DEBOUNCE_MS: u64 = 200;

/// Preview lifecycle state owned by [`crate::builder::BuilderState`]. Bundles
/// the debounce timer, the latest scratch sector, the in-flight worker handle,
/// and the revision counter used to discard stale results.
#[derive(Default)]
pub struct PreviewState {
    /// Latest scratch sector emitted by the worker. Cleared on
    /// [`PreviewState::schedule`].
    pub sector: Option<GeneratedSector>,
    /// In-flight job. `Some` between dispatch and result drain.
    pub job: Option<JobHandle<PreviewJobResult>>,
    /// Egui time (`ctx.input(|i| i.time)`) at which the next dispatch fires.
    /// `Some` after a schedule, cleared once the job is spawned.
    pub timer: Option<f64>,
    /// Monotonically incremented on every [`PreviewState::schedule`]; results
    /// posted by older revisions are dropped.
    pub revision: u64,
    /// Human-readable error string from the most recent failed attempt.
    pub error: Option<String>,
}

impl PreviewState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule a fresh preview to run `delay_seconds` after `now`. Cancels
    /// any in-flight job and clears the cached scratch sector so the panel
    /// shows the spinner instead of stale bytes.
    pub fn schedule(&mut self, now: f64, delay_seconds: f64) {
        self.revision = self.revision.wrapping_add(1);
        self.sector = None;
        self.error = None;
        if let Some(job) = &self.job {
            job.cancel();
        }
        self.timer = Some(now + delay_seconds);
    }

    /// Cancel any in-flight job and clear preview state without re-scheduling.
    /// Used when the generation panel is closed or a project switch happens.
    pub fn clear(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.sector = None;
        self.error = None;
        self.timer = None;
        if let Some(job) = self.job.take() {
            job.cancel();
        }
    }

    /// Apply a worker result if it matches the current revision.
    /// Returns `true` when the result was accepted.
    pub fn apply_result(&mut self, revision: u64, result: PreviewJobResult) -> bool {
        if revision != self.revision {
            return false;
        }
        match result {
            PreviewJobResult::Ready(sector) => {
                self.sector = Some(sector);
                self.error = None;
            }
            PreviewJobResult::Cancelled => {}
            PreviewJobResult::Failed(message) => {
                self.sector = None;
                self.error = Some(message);
            }
        }
        true
    }

    /// Per-frame poll. When the debounce window has elapsed since the most
    /// recent [`Self::schedule`] call, spawn the off-thread job. Then drain
    /// any completed worker result. Returns `true` when a fresh result landed
    /// this tick so the caller can request a repaint.
    pub fn pump(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        build_input: impl FnOnce() -> Option<sectorforge::input::ProjectInput>,
    ) -> bool {
        if let Some(due) = self.timer {
            if now >= due {
                self.timer = None;
                if let Some(input) = build_input() {
                    if let Some(job) = &self.job {
                        job.cancel();
                    }
                    let revision = self.revision;
                    self.job = Some(spawn_job(
                        "builder-preview",
                        revision,
                        "Generating preview...",
                        ctx.clone(),
                        move |job_ctx| run_preview_worker(input, job_ctx),
                    ));
                } else {
                    self.error =
                        Some("Preview unavailable: load a project with a worlds catalog.".into());
                }
            }
        }

        let drained = self
            .job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some((job.revision, result)),
                Err(TryRecvError::Disconnected) => Some((
                    job.revision,
                    PreviewJobResult::Failed("preview worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            });

        if let Some((revision, result)) = drained {
            let accepted = self.apply_result(revision, result);
            self.job = None;
            return accepted;
        }
        false
    }
}

fn run_preview_worker(
    input: sectorforge::input::ProjectInput,
    job_ctx: sectorforge_gui_core::jobs::JobContext,
) -> PreviewJobResult {
    let result = sectorforge::generate_sector_with_progress_and_cancel(
        input,
        |event| job_ctx.set_progress(preview_progress(&event)),
        || job_ctx.is_cancelled(),
    );
    match result {
        Ok(sector) => PreviewJobResult::Ready(sector),
        Err(SectorError::GenerationCancelled) => PreviewJobResult::Cancelled,
        Err(err) => PreviewJobResult::Failed(format!("preview failed: {err}")),
    }
}

fn preview_progress(event: &SectorProgress) -> f32 {
    use SectorProgress::*;
    match event {
        WorldPoolBuilt { .. } => 0.05,
        SystemsPlaced { .. } => 0.10,
        RegionsBuilt { .. } => 0.12,
        SystemBuilt { current, total, .. } => 0.12 + fraction(*current, *total) * 0.28,
        FactionsAssigned { .. } | FactionsAggregated { .. } => 0.42,
        RoutesGenerated { .. }
        | RegionEffectsApplied { .. }
        | HiddenRoutesApplied { .. }
        | RouteControlsDerived { .. } => 0.55,
        RouteControlsProgress { current, total } => 0.45 + fraction(*current, *total) * 0.08,
        SystemStateDerived { current, total } => 0.55 + fraction(*current, *total) * 0.10,
        ManifestBuilt { .. } => 0.66,
        Complete { .. } => 1.0,
        _ => 0.70,
    }
}

fn fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

/// §G2: derive the next re-roll seed deterministically from the current root
/// seed and the re-roll counter. Mirrors the spec wording exactly.
#[must_use]
pub fn derive_reroll_seed(root_seed: &str, counter: u64) -> String {
    let input = format!("sectorforge:{root_seed}:reroll:{counter}");
    sectorforge::rng::digest_bytes(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_bumps_revision_and_clears_sector() {
        let mut p = PreviewState::new();
        p.sector = Some(GeneratedSector::empty("x", "X", "s", 1, 1));
        let r0 = p.revision;
        p.schedule(10.0, 0.2);
        assert!(p.sector.is_none());
        assert_eq!(p.timer, Some(10.2));
        assert_ne!(p.revision, r0);
    }

    #[test]
    fn apply_result_drops_stale_revision() {
        let mut p = PreviewState::new();
        p.revision = 5;
        let accepted = p.apply_result(
            4,
            PreviewJobResult::Ready(GeneratedSector::empty("x", "X", "s", 1, 1)),
        );
        assert!(!accepted);
        assert!(p.sector.is_none());
    }

    #[test]
    fn apply_result_keeps_current_revision() {
        let mut p = PreviewState::new();
        p.revision = 7;
        let accepted = p.apply_result(
            7,
            PreviewJobResult::Ready(GeneratedSector::empty("y", "Y", "s", 1, 1)),
        );
        assert!(accepted);
        assert!(p.sector.is_some());
    }

    #[test]
    fn derive_reroll_seed_is_deterministic_and_counter_sensitive() {
        let a = derive_reroll_seed("seed-1", 0);
        let b = derive_reroll_seed("seed-1", 0);
        let c = derive_reroll_seed("seed-1", 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn clear_cancels_in_flight_job() {
        let mut p = PreviewState::new();
        p.schedule(0.0, 0.0);
        let pre = p.revision;
        p.clear();
        assert!(p.sector.is_none());
        assert!(p.timer.is_none());
        assert_ne!(p.revision, pre);
    }
}
