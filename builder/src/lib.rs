#![forbid(unsafe_code)]

pub mod app;
pub mod builder;

pub use app::BuilderApp;

// exposed for benches/tests — the `builder_mutations` bench (T7/§40, PERF1/§42)
// links this lib and can only see public items, so the command bus, the
// archetype-apply mask, the derivation-kind enum, and the session round-trip
// constructor below are surfaced here. Kept intentionally minimal.
pub use builder::command::{ArchetypeApplyFlags, BuilderCommand};
// exposed for benches/tests
pub use builder::derivation_cache::{DepClass, DerivationKind};
// exposed for benches/tests
pub use builder::session::{load_session, save_session, SessionFile};
// exposed for benches/tests
pub use builder::state::BuilderState;

/// Build a [`BuilderState`] that wraps an already-generated sector plus the
/// [`AppConfig`](sectorforge::config::AppConfig) that produced it.
///
/// Exposed for benches/tests (T7/§40): the public constructors are
/// [`BuilderState::new_blank`] (empty sector) and the `.sgforge` session
/// loader, neither of which lets a bench seed a *large* generated sector
/// without touching disk. This composes the existing public [`SessionFile`]
/// envelope — the same path `load_session` takes — so it adds no new logic and
/// keeps every document mutation on the command bus. The blank `version` field
/// is irrelevant: [`SessionFile::into_state`] never reads it (only
/// `load_session` checks the on-disk version).
#[must_use]
pub fn state_from_generated_sector(
    sector: sectorforge::sector_model::GeneratedSector,
    config: sectorforge::config::AppConfig,
) -> BuilderState {
    SessionFile {
        version: 0,
        sector,
        config,
        command_log: Vec::new(),
        command_cursor: 0,
        snapshots: Vec::new(),
        pinned_systems: std::collections::BTreeSet::new(),
        pinned_worlds: std::collections::BTreeSet::new(),
        project_path: None,
    }
    .into_state()
}
