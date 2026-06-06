//! Audit finding #8 — catalog-edit undo/redo via the command bus.
//!
//! Catalog edits (the 13 `data/*.toml` mirrors on [`super::BuilderState::
//! data_catalogs`] plus the five §E1..§E3 economy override side-tables) used to
//! mutate `BuilderState` directly, bypassing the command bus, so they could not
//! be undone. This module routes them through
//! [`crate::builder::command::BuilderCommand::EditCatalog`] while preserving the
//! existing live-editing UX:
//!
//! * **Discrete one-shot edits** (a delete-row / duplicate / "create defaults"
//!   button) call [`super::BuilderState::edit_catalogs`], which snapshots the
//!   slice, applies the closure, and commits one `EditCatalog` immediately.
//!
//! * **Continuous editors** (a slider drag, a multi-keystroke text edit, the
//!   per-table editors that mutate `data_catalogs.X.as_mut()` live across an
//!   egui frame) call [`super::BuilderState::note_catalog_edit`] each frame a
//!   widget reports `.changed()`. A *session baseline* is captured on the first
//!   edit of the burst and the coalesced diff is committed as **one** undo entry
//!   when the burst settles — detected at the top of the next catalog-panel
//!   frame ([`super::BuilderState::begin_catalog_session`]) by a frame with a
//!   live baseline but no edit, or eagerly on tab-switch
//!   ([`super::BuilderState::flush_catalog_session`]).
//!
//! The `EditCatalog::{apply,revert}` are no-ops on the sector; the actual
//! catalog/override swap happens in `BuilderState::{run,undo,redo}` (see
//! `state/undo.rs`) immediately after the sector apply/revert, because the
//! borrow checker forbids touching the sector and the catalogs at once.

use super::super::command::BuilderCommand;
use super::super::data_catalogs::CatalogSnapshot;
use super::super::errors::BuilderError;
use super::BuilderState;

impl BuilderState {
    /// Capture the live catalog-edit slice (the 13 config mirrors + the five
    /// economy override maps) into a [`CatalogSnapshot`]. Used for both session
    /// baselines and the `before`/`after` halves of `EditCatalog`.
    pub(crate) fn snapshot_catalogs(&self) -> CatalogSnapshot {
        CatalogSnapshot {
            catalogs: self.data_catalogs.clone(),
            world_economy_overrides: self.world_economy_overrides.clone(),
            world_strategic_overrides: self.world_strategic_overrides.clone(),
            system_tithe_overrides: self.system_tithe_overrides.clone(),
            system_supply_overrides: self.system_supply_overrides.clone(),
            system_priority_overrides: self.system_priority_overrides.clone(),
        }
    }

    /// Restore a [`CatalogSnapshot`] into the live fields. Called by
    /// `BuilderState::{run,undo,redo}` to install the `after`/`before` half of
    /// an `EditCatalog`. Index rebuilds / derivation invalidation are handled by
    /// the caller (the catalog slice does not change the sector graph, but the
    /// economy overrides feed `recompute_economy`).
    pub(crate) fn restore_catalogs(&mut self, snap: CatalogSnapshot) {
        self.data_catalogs = snap.catalogs;
        self.world_economy_overrides = snap.world_economy_overrides;
        self.world_strategic_overrides = snap.world_strategic_overrides;
        self.system_tithe_overrides = snap.system_tithe_overrides;
        self.system_supply_overrides = snap.system_supply_overrides;
        self.system_priority_overrides = snap.system_priority_overrides;
    }

