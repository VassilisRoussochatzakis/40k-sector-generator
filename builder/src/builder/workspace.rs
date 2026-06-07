//! §G6 workspace: a ring of open [`BuilderState`] sessions.
//!
//! The §6 G6 requirement says "New sector from preset" inside the builder must
//! reuse the §P1 wizard logic *and* keep the current builder open in a new
//! tab. A single [`BuilderState`] cannot host that on its own — the host shell
//! needs a workspace concept that owns several states and routes the active
//! tab strip and inspector panels to the focused one.
//!
//! [`BuilderWorkspace`] is the minimal implementation: a `Vec<BuilderState>` +
//! an active index. [`Self::active`] / [`Self::active_mut`] return the current
//! focused state so existing nav code (`panels/nav.rs`) can call
//! `show_active_panel(ui, workspace.active_mut())` without further refactor.
//! `push` appends a fresh state and focuses it, `switch_to` re-points the
//! cursor, and `close_active` drops the focused state while collapsing the
//! cursor to the previous slot.
//!
//! Host wiring lives in [`crate::app::BuilderApp`], which owns the workspace
//! and hands the active [`BuilderState`] to the nav router each frame.

use super::state::BuilderState;

/// Multi-state workspace owning the open builder sessions.
#[derive(Default)]
pub struct BuilderWorkspace {
    states: Vec<BuilderState>,
    active: usize,
}

impl BuilderWorkspace {
    /// Seed the workspace with one initial state. The active index points at
    /// it.
    #[must_use]
    pub fn new(initial: BuilderState) -> Self {
        Self {
            states: vec![initial],
            active: 0,
        }
    }

    /// Number of open sessions. Always >= 1 unless the caller used the
    /// `Default` impl.
    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// True iff `len() == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Index of the focused session. Returns 0 when [`Self::is_empty`].
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Read-only handle to the focused state. Panics when the workspace is
    /// empty — callers must `push` first or use [`Self::try_active`].
    #[must_use]
    pub fn active(&self) -> &BuilderState {
        &self.states[self.active]
    }

    /// Mutable handle to the focused state.
    pub fn active_mut(&mut self) -> &mut BuilderState {
        &mut self.states[self.active]
    }

    /// Read-only handle to a specific slot.
    #[must_use]
    pub fn get(&self, idx: usize) -> Option<&BuilderState> {
        self.states.get(idx)
    }

    /// Iterator over `(index, &BuilderState)` so the host tab strip can render
    /// one chip per session with its project title.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &BuilderState)> {
        self.states.iter().enumerate()
    }

    /// Audit finding #30: mutable iterator over every open session, so the host
    /// can flush each one's pending auto-save on app close/exit (not just the
    /// focused state).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut BuilderState> {
        self.states.iter_mut()
    }

    /// Push a new state onto the workspace and focus it. Returns the new
    /// active index. This is the §G6 "open in a new tab" entry point.
    pub fn push(&mut self, state: BuilderState) -> usize {
        self.states.push(state);
        self.active = self.states.len() - 1;
        self.active
    }

    /// Focus the slot at `idx`. No-op when out of bounds.
    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.states.len() {
            self.active = idx;
        }
    }

    /// Close the focused state. The previous slot becomes active (or 0 when
    /// the closed slot was the first). Returns the closed state so the host
    /// can persist it or run shutdown work. `None` when the workspace was
    /// empty.
    pub fn close_active(&mut self) -> Option<BuilderState> {
        if self.states.is_empty() {
            return None;
        }
        let idx = self.active;
        let removed = self.states.remove(idx);
        if self.states.is_empty() {
            self.active = 0;
        } else if self.active >= self.states.len() {
            self.active = self.states.len() - 1;
        }
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str) -> BuilderState {
        BuilderState::new_blank(id, id, "seed", 4, 4)
    }

    #[test]
    fn push_focuses_new_slot() {
        let mut w = BuilderWorkspace::new(s("a"));
        assert_eq!(w.active_index(), 0);
        let idx = w.push(s("b"));
        assert_eq!(idx, 1);
        assert_eq!(w.active_index(), 1);
        assert_eq!(w.active().sector.id.as_ref(), "b");
    }

    #[test]
    fn switch_to_changes_active_within_bounds() {
        let mut w = BuilderWorkspace::new(s("a"));
        w.push(s("b"));
        w.switch_to(0);
        assert_eq!(w.active().sector.id.as_ref(), "a");
        // Out of bounds: no-op
        w.switch_to(99);
        assert_eq!(w.active_index(), 0);
    }

    #[test]
    fn close_active_collapses_to_previous_slot() {
        let mut w = BuilderWorkspace::new(s("a"));
        w.push(s("b"));
        w.push(s("c"));
        assert_eq!(w.active_index(), 2);
        let dropped = w.close_active().unwrap();
        assert_eq!(dropped.sector.id.as_ref(), "c");
        assert_eq!(w.active_index(), 1);
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn close_first_keeps_focus_on_next() {
        let mut w = BuilderWorkspace::new(s("a"));
        w.push(s("b"));
        w.switch_to(0);
        let dropped = w.close_active().unwrap();
        assert_eq!(dropped.sector.id.as_ref(), "a");
        assert_eq!(w.len(), 1);
        assert_eq!(w.active().sector.id.as_ref(), "b");
    }

    #[test]
    fn iter_emits_all_states_in_insertion_order() {
        let mut w = BuilderWorkspace::new(s("a"));
        w.push(s("b"));
        let ids: Vec<&str> = w.iter().map(|(_, st)| st.sector.id.as_ref()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
