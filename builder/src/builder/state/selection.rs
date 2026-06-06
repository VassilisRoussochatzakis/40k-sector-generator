//! Selection helpers on [`super::BuilderState`]: single-focus and multi-select
//! toggles for the SYSTEM tab and map, plus §LINK cross-tab navigation.

use sectorforge::ids::SystemId;

use sectorforge::ids::FactionId;

use super::nav::EntityRef;
use super::types::BuilderTab;
use super::BuilderState;

const NAV_STACK_CAP: usize = 64;

impl BuilderState {
    /// §S1: focus a single system. Replaces any multi-selection with `{id}`.
    pub fn focus_system(&mut self, id: SystemId) {
        self.selection.systems.clear();
        self.selection.systems.insert(id.clone());
        self.selection.system_id = Some(id);
    }

    /// §S4: shift-click toggle — add/remove the system from `selected_systems`
    /// while leaving `selected_system_id` pointing at the most recent pick.
    pub fn toggle_system_selection(&mut self, id: SystemId) {
        if self.selection.systems.contains(&id) {
            self.selection.systems.remove(&id);
            if self.selection.system_id.as_ref() == Some(&id) {
                self.selection.system_id = self.selection.systems.iter().next().cloned();
            }
        } else {
            self.selection.systems.insert(id.clone());
            self.selection.system_id = Some(id);
        }
    }

    /// §CTX1 §10 — switch the active tab and dismiss any open context menu so
    /// the menu does not linger off-tab. Use this in place of a direct
    /// `state.active_tab = ...` write whenever the source of truth is a user
    /// action (tab-bar click, focus-entity, keyboard shortcut). Tests may still
    /// poke the field directly.
    pub fn set_active_tab(&mut self, tab: BuilderTab) {
        if self.active_tab != tab {
            self.map_view.sector_context_menu = None;
            self.map_view.system_context_menu = None;
        }
        self.active_tab = tab;
    }

    /// Set the selected faction without changing the active tab (unlike
    /// [`Self::focus_entity`]). Backs the builder bin's `--select-faction` dev
    /// flag; use this instead of writing the now-`pub(crate)` field directly.
    pub fn select_faction(&mut self, id: Option<FactionId>) {
        self.selection.faction_id = id;
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
            self.selection.nav_back_stack.push(prev);
            if self.selection.nav_back_stack.len() > NAV_STACK_CAP {
                self.selection.nav_back_stack.remove(0);
            }
            self.selection.nav_forward_stack.clear();
        }
        self.apply_focus(target);
    }

    /// §LINK3 — pop the most recent focus off the back-stack, restore it, and
    /// push the current focus onto the forward stack.
    pub fn nav_back(&mut self) {
        let Some(prev) = self.selection.nav_back_stack.pop() else {
            return;
        };
        if let Some(cur) = self.current_focus() {
            self.selection.nav_forward_stack.push(cur);
            if self.selection.nav_forward_stack.len() > NAV_STACK_CAP {
                self.selection.nav_forward_stack.remove(0);
            }
        }
        self.apply_focus(prev);
    }

    /// §LINK3 — symmetric to [`Self::nav_back`].
    pub fn nav_forward(&mut self) {
        let Some(next) = self.selection.nav_forward_stack.pop() else {
            return;
        };
        if let Some(cur) = self.current_focus() {
            self.selection.nav_back_stack.push(cur);
            if self.selection.nav_back_stack.len() > NAV_STACK_CAP {
                self.selection.nav_back_stack.remove(0);
            }
        }
        self.apply_focus(next);
    }

    /// Snapshot the currently-focused entity on the active tab. Drives the
    /// back-stack so that returning to a tab restores the previous focus.
    pub fn current_focus(&self) -> Option<EntityRef> {
        match self.active_tab {
            BuilderTab::System => self.selection.system_id.clone().map(EntityRef::System),
            BuilderTab::World => match (&self.selection.system_id, &self.selection.world_id) {
                (Some(s), Some(w)) => Some(EntityRef::World {
                    system: s.clone(),
                    world: w.clone(),
                }),
                _ => None,
            },
            BuilderTab::Factions => self.selection.faction_id.clone().map(EntityRef::Faction),
            BuilderTab::Routes => self.selection.route_id.clone().map(EntityRef::Route),
            BuilderTab::Regions => self.selection.region_id.clone().map(EntityRef::Region),
            BuilderTab::Subsectors => self
                .selection
                .subsector_id
                .clone()
                .map(EntityRef::Subsector),
            BuilderTab::Personae => self.selection.persona_id.clone().map(EntityRef::Persona),
            BuilderTab::History => self
                .selection
                .history_event
                .clone()
                .map(EntityRef::HistoryEvent),
            BuilderTab::Hooks => self.selection.hook_id.clone().map(EntityRef::Hook),
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
                self.selection.world_id = Some(world.clone());
            }
            EntityRef::Faction(fid) => {
                self.selection.faction_id = Some(fid.clone());
            }
            EntityRef::Route(rid) => {
                self.selection.route_id = Some(rid.clone());
            }
            EntityRef::Region(rid) => {
                self.selection.region_id = Some(rid.clone());
            }
            EntityRef::Subsector(sid) => {
                self.selection.subsector_id = Some(sid.clone());
            }
            EntityRef::Persona(pid) => {
                self.selection.persona_id = Some(pid.clone());
            }
            EntityRef::HistoryEvent(eid) => {
                self.selection.history_event = Some(eid.clone());
            }
            EntityRef::Hook(hid) => {
                self.selection.hook_id = Some(hid.clone());
            }
            EntityRef::Tab(_) => {}
        }
        self.set_active_tab(target.target_tab());
    }
}
