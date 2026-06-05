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
    Generation,
    Wishes,
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
        irregular_dimensions: bool,
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

#[allow(clippy::large_enum_variant)]
pub enum PreviewJobResult {
    Ready(GeneratedSector),
    Cancelled,
    Failed(String),
}

pub struct EditorState {
    pub sector: Option<GeneratedSector>,
    pub project_input: Option<sectorforge::input::ProjectInput>,
    pub wishes: Option<sectorforge::search::WishesFile>,
    pub loaded_from: Option<String>,
    pub dirty: bool,
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
    pub auto_generate: bool,
    pub auto_save: bool,
    pub stable_ids_on_rename: bool,
    pub drag_id: Option<SystemId>,
    pub pending_route_start: Option<SystemId>,
    pub search_outcome: Option<sectorforge::search::SearchOutcome>,
    /// Live preview for generation config (§6.G3).
    pub preview_sector: Option<GeneratedSector>,
    pub preview_job: Option<crate::jobs::JobHandle<PreviewJobResult>>,
    pub preview_timer: Option<f64>,
    pub preview_revision: u64,
    pub preview_error: Option<String>,
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
            project_input: None,
            wishes: None,
            loaded_from: None,
            dirty: false,
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
            auto_generate: false,
            auto_save: false,
            stable_ids_on_rename: true,
            drag_id: None,
            pending_route_start: None,
            search_outcome: None,
            preview_sector: None,
            preview_job: None,
            preview_timer: None,
            preview_revision: 0,
            preview_error: None,
            map_cache: None,
        }
    }
}

impl EditorState {
    pub(crate) fn set_sector(
        &mut self,
        sector: GeneratedSector,
        project_input: Option<sectorforge::input::ProjectInput>,
        source_path: Option<String>,
    ) {
        self.sector = Some(sector);
        self.project_input = project_input;
        self.loaded_from = source_path;
        self.dirty = false;
        self.map_cache = None;
        self.selection = Selection::None;
        self.tab = Tab::Map;
        self.dialog = Dialog::None;
        self.route_pick = None;
        self.pending_route_start = None;
        self.invalidate_preview();
        self.preview_timer = None;
        self.preview_job = None;

        // Try load wishes if we have project_input
        self.wishes = None;
        if let Some(pi) = &self.project_input {
            let wishes_path = pi.root_dir.join("wishes.toml");
            if wishes_path.exists() {
                if let Ok(w) = sectorforge::search::load_wishes(&wishes_path) {
                    self.wishes = Some(w);
                }
            }
        }
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
        // Sector mutated — drop the stale editor map render cache (AREA_F F4).
        self.map_cache = None;
    }

    pub(crate) fn schedule_preview(&mut self, due_time: f64) {
        self.invalidate_preview();
        self.preview_timer = Some(due_time);
    }

    pub(crate) fn invalidate_preview(&mut self) {
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.preview_sector = None;
        self.preview_error = None;
        if let Some(job) = &self.preview_job {
            job.cancel();
        }
    }

    pub(crate) fn apply_preview_result(&mut self, revision: u64, result: PreviewJobResult) -> bool {
        if revision != self.preview_revision {
            return false;
        }
        match result {
            PreviewJobResult::Ready(sector) => {
                self.preview_sector = Some(sector);
                self.preview_error = None;
            }
            PreviewJobResult::Cancelled => {}
            PreviewJobResult::Failed(message) => {
                self.preview_sector = None;
                self.preview_error = Some(message);
            }
        }
        true
    }

    pub(crate) fn next_system_index(&self) -> usize {
        self.sector
            .as_ref()
            .map(|s| s.systems.iter().map(|sys| sys.index).max().unwrap_or(0) + 1)
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::empty_sector;
    use std::sync::atomic::Ordering;
    use std::sync::{mpsc, Arc, Mutex};

    fn preview_job(revision: u64) -> crate::jobs::JobHandle<PreviewJobResult> {
        let (_tx, rx) = mpsc::channel();
        crate::jobs::JobHandle {
            id: "preview-gen".to_string(),
            revision,
            description: "preview".to_string(),
            progress: Arc::new(Mutex::new(0.0)),
            status: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            receiver: rx,
        }
    }

    #[test]
    fn stale_preview_revision_is_discarded_after_new_request() {
        let mut state = EditorState {
            preview_revision: 1,
            preview_job: Some(preview_job(1)),
            ..EditorState::default()
        };
        let cancel_flag = state.preview_job.as_ref().unwrap().cancelled.clone();

        state.schedule_preview(2.0);

        assert!(cancel_flag.load(Ordering::SeqCst));
        assert_eq!(state.preview_revision, 2);
        assert!(!state.apply_preview_result(
            1,
            PreviewJobResult::Ready(empty_sector("old", "Old", "old", 1, 1))
        ));
        assert!(state.preview_sector.is_none());

        assert!(state.apply_preview_result(
            2,
            PreviewJobResult::Ready(empty_sector("new", "New", "new", 1, 1))
        ));
        assert_eq!(
            state.preview_sector.as_ref().map(|s| s.seed.to_string()),
            Some("new".to_string())
        );
    }
}
