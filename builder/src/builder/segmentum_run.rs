//! §SG1..§SG5 SEGMENTUM compose runtime: the editable `segmentum.toml`
//! document, the off-thread compose job plus its per-child progress snapshot,
//! and the most recently composed [`Segmentum`] (super-manifest + inter-sector
//! links) used by the SG3 preview / SG4 link editor / SG5 in-tab view.
//!
//! Mirrors the [`super::search_run::SearchState`] lifecycle — a single
//! in-flight job, a revision counter that discards stale results, a shared
//! progress cell the worker writes and the panel reads, and a per-frame
//! [`SegmentumState::pump`]. Composition itself runs in
//! [`sectorforge::compose_segmentum_with_progress`], which loads + generates
//! every child sector sequentially, so this module only owns the result
//! channel + the progress cell.
//!
//! Cancellation note: `compose_segmentum_with_progress` has no early-out hook,
//! so [`SegmentumState::cancel`] only *detaches* the in-flight job — the worker
//! runs to completion in the background and its result is dropped on revision
//! mismatch.

use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;

use sectorforge::sector_model::{RouteStability, RouteType};
use sectorforge::segmentum::{
    FactionMode, Segmentum, SegmentumConfig, SegmentumFile, SegmentumProgress, StitchConfig,
};
use sectorforge_gui_core::jobs::{spawn_job, JobHandle};

/// Result the compose worker posts back over the channel.
pub enum ComposeJobResult {
    Done(Box<Segmentum>),
    Failed(String),
}

impl sectorforge_gui_core::jobs::FromJobPanic for ComposeJobResult {
    fn from_job_panic(message: String) -> Option<Self> {
        Some(Self::Failed(format!("compose panicked: {message}")))
    }
}

/// §SG4 scratch row for the "add inter-sector link" form. Held outside the
/// composed [`Segmentum`] so the form survives even when no segmentum is
/// composed yet.
pub struct NewLinkDraft {
    pub from_child_id: String,
    pub to_child_id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub route_type: RouteType,
    pub stability: RouteStability,
}

impl Default for NewLinkDraft {
    fn default() -> Self {
        Self {
            from_child_id: String::new(),
            to_child_id: String::new(),
            from_system_id: String::new(),
            to_system_id: String::new(),
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Unstable,
        }
    }
}

/// Per-frame SEGMENTUM state owned by [`crate::builder::BuilderState`].
/// In-memory only; the `segmentum.toml` document round-trips to disk
/// separately via [`SegmentumState::file_path`].
#[derive(Default)]
pub struct SegmentumState {
    /// §SG1 editable config. `None` until the user creates / loads one.
    pub file: Option<SegmentumFile>,
    /// On-disk path of the edited `segmentum.toml`. Used for Save and as the
    /// base dir for resolving relative child project paths. `None` until the
    /// document is first saved to or was loaded from disk.
    pub file_path: Option<Utf8PathBuf>,
    /// §SG2 output directory for the next compose run.
    pub output_dir: Option<Utf8PathBuf>,
    /// §SG2 in-flight compose job. `Some` between dispatch and result drain.
    pub job: Option<JobHandle<ComposeJobResult>>,
    /// §SG2 live progress snapshot, written by the worker, read by the panel.
    pub progress: Arc<Mutex<Option<SegmentumProgress>>>,
    /// §SG3 / §SG5 most recently composed segmentum (super-manifest + links).
    /// `None` until a compose finishes or a `segmentum.json` is loaded.
    pub composed: Option<Segmentum>,
    /// Human-readable status / error from the most recent compose / load /
    /// save action.
    pub error: Option<String>,
    /// Bumped on every dispatch / cancel; stale worker results are dropped.
    pub revision: u64,
    /// §SG4 transient "add link" form.
    pub new_link: NewLinkDraft,
}

impl SegmentumState {
    /// True while a compose worker is in flight.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// §SG1: a blank `segmentum.toml` document — a 2×1 super-grid with no
    /// children and the library's default stitch policy.
    #[must_use]
    pub fn blank_file(id: &str, title: &str, stitch_seed: &str) -> SegmentumFile {
        SegmentumFile {
            segmentum: SegmentumConfig {
                id: id.to_string(),
                title: title.to_string(),
                stitch_seed: stitch_seed.to_string(),
                columns: 2,
                rows: 1,
                faction_mode: FactionMode::Shared,
            },
            stitch: StitchConfig::default(),
            children: Vec::new(),
        }
    }

