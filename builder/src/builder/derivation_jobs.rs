//! §39 LD3 — off-thread overlay re-derivation.
//!
//! The synchronous half of LD3/LD4 (the stale-tag surface, the lazy
//! [`super::BuilderState::ensure_fresh`] refresh of the *active* tab, the
//! `deriving` ledger slot and the `mark_deriving` hook) is built elsewhere.
//! This module adds the missing piece: a per-frame dispatch + drain that pushes
//! stale overlay derivations — **including off-tab ones the user has not
//! visited yet** — onto a background worker thread so the GUI thread never
//! blocks on a recompute.
//!
//! It mirrors [`super::search_run::SearchState`] exactly: each in-flight job is
//! a [`JobHandle`] carrying its result over an `mpsc` channel, paired with the
//! input fingerprint captured at dispatch. On drain we re-check the live
//! fingerprint against the captured one and **discard a stale result** (drop
//! the handle, leave the kind stale so it re-dispatches) rather than installing
//! a value computed from inputs that have since changed. This is what keeps the
//! background path byte-identical to the synchronous path: the worker only ever
//! reads an owned snapshot and calls the same pure derive fn the UI thread
//! would, and a result whose inputs drifted never lands.
//!
//! Determinism: the pure derive fns (`sectorforge::*::derive_with` and
//! [`compute_chronicle`]) take borrowed snapshots and return the derived
//! payload with no `&mut self`, so the UI thread and the worker compute
//! identical results from identical inputs. All ledger writes happen on the UI
//! thread only (single-threaded) in
//! [`super::BuilderState::pump_derivation_jobs`].

use std::collections::BTreeMap;

use sectorforge::sector_model::GeneratedSector;
use sectorforge_gui_core::jobs::JobHandle;

use crate::builder::derivation_cache::DerivationKind;

/// The derived payload a background worker posts back over the channel, one
/// variant per background-eligible [`DerivationKind`]. The kind is not carried
/// on the value itself — the drain resolves it from the in-flight map slot the
/// job was filed under (the authority for `mark_deriving` / fingerprint
/// re-check), so the worker only needs to ship the payload. Boxed because the
/// reports (`EconomyReport`-sized graphs of per-world / per-system rows) are
/// large and would otherwise bloat every payload moved across the channel.
pub enum DerivationPayload {
    Relations(Box<sectorforge::relations::RelationsMatrix>),
    History(Box<sectorforge::history::SectorChronicle>),
    Personae(Box<sectorforge::personae::PersonaeReport>),
    Hooks(Box<sectorforge::hooks::HooksReport>),
    Sites(Box<sectorforge::sites::SitesReport>),
    Missions(Box<sectorforge::missions::MissionsReport>),
    Prose(Box<sectorforge::prose::ProseReport>),
}

impl sectorforge_gui_core::jobs::FromJobPanic for DerivationPayload {
    /// `None`: a payload cannot represent a failure (every derivation is
    /// total), so a panicking derivation worker drops its sender and the
    /// drain's existing `TryRecvError::Disconnected` arm handles it (discard
    /// the result, leave the kind stale so it re-dispatches) — same as a
    /// vanished worker. The `diagnostics` panic hook records the crash note.
    fn from_job_panic(_message: String) -> Option<Self> {
        None
    }
}

/// One in-flight background derivation: the worker handle plus the input
/// fingerprint captured at dispatch (the LD3 stale-guard).
pub struct InFlightDerivation {
    pub job: JobHandle<DerivationPayload>,
    /// `BuilderState::derivation_fingerprint(kind)` at the moment of dispatch.
    /// Re-checked on drain; a mismatch means a dependency changed mid-flight so
    /// the result is discarded.
    pub fingerprint: String,
}

/// Transient store of in-flight off-thread derivations, keyed by
/// [`DerivationKind`]. Lives directly on [`super::BuilderState`] as runtime UI
/// state — never serialized, never undoable, written directly (not via a
/// `BuilderCommand`) like the sibling [`super::search_run::SearchState`] job
/// slot. A `BTreeMap` keeps the keyspace deterministic; we only ever look up by
/// key and drain, never emit the map, so iteration order is incidental but
/// stable regardless.
#[derive(Default)]
pub struct DerivationJobs {
    pub in_flight: BTreeMap<DerivationKind, InFlightDerivation>,
}

impl DerivationJobs {
    /// True when a background recompute for `kind` is already dispatched, so the
    /// per-frame pump does not double-spawn it.
    pub fn has_in_flight(&self, kind: DerivationKind) -> bool {
        self.in_flight.contains_key(&kind)
    }
}

// ── pure compute functions (UI thread == worker, by construction) ───────────
//
// `compute_chronicle` is the one derivation with real logic beyond a bare
// `derive_with` call (it preserves `manual = true` events), so it earns a
// wrapper both the synchronous `recompute_chronicle` and the background
// worker share. The other overlays (relations / personae / hooks / sites /
// missions / prose) are one-line passthroughs to `sectorforge::*::derive_with`
// and are called directly at both call sites instead (audit finding #16).
// None of them draw RNG outside `src/model/rng.rs` (the underlying
// `derive_with` functions already route through the stage RNG) and none
// iterate an `Fx*` map for output.

/// Pure chronicle derivation (§H6). Rebuilds the chronicle from the sector +
/// history catalog while preserving every `manual = true` event already on the
/// snapshot's `chronicle`. Mirrors the old `compute_chronicle` body so the
/// passive refresh is unchanged; living here lets the worker run it too.
pub fn compute_chronicle(
    sector: &GeneratedSector,
    cfg: &sectorforge::history::HistoryConfig,
) -> sectorforge::history::SectorChronicle {
    let manual: Vec<sectorforge::history::HistoryEvent> = sector
        .chronicle
        .events
        .iter()
        .filter(|e| e.manual)
        .cloned()
        .collect();
    let mut report = sectorforge::history::derive_with(sector, cfg);
    report.events.extend(manual);
    report
        .events
        .sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    report
}
