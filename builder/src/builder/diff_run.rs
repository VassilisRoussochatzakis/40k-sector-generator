//! §DF1..§DF5 DIFF runtime: the two scratch sector slots, the diff/tick
//! configuration, and the most recently computed [`SectorDiff`].
//!
//! The diff itself is a pure derivation
//! ([`sectorforge::diff::diff_sectors_with`]) and the tick simulation
//! ([`sectorforge::conflict::advance_sector`]) is cheap, so unlike the SEARCH
//! tab this runtime is fully synchronous — there is no off-thread job. The
//! panel resolves both slots into concrete [`GeneratedSector`] values, runs
//! the diff inline, and stashes the result here so it survives tab switches.

use camino::Utf8PathBuf;

use sectorforge::diff::{DiffConfig, SectorDiff};
use sectorforge::sector_model::GeneratedSector;

/// §DF1 / §DF2 top-level mode toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffMode {
    /// §DF1: compare two independently-chosen sectors (current / snapshot /
    /// file on disk).
    #[default]
    TwoSector,
    /// §DF2: clone the current sector, advance N conflict ticks, then diff the
    /// result against the un-advanced original.
    Ticks,
}

/// §DF1: where one side of a two-sector diff comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotKind {
    /// The live in-memory sector.
    #[default]
    Current,
    /// A named snapshot from [`crate::builder::BuilderState::snapshots`].
    Snapshot,
    /// A `sector.json` loaded from disk.
    File,
}

/// A `sector.json` loaded from disk for one diff slot.
pub struct LoadedFile {
    pub path: Utf8PathBuf,
    pub sector: GeneratedSector,
}

/// Per-frame DIFF state owned by [`crate::builder::BuilderState`]. In-memory
/// only — never serialised into `sector.json`.
#[derive(Default)]
pub struct DiffState {
    pub mode: DiffMode,
    /// §DF1 before-slot source + its resolved inputs.
    pub before_kind: SlotKind,
    pub before_snapshot: Option<String>,
    pub before_file: Option<LoadedFile>,
    /// §DF1 after-slot source + its resolved inputs.
    pub after_kind: SlotKind,
    pub after_snapshot: Option<String>,
    pub after_file: Option<LoadedFile>,
    /// §DF2 number of conflict ticks to simulate before diffing.
    pub ticks: u32,
    /// §DF4 filters, mirrored into a [`DiffConfig`] at compute time.
    pub skip_worlds: bool,
    pub skip_routes: bool,
    pub min_faction_delta: f32,
    pub top_faction_deltas: u32,
    /// §DF3 latest computed diff. `None` until the user clicks "Compute diff".
    pub report: Option<SectorDiff>,
    /// Human-readable error from the most recent compute / file load.
    pub error: Option<String>,
    /// §DF5 last folder picked by the export dialog.
    pub export_dir: Option<Utf8PathBuf>,
}

impl DiffState {
    /// Construct with the same defaults as [`DiffConfig::default`] so the panel
    /// opens on the library's canonical thresholds.
    #[must_use]
    pub fn new() -> Self {
        let cfg = DiffConfig::default();
        Self {
            ticks: 1,
            skip_worlds: cfg.skip_worlds,
            skip_routes: cfg.skip_routes,
            min_faction_delta: cfg.min_faction_delta,
            top_faction_deltas: cfg.top_faction_deltas,
            ..Self::default()
        }
    }

    /// §DF4: assemble the [`DiffConfig`] from the current filter fields.
    #[must_use]
    pub fn config(&self) -> DiffConfig {
        DiffConfig {
            skip_worlds: self.skip_worlds,
            skip_routes: self.skip_routes,
            min_faction_delta: self.min_faction_delta,
            top_faction_deltas: self.top_faction_deltas,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_mirrors_diff_config_defaults() {
        let s = DiffState::new();
        let d = DiffConfig::default();
        assert_eq!(s.skip_worlds, d.skip_worlds);
        assert_eq!(s.skip_routes, d.skip_routes);
        assert_eq!(s.min_faction_delta, d.min_faction_delta);
        assert_eq!(s.top_faction_deltas, d.top_faction_deltas);
        assert_eq!(s.ticks, 1);
        assert_eq!(s.mode, DiffMode::TwoSector);
        assert_eq!(s.before_kind, SlotKind::Current);
        assert_eq!(s.after_kind, SlotKind::Current);
        assert!(s.report.is_none());
    }

    #[test]
    fn config_round_trips_filter_fields() {
        let mut s = DiffState::new();
        s.skip_worlds = true;
        s.skip_routes = true;
        s.min_faction_delta = 4.5;
        s.top_faction_deltas = 25;
        let cfg = s.config();
        assert!(cfg.skip_worlds);
        assert!(cfg.skip_routes);
        assert_eq!(cfg.min_faction_delta, 4.5);
        assert_eq!(cfg.top_faction_deltas, 25);
    }
}
