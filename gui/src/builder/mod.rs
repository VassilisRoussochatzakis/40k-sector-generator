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
pub mod index;
pub mod session;
pub mod snapshot;
pub mod state;

pub use command::BuilderCommand;
pub use data_catalogs::DataCatalogs;
pub use derivation_cache::DerivationCache;
pub use errors::BuilderError;
pub use index::BuilderIndex;
pub use session::{load_session, save_session, SessionFile};
pub use snapshot::Snapshot;
pub use state::{BuilderState, ModalKind};
