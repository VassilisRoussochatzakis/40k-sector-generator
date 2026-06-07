//! Command-bus mutation entry point + undo/redo + snapshot + auto-save.
//! Implements R4 of docs/BUILDER_REQS: every mutation routes through
//! [`BuilderState::run`], invariants re-check, the ring buffer trims, and the
//! auto-save fires when configured.

use std::path::Path;
use std::time::{Duration, Instant};

use super::super::command::BuilderCommand;
use super::super::derivation_cache::DepClass;
use super::super::errors::BuilderError;
use super::super::index::BuilderIndex;
use super::super::snapshot::Snapshot;
use super::BuilderState;

/// Audit finding #46: decide whether a mutation's [`DepClass`] set can change
/// the [`BuilderIndex`] (the ID → positional-index maps for systems / worlds /
/// routes / factions). The index is purely structural, so a class that only
/// ever feeds *derivations* without touching an indexed `GeneratedSector`
/// collection can skip the O(n) rebuild.
///
/// Conservative by construction (correctness over the micro-opt): `Regions`,
/// `RelationsCfg`, and `EconomyCfg` never mutate `sector.systems` / `worlds` /
/// `routes` / `factions`, so a command whose classes are drawn *only* from
/// those skips the rebuild. Anything carrying `SystemsWorlds`, `Factions`, or
/// `Routes` rebuilds — those classes *may* reflect an add / remove / reindex,
/// and `dep_classes` is too coarse to prove a given edit was field-only (e.g.
/// `RenameWorld` and `AddSystem` both carry `SystemsWorlds`), so we rebuild to
/// be safe. The win lands on warp-region edits and pure catalog-config edits,
/// which provably leave every indexed collection untouched.
fn dep_classes_touch_index(classes: &[DepClass]) -> bool {
    classes.iter().any(|c| {
        matches!(
            c,
            DepClass::SystemsWorlds | DepClass::Factions | DepClass::Routes
        )
    })
}

impl BuilderState {
    /// Audit finding #30: minimum wall-clock gap between two blocking
    /// auto-save `fs::write`s. A burst of commands inside one window (e.g.
    /// every tick of a value-drag) coalesces into a single deferred write;
    /// the trailing edit is captured by [`Self::flush_auto_save`] from the
    /// throttled app-update call and an unconditional close-flush, so no edit
    /// is ever lost.
    pub(crate) const AUTO_SAVE_MIN_INTERVAL: Duration = Duration::from_millis(750);

    /// Run a [`BuilderCommand`] through the command bus.
    ///
    /// Per R4 the bus enforces, in order:
    ///   (a) the live-validation debounce is armed via
    ///       [`Self::mark_validation_dirty`]. The invariant re-check that fills
    ///       [`Self::invariant_report`] (so the status bar can surface red) now
    ///       runs alongside rules-validation in [`Self::revalidate_now`] once
    ///       the debounce elapses, *not* synchronously here — `check_sector` on
    ///       a large sector is ~0.5 ms, which alone blew the §42/PERF1 1 ms
    ///       single-apply budget. Deferring it keeps every apply well under.
    ///   (b) snapshot/undo stack maintenance — the redo tail is dropped and
    ///       the command is pushed onto the log,
    ///   (c) auto-save trigger via [`Self::trigger_auto_save`] when an
    ///       `auto_save_path` is configured,
    ///   (d) cache invalidation. The generic [`crate::builder::DerivationCache`]
    ///       (map render / world preview) is flushed, and the §39 live-derivation
    ///       ledger is invalidated *precisely* by the command's
    ///       [`BuilderCommand::dep_classes`] (LD2) — only the overlays
    ///       downstream of the touched input classes are marked stale.
    ///
    /// The command itself is never rolled back here even if invariants fail —
    /// the report exposes the violation so the user can choose to undo. This
    /// matches the spec's "soft" invariant policy outside of export.
    pub fn run(&mut self, mut cmd: BuilderCommand) -> Result<(), BuilderError> {
        cmd.apply(self.sector_mut())?;
        // Audit finding #8: `EditCatalog::apply` is a no-op on the sector; the
        // catalog/override slice it carries is swapped here, sequentially after
        // the (borrowed-mut) sector apply, so the two never borrow at once. The
        // live edits already match `after` (the panel mutated `data_catalogs`
        // in place), so this clone is a harmless idempotent re-assign for the
        // first apply — but it is what makes `redo` re-install the slice.
        if let BuilderCommand::EditCatalog { after, .. } = &cmd {
            let after = (**after).clone();
            self.restore_catalogs(after);
        }
        // Audit finding #46: only rebuild the structural lookup index when this
        // command's `dep_classes` could have changed it. A field-only edge of
        // the dep table (region / catalog-config classes) leaves every indexed
        // collection untouched, so the O(n) rebuild is skipped there.
        let classes = cmd.dep_classes();
        if dep_classes_touch_index(classes) {
            self.index = BuilderIndex::rebuild(&self.sector);
        }
        self.derivation_cache.clear();
        // LD2: stale exactly the overlays this mutation's inputs feed.
        self.derivations.invalidate(classes);
        self.command_log.truncate(self.command_cursor);
        self.command_log.push(cmd);
        self.command_cursor = self.command_log.len();
        self.enforce_command_log_capacity();
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
    }

