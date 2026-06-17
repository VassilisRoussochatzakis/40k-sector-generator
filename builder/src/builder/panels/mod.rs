//! Builder panel modules (R10, §41 / §N2).
//!
//! Every panel in this directory is a free function with the contract:
//!
//! ```ignore
//! pub fn show(ui: &mut egui::Ui, state: &mut crate::builder::BuilderState) {
//!     // read state.sector / state.config / state.derivations
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

mod text_buf;
pub use text_buf::{persistent_multiline, persistent_singleline, persistent_text_clear};

pub(crate) mod presence_widgets;

/// The selectable `SystemState` values, in display order. Single source shared
/// by the CONTROL and HISTORY panels (§E6) — previously a byte-identical const
/// in each, so a new variant only had to be added in one of two places.
pub(crate) const SYSTEM_STATES: &[sectorforge::sector_model::SystemState] = &[
    sectorforge::sector_model::SystemState::Pacified,
    sectorforge::sector_model::SystemState::Fragmented,
    sectorforge::sector_model::SystemState::Blockaded,
    sectorforge::sector_model::SystemState::Warzone,
    sectorforge::sector_model::SystemState::Infiltrated,
    sectorforge::sector_model::SystemState::Quarantined,
    sectorforge::sector_model::SystemState::Uncharted,
];

pub mod conflict_resolver;
pub mod invariants;
pub mod new_project;
pub mod open_project;
pub mod preferences;
pub mod project_tree;
pub mod save_project;
pub mod shortcuts;
pub mod status;
pub mod validation;

// §N1 router + per-tab modules.
pub mod analytics;
pub mod briefing;
pub mod command_palette;
pub mod conflict;
pub mod control;
pub mod diff;
pub mod economy;
pub mod export;
pub mod factions;
pub mod files;
pub mod generate_random;
pub mod generation;
pub mod history;
pub mod hooks;
pub mod intel;
pub mod interestingness;
pub mod iterative_gen;
pub mod map;
pub mod missions;
pub mod nav;
pub mod orbital;
pub mod personae;
pub mod project;
pub mod prose;
pub mod regions;
pub mod relations;
pub(crate) mod roster;
pub mod routes;
pub mod search;
pub mod segmentum;
pub mod sites;
pub mod subsectors;
pub mod surface_regions;
pub mod system;
pub mod system_map;
pub mod world;
pub mod worlds_editor;
