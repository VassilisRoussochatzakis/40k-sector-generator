//! Editor state machine. Holds the working sector + selection + pending dialogs.

use std::collections::BTreeSet;

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::sector_model::{GeneratedSector, HexCoord, SystemKind};

use crate::sector_view::SectorMapCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FactionSort {
    #[default]
    PowerDesc,
    PowerAsc,
    NameAsc,
}

impl FactionSort {
    pub(crate) const ALL: [Self; 3] = [Self::PowerDesc, Self::PowerAsc, Self::NameAsc];

    pub(crate) fn label(&self) -> &'static str {
        match self {
            FactionSort::PowerDesc => "POWER ↓",
            FactionSort::PowerAsc => "POWER ↑",
            FactionSort::NameAsc => "NAME ↑",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Map,
    Routes,
    Factions,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    None,
    System(SystemId),
    World {
        system_id: SystemId,
        world_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteEndpoint {
    From,
    To,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    None,
    OpenProject {
        projects: Vec<String>,
        selected: Option<String>,
    },
    NewSector {
        name: String,
        title: String,
        seed: String,
        width: u32,
        height: u32,
    },
    SaveAs {
        name: String,
        error: Option<String>,
    },
    PlaceSystem {
        coord: HexCoord,
        name: String,
        kind: SystemKind,
        has_star: bool,
    },
    Message(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectorEditTool {
    #[default]
    Select,
    AddSystem,
    AddRoute,
    Delete,
}

pub struct EditorState {
    pub sector: Option<GeneratedSector>,
    pub loaded_from: Option<String>,
    pub dirty: bool,
    /// Monotonic revision of `sector`, bumped on every mutation (`mark_dirty`)
    /// and on `set_sector`. The App's frame bridge re-derives its read snapshot
    /// only when this advances past its last-synced value, so an idle unsaved
    /// sector isn't deep-cloned every frame (F-S1).
    pub revision: u64,
    pub tab: Tab,
    pub selection: Selection,
    pub tool: SectorEditTool,
    pub dialog: Dialog,
    pub hex_size: f32,
    // VED-1: removed dead `system_side` field — it was shadowed by the live `App.system_side`
    // (app/mod.rs) which actually drives system_view.rs. This copy was never read.
    pub route_pick: Option<(usize, RouteEndpoint)>,
    /// Factions-panel filter / sort / pin state (§14).
    pub faction_filter_kind: Option<String>,
    pub faction_filter_disposition: Option<String>,
    pub faction_sort: FactionSort,
    pub faction_pinned: BTreeSet<FactionId>,
    pub route_view_mode: sectorforge::sector_model::RouteViewMode,
    pub auto_save: bool,
    pub stable_ids_on_rename: bool,
    pub drag_id: Option<SystemId>,
    pub pending_route_start: Option<SystemId>,
    /// Transient per-sector render cache for the editor map (AREA_F F4). Lazily
    /// built in `map_panel::show_map`, invalidated (`None`) on any sector change
    /// via `set_sector` / `mark_dirty`, so the `SectorView` render avoids the
    /// O(regions·hexes) per-frame hex→region fallback scan that `cache: None`
    /// took. Transient view state — not part of the saved document.
    pub map_cache: Option<SectorMapCache>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            sector: None,
            loaded_from: None,
            dirty: false,
            revision: 0,
            tab: Tab::Map,
            selection: Selection::None,
            tool: SectorEditTool::default(),
            dialog: Dialog::None,
            hex_size: 44.0,
            route_pick: None,
            faction_filter_kind: None,
            faction_filter_disposition: None,
            faction_sort: FactionSort::default(),
            faction_pinned: BTreeSet::new(),
            route_view_mode: sectorforge::sector_model::RouteViewMode::default(),
            auto_save: false,
            stable_ids_on_rename: true,
            drag_id: None,
            pending_route_start: None,
            map_cache: None,
        }
    }
}

impl EditorState {
    pub(crate) fn set_sector(&mut self, sector: GeneratedSector, source_path: Option<String>) {
        self.sector = Some(sector);
        self.loaded_from = source_path;
        self.dirty = false;
        self.revision = self.revision.wrapping_add(1);
        self.map_cache = None;
        self.selection = Selection::None;
        self.tab = Tab::Map;
        self.dialog = Dialog::None;
        self.route_pick = None;
        self.pending_route_start = None;
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        // Sector mutated — drop the stale editor map render cache (AREA_F F4).
        self.map_cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::empty_sector;

    // Gap 227: `set_sector` clears dirty, bumps revision (wrapping_add(1)), drops
    // the map cache, and resets transient editor state (selection/tab/dialog/
    // route_pick/pending_route_start). `loaded_from` is set from `source_path`.
    #[test]
    fn set_sector_clears_dirty_bumps_revision_and_resets_state() {
        // Dirty it up + set non-default transient state to prove the reset.
        // `map_cache` stays at its `None` default (building a real
        // SectorMapCache is heavy; `set_sector` drops it unconditionally).
        let mut st = EditorState {
            dirty: true,
            revision: 5,
            selection: Selection::System(SystemId::new("sys-0001")),
            tab: Tab::Routes,
            dialog: Dialog::Message("x".into()),
            route_pick: Some((0, RouteEndpoint::From)),
            pending_route_start: Some(SystemId::new("sys-0001")),
            ..Default::default()
        };

        let before_rev = st.revision;
        st.set_sector(empty_sector("a", "A", "s", 8, 8), Some("path".into()));

        assert!(!st.dirty);
        assert_eq!(st.revision, before_rev + 1); // wrapping_add(1)
        assert!(matches!(st.selection, Selection::None));
        assert!(matches!(st.tab, Tab::Map));
        assert!(matches!(st.dialog, Dialog::None));
        assert!(st.route_pick.is_none());
        assert!(st.pending_route_start.is_none());
        assert_eq!(st.loaded_from, Some("path".to_string()));
        assert!(st.sector.is_some());
        assert!(st.map_cache.is_none());
    }

    // Gap 227: `mark_dirty` sets dirty, bumps revision, drops the map cache.
    #[test]
    fn mark_dirty_sets_dirty_bumps_revision_drops_cache() {
        let mut st = EditorState::default();
        st.set_sector(empty_sector("a", "A", "s", 8, 8), None); // dirty=false, revision=1
        assert!(!st.dirty);
        let r = st.revision;
        st.mark_dirty();
        assert!(st.dirty);
        assert_eq!(st.revision, r + 1);
        assert!(st.map_cache.is_none());
    }
}