    /// §U2: drop the oldest commands when the log exceeds the configured
    /// ring-buffer capacity. The cursor and snapshot positions are shifted
    /// by the same drop-count so undo/redo references stay coherent.
    /// `command_log_capacity == 0` disables the cap (unbounded log).
    fn enforce_command_log_capacity(&mut self) {
        let cap = self.command_log_capacity;
        if cap == 0 || self.command_log.len() <= cap {
            return;
        }
        let drop = self.command_log.len() - cap;
        self.command_log.drain(0..drop);
        self.command_cursor = self.command_cursor.saturating_sub(drop);
        for snap in &mut self.snapshots {
            snap.command_log_position = snap.command_log_position.saturating_sub(drop);
        }
    }

    /// Audit finding #30: request an auto-save. No-op when no
    /// [`Self::auto_save_path`] is configured. Rather than serialize the whole
    /// `GeneratedSector` and do a blocking `fs::write` on *every* command (e.g.
    /// every tick of a value-drag), this only writes immediately when the
    /// [`Self::AUTO_SAVE_MIN_INTERVAL`] has elapsed since the last write;
    /// otherwise it marks [`Self::auto_save_pending`] so a later
    /// [`Self::flush_auto_save`] (throttled from the app update loop, or
    /// unconditionally on close) performs the trailing write. The serialize +
    /// write stays synchronous — off-thread is out of scope — the win is
    /// eliminating per-keystroke writes while guaranteeing a final flush.
    pub fn trigger_auto_save(&mut self) {
        if self.auto_save_path.is_none() {
            return;
        }
        // Always owe a write after a mutation; the question is only *when*.
        self.auto_save_pending = true;
        // Audit finding #22: while a re-establishment burst is in progress
        // (undo/redo recomputing in-sector derived state) never write — defer
        // so only the final, fully-consistent sector reaches disk.
        if self.suspend_auto_save_writes {
            return;
        }
        let due = self
            .last_auto_save
            .is_none_or(|t| t.elapsed() >= Self::AUTO_SAVE_MIN_INTERVAL);
        if due {
            self.write_auto_save_now();
        }
    }

