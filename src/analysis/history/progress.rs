//! Streaming progress events emitted by [`derive_with_progress`].

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub(super) fn should_report_history_progress(current: usize, total: usize) -> bool {
    if total == 0 {
        return false;
    }
    current == 1 || current == total || current.is_multiple_of(history_progress_stride(total))
}

fn history_progress_stride(total: usize) -> usize {
    match total {
        0..=25 => 1,
        26..=100 => 10,
        101..=500 => 25,
        _ => 100,
    }
}
