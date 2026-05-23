//! Editor state machine. Holds the working sector + selection + pending dialogs.

use std::collections::BTreeSet;

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::sector_model::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld,
    GenerationManifest, HexCoord, RouteStability, RouteType, SystemKind, WorldDto,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactionSort {
    PowerDesc,
    PowerAsc,
    NameAsc,
}

impl Default for FactionSort {
    fn default() -> Self {
        Self::PowerDesc
    }
}

impl FactionSort {
    pub const ALL: [Self; 3] = [Self::PowerDesc, Self::PowerAsc, Self::NameAsc];

    pub fn label(&self) -> &'static str {
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
    pub system_side: f32,
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
    pub preview_job: Option<crate::jobs::JobHandle<GeneratedSector>>,
    pub preview_timer: Option<f64>,
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
            system_side: 700.0,
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
        }
    }
}

impl EditorState {
    pub fn set_sector(
        &mut self,
        sector: GeneratedSector,
        project_input: Option<sectorforge::input::ProjectInput>,
        source_path: Option<String>,
    ) {
        self.sector = Some(sector);
        self.project_input = project_input;
        self.loaded_from = source_path;
        self.dirty = false;
        self.selection = Selection::None;
        self.tab = Tab::Map;
        self.dialog = Dialog::None;
        self.route_pick = None;
        self.pending_route_start = None;

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

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn next_system_index(&self) -> usize {
        self.sector
            .as_ref()
            .map(|s| s.systems.iter().map(|sys| sys.index).max().unwrap_or(0) + 1)
            .unwrap_or(1)
    }
}

pub fn empty_sector(id: &str, title: &str, seed: &str, width: u32, height: u32) -> GeneratedSector {
    GeneratedSector {
        id: id.into(),
        title: title.into(),
        seed: seed.into(),
        generator_name: "sectorforge".into(),
        generator_version: "0.1.0".into(),
        width,
        height,
        manifest: GenerationManifest {
            project_id: id.into(),
            generated_at_policy: "unknown".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: seed.into(),
            seed_hash: "".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: Default::default(),
            settings_digest: "".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        },
        systems: Vec::new(),
        routes: Vec::new(),
        factions: Vec::new(),
        relations: Default::default(),
        regions: Vec::new().into(),
        economy: Default::default(),
        chronicle: Default::default(),
        influence_field: Default::default(),
        power_projection: Default::default(),
        id_history: Default::default(),
    }
}

pub fn empty_system(
    id: SystemId,
    index: usize,
    name: String,
    coord: HexCoord,
    kind: SystemKind,
    star: Option<sectorforge::sector_model::GeneratedStar>,
) -> GeneratedSystem {
    GeneratedSystem {
        id,
        index,
        name: name.into(),
        coord,
        kind,
        star,
        worlds: Vec::new(),
        primary_factions: Vec::new(),
        tags: Vec::new(),
        notes: Vec::new(),
        control: Default::default(),
        stability: Default::default(),
        orbital_assets: Vec::new(),
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    }
}

pub fn empty_world(system_index: usize, index: usize, name: String) -> GeneratedWorld {
    let id = sectorforge::ids::world_id(system_index, index);
    GeneratedWorld {
        id,
        index,
        name: name.into(),
        orbit: 1,
        source_row_index: 0,
        world: WorldDto {
            star_colour: "white".into(),
            star_colour_code: "W".into(),
            world_type: "dead".into(),
            atmosphere: "none".into(),
            temperature: "temperate".into(),
            biosphere: "none".into(),
            population: "none".into(),
            tech_level: "low".into(),
            government: "none".into(),
            notable_features: Vec::new(),
        },
        factions: Vec::new(),
        tags: Vec::new(),
        notes: Vec::new(),
        claims: Vec::new(),
        control: Default::default(),
        stability: Default::default(),
        regions: Vec::new(),
        conflict: Default::default(),
    }
}

pub fn empty_route(from: SystemId, to: SystemId) -> GeneratedRoute {
    let id = sectorforge::ids::route_id(&from, &to);
    GeneratedRoute {
        id,
        from_system_id: from,
        to_system_id: to,
        route_type: RouteType::StableWarpLane,
        stability: RouteStability::Stable,
        distance: 1,
        tags: Vec::new(),
        controls: Vec::new(),
    }
}

pub fn empty_faction(id: &FactionId) -> GeneratedFaction {
    GeneratedFaction {
        id: id.clone(),
        name: id.as_str().into(),
        kind: "imperial".into(),
        disposition: "neutral".into(),
        subfactions: Vec::new(),
        system_presence: Vec::new(),
        world_presence: Vec::new(),
        power: Default::default(),
    }
}
