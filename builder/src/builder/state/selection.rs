//! Selection helpers on [`super::BuilderState`]: single-focus and multi-select
//! toggles for the SYSTEM tab and map.

use sectorforge::ids::SystemId;

use super::BuilderState;

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
}
