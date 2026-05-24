//! Builder panel modules (R10, §41 / §N2).
//!
//! Every panel in this directory is a free function with the contract:
//!
//! ```ignore
//! pub fn show(ui: &mut egui::Ui, state: &mut crate::builder::BuilderState) {
//!     // read state.sector / state.config / state.derivation_cache
//!     // mutate via state.run(BuilderCommand::...)
//!     // never hold module-level mutable state
//!     // never carry raw String IDs across boundaries — use the typed
//!     // SystemId / WorldId / RouteId / FactionId from sectorforge::ids
//! }
//! ```
//!
//! Modal state (file pickers, confirmation prompts, picker dialogs) lives in
//! [`super::ModalKind`] — never in module-level statics.
//!
//! As Phase B / Phase C panels land they get added below.

pub mod conflict_resolver;
pub mod new_project;
pub mod open_project;
pub mod preferences;
pub mod project_tree;
pub mod save_project;
pub mod shortcuts;
pub mod status;