    /// Discrete one-shot catalog edit: snapshot the slice, run `f` against a
    /// working copy of the live state, and commit a single
    /// `BuilderCommand::EditCatalog` capturing `before → after`. Used by the
    /// atomic button handlers (delete / duplicate / add row, "create
    /// defaults", delete-rule, …) so each lands as exactly one undo entry.
    ///
    /// Any pending continuous-edit session is flushed *first* so a discrete edit
    /// never folds into (or double-counts) an open burst — they become two
    /// adjacent undo entries instead. Callers keep their existing
    /// `mark_catalog_dirty` / `recompute_*` calls: `run` marks the session dirty
    /// and invalidates derivations, but does **not** track the catalog file path
    /// for save (that is what `mark_catalog_dirty` does).
    pub fn edit_catalogs(
        &mut self,
        f: impl FnOnce(&mut CatalogSnapshot),
    ) -> Result<(), BuilderError> {
        self.flush_catalog_session()?;
        let before = self.snapshot_catalogs();
        let mut after = before.clone();
        f(&mut after);
        // No-diff guard: a closure that changed nothing (e.g. an `ensure_*` that
        // found the catalog already present) must not push an empty undo entry.
        if catalog_snapshots_eq(&before, &after) {
            return Ok(());
        }
        self.run(BuilderCommand::EditCatalog {
            before: Box::new(before),
            after: Box::new(after),
        })
    }

    /// Open a coalescing session at the top of a catalog panel's `show`.
    ///
    /// Two jobs, in order:
    /// 1. **Settle detection.** If a baseline holds an uncommitted diff and the
    ///    *previous* frame recorded no edit (`!catalog_edited_this_frame`, still
    ///    holding last frame's value here because we reset it below), the burst
    ///    has settled — commit it now as one `EditCatalog`.
    /// 2. **Baseline capture.** Reset the per-frame edit flag, then snapshot the
    ///    current slice as the baseline if none is live, so the *first*
    ///    `note_catalog_edit` of a new burst has a `before` to diff against.
    ///
    /// Call this unconditionally at the top of every catalog panel `show`.
    pub fn begin_catalog_session(&mut self) {
        if self.catalog_edit_uncommitted && !self.catalog_edited_this_frame {
            // Best-effort: a failed commit (validation soft-fail does not error
            // here) leaves the baseline so the next frame retries.
            let _ = self.commit_catalog_session();
        }
        self.catalog_edited_this_frame = false;
        if self.catalog_session_baseline.is_none() {
            self.catalog_session_baseline = Some(Box::new(self.snapshot_catalogs()));
        }
    }

    /// Record that a catalog widget changed this frame (called from each
    /// panel's `on_catalog_edited` and the continuous-editor `changed` blocks).
    /// Ensures a baseline exists (covers the rare case a panel mutates before
    /// `begin_catalog_session` ran) and arms the burst so it commits on settle.
    /// Does **not** itself push a command — coalescing waits for the burst to
    /// settle so a slider drag is one undo entry, not dozens.
    pub fn note_catalog_edit(&mut self) {
        if self.catalog_session_baseline.is_none() {
            self.catalog_session_baseline = Some(Box::new(self.snapshot_catalogs()));
        }
        self.catalog_edited_this_frame = true;
        self.catalog_edit_uncommitted = true;
    }

    /// Commit the current session: diff the baseline against the live slice and,
    /// if they differ, push one `BuilderCommand::EditCatalog`. Clears the
    /// session regardless. Re-entrancy-safe — the baseline is taken *before*
    /// `run` is called, so the `run → EditCatalog` path cannot recurse back into
    /// a second commit.
    pub(crate) fn commit_catalog_session(&mut self) -> Result<(), BuilderError> {
        let Some(baseline) = self.catalog_session_baseline.take() else {
            self.catalog_edit_uncommitted = false;
            return Ok(());
        };
        self.catalog_edit_uncommitted = false;
        let after = self.snapshot_catalogs();
        if catalog_snapshots_eq(&baseline, &after) {
            return Ok(());
        }
        self.run(BuilderCommand::EditCatalog {
            before: baseline,
            after: Box::new(after),
        })
    }

    /// Eagerly settle any pending catalog session — used on tab-switch (so
    /// leaving a catalog tab mid-edit does not strand the burst) and before a
    /// discrete `edit_catalogs`. No-op when no uncommitted edit is pending.
    pub fn flush_catalog_session(&mut self) -> Result<(), BuilderError> {
        if self.catalog_edit_uncommitted {
            self.commit_catalog_session()
        } else {
            // Drop a baseline with no edits so a stale snapshot is not diffed
            // against a later, unrelated burst.
            self.catalog_session_baseline = None;
            Ok(())
        }
    }
}