    /// Audit finding #30: throttled flush, called from the app's per-frame
    /// update loop. Performs the deferred auto-save write *iff* one is pending
    /// and the [`Self::AUTO_SAVE_MIN_INTERVAL`] has elapsed since the last
    /// write — bounding blocking `fs::write` to once per interval while still
    /// catching the trailing edit of a burst within one interval of the user
    /// going idle.
    ///
    /// Returns `Some(remaining)` when a write is still owed but the interval
    /// has not elapsed, so the caller can schedule a repaint after `remaining`
    /// to guarantee the flush fires even if the app would otherwise sit idle
    /// (no further user input to drive a frame). Returns `None` when nothing is
    /// owed or the pending write was performed this call.
    pub fn tick_auto_save(&mut self) -> Option<Duration> {
        if !self.auto_save_pending {
            return None;
        }
        let remaining = match self.last_auto_save {
            None => Duration::ZERO,
            Some(t) => Self::AUTO_SAVE_MIN_INTERVAL.saturating_sub(t.elapsed()),
        };
        if remaining.is_zero() {
            self.write_auto_save_now();
            None
        } else {
            Some(remaining)
        }
    }

    /// Audit finding #30: force the pending auto-save to disk *now*, ignoring
    /// the debounce interval. Wired to app close/exit so the trailing edit of
    /// a burst is never dropped. No-op when nothing is owed or no path is set.
    pub fn flush_auto_save(&mut self) {
        if self.auto_save_pending && self.auto_save_path.is_some() {
            self.write_auto_save_now();
        }
    }

    /// Audit finding #30: the actual serialize + blocking write to
    /// [`Self::auto_save_path`] as pretty JSON. Clears [`Self::auto_save_pending`]
    /// and stamps [`Self::last_auto_save`] on success so the debounce window
    /// restarts. On failure, leaves `dirty = true` and `auto_save_pending`
    /// set (so the next tick/flush retries) and stores the error in
    /// `feedback.last_save_error` for the status bar to render.
    fn write_auto_save_now(&mut self) {
        let Some(path) = self.auto_save_path.as_ref() else {
            return;
        };
        let text = match serde_json::to_string_pretty(&self.sector) {
            Ok(t) => t,
            Err(e) => {
                self.feedback.last_save_error = Some(format!("auto-save serialize: {e}"));
                return;
            }
        };
        match std::fs::write(Path::new(path.as_std_path()), text) {
            Ok(()) => {
                self.dirty = false;
                self.auto_save_pending = false;
                self.last_auto_save = Some(Instant::now());
                self.feedback.last_save_error = None;
            }
            Err(e) => {
                self.feedback.last_save_error = Some(format!("auto-save write to {path}: {e}"));
            }
        }
    }

    /// Undo the most recent command. No-op when the cursor is at 0.
    pub fn undo(&mut self) -> Result<(), BuilderError> {
        if self.command_cursor == 0 {
            return Ok(());
        }
        let cmd = &self.command_log[self.command_cursor - 1];
        let classes = cmd.dep_classes();
        // Audit finding #8: clone the prior catalog snapshot out *before*
        // reverting the sector so we never hold a `&self.command_log` borrow and
        // a `&mut self` borrow at once. `revert` is a no-op on the sector for
        // `EditCatalog`; the slice restore happens right after.
        let restore_before = match cmd {
            BuilderCommand::EditCatalog { before, .. } => Some((**before).clone()),
            _ => None,
        };
        cmd.revert(&mut self.sector)?;
        if let Some(before) = restore_before {
            self.restore_catalogs(before);
        }
        self.command_cursor -= 1;
        // Audit finding #46: gate the structural index rebuild on dep_classes —
        // a reverted field-only edit cannot have changed an indexed collection.
        if dep_classes_touch_index(classes) {
            self.index = BuilderIndex::rebuild(&self.sector);
        }
        self.derivation_cache.clear();
        self.derivations.invalidate(classes);
        self.dirty = true;
        self.mark_validation_dirty();
        // Audit finding #22: re-establish the derived state that lives *inside*
        // the sector (and is therefore serialized into `sector.json`) so an
        // auto-save flush can never persist a sector whose `economy` /
        // `relations` / `chronicle` describe the pre-undo graph. Must run before
        // `trigger_auto_save` so the (possibly immediate) write sees the
        // recomputed, self-consistent pair.
        self.reestablish_persisted_derived();
        self.trigger_auto_save();
        Ok(())
    }

