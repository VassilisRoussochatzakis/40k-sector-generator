//! Auxiliary types backing [`super::BuilderState`]: tab/tool/overlay enums,
//! pending-input payloads, modal kinds, the health-pip enum, and small caches.
//! All `pub` here so the facade can re-export them under
//! `crate::builder::state::*`.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use sectorforge::ids::{FactionId, RouteId, SystemId, WorldId};

/// §U2: default cap for the undo/redo ring buffer.
pub const DEFAULT_COMMAND_LOG_CAPACITY: usize = 200;

/// §V3: default debounce window between a mutation and the live re-validation
/// pass. Validation walks the full project so we don't want to repay the cost
/// for every keystroke — 200 ms is the same window used by the §G3 live
/// preview.
pub const DEFAULT_VALIDATION_DEBOUNCE_MS: u64 = 200;

/// Modal dialogs the builder can have open. Only one at a time.
#[derive(Debug, Clone)]
pub enum ModalKind {
    NewProject {
        name: String,
        title: String,
        seed: String,
        width: u32,
        height: u32,
    },
    OpenProject {
        path: Option<Utf8PathBuf>,
    },
    SaveAs {
        path: Utf8PathBuf,
    },
    PlaceSystem {
        coord: sectorforge::sector_model::HexCoord,
        name: String,
    },
    ConfirmRevertSnapshot {
        snapshot_name: String,
    },
    /// §G6: "New sector from preset" modal opened from the generation panel.
    /// On confirm the host scaffolds the project and pushes the resulting
    /// state onto a [`crate::builder::workspace::BuilderWorkspace`].
    NewFromPreset {
        preset_id: String,
        dest: Utf8PathBuf,
        seed: String,
    },
    /// RANDOM.md §7.3: "Random sector" wizard. On confirm the host synthesises
    /// a fully-randomised project under a chosen folder, generates it, and
    /// replaces the active [`BuilderState`] with the freshly opened result.
    /// `size` is one of
    /// `small`/`medium`/`large`/`vast`/`massive`/`huge`/`custom`; the custom
    /// dims are used only when `size == "custom"` and are locked equal
    /// (sectors are square). An empty `seed` mints one.
    GenerateRandom {
        size: String,
        custom_w: u32,
        custom_h: u32,
        seed: String,
    },
    Message(String),
    /// §P5: an external file change was detected and the in-memory buffer is
    /// dirty. The user must decide whether to reload from disk (losing the
    /// in-memory edits) or keep the in-memory buffer (potentially overwriting
    /// the on-disk version on the next save).
    ConflictResolver {
        /// Project-relative path of the file that changed.
        rel_path: String,
    },
}

/// §V3: combined health summary surfaced by the status bar. Green = clean
/// validation + invariants, yellow = warnings or no report yet, red = at least
/// one validation error or invariant violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthLevel {
    Green,
    Yellow,
    Red,
}

/// §N1: top-level builder tab. Each variant maps 1:1 to a panel module under
/// `builder/src/builder/panels/` (§N2). The router lives in
/// [`crate::builder::panels::nav`]; the active tab is persisted on
/// [`super::BuilderState::active_tab`] so panels never reach for global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuilderTab {
    Project,
    Map,
    System,
    World,
    Factions,
    Control,
    Regions,
    Routes,
    Subsectors,
    Economy,
    Relations,
    History,
    Personae,
    Hooks,
    Sites,
    Missions,
    Prose,
    Analytics,
    Interestingness,
    Search,
    Diff,
    Briefing,
    Segmentum,
    Export,
}

