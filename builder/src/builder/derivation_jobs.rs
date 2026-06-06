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
//! reads an owned snapshot and calls the same pure `compute_*` function the UI
//! thread would, and a result whose inputs drifted never lands.
//!
//! Determinism: the pure `compute_*` functions take owned/borrowed snapshots
//! and return the derived payload with no `&mut self`, so the UI thread and the
//! worker compute identical results from identical inputs. All ledger writes
//! happen on the UI thread only (single-threaded) in
//! [`super::BuilderState::pump_derivation_jobs`].

use std::collections::BTreeMap;

use sectorforge::sector_model::GeneratedSector;
use sectorforge_gui_core::jobs::JobHandle;

use crate::builder::derivation_cache::DerivationKind;

/// The derived payload a background worker produces, one variant per
/// background-eligible [`DerivationKind`]. Boxed because the reports
/// (`EconomyReport`-sized graphs of per-world / per-system rows) are large and
/// would otherwise bloat every `DerivationJobResult` moved across the channel.
pub enum DerivationPayload {
    Relations(Box<sectorforge::relations::RelationsMatrix>),
    History(Box<sectorforge::history::SectorChronicle>),
    Personae(Box<sectorforge::personae::PersonaeReport>),
    Hooks(Box<sectorforge::hooks::HooksReport>),
    Sites(Box<sectorforge::sites::SitesReport>),
    Missions(Box<sectorforge::missions::MissionsReport>),
    Prose(Box<sectorforge::prose::ProseReport>),
}

/// Result a derivation worker posts back over the channel. The kind is not
/// carried on the value itself — the drain resolves it from the in-flight map
/// slot the job was filed under (the authority for `mark_deriving` /
/// fingerprint re-check), so the worker only needs to ship the payload.
pub enum DerivationJobResult {
    Done(DerivationPayload),
    /// Reserved for future fallible derivations: today every `compute_*` is
    /// total, so this is only the conceptual counterpart to the
    /// `TryRecvError::Disconnected` (worker-vanished) drain arm. `kind` lets a
    /// future fallible worker name itself in the failure.
    #[allow(dead_code)]
    Failed {
        kind: DerivationKind,
        message: String,
    },
}

/// One in-flight background derivation: the worker handle plus the input
/// fingerprint captured at dispatch (the LD3 stale-guard).
pub struct InFlightDerivation {
    pub job: JobHandle<DerivationJobResult>,
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
// Each takes owned/borrowed snapshots of exactly what the derivation reads and
// returns the payload, with no `&mut self`. The synchronous `recompute_*`
// methods and the background workers both call these, so a value computed off
// thread is identical to one computed inline from the same inputs. None of them
// draw RNG outside `src/model/rng.rs` (the underlying `derive_with` functions
// already route through the stage RNG) and none iterate an `Fx*` map for output.

/// Pure relations derivation (§REL9). Mirrors the body of
/// [`super::BuilderState::recompute_relations`] sans the install.
pub fn compute_relations(
    sector: &GeneratedSector,
    cfg: &sectorforge::relations::RelationsConfig,
    min_world_presence: usize,
) -> sectorforge::relations::RelationsMatrix {
    sectorforge::relations::derive_with_threshold(sector, cfg, min_world_presence)
}

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

/// Pure personae derivation (§PER1..§PER5).
pub fn compute_personae(
    sector: &GeneratedSector,
    cfg: &sectorforge::personae::PersonaeConfig,
) -> sectorforge::personae::PersonaeReport {
    sectorforge::personae::derive_with(sector, cfg)
}

/// Pure hooks derivation (§HK1..§HK6). `cfg.hide_hidden_hooks` must already be
/// set from the §HK5 player-edition toggle by the caller.
pub fn compute_hooks(
    sector: &GeneratedSector,
    cfg: &sectorforge::hooks::HooksConfig,
) -> sectorforge::hooks::HooksReport {
    sectorforge::hooks::derive_with(sector, cfg)
}

/// Pure sites derivation (§ST1..§ST4). `cfg.player_edition` must already be set
/// by the caller.
pub fn compute_sites(
    sector: &GeneratedSector,
    cfg: &sectorforge::sites::SitesConfig,
) -> sectorforge::sites::SitesReport {
    sectorforge::sites::derive_with(sector, cfg)
}

/// Pure missions derivation (§M1..§M5). `cfg.player_edition` must already be
/// set by the caller.
pub fn compute_missions(
    sector: &GeneratedSector,
    cfg: &sectorforge::missions::MissionsConfig,
) -> sectorforge::missions::MissionsReport {
    sectorforge::missions::derive_with(sector, cfg)
}

/// Pure prose derivation (§PR1..§PR4).
pub fn compute_prose(
    sector: &GeneratedSector,
    cfg: &sectorforge::prose::ProseConfig,
) -> sectorforge::prose::ProseReport {
    sectorforge::prose::derive_with(sector, cfg)
}
