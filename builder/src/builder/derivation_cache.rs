//! §39 live-derivation ledger (LD1..LD4) plus the [`digest_input`] BLAKE3
//! fingerprint helper (R5).
//!
//! Per R5 derivations must be pure functions of their input slice. Callers
//! compute the input fingerprint with [`digest_input`] over the canonical JSON
//! of that slice; the [`DerivationLedger`] tracks, per overlay, whether the
//! currently-cached value still matches that fingerprint.
//!
//! This module defines the §39 **live-derivation ledger** (LD1..LD4): the
//! [`DerivationKind`] catalogue of the sixteen tracked overlays, the
//! [`DepClass`] mutation-input classes, the LD2 dependency table wiring the two
//! together, and the [`DerivationLedger`] that records per-kind input
//! fingerprints (LD1), staleness (LD2), and in-flight background recomputes
//! (LD3). [`super::BuilderState`] drives the ledger from the command bus and
//! the overlay panels read freshness through it (LD4).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use sectorforge::rng::digest_bytes;

/// Compute the BLAKE3 hex digest of a serializable input slice. Used as the
/// cache key for an overlay. Determinism relies on the slice serializing in a
/// stable order — prefer types backed by `BTreeMap`/`Vec`, not `HashMap`.
///
/// Returns `None` when the input fails to serialize. Callers MUST treat `None`
/// as "no usable cache key": neither read nor write the cache for it, and
/// recompute the value fresh that round. (The old `unwrap_or_default()` hashed
/// every failed input to the empty byte string, so two distinct un-serializable
/// inputs collided to the same key and could read each other's cache entry.)
pub fn digest_input<T: Serialize>(input: &T) -> Option<String> {
    let bytes = serde_json::to_vec(input).ok()?;
    Some(digest_bytes(&bytes))
}

// ── §39 live-derivation ledger (LD1..LD4) ───────────────────────────────────

/// LD2 — the mutation *input* classes the dependency table keys on. Every
/// [`crate::builder::command::BuilderCommand`] maps to the set of classes it
/// touches via `BuilderCommand::dep_classes`, and each class fans out to the
/// [`DerivationKind`]s it invalidates via [`Self::dependents`].
///
/// The split mirrors §39's table: structural sector edits
/// (`SystemsWorlds` / `Factions` / `Regions` / `Routes`) plus the two
/// catalog-config classes (`RelationsCfg` / `EconomyCfg`) that a panel can
/// mutate without changing the sector graph itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DepClass {
    /// Any system or world add/remove/move/edit. Per §39 this invalidates
    /// **all** derivations.
    SystemsWorlds,
    /// Faction roster add/remove or presence/claim edits.
    Factions,
    /// Warp-region paint / kind / rename edits.
    Regions,
    /// Route add/remove/retype/restability edits.
    Routes,
    /// `relations.toml` catalog edits (stances, thresholds).
    RelationsCfg,
    /// `economy.toml` catalog edits (resource weighting, lifelines).
    EconomyCfg,
}

impl DepClass {
    /// Every mutation-input class the ledger knows about.
    pub const ALL: &'static [DepClass] = &[
        DepClass::SystemsWorlds,
        DepClass::Factions,
        DepClass::Regions,
        DepClass::Routes,
        DepClass::RelationsCfg,
        DepClass::EconomyCfg,
    ];

    /// LD2 dependency table — the derivations invalidated when an input of
    /// this class changes. The transpose of [`DerivationKind::deps`]; the
    /// `dependency_table_is_consistent_transpose` test pins the two in sync.
    ///
    /// Mirrors §39 verbatim:
    /// ```text
    /// systems/worlds → all
    /// factions       → analytics, relations, personae, hooks, sites,
    ///                   missions, briefing, intel, power_projection,
    ///                   influence_field
    /// regions        → regions, economy            (+ route stability, invariants)
    /// routes         → analytics, economy, hooks
    /// relations.toml → relations, briefing          (+ conflict)
    /// economy.toml   → economy, hooks
    /// ```
    /// The parenthesised effects (route-stability recompute, invariant
    /// re-check, conflict re-tick) are sector mutations / checks rather than
    /// cached overlays, so they live on the command bus, not in this table.
    pub fn dependents(self) -> &'static [DerivationKind] {
        use DerivationKind as K;
        match self {
            DepClass::SystemsWorlds => DerivationKind::ALL,
            DepClass::Factions => &[
                K::Analytics,
                K::Relations,
                K::Personae,
                K::Hooks,
                K::Sites,
                K::Missions,
                K::Briefing,
                K::Intel,
                K::PowerProjection,
                K::InfluenceField,
            ],
            DepClass::Regions => &[K::Regions, K::Economy],
            DepClass::Routes => &[K::Analytics, K::Economy, K::Hooks],
            DepClass::RelationsCfg => &[K::Relations, K::Briefing],
            DepClass::EconomyCfg => &[K::Economy, K::Hooks],
        }
    }
}

