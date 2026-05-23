//! GUI builder foundation (Phase A of BUILDER_REQS.txt).
//!
//! The builder owns a single working `GeneratedSector` plus the configs that
//! produced it. Every mutation routes through [`BuilderCommand`], which
//! re-checks invariants, records undo state, and invalidates derivation
//! caches. Long-running work (search, compose, large regen) lives in
//! [`crate::jobs`] off the UI thread.
//!
//! See `BUILDER_REQS.txt` §D5 / §D6 for the contract.

pub mod command;
pub mod data_catalogs;
pub mod derivation_cache;
pub mod errors;
pub mod file_watcher;
pub mod index;
pub mod panels;
pub mod preferences;
pub mod project_io;
pub mod session;
pub mod snapshot;
pub mod state;

pub use command::BuilderCommand;
pub use data_catalogs::DataCatalogs;
pub use derivation_cache::{digest_input, DerivationCache};
pub use errors::BuilderError;
pub use file_watcher::{FileChange, FileWatcher};
pub use index::BuilderIndex;
pub use preferences::Preferences;
pub use project_io::{
    drain_watcher_events, new_project, open_project, reload_catalog, save_project, save_project_as,
    NewProjectOptions,
};
pub use session::{load_session, save_session, SessionFile};
pub use snapshot::Snapshot;
pub use state::{BuilderState, ModalKind};