    /// Re-apply a previously undone command. No-op past the log tail.
    pub fn redo(&mut self) -> Result<(), BuilderError> {
        if self.command_cursor >= self.command_log.len() {
            return Ok(());
        }
        let mut cmd = self.command_log[self.command_cursor].clone();
        let classes = cmd.dep_classes();
        cmd.apply(self.sector_mut())?;
        // Audit finding #8: re-install the catalog/override slice the redone
        // `EditCatalog` carries (its `apply` is a no-op on the sector).
        if let BuilderCommand::EditCatalog { after, .. } = &cmd {
            let after = (**after).clone();
            self.restore_catalogs(after);
        }
        self.command_log[self.command_cursor] = cmd;
        self.command_cursor += 1;
        // Audit finding #46: gate the structural index rebuild on dep_classes —
        // a re-applied field-only edit cannot have changed an indexed collection.
        if dep_classes_touch_index(classes) {
            self.index = BuilderIndex::rebuild(&self.sector);
        }
        self.derivation_cache.clear();
        self.derivations.invalidate(classes);
        self.dirty = true;
        self.mark_validation_dirty();
        // Audit finding #22: same as `undo` — recompute the in-sector derived
        // state before any auto-save flush so the serialized artifact is
        // self-consistent with the redone graph.
        self.reestablish_persisted_derived();
        self.trigger_auto_save();
        Ok(())
    }

    /// Audit finding #22: after an `undo` / `redo` re-establish the derived
    /// state that is stored *inside* the `GeneratedSector` and thus serialized
    /// into `sector.json` — `economy` (`sector.economy`), `relations`
    /// (`sector.relations`), and `chronicle` (`sector.chronicle`). Each is
    /// recomputed via [`Self::ensure_fresh`], which is a no-op unless the
    /// command's `dep_classes` invalidation actually staled that overlay *and*
    /// it was previously derived (a cold overlay's in-sector value was already
    /// restored verbatim by `revert`/`apply`). The other tracked overlays
    /// (`personae` / `hooks` / …) live on `BuilderState` fields, not in the
    /// sector, so they do not affect the persisted artifact and are refreshed
    /// lazily by the normal LD3/LD4 pump.
    fn reestablish_persisted_derived(&mut self) {
        use crate::builder::derivation_cache::DerivationKind;
        // Suspend immediate auto-save writes so the internal `recompute_*`
        // writes during this burst collapse into the single trailing
        // `trigger_auto_save` the caller issues against the consistent sector —
        // never persisting an Economy-fresh / Relations-stale intermediate (the
        // §E4 feed-stability nudge re-stales Relations + History, which the
        // ordering below then refreshes against the nudged graph).
        let prev = std::mem::replace(&mut self.suspend_auto_save_writes, true);
        self.ensure_fresh(DerivationKind::Economy);
        self.ensure_fresh(DerivationKind::Relations);
        self.ensure_fresh(DerivationKind::History);
        self.suspend_auto_save_writes = prev;
    }

    /// Capture a named snapshot at the current command-log position.
    pub fn snapshot(&mut self, name: impl Into<String>) {
        self.snapshots.push(Snapshot::new(
            name,
            (*self.sector).clone(),
            self.command_cursor,
        ));
    }

    /// Revert to a named snapshot: restores the sector and rewinds the
    /// command cursor. Subsequent `run` calls evict the redo tail.
    pub fn revert_to_snapshot(&mut self, name: &str) -> bool {
        let Some(snap) = self.snapshots.iter().find(|s| s.name == name).cloned() else {
            return false;
        };
        self.sector = snap.sector.into();
        self.command_cursor = snap.command_log_position.min(self.command_log.len());
        self.derivation_cache.clear();
        self.derivations.invalidate_all();
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
        true
    }
}