/// LD1 — the sixteen live overlay derivations the builder caches by input
/// fingerprint. The variant set is the §39 cache-entry list verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivationKind {
    Analytics,
    Relations,
    Regions,
    Economy,
    History,
    Personae,
    Hooks,
    Prose,
    Sites,
    Missions,
    Briefing,
    Interestingness,
    Subsectors,
    PowerProjection,
    InfluenceField,
    Intel,
}

impl DerivationKind {
    /// Canonical ordering, used as the stable key space for the ledger maps.
    pub const ALL: &'static [DerivationKind] = &[
        DerivationKind::Analytics,
        DerivationKind::Relations,
        DerivationKind::Regions,
        DerivationKind::Economy,
        DerivationKind::History,
        DerivationKind::Personae,
        DerivationKind::Hooks,
        DerivationKind::Prose,
        DerivationKind::Sites,
        DerivationKind::Missions,
        DerivationKind::Briefing,
        DerivationKind::Interestingness,
        DerivationKind::Subsectors,
        DerivationKind::PowerProjection,
        DerivationKind::InfluenceField,
        DerivationKind::Intel,
    ];

    /// Stable string key (matches the §39 entry names + the per-overlay CLI
    /// subcommand). Used as the BLAKE3 fingerprint domain-separator so two
    /// kinds reading the same slice never collide.
    pub fn key(self) -> &'static str {
        match self {
            DerivationKind::Analytics => "analytics",
            DerivationKind::Relations => "relations",
            DerivationKind::Regions => "regions",
            DerivationKind::Economy => "economy",
            DerivationKind::History => "history",
            DerivationKind::Personae => "personae",
            DerivationKind::Hooks => "hooks",
            DerivationKind::Prose => "prose",
            DerivationKind::Sites => "sites",
            DerivationKind::Missions => "missions",
            DerivationKind::Briefing => "briefing",
            DerivationKind::Interestingness => "interestingness",
            DerivationKind::Subsectors => "subsectors",
            DerivationKind::PowerProjection => "power_projection",
            DerivationKind::InfluenceField => "influence_field",
            DerivationKind::Intel => "intel",
        }
    }

    /// LD1 — the input classes this derivation reads. Drives both the
    /// fingerprint slice (which sector/catalog slices feed the BLAKE3 key) and
    /// the transpose check against [`DepClass::dependents`].
    pub fn deps(self) -> &'static [DepClass] {
        use DepClass as D;
        match self {
            DerivationKind::Analytics => &[D::SystemsWorlds, D::Factions, D::Routes],
            DerivationKind::Relations => &[D::SystemsWorlds, D::Factions, D::RelationsCfg],
            DerivationKind::Regions => &[D::SystemsWorlds, D::Regions],
            DerivationKind::Economy => &[D::SystemsWorlds, D::Regions, D::Routes, D::EconomyCfg],
            DerivationKind::History => &[D::SystemsWorlds],
            DerivationKind::Personae => &[D::SystemsWorlds, D::Factions],
            DerivationKind::Hooks => &[D::SystemsWorlds, D::Factions, D::Routes, D::EconomyCfg],
            DerivationKind::Prose => &[D::SystemsWorlds],
            DerivationKind::Sites => &[D::SystemsWorlds, D::Factions],
            DerivationKind::Missions => &[D::SystemsWorlds, D::Factions],
            DerivationKind::Briefing => &[D::SystemsWorlds, D::Factions, D::RelationsCfg],
            DerivationKind::Interestingness => &[D::SystemsWorlds],
            DerivationKind::Subsectors => &[D::SystemsWorlds],
            DerivationKind::PowerProjection => &[D::SystemsWorlds, D::Factions],
            DerivationKind::InfluenceField => &[D::SystemsWorlds, D::Factions],
            DerivationKind::Intel => &[D::SystemsWorlds, D::Factions],
        }
    }
}

/// LD3 — per-kind freshness as surfaced to panels + the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationStatus {
    /// Never derived this session — no cached value yet.
    Cold,
    /// Cached value matches the current input fingerprint.
    Fresh,
    /// A dependency changed; the cached value is out of date.
    Stale,
    /// A background recompute is in flight (LD3).
    Deriving,
}