impl BuilderTab {
    /// Stable display label rendered in the top tab strip.
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "PROJECT",
            Self::Map => "MAP",
            Self::System => "SYSTEM",
            Self::World => "WORLD",
            Self::Factions => "FACTIONS",
            Self::Control => "CONTROL",
            Self::Regions => "REGIONS",
            Self::Routes => "ROUTES",
            Self::Subsectors => "SUBSECTORS",
            Self::Economy => "ECONOMY",
            Self::Relations => "RELATIONS",
            Self::History => "HISTORY",
            Self::Personae => "PERSONAE",
            Self::Hooks => "HOOKS",
            Self::Sites => "SITES",
            Self::Missions => "MISSIONS",
            Self::Prose => "PROSE",
            Self::Analytics => "ANALYTICS",
            Self::Interestingness => "INTERESTINGNESS",
            Self::Search => "SEARCH",
            Self::Diff => "DIFF",
            Self::Briefing => "BRIEFING",
            Self::Segmentum => "SEGMENTUM",
            Self::Export => "EXPORT",
        }
    }

    /// Canonical ordering used by the top tab strip (§N1).
    pub const ALL: &'static [BuilderTab] = &[
        Self::Project,
        Self::Map,
        Self::System,
        Self::World,
        Self::Factions,
        Self::Control,
        Self::Regions,
        Self::Routes,
        Self::Subsectors,
        Self::Economy,
        Self::Relations,
        Self::History,
        Self::Personae,
        Self::Hooks,
        Self::Sites,
        Self::Missions,
        Self::Prose,
        Self::Analytics,
        Self::Interestingness,
        Self::Search,
        Self::Diff,
        Self::Briefing,
        Self::Segmentum,
        Self::Export,
    ];
}

/// §N3: armed tool on the MAP tab. Bound to [`super::BuilderState::map_tool`]
/// so the router and inspector tabs can read it without reaching for global
/// state. Phase B panels (§S1 / §R2 / §REG2) consume this when the user clicks
/// the hex map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTool {
    /// Default — clicks select the entity under the cursor.
    Select,
    /// Click an empty hex → `BuilderCommand::AddSystem`.
    AddSystem,
    /// Click a system → `BuilderCommand::RemoveSystem`.
    DeleteSystem,
    /// Drag a system → `BuilderCommand::MoveSystem`.
    MoveSystem,
    /// Click two systems → `BuilderCommand::AddRoute`.
    AddRoute,
    /// Brush hexes → `GeneratedSector::add_region_hex` / `remove_region_hex`.
    RegionPaint,
}

impl MapTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "SELECT",
            Self::AddSystem => "ADD SYSTEM",
            Self::DeleteSystem => "DELETE SYSTEM",
            Self::MoveSystem => "MOVE SYSTEM",
            Self::AddRoute => "ADD ROUTE",
            Self::RegionPaint => "REGION PAINT",
        }
    }
}

/// Generic job handle the builder tracks for off-thread work (§47).
/// Type-erased so it can sit alongside other pending jobs in a vec.
pub struct JobHandle {
    pub id: String,
    pub description: String,
}

/// §H5: scratch buffer for the add-event wizard. The wizard collects the
/// anchor pick (system / world / route / region / sector), the event kind,
/// suggested + selected factions, optional date / narrative overrides, and a
/// generated id preview. On confirm the panel commits a new
/// [`sectorforge::history::HistoryEvent`] with `manual = true` so it survives
/// the §H6 regeneration pass.
#[derive(Debug, Clone)]
pub struct HistoryWizardState {
    pub anchor_kind: HistoryAnchorKind,
    pub anchor_system: Option<SystemId>,
    pub anchor_world: Option<WorldId>,
    pub anchor_route: Option<RouteId>,
    pub anchor_region: Option<String>,
    pub kind: sectorforge::history::EventKind,
    pub selected_factions: BTreeSet<FactionId>,
    pub date: String,
    pub narrative: String,
}

impl Default for HistoryWizardState {
    fn default() -> Self {
        Self {
            anchor_kind: HistoryAnchorKind::System,
            anchor_system: None,
            anchor_world: None,
            anchor_route: None,
            anchor_region: None,
            kind: sectorforge::history::EventKind::Foundation,
            selected_factions: BTreeSet::new(),
            date: String::new(),
            narrative: String::new(),
        }
    }
}

/// §H5: shape of the anchor the wizard is building.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAnchorKind {
    Sector,
    System,
    World,
    Route,
    Region,
}

impl HistoryAnchorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sector => "Sector",
            Self::System => "System",
            Self::World => "World",
            Self::Route => "Route",
            Self::Region => "Region",
        }
    }
    pub const ALL: &'static [Self] = &[
        Self::Sector,
        Self::System,
        Self::World,
        Self::Route,
        Self::Region,
    ];
}

