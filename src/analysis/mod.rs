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
use std::collections::BTreeMap;

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

/// A weighted, anchor-bucketed report item (hooks, missions): a dramatic
/// `weight` to rank by, an `id` to break ties, and an `anchor_key` to bucket on.
pub(crate) trait WeightedAnchored {
    fn weight(&self) -> u32;
    fn id(&self) -> &str;
    fn anchor_key(&self) -> String;
}

/// Keep at most `cap` items per anchor key, preferring higher weight with the id
/// as a stable tiebreak. Shared by the hooks and missions derivations (§B4) —
/// the two had byte-identical bodies differing only in element type and how the
/// anchor key was computed. The `id` tiebreak is byte-identical to the old
/// `HookId`/`MissionId` comparison: those ids derive `Ord` over their inner
/// `Arc<str>`, so comparing `as_str()` yields the same order.
pub(crate) fn cap_per_anchor<T: WeightedAnchored>(items: &mut Vec<T>, cap: u32) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    items.sort_by(|a, b| b.weight().cmp(&a.weight()).then_with(|| a.id().cmp(b.id())));
    items.retain(|item| {
        let entry = counts.entry(item.anchor_key()).or_insert(0);
        if *entry < cap {
            *entry += 1;
            true
        } else {
            false
        }
    });
}
