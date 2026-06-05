//! Read-only derivations over a generated sector.
//!
//! Every submodule here is a pure function of an immutable `GeneratedSector`
//! (plus optional config) → some `Report` / overlay type. Side-effect-free,
//! re-runnable, deterministic.
//!
//! Note: `economy` and `relations` are grouped here even though they were not
//! in the original refactor plan's listing — both are pure derivations over
//! sector state and read more naturally next to `analytics` / `interestingness`
//! than as top-level modules.

pub mod analytics;
pub mod briefing;
pub mod conflict;
pub mod control;
pub mod economy;
pub mod history;
pub mod hooks;
pub mod importance;
pub mod influence_field;
pub mod intel;
pub mod interestingness;
pub mod missions;
pub mod personae;
pub mod power_projection;
pub mod prose;
pub mod relations;
pub mod route_control;
pub mod scores;
pub mod search;
pub mod stability;

use std::cmp::Ordering;

/// Descending `f32` comparison with the crate-wide NaN policy: NaN sorts as a
/// tie (`Equal`). Centralizes the historical `partial_cmp(..).unwrap_or(Equal)`
/// idiom that was hand-copied across the analysis sorts (§B9), so the policy
/// lives in one place. Pass the closure's args in order —
/// `cmp_f32_desc(a.score, b.score)` puts the larger value first; chain
/// `.then_with(..)` at the call site for tiebreakers.
pub(crate) fn cmp_f32_desc(a: f32, b: f32) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Ascending counterpart of [`cmp_f32_desc`]: `cmp_f32_asc(a.x, b.x)` puts the
/// smaller value first. Same NaN-as-`Equal` policy.
pub(crate) fn cmp_f32_asc(a: f32, b: f32) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}
