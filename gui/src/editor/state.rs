//! Editor state machine. Holds the working sector + selection + pending dialogs.

use sectorforge::ids::{FactionId, SystemId};
use sectorforge::sector_model::{
    GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, HexCoord,
};
use std::collections::BTreeSet;

/// Sort order for the factions panel (§14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FactionSort {
    #[default]
    Default,
    PowerDesc,
    PowerAsc,
    NameAsc,
}

impl FactionSort {
    pub const ALL: &'static [FactionSort] = &[
        FactionSort::Default,
        FactionSort::PowerDesc,
        FactionSort::PowerAsc,
        FactionSort::NameAsc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FactionSort::Default => "DEFAULT",
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
        kind: sectorforge::sector_model::SystemKind,
        has_star: bool,
    },
    Message(String),
}

pub struct EditorState {
    pub sector: Option<GeneratedSector>,
    pub loaded_from: Option<String>,
    pub dirty: bool,
    pub tab: Tab,
    pub selection: Selection,
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
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            sector: None,
            loaded_from: None,
            dirty: false,
            tab: Tab::Map,
            selection: Selection::None,
            dialog: Dialog::None,
            hex_size: 44.0,
            system_side: 700.0,
            route_pick: None,
            faction_filter_kind: None,
            faction_filter_disposition: None,
            faction_sort: FactionSort::default(),
            faction_pinned: BTreeSet::new(),
            route_view_mode: sectorforge::sector_model::RouteViewMode::default(),
        }
    }
}

impl EditorState {
    pub fn set_sector(&mut self, sector: GeneratedSector, source_path: Option<String>) {
        self.sector = Some(sector);
        self.loaded_from = source_path;
        self.dirty = false;
        self.selection = Selection::None;
        self.tab = Tab::Map;
        self.dialog = Dialog::None;
        self.route_pick = None;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn next_system_index(&self) -> usize {
        self.sector.as_ref().map_or(1, |s| {
            s.systems.iter().map(|sys| sys.index).max().unwrap_or(0) + 1
        })
    }

    pub fn system_ids(&self) -> Vec<SystemId> {
        self.sector
            .as_ref()
            .map(|s| s.systems.iter().map(|sys| sys.id.clone()).collect())
            .unwrap_or_default()
    }
}

pub fn empty_sector(
    name: &str,
    title: &str,
    seed: &str,
    width: u32,
    height: u32,
) -> GeneratedSector {
    use sectorforge::sector_model::GenerationManifest;
    use std::collections::BTreeMap;

    GeneratedSector {
        id: name.into(),
        title: title.into(),
        seed: seed.into(),
        generator_name: sectorforge::GENERATOR_NAME.into(),
        generator_version: sectorforge::GENERATOR_VERSION.into(),
        width,
        height,
        systems: Vec::new(),
        routes: Vec::new(),
        factions: Vec::new(),
        manifest: GenerationManifest {
            project_id: name.into(),
            generated_at_policy: "manual_edit".into(),
            generator_name: sectorforge::GENERATOR_NAME.into(),
            generator_version: sectorforge::GENERATOR_VERSION.into(),
            seed: seed.into(),
            seed_hash: String::new().into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: BTreeMap::new(),
            settings_digest: String::new().into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        },
        influence_field: Default::default(),
        power_projection: Default::default(),
        relations: Default::default(),
        regions: Vec::new().into(),
        economy: Default::default(),
        chronicle: Default::default(),
    }
}

pub fn empty_system(
    id: SystemId,
    index: usize,
    name: String,
    coord: HexCoord,
    kind: sectorforge::sector_model::SystemKind,
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

pub fn empty_world(system_index: usize, world_index: usize, name: String) -> GeneratedWorld {
    use sectorforge::sector_model::WorldDto;
    GeneratedWorld {
        id: sectorforge::ids::world_id(system_index, world_index),
        index: world_index,
        name: name.into(),
        orbit: world_index as u8,
        source_row_index: 0,
        world: WorldDto {
            star_colour: "yellow".into(),
            star_colour_code: "G".into(),
            world_type: "FrontierWorld".into(),
            atmosphere: "Breathable".into(),
            temperature: "Temperate".into(),
            biosphere: "Thriving".into(),
            population: "LightlyPopulated".into(),
            tech_level: "Standard".into(),
            government: "MagistrateCouncil".into(),
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
    use sectorforge::sector_model::{RouteStability, RouteType};
    let id = sectorforge::ids::route_id(&from, &to);
    GeneratedRoute {
        id,
        from_system_id: from,
        to_system_id: to,
        distance: 0,
        route_type: RouteType::ChartedPassage,
        stability: RouteStability::Stable,
        tags: Vec::new(),
        controls: Vec::new(),
    }
}

pub fn empty_faction(id: FactionId) -> GeneratedFaction {
    let name = id.as_str().into();
    GeneratedFaction {
        id,
        name,
        kind: "imperial".into(),
        disposition: "neutral".into(),
        subfactions: Vec::new(),
        system_presence: Vec::new(),
        world_presence: Vec::new(),
        power: Default::default(),
    }
}
