//! Selection helpers on [`super::BuilderState`]: single-focus and multi-select
//! toggles for the SYSTEM tab and map, plus §LINK cross-tab navigation.

use sectorforge::ids::SystemId;

use super::nav::EntityRef;
use super::types::BuilderTab;
use super::BuilderState;

const NAV_STACK_CAP: usize = 64;

impl BuilderState {
    /// §S1: focus a single system. Replaces any multi-selection with `{id}`.
    pub fn focus_system(&mut self, id: SystemId) {
        self.selected_systems.clear();
        self.selected_systems.insert(id.clone());
        self.selected_system_id = Some(id);
    }

    /// §S4: shift-click toggle — add/remove the system from `selected_systems`
    /// while leaving `selected_system_id` pointing at the most recent pick.
    pub fn toggle_system_selection(&mut self, id: SystemId) {
        if self.selected_systems.contains(&id) {
            self.selected_systems.remove(&id);
            if self.selected_system_id.as_ref() == Some(&id) {
                self.selected_system_id = self.selected_systems.iter().next().cloned();
            }
        } else {
            self.selected_systems.insert(id.clone());
            self.selected_system_id = Some(id);
        }
    }

    /// §LINK2 — navigate to an entity: switch tabs and populate the matching
    /// selection field. Idempotent. Pushes prior focus onto `nav_back_stack`
    /// (capped at 64) and clears `nav_forward_stack`.
    pub fn focus_entity(&mut self, target: EntityRef) {
        let prev = self.current_focus();
        if prev.as_ref() == Some(&target) {
            return;
        }
        if let Some(prev) = prev {
            self.nav_back_stack.push(prev);
            if self.nav_back_stack.len() > NAV_STACK_CAP {
                self.nav_back_stack.remove(0);
            }
            self.nav_forward_stack.clear();
        }
        self.apply_focus(target);
    }

    /// §LINK3 — pop the most recent focus off the back-stack, restore it, and
    /// push the current focus onto the forward stack.
    pub fn nav_back(&mut self) {
        let Some(prev) = self.nav_back_stack.pop() else {
            return;
        };
        if let Some(cur) = self.current_focus() {
            self.nav_forward_stack.push(cur);
            if self.nav_forward_stack.len() > NAV_STACK_CAP {
                self.nav_forward_stack.remove(0);
            }
        }
        self.apply_focus(prev);
    }

    /// §LINK3 — symmetric to [`Self::nav_back`].
    pub fn nav_forward(&mut self) {
        let Some(next) = self.nav_forward_stack.pop() else {
            return;
        };
        if let Some(cur) = self.current_focus() {
            self.nav_back_stack.push(cur);
            if self.nav_back_stack.len() > NAV_STACK_CAP {
                self.nav_back_stack.remove(0);
            }
        }
        self.apply_focus(next);
    }

    /// Snapshot the currently-focused entity on the active tab. Drives the
    /// back-stack so that returning to a tab restores the previous focus.
    pub fn current_focus(&self) -> Option<EntityRef> {
        match self.active_tab {
            BuilderTab::System => self.selected_system_id.clone().map(EntityRef::System),
            BuilderTab::World => match (&self.selected_system_id, &self.selected_world_id) {
                (Some(s), Some(w)) => Some(EntityRef::World {
                    system: s.clone(),
                    world: w.clone(),
                }),
                _ => None,
            },
            BuilderTab::Factions => self.selected_faction_id.clone().map(EntityRef::Faction),
            BuilderTab::Routes => self.selected_route_id.clone().map(EntityRef::Route),
            BuilderTab::Regions => self.selected_region_id.clone().map(EntityRef::Region),
            BuilderTab::Subsectors => self.selected_subsector_id.clone().map(EntityRef::Subsector),
            BuilderTab::Personae => self.selected_persona_id.clone().map(EntityRef::Persona),
            BuilderTab::History => self
                .selected_history_event
                .clone()
                .map(EntityRef::HistoryEvent),
            BuilderTab::Hooks => self.selected_hook_id.clone().map(EntityRef::Hook),
            other => Some(EntityRef::Tab(other)),
        }
    }

    fn apply_focus(&mut self, target: EntityRef) {
        match &target {
            EntityRef::System(id) => {
                self.focus_system(id.clone());
            }
            EntityRef::World { system, world } => {
                self.focus_system(system.clone());
                self.selected_world_id = Some(world.clone());
            }
            EntityRef::Faction(fid) => {
                self.selected_faction_id = Some(fid.clone());
            }
            EntityRef::Route(rid) => {
                self.selected_route_id = Some(rid.clone());
            }
            EntityRef::Region(rid) => {
                self.selected_region_id = Some(rid.clone());
            }
            EntityRef::Subsector(sid) => {
                self.selected_subsector_id = Some(sid.clone());
            }
            EntityRef::Persona(pid) => {
                self.selected_persona_id = Some(pid.clone());
            }
            EntityRef::HistoryEvent(eid) => {
                self.selected_history_event = Some(eid.clone());
            }
            EntityRef::Hook(hid) => {
                self.selected_hook_id = Some(hid.clone());
            }
            EntityRef::Tab(_) => {}
        }
        self.active_tab = target.target_tab();
    }
}