/// Equality over the serialized form of two snapshots. `CatalogSnapshot` (and
/// the configs it nests) deliberately does not derive `PartialEq`, so the
/// no-diff guards compare canonical JSON instead — cheap relative to the user
/// edits that trigger a commit, and exact.
fn catalog_snapshots_eq(a: &CatalogSnapshot, b: &CatalogSnapshot) -> bool {
    serde_json::to_vec(a).unwrap_or_default() == serde_json::to_vec(b).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    //! Audit finding #8 — catalog-edit undo/redo round-trips.
    //!
    //! Each test drives the *public* command-bus surface (`edit_catalogs` /
    //! `note_catalog_edit` + `begin_catalog_session` + `undo` / `redo`) so it
    //! exercises the real apply → swap → revert → re-apply path, not a shortcut.

    use super::*;
    use sectorforge::economy::{EconomyConfig, ResourceVector};
    use sectorforge::factions::{FactionDef, FactionsFile};
    use sectorforge::ids::{FactionId, WorldId};
    use sectorforge::personae::PersonaeConfig;

    fn blank() -> BuilderState {
        BuilderState::new_blank("cat-test", "Catalog", "seed", 8, 8)
    }

    fn faction_row(id: &str) -> FactionDef {
        FactionDef {
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            id: FactionId::new(id),
            name: "Test".into(),
            kind: "imperial".into(),
            weight: 1.0,
            default_disposition: "lawful".into(),
            preferred_world_types: Vec::new(),
            preferred_governments: Vec::new(),
            preferred_notable_features: Vec::new(),
            style_fill: None,
            style_accent: None,
            style_glyph: None,
            style_border: None,
            legend_visible: None,
        }
    }

    /// Family 1 — an economy *config* edit (a `data/*.toml` mirror) round-trips:
    /// edit → changed → undo restores → redo re-applies.
    #[test]
    fn economy_config_edit_round_trips() {
        let mut state = blank();
        assert!(state.data_catalogs.economy.is_none());

        state
            .edit_catalogs(|snap| {
                snap.catalogs.economy = Some(EconomyConfig {
                    enabled: true,
                    ..EconomyConfig::default()
                });
            })
            .unwrap();
        assert!(
            state.data_catalogs.economy.as_ref().is_some_and(|c| c.enabled),
            "edit must install the economy catalog"
        );

        state.undo().unwrap();
        assert!(
            state.data_catalogs.economy.is_none(),
            "undo must restore the prior (absent) economy catalog"
        );

        state.redo().unwrap();
        assert!(
            state.data_catalogs.economy.as_ref().is_some_and(|c| c.enabled),
            "redo must re-install the economy catalog"
        );
    }

    /// Family 2 — a personae *config* field edit round-trips. (The persona
    /// `manual` row type is not `Default`-constructible without a `non_exhaustive`
    /// anchor, so a scalar config field stands in for the personae catalog; the
    /// faction-roster test below covers a genuine list add-row.)
    #[test]
    fn personae_config_edit_round_trips() {
        let mut state = blank();
        let mut cfg = PersonaeConfig::default();
        let baseline_cap = cfg.max_per_world;
        cfg.max_per_world = baseline_cap + 7;
        // Seed the prior value, then edit through the bus so undo has a baseline.
        state.data_catalogs.personae = Some(PersonaeConfig::default());

        state
            .edit_catalogs(|snap| {
                snap.catalogs.personae = Some(cfg.clone());
            })
            .unwrap();
        assert_eq!(
            state.data_catalogs.personae.as_ref().unwrap().max_per_world,
            baseline_cap + 7,
            "edit must install the personae config change"
        );

        state.undo().unwrap();
        assert_eq!(
            state.data_catalogs.personae.as_ref().unwrap().max_per_world,
            baseline_cap,
            "undo must restore the prior personae config"
        );

        state.redo().unwrap();
        assert_eq!(
            state.data_catalogs.personae.as_ref().unwrap().max_per_world,
            baseline_cap + 7,
            "redo must re-apply the personae config change"
        );
    }

    /// A *faction roster* add-row (a genuine list append) round-trips — the
    /// add-row family the spec asks for.
    #[test]
    fn factions_add_row_round_trips() {
        let mut state = blank();
        state.data_catalogs.factions = Some(FactionsFile {
            factions: Vec::new(),
        });

        state
            .edit_catalogs(|snap| {
                snap.catalogs
                    .factions
                    .as_mut()
                    .unwrap()
                    .factions
                    .push(faction_row("imperium"));
            })
            .unwrap();
        assert_eq!(state.data_catalogs.factions.as_ref().unwrap().factions.len(), 1);

        state.undo().unwrap();
        assert_eq!(state.data_catalogs.factions.as_ref().unwrap().factions.len(), 0);

        state.redo().unwrap();
        assert_eq!(state.data_catalogs.factions.as_ref().unwrap().factions.len(), 1);
    }

    /// The §E1 economy *override* side-table (which lives on `BuilderState`, not
    /// inside `DataCatalogs`, and is bundled into `CatalogSnapshot`) round-trips
    /// alongside the catalogs — this is the "economy-override" coverage the spec
    /// requires now that overrides ride inside `EditCatalog` rather than a
    /// separate command.
    #[test]
    fn economy_override_round_trips() {
        let mut state = blank();
        let wid = WorldId::from("world-0001");
        assert!(state.world_economy_overrides.is_empty());

        state
            .edit_catalogs(|snap| {
                snap.world_economy_overrides.insert(
                    wid.clone(),
                    ResourceVector {
                        ore: 42.0,
                        ..ResourceVector::default()
                    },
                );
            })
            .unwrap();
        assert_eq!(
            state.world_economy_overrides.get(&wid).map(|v| v.ore),
            Some(42.0),
            "edit must install the per-world economy override"
        );

        state.undo().unwrap();
        assert!(
            state.world_economy_overrides.is_empty(),
            "undo must restore the override side-table"
        );

        state.redo().unwrap();
        assert_eq!(
            state.world_economy_overrides.get(&wid).map(|v| v.ore),
            Some(42.0),
            "redo must re-install the override"
        );
    }

    /// Coalescing — a multi-frame burst of `note_catalog_edit` (a slider drag)
    /// commits **exactly one** `EditCatalog` once it settles, and that single
    /// entry undoes the whole burst.
    #[test]
    fn coalesced_burst_is_one_undo_entry() {
        let mut state = blank();
        state.data_catalogs.economy = Some(EconomyConfig::default());
        let log_before = state.command_log.len();

        // Frame 1: panel opens a session, user edits.
        state.begin_catalog_session();
        state.data_catalogs.economy.as_mut().unwrap().enabled = true;
        state.note_catalog_edit();

        // Frame 2: still dragging — another live edit, no commit yet.
        state.begin_catalog_session();
        state.data_catalogs.economy.as_mut().unwrap().feed_stability = true;
        state.note_catalog_edit();
        assert_eq!(
            state.command_log.len(),
            log_before,
            "mid-burst frames must not commit"
        );

        // Frame 3: user released last frame (no edit this frame). The frame-edge
        // is one frame wide, so frame 3 resets the flag; frame 4 settles.
        state.begin_catalog_session();
        state.begin_catalog_session();
        assert_eq!(
            state.command_log.len(),
            log_before + 1,
            "the whole burst settles into exactly one EditCatalog"
        );

        // That one entry undoes the entire burst back to the baseline.
        state.undo().unwrap();
        let econ = state.data_catalogs.economy.as_ref().unwrap();
        assert!(
            !econ.enabled && !econ.feed_stability,
            "one undo reverts every edit in the coalesced burst"
        );
    }

    /// A no-op edit (closure changes nothing) pushes **no** undo entry, so the
    /// stack is never polluted by `ensure_*`-style idempotent passes.
    #[test]
    fn no_op_edit_pushes_nothing() {
        let mut state = blank();
        state.data_catalogs.economy = Some(EconomyConfig::default());
        let log_before = state.command_log.len();
        state.edit_catalogs(|_snap| {}).unwrap();
        assert_eq!(
            state.command_log.len(),
            log_before,
            "a no-diff edit must not push an EditCatalog"
        );
    }
}
