//! §LINK1 — cross-tab navigation: entity references + dispatch helpers.
//!
//! `EntityRef` names every entity that the builder UI can link to. A click on
//! a link calls [`super::BuilderState::focus_entity`] (in
//! [`super::selection`]) which switches the active tab and populates the
//! matching `selected_*_id` field in one shot. The previous focus is pushed
//! onto a back-stack so Alt+← walks it.

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};

use super::types::BuilderTab;

/// A reference to any entity the builder can jump to. The variants intentionally
/// cover surfaces that do not yet have inspector panels (Persona, Hook) so
/// inbound links land first-class when the Phase D panels ship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityRef {
    System(SystemId),
    World {
        system: SystemId,
        world: WorldId,
    },
    Faction(FactionId),
    Route(RouteId),
    Region(String),
    Subsector(String),
    Persona(String),
    HistoryEvent(String),
    Hook(String),
    /// Tab-only jump with no entity selection change.
    Tab(BuilderTab),
}

impl EntityRef {
    /// Tab that owns this entity. Used by `focus_entity` to set `active_tab`.
    pub fn target_tab(&self) -> BuilderTab {
        match self {
            EntityRef::System(_) => BuilderTab::System,
            EntityRef::World { .. } => BuilderTab::World,
            EntityRef::Faction(_) => BuilderTab::Factions,
            EntityRef::Route(_) => BuilderTab::Routes,
            EntityRef::Region(_) => BuilderTab::Regions,
            EntityRef::Subsector(_) => BuilderTab::Subsectors,
            EntityRef::Persona(_) => BuilderTab::Personae,
            EntityRef::HistoryEvent(_) => BuilderTab::History,
            EntityRef::Hook(_) => BuilderTab::Hooks,
            EntityRef::Tab(tab) => *tab,
        }
    }
}
