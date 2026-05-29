//! §SR1..§SR5 SEARCH runtime: the editable wishes document plus the
//! off-thread constraint-search job, its live progress snapshot, and the
//! latest outcome.
//!
//! Mirrors the [`super::preview::PreviewState`] lifecycle — a single in-flight
//! job, a revision counter that discards stale results, and a per-frame
//! [`SearchState::pump`]. The enumeration itself runs in
//! [`sectorforge::search::run_search_with_progress`] (rayon), so this module
//! only owns the result channel + the shared progress cell the worker writes
//! and the panel reads.
//!
//! Cancellation note: `sectorforge::search` has no early-out hook, so
//! [`SearchState::cancel`] only *detaches* the in-flight job — the worker runs
//! to completion in the background and its result is dropped on revision
//! mismatch.

use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

use sectorforge::input::ProjectInput;
use sectorforge::search::{
    run_search_with_progress, Constraint, RegionKindName, SearchOutcome, SearchProgress,
    StanceName, SystemStateName, WishesFile,
};
use sectorforge_gui_core::jobs::{spawn_job, JobHandle};

/// Result the search worker posts back over the channel.
pub enum SearchJobResult {
    Done(Box<SearchOutcome>),
    Failed(String),
}

/// Per-frame SEARCH state owned by [`crate::builder::BuilderState`].
#[derive(Default)]
pub struct SearchState {
    /// §SR1 / §SR4 editable wishes document. `None` until the user creates one.
    pub wishes: Option<WishesFile>,
    /// §SR3 latest finished outcome (winner + near-misses + preflight errors).
    pub outcome: Option<SearchOutcome>,
    /// In-flight job. `Some` between dispatch and result drain.
    pub job: Option<JobHandle<SearchJobResult>>,
    /// §SR2 live progress snapshot, written by the worker, read by the panel.
    pub progress: Arc<Mutex<Option<SearchProgress>>>,
    /// Human-readable error from the most recent failed run.
    pub error: Option<String>,
    /// Bumped on every dispatch / cancel; stale worker results are dropped.
    pub revision: u64,
    /// §SR1 transient: kind selected in the "add constraint" combo.
    pub new_constraint_kind: NewConstraintKind,
}

impl SearchState {
    /// True while a search worker is in flight.
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// §SR2 dispatch. Cancels any prior job, clears the previous outcome,
    /// resets progress, then spawns the rayon search on a worker thread,
    /// forwarding every [`SearchProgress`] snapshot into [`Self::progress`].
    pub fn spawn(&mut self, ctx: &egui::Context, input: ProjectInput, wishes: WishesFile) {
        self.revision = self.revision.wrapping_add(1);
        self.error = None;
        self.outcome = None;
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        *self.progress.lock().unwrap() = None;

        let revision = self.revision;
        let progress_cell = Arc::clone(&self.progress);
        self.job = Some(spawn_job(
            "builder-search",
            revision,
            "Searching seeds…",
            ctx.clone(),
            move |job_ctx| {
                let result = run_search_with_progress(&input, &wishes, |p| {
                    *progress_cell.lock().unwrap() = Some(p);
                    let frac = if p.budget == 0 {
                        0.0
                    } else {
                        p.tried as f32 / p.budget as f32
                    };
                    job_ctx.set_progress(frac);
                });
                match result {
                    Ok(outcome) => SearchJobResult::Done(Box::new(outcome)),
                    Err(err) => SearchJobResult::Failed(format!("search failed: {err}")),
                }
            },
        ));
    }

    /// Detach the in-flight job (see module note — the worker keeps running).
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
                    SearchJobResult::Failed("search worker disconnected".to_string()),
                )),
                Err(TryRecvError::Empty) => None,
            });

        if let Some((revision, result)) = drained {
            self.job = None;
            if revision != self.revision {
                return false;
            }
            match result {
                SearchJobResult::Done(outcome) => {
                    self.outcome = Some(*outcome);
                    self.error = None;
                }
                SearchJobResult::Failed(message) => {
                    self.error = Some(message);
                }
            }
            *self.progress.lock().unwrap() = None;
            return true;
        }
        false
    }

    /// Read the latest progress snapshot (cloned out of the shared cell).
    pub fn progress_snapshot(&self) -> Option<SearchProgress> {
        *self.progress.lock().unwrap()
    }
}

/// §SR1 selector for the "add constraint" combo. Covers exactly the constraint
/// kinds enumerated in `docs/BUILDER_REQS.txt §SR1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewConstraintKind {
    #[default]
    FactionShareMin,
    FactionShareMax,
    FactionWorldCountMin,
    FactionWorldCountMax,
    FactionSystemCountMin,
    FactionSystemCountMax,
    WorldTypeExists,
    ContestedWorldMin,
    ContestedWorldMax,
    SystemStateCountMin,
    SystemStateCountMax,
    RouteGraphConnected,
    NoArticulationPoints,
    DiameterMax,
    IsolatedSystemsMax,
    ContestedRatioMin,
    ContestedRatioMax,
    StanceCountMin,
    StanceCountMax,
    RegionCountMin,
    RegionCountMax,
    EconomyStrandedMax,
    EconomyResourceMin,
}