/// §39 live-derivation ledger (LD1..LD4). Tracks, per [`DerivationKind`]:
/// the input fingerprint that produced the currently-cached value (LD1),
/// whether a dependency has since changed (LD2), and whether a background
/// recompute is running (LD3). The cached *values* still live in their
/// existing homes (`BuilderState::*_report`, `sector.economy`, …); this ledger
/// is the freshness authority those panels consult before rendering (LD4).
#[derive(Debug, Clone, Default)]
pub struct DerivationLedger {
    /// LD1 — input digest (`BuilderState::derivation_fingerprint`) that
    /// produced the value currently cached for each kind. Absent ⇒ cold.
    pub fingerprints: BTreeMap<DerivationKind, String>,
    /// LD2 — kinds whose dependency inputs changed since their last derive and
    /// have not yet been recomputed.
    pub stale: BTreeSet<DerivationKind>,
    /// LD3 — kinds with an in-flight background recompute.
    pub deriving: BTreeSet<DerivationKind>,
}

impl DerivationLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// LD2 — precise invalidation. Mark every *already-derived* derivation
    /// downstream of any of `classes` stale, for the sixteen tracked overlays.
    ///
    /// Kinds with no recorded fingerprint (never derived this session) are
    /// left cold rather than flagged stale, so `stale ⊆ fingerprints` always
    /// holds and the status-bar "stale" count never includes overlays the user
    /// has not opened yet.
    pub fn invalidate(&mut self, classes: &[DepClass]) {
        for class in classes {
            for kind in class.dependents() {
                if self.fingerprints.contains_key(kind) {
                    self.stale.insert(*kind);
                }
            }
        }
    }

    /// Audit finding #21 — invalidate specific overlay *kinds* directly,
    /// bypassing the [`DepClass`] fan-out. Used by a watcher-driven catalog
    /// reload to stale exactly the overlays a per-kind catalog file feeds
    /// (e.g. `hooks.toml` → [`DerivationKind::Hooks`]) when no `DepClass`
    /// captures that input. Same already-derived guard as [`Self::invalidate`]:
    /// a never-derived (cold) kind is left cold so `stale ⊆ fingerprints`.
    pub fn invalidate_kinds(&mut self, kinds: &[DerivationKind]) {
        for kind in kinds {
            if self.fingerprints.contains_key(kind) {
                self.stale.insert(*kind);
            }
        }
    }

    /// Mark every already-derived overlay stale — used by full-sector swaps
    /// (apply-preview, partial regen, search-seed apply) where a precise diff
    /// would invalidate everything anyway.
    pub fn invalidate_all(&mut self) {
        for kind in DerivationKind::ALL {
            if self.fingerprints.contains_key(kind) {
                self.stale.insert(*kind);
            }
        }
    }

    /// Record that `kind` was just (re)derived from the input identified by
    /// `fingerprint`: stores the fingerprint and clears the stale / deriving
    /// flags.
    pub fn mark_fresh(&mut self, kind: DerivationKind, fingerprint: String) {
        self.fingerprints.insert(kind, fingerprint);
        self.stale.remove(&kind);
        self.deriving.remove(&kind);
    }

    /// LD3 — flag a kind as having a background recompute in flight.
    pub fn mark_deriving(&mut self, kind: DerivationKind) {
        self.deriving.insert(kind);
    }

    pub fn is_stale(&self, kind: DerivationKind) -> bool {
        self.stale.contains(&kind)
    }

    /// LD3 — freshness for `kind` given the caller-computed `current` input
    /// fingerprint. A stored fingerprint that still matches `current` reads
    /// `Fresh` even if conservatively flagged stale, which is the precise half
    /// of LD2 (a dependency-table hit that did not actually change inputs does
    /// not force a recompute).
    pub fn status(&self, kind: DerivationKind, current: &str) -> DerivationStatus {
        if self.deriving.contains(&kind) {
            return DerivationStatus::Deriving;
        }
        match self.fingerprints.get(&kind) {
            None => DerivationStatus::Cold,
            Some(stored) if stored == current => DerivationStatus::Fresh,
            Some(_) => DerivationStatus::Stale,
        }
    }

    /// Count of kinds currently flagged stale (status-bar readout).
    pub fn stale_count(&self) -> usize {
        self.stale.len()
    }

    /// Drop all freshness state — only for true session boundaries
    /// (open project, random sector) where the cached values themselves are
    /// being replaced wholesale.
    pub fn clear(&mut self) {
        self.fingerprints.clear();
        self.stale.clear();
        self.deriving.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_input_is_stable() {
        let a = digest_input(&("alpha", 1u32));
        let b = digest_input(&("alpha", 1u32));
        assert!(a.is_some());
        assert_eq!(a, b);
        let c = digest_input(&("alpha", 2u32));
        assert_ne!(a, c);
    }

    #[test]
    fn derivation_kind_keys_are_unique() {
        let mut seen = BTreeSet::new();
        for kind in DerivationKind::ALL {
            assert!(seen.insert(kind.key()), "duplicate key {}", kind.key());
        }
        assert_eq!(seen.len(), 16, "§39 lists sixteen cached derivations");
    }

    /// LD2 — [`DerivationKind::deps`] and [`DepClass::dependents`] must be exact
    /// transposes: `kind ∈ class.dependents()` iff `class ∈ kind.deps()`.
    #[test]
    fn dependency_table_is_consistent_transpose() {
        for class in DepClass::ALL {
            for kind in class.dependents() {
                assert!(
                    kind.deps().contains(class),
                    "{kind:?} is listed under {class:?}.dependents() but {class:?} \
                     is missing from {kind:?}.deps()"
                );
            }
        }
        for kind in DerivationKind::ALL {
            for class in kind.deps() {
                assert!(
                    class.dependents().contains(kind),
                    "{class:?} is listed under {kind:?}.deps() but {kind:?} \
                     is missing from {class:?}.dependents()"
                );
            }
        }
    }

    /// A ledger with every kind derived (fingerprinted) so `invalidate` —
    /// which only flags already-derived overlays — has something to flag.
    fn all_fresh() -> DerivationLedger {
        let mut led = DerivationLedger::new();
        for kind in DerivationKind::ALL {
            led.mark_fresh(*kind, "seed".into());
        }
        led
    }

    /// `invalidate` never flags a never-derived (cold) overlay stale.
    #[test]
    fn invalidate_skips_cold_kinds() {
        let mut led = DerivationLedger::new();
        led.invalidate(&[DepClass::SystemsWorlds]);
        assert_eq!(led.stale_count(), 0, "cold overlays stay cold, not stale");
    }

    /// §39: systems/worlds edits invalidate every derived overlay.
    #[test]
    fn systems_worlds_invalidates_all() {
        let mut led = all_fresh();
        led.invalidate(&[DepClass::SystemsWorlds]);
        for kind in DerivationKind::ALL {
            assert!(led.is_stale(*kind), "{kind:?} should be stale");
        }
        assert_eq!(led.stale_count(), 16);
    }

    /// §39: a route edit invalidates exactly {analytics, economy, hooks}.
    #[test]
    fn routes_invalidate_only_their_dependents() {
        let mut led = all_fresh();
        led.invalidate(&[DepClass::Routes]);
        let want = [
            DerivationKind::Analytics,
            DerivationKind::Economy,
            DerivationKind::Hooks,
        ];
        assert_eq!(led.stale_count(), want.len());
        for kind in want {
            assert!(led.is_stale(kind));
        }
        assert!(!led.is_stale(DerivationKind::Relations));
        assert!(!led.is_stale(DerivationKind::Personae));
    }

    #[test]
    fn mark_fresh_clears_stale_and_records_fingerprint() {
        let mut led = all_fresh();
        led.invalidate(&[DepClass::Factions]);
        assert!(led.is_stale(DerivationKind::Personae));
        led.mark_fresh(DerivationKind::Personae, "fp-1".into());
        assert!(!led.is_stale(DerivationKind::Personae));
        assert_eq!(
            led.status(DerivationKind::Personae, "fp-1"),
            DerivationStatus::Fresh
        );
    }

    #[test]
    fn status_distinguishes_cold_fresh_stale_deriving() {
        let mut led = DerivationLedger::new();
        assert_eq!(
            led.status(DerivationKind::Hooks, "fp"),
            DerivationStatus::Cold
        );
        led.mark_fresh(DerivationKind::Hooks, "fp".into());
        assert_eq!(
            led.status(DerivationKind::Hooks, "fp"),
            DerivationStatus::Fresh
        );
        // A changed fingerprint reads stale even without an explicit invalidate.
        assert_eq!(
            led.status(DerivationKind::Hooks, "fp2"),
            DerivationStatus::Stale
        );
        led.mark_deriving(DerivationKind::Hooks);
        assert_eq!(
            led.status(DerivationKind::Hooks, "fp"),
            DerivationStatus::Deriving
        );
    }
}