    /// Directory that relative child `project` paths resolve against — the
    /// directory holding the edited `segmentum.toml`, falling back to `fallback`
    /// (typically the open project's parent) and finally to `.`.
    #[must_use]
    pub fn base_dir(&self, fallback: Option<&Utf8PathBuf>) -> Utf8PathBuf {
        if let Some(path) = &self.file_path {
            if let Some(parent) = path.parent() {
                return parent.to_path_buf();
            }
        }
        fallback.cloned().unwrap_or_else(|| Utf8PathBuf::from("."))
    }

    /// §SG2 dispatch. Cancels any prior job, clears the previous result, resets
    /// progress, then spawns [`sectorforge::compose_segmentum_with_progress`]
    /// on a worker thread, forwarding every [`SegmentumProgress`] snapshot into
    /// [`Self::progress`].
    pub fn spawn(
        &mut self,
        ctx: &egui::Context,
        file: SegmentumFile,
        base_dir: Utf8PathBuf,
        output_dir: Utf8PathBuf,
    ) {
        self.revision = self.revision.wrapping_add(1);
        self.error = None;
        self.composed = None;
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        *self.progress.lock().unwrap() = None;

        let revision = self.revision;
        let progress_cell = Arc::clone(&self.progress);
        self.job = Some(spawn_job(
            "builder-segmentum-compose",
            revision,
            "Composing segmentum…",
            ctx.clone(),
            move |job_ctx| {
                let result = sectorforge::compose_segmentum_with_progress(
                    &file,
                    &base_dir,
                    &output_dir,
                    |p| {
                        if let Some(frac) = progress_fraction(&p) {
                            job_ctx.set_progress(frac);
                        }
                        *progress_cell.lock().unwrap() = Some(p);
                    },
                );
                match result {
                    Ok(seg) => ComposeJobResult::Done(Box::new(seg)),
                    Err(err) => ComposeJobResult::Failed(format!("compose failed: {err}")),
                }
            },
        ));
    }