impl NewConstraintKind {
    /// All selectable kinds, in the order they appear in the combo.
    pub const ALL: &'static [Self] = &[
        Self::FactionShareMin,
        Self::FactionShareMax,
        Self::FactionWorldCountMin,
        Self::FactionWorldCountMax,
        Self::FactionSystemCountMin,
        Self::FactionSystemCountMax,
        Self::WorldTypeExists,
        Self::ContestedWorldMin,
        Self::ContestedWorldMax,
        Self::SystemStateCountMin,
        Self::SystemStateCountMax,
        Self::RouteGraphConnected,
        Self::NoArticulationPoints,
        Self::DiameterMax,
        Self::IsolatedSystemsMax,
        Self::ContestedRatioMin,
        Self::ContestedRatioMax,
        Self::StanceCountMin,
        Self::StanceCountMax,
        Self::RegionCountMin,
        Self::RegionCountMax,
        Self::EconomyStrandedMax,
        Self::EconomyResourceMin,
    ];

    /// Combo label (the on-disk `kind` slug).
    pub fn label(self) -> &'static str {
        match self {
            Self::FactionShareMin => "faction_share_min",
            Self::FactionShareMax => "faction_share_max",
            Self::FactionWorldCountMin => "faction_world_count_min",
            Self::FactionWorldCountMax => "faction_world_count_max",
            Self::FactionSystemCountMin => "faction_system_count_min",
            Self::FactionSystemCountMax => "faction_system_count_max",
            Self::WorldTypeExists => "world_type_exists",
            Self::ContestedWorldMin => "contested_world_min",
            Self::ContestedWorldMax => "contested_world_max",
            Self::SystemStateCountMin => "system_state_count_min",
            Self::SystemStateCountMax => "system_state_count_max",
            Self::RouteGraphConnected => "route_graph_connected",
            Self::NoArticulationPoints => "no_articulation_points",
            Self::DiameterMax => "diameter_max",
            Self::IsolatedSystemsMax => "isolated_systems_max",
            Self::ContestedRatioMin => "contested_ratio_min",
            Self::ContestedRatioMax => "contested_ratio_max",
            Self::StanceCountMin => "stance_count_min",
            Self::StanceCountMax => "stance_count_max",
            Self::RegionCountMin => "region_count_min",
            Self::RegionCountMax => "region_count_max",
            Self::EconomyStrandedMax => "economy_stranded_max",
            Self::EconomyResourceMin => "economy_resource_min",
        }
    }

    /// Build a default-valued [`Constraint`] of this kind. `default_faction`
    /// seeds faction-scoped constraints with the first roster faction (empty
    /// when the roster is empty — the editor then surfaces the §SR5 warning).
    pub fn make(self, default_faction: &str) -> Constraint {
        let faction_id = default_faction.to_string();
        match self {
            Self::FactionShareMin => Constraint::FactionShareMin {
                faction_id,
                min: 0.2,
            },
            Self::FactionShareMax => Constraint::FactionShareMax {
                faction_id,
                max: 0.5,
            },
            Self::FactionWorldCountMin => Constraint::FactionWorldCountMin { faction_id, min: 1 },
            Self::FactionWorldCountMax => Constraint::FactionWorldCountMax { faction_id, max: 5 },
            Self::FactionSystemCountMin => Constraint::FactionSystemCountMin { faction_id, min: 1 },
            Self::FactionSystemCountMax => Constraint::FactionSystemCountMax { faction_id, max: 5 },
            Self::WorldTypeExists => Constraint::WorldTypeExists {
                world_type: "HiveWorld".to_string(),
                dominant_faction_id: None,
                min_count: 1,
            },
            Self::ContestedWorldMin => Constraint::ContestedWorldMin {
                min: 1,
                n_way: None,
            },
            Self::ContestedWorldMax => Constraint::ContestedWorldMax { max: 3 },
            Self::SystemStateCountMin => Constraint::SystemStateCountMin {
                state: SystemStateName::Warzone,
                min: 1,
            },
            Self::SystemStateCountMax => Constraint::SystemStateCountMax {
                state: SystemStateName::Warzone,
                max: 3,
            },
            Self::RouteGraphConnected => Constraint::RouteGraphConnected,
            Self::NoArticulationPoints => Constraint::NoArticulationPoints,
            Self::DiameterMax => Constraint::DiameterMax { max_hops: 8 },
            Self::IsolatedSystemsMax => Constraint::IsolatedSystemsMax { max: 0 },
            Self::ContestedRatioMin => Constraint::ContestedRatioMin { min: 0.1 },
            Self::ContestedRatioMax => Constraint::ContestedRatioMax { max: 0.4 },
            Self::StanceCountMin => Constraint::StanceCountMin {
                stance: StanceName::Hostile,
                min: 1,
            },
            Self::StanceCountMax => Constraint::StanceCountMax {
                stance: StanceName::Hostile,
                max: 5,
            },
            Self::RegionCountMin => Constraint::RegionCountMin {
                region_kind: RegionKindName::WarpStorm,
                min: 1,
            },
            Self::RegionCountMax => Constraint::RegionCountMax {
                region_kind: RegionKindName::WarpStorm,
                max: 3,
            },
            Self::EconomyStrandedMax => Constraint::EconomyStrandedMax { max: 0 },
            Self::EconomyResourceMin => Constraint::EconomyResourceMin {
                resource: "ore".to_string(),
                min: 0.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn all_kinds_have_unique_labels() {
        let labels: BTreeSet<&str> = NewConstraintKind::ALL.iter().map(|k| k.label()).collect();
        assert_eq!(labels.len(), NewConstraintKind::ALL.len());
    }

    #[test]
    fn make_produces_constraint_whose_kind_tag_matches_label() {
        // The combo label is the on-disk `kind` slug. Serialise each
        // constructed constraint and confirm its serde tag equals `label()`,
        // so the editor combo can never drift from the file format.
        for kind in NewConstraintKind::ALL {
            let c = kind.make("imperium");
            let value = serde_json::to_value(&c).unwrap();
            let tag = value
                .get("kind")
                .and_then(|v| v.as_str())
                .expect("constraint serialises with a `kind` tag");
            assert_eq!(tag, kind.label(), "kind tag mismatch for {kind:?}");
        }
    }
}