/// §S1: pending ADD SYSTEM input — coord chosen by the user, name being typed.
#[derive(Debug, Clone)]
pub struct PendingPlace {
    pub coord: sectorforge::sector_model::HexCoord,
    pub name: String,
}

/// §S1: pending RENAME input — target id and the editable buffer.
#[derive(Debug, Clone)]
pub struct PendingRename {
    pub id: SystemId,
    pub text: String,
}

/// §S6: drag-drop collision payload surfaced to the swap / cancel dialog.
#[derive(Debug, Clone)]
pub struct PendingCollision {
    pub dragging: SystemId,
    pub target: sectorforge::sector_model::HexCoord,
    pub occupant: SystemId,
}

/// §CTX1 Phase 5 — pending REGION RENAME input — region id and editable
/// buffer. Surfaced by the MAP tab's right-click region-hex menu (`§6.5`)
/// and the REGIONS-tab rename row. Dispatches through
/// `BuilderCommand::RenameRegion` on commit.
#[derive(Debug, Clone)]
pub struct PendingRegionRename {
    pub region: String,
    pub text: String,
}

/// §CTX1 Phase 6 — pending WORLD RENAME input — world id and editable buffer.
/// Surfaced by the SYSTEM tab's in-system right-click menu (`§6.7
/// RENAME…`). Dispatches through `BuilderCommand::RenameWorld` on commit. The
/// `system` field is cached so the dialog can re-locate the world if the
/// underlying selection moves before the user clicks Rename.
#[derive(Debug, Clone)]
pub struct PendingWorldRename {
    pub system: SystemId,
    pub world: WorldId,
    pub text: String,
}

/// §CTX1 Phase 3 — pending BULK RENAME pattern, surfaced by the MAP tab's
/// right-click multi-selection menu. `pattern` is the editable buffer; the
/// dialog reuses the [`PendingRename`] modal pattern and dispatches through
/// [`super::super::panels::system::apply_bulk_rename`] on commit. Transient —
/// never serialised.
#[derive(Debug, Clone)]
pub struct PendingBulkRename {
    pub pattern: String,
}

/// §CF5: scope of a single `tick_log` entry — system-level summary or per-world
/// detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickLogScope {
    System(SystemId),
    World { system: SystemId, world: WorldId },
}

/// §CF5: one row of the chronological conflict-tick log surfaced from the
/// CONFLICT section. Captured by [`super::BuilderState::record_tick_diffs`]
/// after each [`super::super::command::BuilderCommand::AdvanceConflictTicks`]
/// run. In-memory only — never serialised to JSON.
#[derive(Debug, Clone)]
pub struct TickLogEntry {
    pub tick_index: u32,
    pub scope: TickLogScope,
    pub momentum_before: i8,
    pub momentum_after: i8,
    pub intensity_before: u8,
    pub intensity_after: u8,
    pub defender_before: Option<FactionId>,
    pub defender_after: Option<FactionId>,
    pub visible_before: Option<FactionId>,
    pub visible_after: Option<FactionId>,
}

/// §S2 cache backing the MAP panel renderer. `digest` is a BLAKE3 hex string
/// over the slice of `GeneratedSector` (systems, routes, regions) that drives
/// the subsector clustering + per-hex lookup tables. The panel rebuilds the
/// cache when the live digest no longer matches.
pub struct MapViewCache {
    pub digest: String,
    pub subsectors: Vec<sectorforge::subsectors::Subsector>,
    pub lookup: sectorforge_gui_core::sector_view::SectorMapCache,
}

/// §35 T5: cached per-system bitmap preview. The SYSTEM tab renders the focused
/// system through the same `system_map` PNG renderer the exporter uses, uploads
/// it once as an egui texture, and reuses it until `key` changes (system id /
/// theme / faction_fill / scale).
pub struct SystemBitmapPreview {
    pub key: String,
    pub texture: egui::TextureHandle,
    /// Pixel dimensions of the rendered image, for aspect-correct display.
    pub size: [usize; 2],
}

