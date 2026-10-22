//! Streaming progress events emitted by [`derive_with_progress_reroll`].

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HistoryProgress {
    Started {
        systems: usize,
        worlds: usize,
        routes: usize,
        max_subsector_events: u32,
    },
    SubsectorEventsStarted {
        exact_cluster_count: usize,
        emitted_cap: u32,
        sampled: bool,
    },
    SubsectorEventsDone {
        events: usize,
    },
    SystemsScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    RoutesScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    EventRulesApplied {
        events: usize,
    },
    SortingStarted {
        events: usize,
    },
    Complete {
        events: usize,
    },
}

/// Progress cadence shared by the chronicle emitter and the CLI progress log:
/// the first, the last and every stride-th of `total` steps.
pub fn should_report_progress(current: usize, total: usize) -> bool {
    current == 1 || current == total || current.is_multiple_of(progress_stride(total))
}

fn progress_stride(total: usize) -> usize {
    match total {
        0..=25 => 1,
        26..=100 => 10,
        101..=500 => 25,
        _ => 100,
    }
}
