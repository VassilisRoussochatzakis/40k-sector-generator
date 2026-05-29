//! GUI builder foundation (Phase A of docs/BUILDER_REQS.txt).
//!
//! The builder owns a single working `GeneratedSector` plus the configs that
//! produced it. Every mutation routes through [`BuilderCommand`], which
//! re-checks invariants, records undo state, and invalidates derivation
//! caches. Long-running work (search, compose, large regen) lives in
//! [`crate::jobs`] off the UI thread.
//!
//! See `docs/BUILDER_REQS.txt` §D5 / §D6 for the contract.

pub mod analytics_run;
pub mod command;
pub mod data_catalogs;
pub mod derivation_cache;
pub mod diff_run;
pub mod errors;
pub mod file_watcher;
pub mod index;
pub mod panels;
pub mod preferences;
pub mod preview;
pub mod project_io;
pub mod search_run;
pub mod segmentum_run;
pub mod session;
pub mod snapshot;
pub mod state;
pub mod workspace;

pub use analytics_run::AnalyticsState;
pub use command::BuilderCommand;
pub use data_catalogs::DataCatalogs;
pub use derivation_cache::{digest_input, DerivationCache};
pub use diff_run::{DiffMode, DiffState, LoadedFile, SlotKind};
pub use errors::BuilderError;
pub use file_watcher::{FileChange, FileWatcher};
pub use index::BuilderIndex;
pub use preferences::Preferences;
pub use preview::{derive_reroll_seed, PreviewJobResult, PreviewState};
pub use project_io::{
    drain_watcher_events, new_project, open_project, reload_catalog, save_project, save_project_as,
    NewProjectOptions,
};
pub use search_run::{NewConstraintKind, SearchJobResult, SearchState};
pub use segmentum_run::{ComposeJobResult, NewLinkDraft, SegmentumState};
pub use session::{load_session, save_session, SessionFile};
pub use snapshot::Snapshot;
pub use state::{BuilderState, ModalKind, PartialRegenRect};
pub use workspace::BuilderWorkspace;