/// §C7 / §C8: live map overlay driven from the CONTROL tab. Off by default;
/// the MAP panel reads this to override its `SectorView::heatmap` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlOverlay {
    None,
    /// Per-system tint from `power_projection::project_sector`.
    PowerProjection,
    /// Per-system tint from `influence_field::build` sampled at the system coord.
    InfluenceField,
    /// Per-presence `admin` rolled up per system; top faction tints the cell.
    Administrative,
    /// Per-presence `military` rolled up per system.
    Military,
    /// Per-presence `orbital` rolled up per system.
    Orbital,
    /// `military + orbital` composite (orbital strike + naval force projection).
    Naval,
    /// Per-presence `economic` rolled up per system (trade / commerce).
    Mercantile,
    /// Per-presence `industrial` rolled up per system (forge / manufactory).
    Industrial,
    /// Per-presence `logistics` rolled up per system (supply chain).
    Logistical,
    /// Per-presence `covert` rolled up per system (intel / cult / heresy).
    Informational,
    /// Per-presence `ideological` rolled up per system (cult / faith).
    Religious,
    /// Per-presence `legitimacy` rolled up per system (sympathetic populace).
    Sympathetic,
}

impl ControlOverlay {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "OFF",
            Self::PowerProjection => "POWER PROJECTION",
            Self::InfluenceField => "INFLUENCE FIELD",
            Self::Administrative => "ADMINISTRATIVE",
            Self::Military => "MILITARY",
            Self::Orbital => "ORBITAL",
            Self::Naval => "NAVAL",
            Self::Mercantile => "MERCANTILE",
            Self::Industrial => "INDUSTRIAL",
            Self::Logistical => "LOGISTICAL",
            Self::Informational => "INFORMATIONAL",
            Self::Religious => "RELIGIOUS",
            Self::Sympathetic => "SYMPATHETIC",
        }
    }
}

/// §CTX1 — Phase 1 of `docs/CONTEXT_MENU.txt`: open right-click context menu
/// payload on the MAP tab. `None` on [`super::BuilderState::sector_context_menu`]
/// means no menu is open. Transient — never serialised.
#[derive(Debug, Clone)]
pub struct SectorContextMenu {
    /// Screen-space anchor for the floating `egui::Area`. Captured at the
    /// moment of the secondary click so the menu does not drift if the canvas
    /// scrolls underneath it.
    pub screen_pos: egui::Pos2,
    /// The entity (or empty hex) under the cursor when the click landed.
    pub target: SectorMenuTarget,
    /// §CTX1 Phase 3 — flips to `true` after the first DELETE ALL click on a
    /// `MultiSelection` target. The row re-renders as "Confirm? [Yes] [No]"
    /// and only the [Yes] branch dispatches the bulk delete. Cleared with the
    /// menu so a re-open starts unarmed.
    pub bulk_delete_confirm: bool,
}

/// §CTX1 Phase 6 — open right-click context menu on the in-system map
/// embedded under the SYSTEM tab (`§6.6`..`§6.9`). `None` on
/// [`super::BuilderState::system_context_menu`] means no menu is open.
/// Transient — never serialised.
#[derive(Debug, Clone)]
pub struct SystemContextMenu {
    /// Screen-space anchor for the floating `egui::Area`.
    pub screen_pos: egui::Pos2,
    /// The disk, ring, planet, or background the click resolved to.
    pub target: SystemMenuTarget,
}

/// §CTX1 Phase 6 — what a right-click on the in-system map resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemMenuTarget {
    /// Click landed on (or near) the central star disk.
    Star { system: SystemId },
    /// Click landed on a planet/world disk.
    World {
        system: SystemId,
        world: WorldId,
        orbit: u8,
    },
    /// Click landed on an orbit ring but not on any planet currently on that
    /// ring.
    EmptyOrbit { system: SystemId, orbit: u8 },
    /// Click landed on empty space inside the widget (no star, no ring, no
    /// planet).
    Background { system: SystemId },
}

