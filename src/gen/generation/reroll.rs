//! Pipeline stage identity (`Stage`) and per-stage re-roll bookkeeping
//! (`RerollNonces`) for the iterative / prefix-run generation seam.
//!
//! See `ITERATIVE_GENERATION.md` §2.2 / §3 / §4 (Phase E). These two small
//! types are the only state the prefix-run entry point
//! ([`super::generate_prefix`]) needs beyond the existing `ProjectInput`:
//!
//! * [`Stage`] names every stage of the 18-stage pipeline **in pipeline order**,
//!   so its derived [`Ord`] *is* the pipeline order. `generate_prefix(through)`
//!   runs exactly the stages `s` with `s <= through`.
//! * [`RerollNonces`] maps a [`Stage`] to a re-roll counter. The counter is
//!   folded into that stage's RNG discriminator via [`RerollNonces::suffix`]:
//!   nonce `0`/absent yields `""` (the *unchanged* legacy discriminator → byte
//!   identical output → golden tests unaffected), while nonce `n > 0` yields the
//!   fixed suffix `":r{n}"` (a deterministic, distinct re-roll). Existing stage
//!   keys are never reordered or renumbered — the suffix is strictly appended.

use std::collections::BTreeMap;

/// One stage of the deterministic generation pipeline, in pipeline order.
///
/// Declaration order **is** pipeline order, so the derived [`Ord`] lets
/// [`super::generate_prefix`] gate each stage with `Stage::X <= through`. The
/// 18 variants mirror the canonical pipeline documented in
/// `ITERATIVE_GENERATION.md` §1; `Worlds` is folded under [`Stage::Systems`] for
/// v1 (one "system contents" user step), so it is not a separate variant.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Stage {
    /// 1 — filter world rows into the candidate pool (no RNG).
    WorldPool,
    /// 2 — Fisher-Yates hex placement + min-distance relax (RNG `("placement","sector")`).
    Placement,
    /// 3 — warp regions; runs **before** worlds so anomaly hexes bias the pool
    ///     (RNG `("regions","sector")`).
    Regions,
    /// 4 — per-system build incl. the per-world sub-stage
    ///     (RNG `("system",<sys_id>)` and `("world",<world_id>)`).
    Systems,
    /// 5 — assign factions to worlds (RNG `("factions","sector")`).
    Factions,
    /// 6 — public route selection (RNG key reserved/unused today).
    Routes,
    /// 7 — apply region effects to routes (no RNG).
    RegionRouteEffects,
    /// 8 — append hidden route layers (no RNG).
    HiddenRoutes,
    /// 9 — re-bucket public route stabilities to targets (no RNG).
    StabilityRebalance,
    /// 10 — per-route per-faction control (no RNG).
    RouteControls,
    /// 11 — per-system/world derived state: surface regions, conflict, orbital
    ///      assets, intel (no RNG).
    SystemState,
    /// 12 — sort + manifest + `GeneratedSector` construction (no RNG).
    Manifest,
    /// 13 — faction-kind archetype rules (no RNG).
    Archetypes,
    /// 14 — route-graph power projection (no RNG).
    PowerProjection,
    /// 15 — Voronoi-style influence field (no RNG).
    InfluenceField,
    /// 16 — inter-faction relations matrix
    ///      (RNG `("relations",<faction_a_id>:<faction_b_id>)`).
    Relations,
    /// 17 — per-world/system economy snapshot (no RNG).
    Economy,
    /// 18 — chronicle / timeline
    ///      (RNG `("history-event",<anchor_key>:<EventKind>:<ordinal>)`).
    Chronicle,
}

impl Stage {
    /// The final pipeline stage. `generate_prefix(.., Stage::LAST, ..)` runs the
    /// whole pipeline and is what the back-compat one-shot wrapper passes.
    pub const LAST: Stage = Stage::Chronicle;
}

/// Per-stage re-roll counters.
///
/// Wraps a [`BTreeMap`] (invariant #3: never an `Fx*` map — but note this map is
/// keyed lookup only and never iterated for output, so ordering is moot; the
/// `BTreeMap` choice keeps the type trivially deterministic regardless).
///
/// `Default` is the all-zero / empty map: every [`suffix`](Self::suffix) is
/// `""`, so a default-`RerollNonces` prefix run is byte-identical to today.
#[derive(Clone, Default)]
pub struct RerollNonces(BTreeMap<Stage, u64>);

impl RerollNonces {
    /// Increment the re-roll counter for `s` and return the new value (`>= 1`).
    pub fn bump(&mut self, s: Stage) -> u64 {
        let n = self.0.entry(s).or_default();
        *n += 1;
        *n
    }

    /// Read the current nonce for `s` (`0` when absent).
    #[must_use]
    pub fn get(&self, s: Stage) -> u64 {
        self.0.get(&s).copied().unwrap_or(0)
    }

    /// Discriminator suffix for stage `s`:
    /// `""` when the nonce is `0`/absent (byte-compatible legacy discriminator,
    /// invariant #2), `":r{n}"` otherwise.
    #[must_use]
    pub fn suffix(&self, s: Stage) -> String {
        match self.0.get(&s) {
            Some(n) if *n > 0 => format!(":r{n}"),
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_order_is_pipeline_order() {
        assert!(Stage::WorldPool < Stage::Placement);
        assert!(Stage::Placement < Stage::Regions);
        assert!(Stage::Regions < Stage::Systems);
        assert!(Stage::Systems < Stage::Factions);
        assert!(Stage::Factions < Stage::Routes);
        assert!(Stage::Placement < Stage::Chronicle);
        assert_eq!(Stage::LAST, Stage::Chronicle);
        // Last variant is the maximum under the derived Ord.
        assert!(Stage::Chronicle >= Stage::Relations);
    }

    #[test]
    fn default_suffix_is_empty_for_every_stage() {
        let n = RerollNonces::default();
        for s in [
            Stage::WorldPool,
            Stage::Placement,
            Stage::Regions,
            Stage::Systems,
            Stage::Factions,
            Stage::Routes,
            Stage::Relations,
            Stage::Chronicle,
        ] {
            assert_eq!(
                n.suffix(s),
                "",
                "default suffix must be empty (byte-compat)"
            );
            assert_eq!(n.get(s), 0);
        }
    }

    #[test]
    fn bump_increments_and_formats_suffix() {
        let mut n = RerollNonces::default();
        assert_eq!(n.bump(Stage::Placement), 1);
        assert_eq!(n.suffix(Stage::Placement), ":r1");
        assert_eq!(n.bump(Stage::Placement), 2);
        assert_eq!(n.suffix(Stage::Placement), ":r2");
        // Bumping one stage leaves others at the empty/zero default.
        assert_eq!(n.suffix(Stage::Regions), "");
        assert_eq!(n.get(Stage::Placement), 2);
    }
}
