//! Read-only derivations over a generated sector.
//!
//! Every submodule here is a pure function of an immutable `GeneratedSector`
//! (plus optional config) → some `Report` / overlay type. Side-effect-free,
//! re-runnable, deterministic.
//!
//! Note: `economy` and `relations` are grouped here even though they were not
//! in the original refactor plan's listing — both are pure derivations over
//! sector state and read more naturally next to `analytics` / `interestingness`
//! than as top-level modules.

pub mod analytics;
pub mod briefing;
pub mod conflict;
pub mod control;
pub mod economy;
pub mod history;
pub mod hooks;
pub mod importance;
pub mod influence_field;
pub mod intel;
pub mod interestingness;
pub mod missions;
pub mod personae;
pub mod power_projection;
pub mod prose;
pub mod relations;
pub mod route_control;
pub mod scores;
pub mod search;
pub mod stability;