/// §CTX1 — what the right-click on the sector hex map resolved to. Phase 1
/// only constructs `EmptyHex`, `System`, and `MultiSelection`; the remaining
/// variants are declared here so later phases can wire them without churning
/// the enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectorMenuTarget {
    /// Click landed on a hex inside sector bounds that does not host a system.
    EmptyHex {
        coord: sectorforge::sector_model::HexCoord,
    },
    /// Click hit the disk of a single system (and the system is not part of an
    /// active multi-selection).
    System {
        id: SystemId,
        coord: sectorforge::sector_model::HexCoord,
    },
    /// Click hit a route line — populated by Phase 5 once the hit-test helper
    /// is in place.
    Route {
        id: RouteId,
        near_coord: sectorforge::sector_model::HexCoord,
    },
    /// Click hit a hex that belongs to a region — populated by Phase 5 once
    /// the region lookup is in the [`MapViewCache`].
    RegionHex {
        region: String,
        coord: sectorforge::sector_model::HexCoord,
    },
    /// Click hit a subsector border edge — reserved for the future
    /// "subsector right-click" workflow (Phase 8+).
    SubsectorBorder {
        subsector: String,
        coord: sectorforge::sector_model::HexCoord,
    },
    /// Click landed inside the live rect-select box or on a system already in
    /// [`super::BuilderState::selected_systems`] with `len >= 2`. Phase 3
    /// reads this to surface bulk actions.
    MultiSelection { ids: Vec<SystemId> },
}

/// §PF2 / §PF4: in-memory state for the raw-TOML editor surface on the PROJECT
/// tab. Each open config file is one "editor tab" keyed by its
/// project-relative path. Buffers persist across tree-selection changes so
/// switching files never drops unsaved edits. In-memory only — never
/// serialised into a session.
#[derive(Debug, Default)]
pub struct TomlEditorState {
    /// Open file buffers keyed by project-relative path. A `BTreeMap` keeps the
    /// tab strip order deterministic regardless of open sequence.
    pub open: BTreeMap<String, OpenTomlBuffer>,
    /// Project-relative path of the active editor tab, or `None` when nothing
    /// is open.
    pub active: Option<String>,
}

/// §PF2: one open file in the raw-TOML editor.
#[derive(Debug, Clone)]
pub struct OpenTomlBuffer {
    /// Text as last read from / written to disk. The buffer is "dirty" when
    /// `buffer != original`.
    pub original: String,
    /// Live edited text bound to the multiline editor.
    pub buffer: String,
    /// Cached validation message from the most recent parse of `buffer`:
    /// `None` once it parses as a TOML document, otherwise the `toml` crate's
    /// `line:col` error string (§PF2 validation surface).
    pub error: Option<String>,
}

impl OpenTomlBuffer {
    /// Wrap freshly-read `text`, validating it immediately so the editor opens
    /// with the parse state already known.
    #[must_use]
    pub fn new(text: String) -> Self {
        let error = validate_toml(&text);
        Self {
            original: text.clone(),
            buffer: text,
            error,
        }
    }

    /// True when the live buffer diverges from the on-disk snapshot.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.buffer != self.original
    }

    /// Re-parse `buffer` and refresh [`Self::error`]. Call after every edit.
    pub fn revalidate(&mut self) {
        self.error = validate_toml(&self.buffer);
    }

    /// Mark the buffer clean by adopting `buffer` as the on-disk snapshot.
    pub fn mark_saved(&mut self) {
        self.original = self.buffer.clone();
    }
}

/// §PF2: parse `text` as a TOML document, returning a one-line error string
/// (the `toml` crate already carries `line N, column M` info) or `None` when
/// it parses cleanly.
#[must_use]
pub fn validate_toml(text: &str) -> Option<String> {
    match toml::from_str::<toml::Table>(text) {
        Ok(_) => None,
        Err(e) => Some(e.to_string()),
    }
}

/// §G5: inclusive rectangle of hex coordinates that
/// [`super::BuilderState::regenerate_partial`] operates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialRegenRect {
    pub min_q: i32,
    pub min_r: i32,
    pub max_q: i32,
    pub max_r: i32,
}

impl PartialRegenRect {
    /// Build a rect from any two corners. Always normalises so `min <= max`.
    #[must_use]
    pub fn from_corners(
        a: sectorforge::sector_model::HexCoord,
        b: sectorforge::sector_model::HexCoord,
    ) -> Self {
        Self {
            min_q: a.q.min(b.q),
            min_r: a.r.min(b.r),
            max_q: a.q.max(b.q),
            max_r: a.r.max(b.r),
        }
    }

    #[must_use]
    pub fn contains(&self, coord: sectorforge::sector_model::HexCoord) -> bool {
        coord.q >= self.min_q
            && coord.q <= self.max_q
            && coord.r >= self.min_r
            && coord.r <= self.max_r
    }
}
