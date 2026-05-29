//! §A1..§A4 ANALYTICS runtime: the editable [`AnalyzeConfig`], the "strict"
//! CI-parity toggle, the most recently computed [`SectorAnalysis`], and the
//! last export folder.
//!
//! The analysis itself is a pure derivation
//! ([`sectorforge::analyze_sector_with`]) over the live sector, so — like the
//! DIFF tab and unlike SEARCH — this runtime is fully synchronous: the panel
//! computes inline and stashes the report here so it survives tab switches.

use camino::Utf8PathBuf;

use sectorforge::analytics::{AnalyzeConfig, FlagSeverity, SectorAnalysis};

/// Per-frame ANALYTICS state owned by [`crate::builder::BuilderState`].
/// In-memory only — never serialised into `sector.json`. The `Default` impl
/// opens on [`AnalyzeConfig::default`] (its manual impl), matching the CLI's
/// canonical thresholds.
#[derive(Default)]
pub struct AnalyticsState {
    /// §A2 editable analyze config. Opens on [`AnalyzeConfig::default`] so the
    /// builder matches the CLI's canonical thresholds; a project's own
    /// `[analyze]` block is layered in by [`Self::seed_from_project`].
    pub config: AnalyzeConfig,
    /// §A3 strict toggle. When set, any health flag is treated as an error for
    /// CI parity with `sectorforge analyze --strict` and surfaced as a red
    /// banner. Purely a display gate — the builder has no exit code.
    pub strict: bool,
    /// §A1 latest computed analysis. `None` until the user clicks "Analyze".
    pub report: Option<SectorAnalysis>,
    /// Human-readable error from the most recent export.
    pub error: Option<String>,
    /// §A4 last folder picked by the export dialog.
    pub export_dir: Option<Utf8PathBuf>,
}

impl AnalyticsState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// §A2: replace the editable config with the project's own `[analyze]`
    /// block, clearing any stale report so the chart never lies about which
    /// thresholds fed it.
    pub fn seed_from_project(&mut self, project: &AnalyzeConfig) {
        self.config = project.clone();
        self.report = None;
    }

    /// §A3: count health flags that count as failures. With `strict`, every
    /// flag fails (CLI parity); otherwise only `Error`-severity flags do.
    #[must_use]
    pub fn failing_flag_count(&self) -> usize {
        let Some(report) = self.report.as_ref() else {
            return 0;
        };
        report
            .health_flags
            .iter()
            .filter(|f| self.strict || f.severity == FlagSeverity::Error)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_mirrors_analyze_config_defaults() {
        let s = AnalyticsState::new();
        let d = AnalyzeConfig::default();
        assert_eq!(s.config.warn_faction_share, d.warn_faction_share);
        assert_eq!(s.config.warn_if_disconnected, d.warn_if_disconnected);
        assert_eq!(s.config.warn_if_articulation, d.warn_if_articulation);
        assert_eq!(s.config.tiny_sector_threshold, d.tiny_sector_threshold);
        assert_eq!(s.config.warn_contested_ratio, d.warn_contested_ratio);
        assert!(!s.strict);
        assert!(s.report.is_none());
        assert!(s.export_dir.is_none());
    }

    #[test]
    fn failing_flag_count_zero_without_report() {
        assert_eq!(AnalyticsState::new().failing_flag_count(), 0);
    }
}