    /// Detach the in-flight job (see module note — the worker keeps running to
    /// completion and its result is dropped on the revision bump).
    pub fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        self.revision = self.revision.wrapping_add(1);
        *self.progress.lock().unwrap() = None;
    }

    /// Per-frame drain. Returns `true` when a fresh, current-revision result
    /// landed this tick so the caller can request a repaint.
    pub fn pump(&mut self) -> bool {
        let drained = self
            .job
            .as_ref()
            .and_then(|job| match job.receiver.try_recv() {
                Ok(result) => Some((job.revision, result)),
                Err(TryRecvError::Disconnected) => Some((
                    job.revision,
                    ComposeJobResult::Failed("compose worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            });

        if let Some((revision, result)) = drained {
            self.job = None;
            if revision != self.revision {
                return false;
            }
            match result {
                ComposeJobResult::Done(seg) => {
                    self.composed = Some(*seg);
                    self.error = None;
                }
                ComposeJobResult::Failed(message) => {
                    self.error = Some(message);
                }
            }
            *self.progress.lock().unwrap() = None;
            return true;
        }
        false
    }

    /// Read the latest progress snapshot (cloned out of the shared cell).
    #[must_use]
    pub fn progress_snapshot(&self) -> Option<SegmentumProgress> {
        self.progress.lock().unwrap().clone()
    }
}

/// Map a [`SegmentumProgress`] event onto a coarse 0.0..=1.0 completion
/// fraction for the §SG2 progress bar. Per-child events spread evenly across
/// the `total` children; `None` for events that carry no position so the bar
/// holds its last value.
fn progress_fraction(p: &SegmentumProgress) -> Option<f32> {
    // `index` is 1-based; `within` is how far through that child the event sits.
    let ratio = |index: usize, total: usize, within: f32| -> f32 {
        if total == 0 {
            0.0
        } else {
            (((index as f32) - 1.0).max(0.0) + within) / total as f32
        }
    };
    match p {
        SegmentumProgress::Started { .. } => Some(0.0),
        SegmentumProgress::ChildLoading { index, total, .. } => Some(ratio(*index, *total, 0.1)),
        SegmentumProgress::ChildValidated { index, total, .. } => Some(ratio(*index, *total, 0.25)),
        SegmentumProgress::ChildGenerating { index, total, .. } => Some(ratio(*index, *total, 0.4)),
        SegmentumProgress::ChildSectorProgress { index, total, .. } => {
            Some(ratio(*index, *total, 0.6))
        }
        SegmentumProgress::ChildInvariantsChecked { index, total, .. } => {
            Some(ratio(*index, *total, 0.8))
        }
        SegmentumProgress::ChildExporting { index, total, .. } => Some(ratio(*index, *total, 0.9)),
        SegmentumProgress::ChildComplete { index, total, .. } => Some(ratio(*index, *total, 1.0)),
        SegmentumProgress::Stitching { .. } => Some(0.97),
        SegmentumProgress::Complete { .. } => Some(1.0),
        // `SegmentumProgress` is `#[non_exhaustive]`; hold the bar on anything new.
        _ => None,
    }
}

/// A short human label for the current §SG2 progress event ("load, validate,
/// generate, invariant, export, stitch" per child). Returned to the panel for
/// the status line beside the progress bar.
#[must_use]
pub fn progress_label(p: &SegmentumProgress) -> String {
    match p {
        SegmentumProgress::Started { children, .. } => {
            format!("starting — {children} child sector(s)")
        }
        SegmentumProgress::ChildLoading {
            index, total, id, ..
        } => format!("[{index}/{total}] loading '{id}'"),
        SegmentumProgress::ChildValidated {
            index,
            total,
            id,
            warnings,
        } => format!("[{index}/{total}] validated '{id}' ({warnings} warn)"),
        SegmentumProgress::ChildGenerating {
            index, total, id, ..
        } => format!("[{index}/{total}] generating '{id}'"),
        SegmentumProgress::ChildSectorProgress {
            index, total, id, ..
        } => format!("[{index}/{total}] generating '{id}'…"),
        SegmentumProgress::ChildInvariantsChecked {
            index, total, id, ..
        } => format!("[{index}/{total}] invariants ok '{id}'"),
        SegmentumProgress::ChildExporting {
            index, total, id, ..
        } => format!("[{index}/{total}] exporting '{id}'"),
        SegmentumProgress::ChildComplete {
            index,
            total,
            id,
            systems,
            worlds,
            routes,
        } => format!("[{index}/{total}] '{id}' done ({systems}s/{worlds}w/{routes}r)"),
        SegmentumProgress::Stitching { children } => {
            format!("stitching {children} child sector(s)")
        }
        SegmentumProgress::Complete {
            children,
            links,
            systems,
            ..
        } => format!("done — {children} children, {links} links, {systems} systems"),
        _ => "working…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_file_is_an_empty_2x1_grid() {
        let f = SegmentumState::blank_file("seg", "Seg", "stitch-1");
        assert_eq!(f.segmentum.columns, 2);
        assert_eq!(f.segmentum.rows, 1);
        assert!(f.children.is_empty());
        assert_eq!(f.segmentum.faction_mode, FactionMode::Shared);
    }

    #[test]
    fn progress_fraction_is_monotonic_within_a_child() {
        let load = progress_fraction(&SegmentumProgress::ChildLoading {
            index: 1,
            total: 2,
            id: "a".into(),
            project: "p".into(),
        })
        .unwrap();
        let done = progress_fraction(&SegmentumProgress::ChildComplete {
            index: 1,
            total: 2,
            id: "a".into(),
            systems: 1,
            worlds: 1,
            routes: 1,
        })
        .unwrap();
        assert!(load < done, "load {load} should precede complete {done}");
        assert!((0.0..=1.0).contains(&done));
    }

    #[test]
    fn base_dir_prefers_file_parent_then_fallback() {
        let mut s = SegmentumState::default();
        let fallback = Utf8PathBuf::from("/projects");
        assert_eq!(s.base_dir(Some(&fallback)), fallback);
        s.file_path = Some(Utf8PathBuf::from("/seg/segmentum.toml"));
        assert_eq!(s.base_dir(Some(&fallback)), Utf8PathBuf::from("/seg"));
    }
}
