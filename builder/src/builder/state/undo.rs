//! Command-bus mutation entry point + undo/redo + snapshot + auto-save.
//! Implements R4 of docs/BUILDER_REQS: every mutation routes through
//! [`BuilderState::run`], invariants re-check, the ring buffer trims, and the
//! auto-save fires when configured.

use std::path::Path;

use super::super::command::BuilderCommand;
use super::super::errors::BuilderError;
use super::super::index::BuilderIndex;
use super::super::snapshot::Snapshot;
use super::BuilderState;

impl BuilderState {
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
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        // LD2: stale exactly the overlays this mutation's inputs feed.
        self.derivations.invalidate(cmd.dep_classes());
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

    /// Write the sector to [`Self::auto_save_path`] as pretty JSON when set.
    /// No-op when no path is configured. On failure, leaves `dirty = true`
    /// (so the next event retries) and stores the error in
    /// `feedback.last_save_error` for the status bar to render.
    pub fn trigger_auto_save(&mut self) {
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
        cmd.revert(&mut self.sector)?;
        self.command_cursor -= 1;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.derivations.invalidate(classes);
        self.dirty = true;
        self.mark_validation_dirty();
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
        self.command_log[self.command_cursor] = cmd;
        self.command_cursor += 1;
        self.index = BuilderIndex::rebuild(&self.sector);
        self.derivation_cache.clear();
        self.derivations.invalidate(classes);
        self.dirty = true;
        self.mark_validation_dirty();
        self.trigger_auto_save();
        Ok(())
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
