//! Deterministic sector chronicle generator (§1 NEW2.md).
//!
//! A `history` derivation pass over a finished `GeneratedSector` that walks
//! every world's claim list, dominance, archetype state, and conflict, and
//! emits a dated chronological list of in-universe events explaining how
//! the present configuration came to be.
//!
//! Pure derivation: no extra RNG draws affect other stages. The stage RNG
//! is seeded from `blake3("sectorforge:{seed}:history:{anchor_id}")`,
//! mirroring the existing per-stage RNG scheme. Same sector ⇒ same
//! chronicle, byte-stable.
//!
//! Output is intentionally narrative-source: lines a GM or writer can paste
//! into session notes. Calendar notation is `M{epoch}.{ddd}` by default.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::archetypes::{GscStage, NecronPhase, TauSphereBand, TyranidStage};
use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{
    ClaimType, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, RouteStability,
    RouteType, SystemState,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryConfig {
    /// Toggle for embedding `chronicle` in generated `sector.json`. The
    /// standalone CLI command forces derivation even when the generated
    /// sector was produced before this field existed.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Imperial-style millennium prefix. Foundation events anchor at
    /// `{epoch_start}.000`, present-day events at `{epoch_end}.999`.
    #[serde(default = "default_epoch_start")]
    pub epoch_start: u32,
    #[serde(default = "default_epoch_end")]
    pub epoch_end: u32,
    /// Maximum events listed per world. The most narratively-weighty events
    /// survive truncation first.
    #[serde(default = "default_per_world")]
    pub max_events_per_world: u32,
    /// Maximum events listed per system (system-anchored events only).
    #[serde(default = "default_per_system")]
    pub max_events_per_system: u32,
    /// Maximum route-origin events listed per route.
    #[serde(default = "default_per_route")]
    pub max_events_per_route: u32,
    /// Cap on the sector-wide "Key events" digest in the Markdown output.
    #[serde(default = "default_key_events")]
    pub key_events_top_n: u32,
    /// Maximum subsector-capital events embedded in the generated chronicle.
    /// Huge sectors sample this many representative systems instead of running
    /// full subsector clustering only for flavor text.
    #[serde(default = "default_max_subsector_events")]
    pub max_subsector_events: u32,
    /// Era bands used to label generated events. `allowed_events` may be
    /// empty, in which case the era is a catch-all fallback.
    #[serde(default = "default_eras")]
    pub eras: Vec<HistoryEra>,
    /// Optional declarative rules that ensure at least N events are present
    /// for matching current-state facts (for example, Warzone ⇒ War).
    #[serde(default)]
    pub event_rules: Vec<HistoryEventRule>,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            epoch_start: default_epoch_start(),
            epoch_end: default_epoch_end(),
            max_events_per_world: default_per_world(),
            max_events_per_system: default_per_system(),
            max_events_per_route: default_per_route(),
            key_events_top_n: default_key_events(),
            max_subsector_events: default_max_subsector_events(),
            eras: default_eras(),
            event_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryFile {
    pub history: HistoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEra {
    pub id: String,
    pub label: String,
    pub relative_start: i32,
    pub relative_end: i32,
    #[serde(default = "default_era_weight")]
    pub weight: f32,
    #[serde(default)]
    pub allowed_events: Vec<EventKind>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryEventRule {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub when_system_state: Option<String>,
    #[serde(default)]
    pub prefer_event: Option<String>,
    #[serde(default = "default_minimum_events")]
    pub minimum_events: u32,
}

fn default_epoch_start() -> u32 {
    36
}
fn default_epoch_end() -> u32 {
    42
}
fn default_per_world() -> u32 {
    6
}
fn default_per_system() -> u32 {
    3
}
fn default_per_route() -> u32 {
    1
}
fn default_key_events() -> u32 {
    20
}
fn default_max_subsector_events() -> u32 {
    64
}
fn default_true() -> bool {
    true
}
fn default_era_weight() -> f32 {
    1.0
}
fn default_minimum_events() -> u32 {
    1
}

fn default_eras() -> Vec<HistoryEra> {
    use EventKind::*;
    vec![
        HistoryEra {
            id: "age_of_foundation".into(),
            label: "Age of Foundation".into(),
            relative_start: -900,
            relative_end: -650,
            weight: 1.0,
            allowed_events: vec![Foundation, Discovery],
        },
        HistoryEra {
            id: "age_of_compliance".into(),
            label: "Age of Compliance".into(),
            relative_start: -649,
            relative_end: -300,
            weight: 1.0,
            allowed_events: vec![
                ImperialMandateGranted,
                CommercialCharter,
                DynasticClaim,
                Consecration,
                Annexation,
            ],
        },
        HistoryEra {
            id: "age_of_fracture".into(),
            label: "Age of Fracture".into(),
            relative_start: -299,
            relative_end: -80,
            weight: 1.0,
            allowed_events: vec![
                Secession,
                Uprising,
                CultExposed,
                AeldariActivity,
                TauContact,
            ],
        },
        HistoryEra {
            id: "age_of_wounds".into(),
            label: "Age of Wounds".into(),
            relative_start: -79,
            relative_end: -1,
            weight: 1.0,
            allowed_events: vec![
                NecronAwakening,
                TyranidContact,
                OrkWaaagh,
                ChaosIncursion,
                WarpStormSurge,
                QuarantineDeclared,
                Blockade,
            ],
        },
        HistoryEra {
            id: "present_crisis".into(),
            label: "Present Crisis".into(),
            relative_start: 0,
            relative_end: 0,
            weight: 1.0,
            allowed_events: vec![Reconquest, Purge],
        },
    ]
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryReport {
    pub sector_id: String,
    pub seed: String,
    #[serde(default)]
    pub eras: Vec<HistoryEra>,
    pub events: Vec<HistoryEvent>,
}

pub type SectorChronicle = HistoryReport;

impl SectorChronicle {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub id: String,
    /// Synthetic in-universe date in `M{epoch}.{ddd}` notation. Strictly
    /// monotonic within a single anchor (foundation before annexation
    /// before any later reconquest).
    pub date: String,
    #[serde(default)]
    pub era_id: String,
    #[serde(default)]
    pub era_label: String,
    #[serde(default)]
    pub relative_year: i32,
    pub anchor: HistoryAnchor,
    pub kind: EventKind,
    /// Short generated prose summary. Kept alongside `narrative` so downstream
    /// tools have a stable short field even if long-form prose grows later.
    #[serde(default)]
    pub summary: String,
    pub narrative: String,
    pub factions: Vec<crate::ids::FactionId>,
    #[serde(default)]
    pub entities: Vec<HistoryEntityRef>,
    #[serde(default)]
    pub consequences: Vec<HistoryConsequence>,
    /// 0..=100. Higher = more dramatically central. Drives the sector-wide
    /// "Key events" digest ordering.
    pub weight: u8,
    /// §H6: when true, this event was authored manually by the builder UI and
    /// must survive `derive_with` regenerations. Default false for everything
    /// the derivation pipeline emits; the §H5 add-event wizard sets it to true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum HistoryAnchor {
    Sector,
    System {
        system_id: crate::ids::SystemId,
    },
    Route {
        route_id: crate::ids::RouteId,
        from_system_id: crate::ids::SystemId,
        to_system_id: crate::ids::SystemId,
    },
    Subsector {
        subsector_id: String,
    },
    Region {
        region_id: String,
    },
    World {
        system_id: crate::ids::SystemId,
        world_id: crate::ids::WorldId,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    #[serde(alias = "Founding", alias = "founding")]
    Foundation,
    Discovery,
    Annexation,
    #[serde(alias = "Compliance", alias = "compliance")]
    ImperialMandateGranted,
    Consecration,
    #[serde(alias = "Treaty", alias = "treaty")]
    CommercialCharter,
    DynasticClaim,
    Secession,
    #[serde(alias = "Rebellion", alias = "rebellion")]
    Uprising,
    #[serde(alias = "War", alias = "war")]
    Reconquest,
    Purge,
    CultExposed,
    #[serde(alias = "Awakening", alias = "awakening")]
    NecronAwakening,
    TyranidContact,
    OrkWaaagh,
    QuarantineDeclared,
    Blockade,
    WarpStormSurge,
    TauContact,
    AeldariActivity,
    ChaosIncursion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryEntityKind {
    Sector,
    System,
    World,
    Route,
    Faction,
    Claim,
    Subsector,
    Region,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntityRef {
    pub kind: HistoryEntityKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HistoryConsequenceKind {
    WorldSettled,
    ClaimEstablished,
    ControlShift,
    ConflictEscalated,
    RouteHazard,
    BlockadeCreated,
    QuarantineDeclared,
    FactionMemory,
    SubsectorCapitalNamed,
    RegionRecorded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryConsequence {
    pub kind: HistoryConsequenceKind,
    pub description: String,
    #[serde(default)]
    pub severity: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}

impl EventKind {
    /// Strict ordering for prerequisite events at the same anchor. Lower
    /// rank fires first (foundation before annexation before reconquest).
    fn topo_rank(self) -> u32 {
        use EventKind::*;
        match self {
            Foundation => 0,
            Discovery => 5,
            ImperialMandateGranted | CommercialCharter | DynasticClaim | Consecration => 10,
            TauContact | AeldariActivity => 20,
            Annexation | Secession => 30,
            Uprising | CultExposed => 40,
            ChaosIncursion | NecronAwakening | TyranidContact | OrkWaaagh | WarpStormSurge => 50,
            Blockade | QuarantineDeclared => 60,
            Purge | Reconquest => 70,
        }
    }

    /// 0..=100 dramatic weight.
    fn base_weight(self) -> u8 {
        use EventKind::*;
        match self {
            Foundation => 10,
            Discovery => 20,
            ImperialMandateGranted | CommercialCharter | DynasticClaim | Consecration => 30,
            TauContact | AeldariActivity => 40,
            Secession => 55,
            Annexation => 60,
            Uprising => 65,
            CultExposed => 70,
            QuarantineDeclared | Blockade => 70,
            WarpStormSurge => 65,
            OrkWaaagh => 70,
            NecronAwakening => 80,
            TyranidContact => 85,
            ChaosIncursion => 80,
            Purge => 75,
            Reconquest => 80,
        }
    }
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> HistoryReport {
    derive_with(sector, &HistoryConfig::default())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryProgress {
    Started {
        systems: usize,
        worlds: usize,
        routes: usize,
        max_subsector_events: u32,
    },
    SubsectorEventsStarted {
        exact_cluster_count: usize,
        emitted_cap: u32,
        sampled: bool,
    },
    SubsectorEventsDone {
        events: usize,
    },
    SystemsScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    RoutesScanned {
        current: usize,
        total: usize,
        events: usize,
    },
    EventRulesApplied {
        events: usize,
    },
    SortingStarted {
        events: usize,
    },
    Complete {
        events: usize,
    },
}

struct EmitContext<'a> {
    cfg: &'a HistoryConfig,
    sector: &'a GeneratedSector,
    faction_names: &'a BTreeMap<&'a str, &'a str>,
    system_names: &'a BTreeMap<&'a str, &'a str>,
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &HistoryConfig) -> HistoryReport {
    derive_with_progress(sector, cfg, |_| {})
}

pub fn derive_with_progress(
    sector: &GeneratedSector,
    cfg: &HistoryConfig,
    mut progress: impl FnMut(HistoryProgress),
) -> HistoryReport {
    if !cfg.enabled {
        return HistoryReport {
            sector_id: sector.id.to_string(),
            seed: sector.seed.to_string(),
            eras: cfg.eras.clone(),
            events: Vec::new(),
        };
    }
    let world_count: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    progress(HistoryProgress::Started {
        systems: sector.systems.len(),
        worlds: world_count,
        routes: sector.routes.len(),
        max_subsector_events: cfg.max_subsector_events,
    });

    let faction_names: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_ref()))
        .collect();
    let system_names: BTreeMap<&str, &str> = sector
        .systems
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_ref()))
        .collect();

    let ctx = EmitContext {
        cfg,
        sector,
        faction_names: &faction_names,
        system_names: &system_names,
    };

    let mut events: Vec<HistoryEvent> = Vec::new();

    emit_subsector_events(&ctx, &mut events, &mut progress);
    emit_region_events(&ctx, &mut events);

    // Per-system + per-world events.
    let system_total = sector.systems.len();
    for (idx, sys) in sector.systems.iter().enumerate() {
        emit_system_events(&ctx, sys, &mut events);
        for world in &sys.worlds {
            emit_world_events(&ctx, sys, world, &mut events);
        }
        let current = idx + 1;
        if should_report_history_progress(current, system_total) {
            progress(HistoryProgress::SystemsScanned {
                current,
                total: system_total,
                events: events.len(),
            });
        }
    }
    let route_total = sector.routes.len();
    for (idx, route) in sector.routes.iter().enumerate() {
        emit_route_events(&ctx, route, &mut events);
        let current = idx + 1;
        if should_report_history_progress(current, route_total) {
            progress(HistoryProgress::RoutesScanned {
                current,
                total: route_total,
                events: events.len(),
            });
        }
    }
    apply_event_rules(&ctx, &mut events);
    progress(HistoryProgress::EventRulesApplied {
        events: events.len(),
    });

    // Stable sort: epoch date then anchor then kind rank. Dates were chosen
    // so that the topo rank already orders events within an anchor; sorting
    // by date alone yields the final chronology.
    progress(HistoryProgress::SortingStarted {
        events: events.len(),
    });
    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| anchor_key(&a.anchor).cmp(&anchor_key(&b.anchor)))
            .then_with(|| a.kind.topo_rank().cmp(&b.kind.topo_rank()))
            .then_with(|| a.id.cmp(&b.id))
    });
    progress(HistoryProgress::Complete {
        events: events.len(),
    });

    HistoryReport {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        eras: cfg.eras.clone(),
        events,
    }
}

fn should_report_history_progress(current: usize, total: usize) -> bool {
    if total == 0 {
        return false;
    }
    current == 1 || current == total || current.is_multiple_of(history_progress_stride(total))
}

fn history_progress_stride(total: usize) -> usize {
    match total {
        0..=25 => 1,
        26..=100 => 10,
        101..=500 => 25,
        _ => 100,
    }
}

fn anchor_key(a: &HistoryAnchor) -> String {
    match a {
        HistoryAnchor::Sector => "0:sector".into(),
        HistoryAnchor::System { system_id } => format!("1:{system_id}"),
        HistoryAnchor::Route { route_id, .. } => format!("2:{route_id}"),
        HistoryAnchor::Subsector { subsector_id } => format!("3:{subsector_id}"),
        HistoryAnchor::Region { region_id } => format!("4:{region_id}"),
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("5:{system_id}:{world_id}"),
    }
}

// ── Event emission ─────────────────────────────────────────────────────────────

fn build_event(
    ctx: &EmitContext,
    anchor: HistoryAnchor,
    kind: EventKind,
    text: String,
    factions: Vec<crate::ids::FactionId>,
    weight: u8,
    ordinal: usize,
) -> HistoryEvent {
    let mut rng = stage_rng(
        &ctx.sector.seed,
        "history-event",
        &format!("{}:{kind:?}:{ordinal}", anchor_key(&anchor)),
    );
    let date = synthesise_date(&mut rng, ctx.cfg, kind, ordinal);
    let (era_id, era_label, relative_year) = synthesise_era(&mut rng, ctx.cfg, kind);
    let mut entities = entities_for_anchor(&anchor);
    for faction_id in &factions {
        entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::Faction,
            id: faction_id.to_string(),
            role: Some("participant".into()),
        });
    }
    HistoryEvent {
        id: event_id(&anchor, kind, ordinal),
        date,
        era_id,
        era_label,
        relative_year,
        anchor,
        kind,
        summary: text.clone(),
        narrative: text,
        factions,
        entities,
        consequences: consequences_for(kind),
        weight,
        manual: false,
    }
}

fn emit_world_events(
    ctx: &EmitContext,
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    out: &mut Vec<HistoryEvent>,
) {
    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();

    // Foundation — every world gets one.
    let foundation_text = format!(
        "Records place the founding of {} in this era; the world was settled as {} and registered {}.",
        w.name,
        article_phrase(&w.world.world_type),
        article_phrase(&w.world.government),
    );
    buf.push((
        EventKind::Foundation,
        foundation_text,
        Vec::new(),
        EventKind::Foundation.base_weight(),
    ));

    // Claim-derived events.
    for c in &w.claims {
        let fname = ctx
            .faction_names
            .get(c.faction_id.as_str())
            .copied()
            .unwrap_or(c.faction_id.as_str());
        let (kind, text) = match c.claim_type {
            ClaimType::LegalSovereignty => (
                EventKind::DynasticClaim,
                format!("{fname} secured legal sovereignty over {}.", w.name),
            ),
            ClaimType::ImperialMandate => (
                EventKind::ImperialMandateGranted,
                format!(
                    "The Adeptus Terra entered {} into the Imperial registry; {fname} took custody under Imperial Mandate.",
                    w.name
                ),
            ),
            ClaimType::TreatyRight => (
                EventKind::CommercialCharter,
                format!("{fname} concluded a treaty asserting standing rights on {}.", w.name),
            ),
            ClaimType::ReligiousMandate => (
                EventKind::Consecration,
                format!("{fname} consecrated {} as a charge of the faith.", w.name),
            ),
            ClaimType::DynasticRight => (
                EventKind::DynasticClaim,
                format!("{fname} pressed an ancestral dynastic right over {}.", w.name),
            ),
            ClaimType::CommercialCharter => (
                EventKind::CommercialCharter,
                format!("{fname} was granted a commercial charter to operate on {}.", w.name),
            ),
            ClaimType::MilitaryOccupation => (
                EventKind::Annexation,
                format!("{fname} seized {} by force of arms.", w.name),
            ),
            ClaimType::AncientDomain => (
                EventKind::Discovery,
                format!("Surveys revealed the ancient domain of {fname} beneath {}.", w.name),
            ),
            ClaimType::HuntingGround => (
                EventKind::AeldariActivity,
                format!("{fname} marked {} as a recurring hunting ground.", w.name),
            ),
            ClaimType::CovertWrit => (
                EventKind::CultExposed,
                format!("{fname} obtained a covert writ binding agents on {}.", w.name),
            ),
            ClaimType::Rebellion => (
                EventKind::Uprising,
                format!(
                    "Loyalist authority on {} collapsed; {fname} declared open rebellion.",
                    w.name
                ),
            ),
        };
        let weight = (kind.base_weight() as u32 + (c.strength as u32 / 4)).min(100) as u8;
        buf.push((kind, text, vec![c.faction_id.clone()], weight));
    }

    // Contested status — a recent reconquest event.
    if w.control.contested {
        if let (Some(dom), Some(sov)) = (&w.control.dominant, &w.control.sovereign) {
            if dom != sov {
                let dom_n = ctx.faction_names.get(dom.as_str()).copied().unwrap_or(dom);
                let sov_n = ctx.faction_names.get(sov.as_str()).copied().unwrap_or(sov);
                buf.push((
                    EventKind::Reconquest,
                    format!(
                        "Authority on {} fractured: {dom_n} seized de facto control while {sov_n} retained the sovereign claim.",
                        w.name
                    ),
                    vec![dom.clone(), sov.clone()],
                    EventKind::Reconquest.base_weight(),
                ));
            }
        }
        if let Some(hidden) = &w.control.hidden_master {
            let n = ctx
                .faction_names
                .get(hidden.as_str())
                .copied()
                .unwrap_or(hidden);
            buf.push((
                EventKind::CultExposed,
                format!(
                    "Inquisitorial probes catalogued covert influence by {n} on {}.",
                    w.name
                ),
                vec![hidden.clone()],
                EventKind::CultExposed.base_weight(),
            ));
        }
    }

    // Conflict — a Purge event if heavy intensity.
    if w.conflict.intensity >= 60 {
        let attacker = w.conflict.attacker.clone().unwrap_or_default();
        let defender = w.conflict.defender.clone().unwrap_or_default();
        let an = ctx
            .faction_names
            .get(attacker.as_str())
            .copied()
            .unwrap_or(attacker.as_str());
        let dn = ctx
            .faction_names
            .get(defender.as_str())
            .copied()
            .unwrap_or(defender.as_str());
        buf.push((
            EventKind::Purge,
            format!(
                "Open warfare engulfed {}; {an} pressed an offensive against {dn}.",
                w.name
            ),
            [attacker, defender]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect(),
            EventKind::Purge.base_weight(),
        ));
    }

    // Truncate per-world, retain strongest.
    if buf.len() as u32 > ctx.cfg.max_events_per_world {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(ctx.cfg.max_events_per_world as usize);
    }

    // Resort by topological rank so the chronicle reads forward.
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::World {
            system_id: sys.id.clone(),
            world_id: w.id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}

fn emit_system_events(ctx: &EmitContext, sys: &GeneratedSystem, out: &mut Vec<HistoryEvent>) {
    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();

    if let Some(state) = sys.control.state {
        match state {
            SystemState::Quarantined => buf.push((
                EventKind::QuarantineDeclared,
                format!(
                    "{} was placed under interdict; warp routes inward sealed.",
                    sys.name
                ),
                Vec::new(),
                EventKind::QuarantineDeclared.base_weight(),
            )),
            SystemState::Blockaded => {
                if sys.blockade.under_blockade {
                    let b = sys.blockade.blockader.clone().unwrap_or_default();
                    let bn = ctx
                        .faction_names
                        .get(b.as_str())
                        .copied()
                        .unwrap_or(b.as_str());
                    buf.push((
                        EventKind::Blockade,
                        format!("{bn} threw a void blockade around {}.", sys.name),
                        if b.is_empty() { vec![] } else { vec![b] },
                        EventKind::Blockade.base_weight(),
                    ));
                }
            }
            SystemState::Warzone => buf.push((
                EventKind::Reconquest,
                format!("{} burned as an open warzone.", sys.name),
                Vec::new(),
                EventKind::Reconquest.base_weight(),
            )),
            SystemState::Infiltrated => buf.push((
                EventKind::CultExposed,
                format!(
                    "Quiet purges revealed entrenched infiltration in {}.",
                    sys.name
                ),
                Vec::new(),
                EventKind::CultExposed.base_weight(),
            )),
            SystemState::Fragmented => buf.push((
                EventKind::Secession,
                format!(
                    "Central authority over {} collapsed into competing seats.",
                    sys.name
                ),
                Vec::new(),
                EventKind::Secession.base_weight(),
            )),
            SystemState::Uncharted => buf.push((
                EventKind::Discovery,
                format!(
                    "{} reappears in the rolls only as an uncharted designate.",
                    sys.name
                ),
                Vec::new(),
                EventKind::Discovery.base_weight(),
            )),
            SystemState::Pacified => {}
        }
    }

    // Archetype-driven events.
    let a = &sys.archetype;
    if a.necron_phase != NecronPhase::None && a.necron_phase != NecronPhase::default() {
        let phrasing = match a.necron_phase {
            NecronPhase::Awakening => "Tomb-stirrings broke the silence in",
            NecronPhase::Awake => "Necron legions ascended openly from the tombs of",
            NecronPhase::Dormant => "Dormant tomb signals were charted beneath",
            NecronPhase::None => "",
        };
        buf.push((
            EventKind::NecronAwakening,
            format!("{phrasing} {}.", sys.name),
            Vec::new(),
            EventKind::NecronAwakening.base_weight(),
        ));
    }
    if a.tyranid_stage != TyranidStage::None {
        let phrase = match a.tyranid_stage {
            TyranidStage::Inhabited => "Splinter bioforms entered",
            TyranidStage::Besieged => "Hive-fleet vanguard pressed",
            TyranidStage::Consumed => "Hive-fleet tendrils stripped",
            TyranidStage::None => "",
        };
        buf.push((
            EventKind::TyranidContact,
            format!("{phrase} {}.", sys.name),
            Vec::new(),
            EventKind::TyranidContact.base_weight(),
        ));
    }
    if a.ork_waaagh >= 40 {
        buf.push((
            EventKind::OrkWaaagh,
            format!("A Waaagh! gathered momentum across {}.", sys.name),
            Vec::new(),
            (EventKind::OrkWaaagh.base_weight() as u32 + a.ork_waaagh as u32 / 5).min(100) as u8,
        ));
    }
    if a.gsc_stage != GscStage::None && a.gsc_stage != GscStage::default() {
        buf.push((
            EventKind::CultExposed,
            format!(
                "Cultist activity surfaced in {} ({}).",
                sys.name,
                gsc_stage_label(a.gsc_stage)
            ),
            Vec::new(),
            EventKind::CultExposed.base_weight(),
        ));
    }
    if a.tau_sphere != TauSphereBand::None && a.tau_sphere != TauSphereBand::default() {
        buf.push((
            EventKind::TauContact,
            format!(
                "T'au expansionists made contact with {} as a {} band.",
                sys.name,
                tau_band_label(a.tau_sphere)
            ),
            Vec::new(),
            EventKind::TauContact.base_weight(),
        ));
    }
    if a.aeldari_activity >= 30 {
        buf.push((
            EventKind::AeldariActivity,
            format!("Aeldari raiding parties returned to {}.", sys.name),
            Vec::new(),
            EventKind::AeldariActivity.base_weight(),
        ));
    }
    if a.chaos_corruption >= 40 || a.daemon_manifestation >= 40 {
        buf.push((
            EventKind::ChaosIncursion,
            format!("Chaotic taint flared visibly through {}.", sys.name),
            Vec::new(),
            (EventKind::ChaosIncursion.base_weight() as u32
                + (a.chaos_corruption as u32 + a.daemon_manifestation as u32) / 10)
                .min(100) as u8,
        ));
    }

    if sys.conflict.intensity >= 50 && !buf.iter().any(|(k, ..)| *k == EventKind::Reconquest) {
        buf.push((
            EventKind::Reconquest,
            format!(
                "Conflict ground on across {}; fronts re-formed annually.",
                sys.name
            ),
            sys.conflict
                .attacker
                .iter()
                .chain(sys.conflict.defender.iter())
                .cloned()
                .collect(),
            EventKind::Reconquest.base_weight(),
        ));
    }

    if buf.len() as u32 > ctx.cfg.max_events_per_system {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(ctx.cfg.max_events_per_system as usize);
    }
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::System {
            system_id: sys.id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}

fn emit_route_events(ctx: &EmitContext, route: &GeneratedRoute, out: &mut Vec<HistoryEvent>) {
    let from = ctx
        .system_names
        .get(route.from_system_id.as_str())
        .copied()
        .unwrap_or(route.from_system_id.as_str());
    let to = ctx
        .system_names
        .get(route.to_system_id.as_str())
        .copied()
        .unwrap_or(route.to_system_id.as_str());

    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();
    if matches!(
        route.stability,
        RouteStability::Unstable | RouteStability::Hazardous | RouteStability::Perilous
    ) {
        let kind = if matches!(route.stability, RouteStability::Perilous) {
            EventKind::WarpStormSurge
        } else {
            EventKind::Discovery
        };
        let weight = (kind.base_weight() as u32
            + match route.stability {
                RouteStability::Stable => 0,
                RouteStability::Unstable => 10,
                RouteStability::Hazardous => 25,
                RouteStability::Perilous => 35,
            })
        .min(100) as u8;
        buf.push((
            kind,
            format!(
                "Navigators marked the lane between {from} and {to} as {:?}; later charts record it as {:?}.",
                route.route_type, route.stability
            ),
            Vec::new(),
            weight,
        ));
    }

    if matches!(
        route.route_type,
        RouteType::SecretPassage
            | RouteType::Webway
            | RouteType::BlackShip
            | RouteType::SmugglingLane
    ) {
        let kind = match route.route_type {
            RouteType::Webway => EventKind::AeldariActivity,
            RouteType::BlackShip => EventKind::ImperialMandateGranted,
            RouteType::SmugglingLane | RouteType::SecretPassage => EventKind::Discovery,
            _ => EventKind::Discovery,
        };
        buf.push((
            kind,
            format!(
                "A concealed passage linking {from} and {to} entered restricted charts as {:?}.",
                route.route_type
            ),
            Vec::new(),
            kind.base_weight(),
        ));
    }

    if let Some(c) = route
        .controls
        .iter()
        .filter(|c| c.interdiction >= 60.0 || c.piracy >= 60.0)
        .max_by(|a, b| {
            (a.interdiction + a.piracy)
                .partial_cmp(&(b.interdiction + b.piracy))
                .unwrap_or(Ordering::Equal)
        })
    {
        let kind = EventKind::Blockade;
        buf.push((
            kind,
            format!(
                "{} became the dominant threat along the {from}-{to} route.",
                c.faction_id
            ),
            vec![c.faction_id.clone()],
            kind.base_weight(),
        ));
    }

    if buf.len() as u32 > ctx.cfg.max_events_per_route {
        buf.sort_by_key(|b| std::cmp::Reverse(b.3));
        buf.truncate(ctx.cfg.max_events_per_route as usize);
    }
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::Route {
            route_id: route.id.clone(),
            from_system_id: route.from_system_id.clone(),
            to_system_id: route.to_system_id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}

fn emit_subsector_events(
    ctx: &EmitContext,
    out: &mut Vec<HistoryEvent>,
    progress: &mut impl FnMut(HistoryProgress),
) {
    if ctx.sector.systems.is_empty() {
        return;
    }
    let exact_cluster_count = ctx
        .sector
        .systems
        .len()
        .div_ceil(crate::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR as usize)
        .max(1);
    let cap = ctx.cfg.max_subsector_events;
    if cap == 0 {
        progress(HistoryProgress::SubsectorEventsStarted {
            exact_cluster_count,
            emitted_cap: cap,
            sampled: true,
        });
        progress(HistoryProgress::SubsectorEventsDone { events: 0 });
        return;
    }
    if exact_cluster_count > cap as usize {
        progress(HistoryProgress::SubsectorEventsStarted {
            exact_cluster_count,
            emitted_cap: cap,
            sampled: true,
        });
        emit_sampled_subsector_events(ctx, out, cap as usize);
        progress(HistoryProgress::SubsectorEventsDone {
            events: cap as usize,
        });
        return;
    }
    progress(HistoryProgress::SubsectorEventsStarted {
        exact_cluster_count,
        emitted_cap: cap,
        sampled: false,
    });
    let Ok(subsectors) = crate::subsectors::build_subsectors(
        ctx.sector,
        crate::subsectors::SubsectorConfig::default(),
    ) else {
        return;
    };
    for (i, sub) in subsectors.iter().enumerate() {
        let Some(cap_sys) = &sub.summary.subsector_capital_system_id else {
            continue;
        };
        let sys_name = ctx
            .sector
            .systems
            .iter()
            .find(|s| s.id == *cap_sys)
            .map(|s| s.name.as_ref())
            .unwrap_or(cap_sys.as_str());
        let text = match &sub.summary.subsector_capital_world_id {
            Some(wid) => format!(
                "{} was elevated around {sys_name}; {} became the recorded capital world.",
                sub.name, wid
            ),
            None => format!(
                "{sub} was charted as an administrative subsector around {sys_name}.",
                sub = sub.name
            ),
        };
        let anchor = HistoryAnchor::Subsector {
            subsector_id: sub.id.to_string(),
        };
        let mut ev = build_event(ctx, anchor, EventKind::Foundation, text, Vec::new(), 35, i);
        ev.entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: cap_sys.to_string(),
            role: Some("capital_system".into()),
        });
        if let Some(wid) = &sub.summary.subsector_capital_world_id {
            ev.entities.push(HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: wid.to_string(),
                role: Some("capital_world".into()),
            });
        }
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::SubsectorCapitalNamed,
            description: format!("{} gained a capital at {sys_name}.", sub.name),
            severity: 35,
            entity_id: Some(sub.id.to_string()),
        });
        out.push(ev);
    }
    progress(HistoryProgress::SubsectorEventsDone {
        events: subsectors.len(),
    });
}

fn emit_sampled_subsector_events(ctx: &EmitContext, out: &mut Vec<HistoryEvent>, cap: usize) {
    if cap == 0 || ctx.sector.systems.is_empty() {
        return;
    }
    let mut systems: Vec<&GeneratedSystem> = ctx.sector.systems.iter().collect();
    systems.sort_by(|a, b| {
        a.coord
            .r
            .cmp(&b.coord.r)
            .then_with(|| a.coord.q.cmp(&b.coord.q))
            .then_with(|| a.id.cmp(&b.id))
    });
    let emit_count = cap.min(systems.len());
    for i in 0..emit_count {
        let idx = (i * systems.len()) / emit_count;
        let sys = systems[idx];
        let capital_world = sys.worlds.iter().max_by(|a, b| {
            population_rank(a.world.population.as_ref())
                .cmp(&population_rank(b.world.population.as_ref()))
                .then_with(|| {
                    tech_rank(a.world.tech_level.as_ref())
                        .cmp(&tech_rank(b.world.tech_level.as_ref()))
                })
                .then_with(|| a.id.cmp(&b.id))
        });
        let subsector_id = format!("chronicle-subsector-{:04}", i + 1);
        let text = match capital_world {
            Some(world) => format!(
                "Chronicle surveyors grouped a vast administrative march around {}; {} became the recorded capital world.",
                sys.name, world.name
            ),
            None => format!(
                "Chronicle surveyors grouped a vast administrative march around {}.",
                sys.name
            ),
        };
        let anchor = HistoryAnchor::Subsector {
            subsector_id: subsector_id.clone(),
        };
        let mut ev = build_event(ctx, anchor, EventKind::Foundation, text, Vec::new(), 35, i);
        ev.entities.push(HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: sys.id.to_string(),
            role: Some("capital_system".into()),
        });
        if let Some(world) = capital_world {
            ev.entities.push(HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: world.id.to_string(),
                role: Some("capital_world".into()),
            });
        }
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::SubsectorCapitalNamed,
            description: format!(
                "A sampled chronicle march gained a capital at {}.",
                sys.name
            ),
            severity: 35,
            entity_id: Some(subsector_id),
        });
        out.push(ev);
    }
}

fn emit_region_events(ctx: &EmitContext, out: &mut Vec<HistoryEvent>) {
    for (i, reg) in ctx.sector.regions.iter().enumerate() {
        let (kind, text, weight) = match reg.kind {
            crate::regions::RegionConditionKind::WarpStorm => (
                EventKind::WarpStormSurge,
                format!(
                    "{} swelled into a warp-storm region across {} charted hexes.",
                    reg.name,
                    reg.hexes.len()
                ),
                75,
            ),
            crate::regions::RegionConditionKind::Turbulence => (
                EventKind::WarpStormSurge,
                format!(
                    "{} became a region of persistent immaterial turbulence.",
                    reg.name
                ),
                60,
            ),
            crate::regions::RegionConditionKind::CalmCorridor => (
                EventKind::Discovery,
                format!("Navigators confirmed {} as a rare calm corridor.", reg.name),
                45,
            ),
            crate::regions::RegionConditionKind::Blackout => (
                EventKind::Discovery,
                format!("Astropathic silence spread through {}.", reg.name),
                65,
            ),
            crate::regions::RegionConditionKind::Anomaly => (
                EventKind::Discovery,
                format!("Surveyors catalogued {} as a persistent anomaly.", reg.name),
                55,
            ),
            crate::regions::RegionConditionKind::NecropolisDrift => (
                EventKind::Discovery,
                format!("Vast debris and dead worlds were charted in {}.", reg.name),
                55,
            ),
            crate::regions::RegionConditionKind::BeaconChain => (
                EventKind::Discovery,
                format!(
                    "Ancient navigation pylons were mapped aligning {}.",
                    reg.name
                ),
                60,
            ),
            crate::regions::RegionConditionKind::EmpyricBleed => (
                EventKind::WarpStormSurge,
                format!(
                    "Unnatural empyric phenomena began bleeding into {}.",
                    reg.name
                ),
                65,
            ),
        };
        let anchor = HistoryAnchor::Region {
            region_id: reg.id.clone(),
        };
        let mut ev = build_event(ctx, anchor, kind, text, Vec::new(), weight, i);
        ev.consequences.push(HistoryConsequence {
            kind: HistoryConsequenceKind::RegionRecorded,
            description: format!("{:?} effects entered the sector chronicle.", reg.kind),
            severity: weight,
            entity_id: Some(reg.id.clone()),
        });
        out.push(ev);
    }
}

fn population_rank(value: &str) -> i32 {
    match value {
        "Massive" => 6,
        "DenselyPopulated" => 5,
        "Standard" => 4,
        "LightlyPopulated" => 3,
        "SoleSettlement" => 2,
        "Minimal" => 1,
        "Uninhabited" => 0,
        _ => 0,
    }
}

fn tech_rank(value: &str) -> i32 {
    match value {
        "Archeotech" => 6,
        "High" => 5,
        "Imperial" => 4,
        "Standard" => 3,
        "Low" => 2,
        "Primitive" => 1,
        "None" => 0,
        _ => 0,
    }
}

fn apply_event_rules(ctx: &EmitContext, out: &mut Vec<HistoryEvent>) {
    for (rule_idx, rule) in ctx.cfg.event_rules.iter().enumerate() {
        let Some(kind) = rule.prefer_event.as_deref().and_then(event_kind_from_str) else {
            continue;
        };
        for sys in &ctx.sector.systems {
            if !rule_matches_system(rule, sys) {
                continue;
            }
            let existing = out
                .iter()
                .filter(|e| e.kind == kind)
                .filter(|e| match &e.anchor {
                    HistoryAnchor::System { system_id } => system_id == &sys.id,
                    _ => false,
                })
                .count() as u32;
            for extra in existing..rule.minimum_events {
                let text = rule_event_text(kind, sys, ctx.faction_names);
                let anchor = HistoryAnchor::System {
                    system_id: sys.id.clone(),
                };
                let mut ev = build_event(
                    ctx,
                    anchor,
                    kind,
                    text,
                    sys.conflict
                        .attacker
                        .iter()
                        .chain(sys.conflict.defender.iter())
                        .cloned()
                        .collect(),
                    kind.base_weight(),
                    rule_idx * 1000 + extra as usize,
                );
                let rule_id = rule
                    .id
                    .as_deref()
                    .map(normalize_key)
                    .unwrap_or_else(|| format!("rule-{rule_idx}"));
                ev.id = format!("evt-{rule_id}-{}-{}-{extra}", sys.id, kind_slug(kind));
                out.push(ev);
            }
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Date is `M{epoch}.{ddd}` where `epoch` is scaled by topo rank so that
/// foundation events fall in the start epoch and post-conflict events fall
/// in the end epoch. `ordinal` shifts the final ddd block within the
/// chosen epoch so multiple events at the same anchor are ordered.
fn synthesise_date(
    rng: &mut impl rand::Rng,
    cfg: &HistoryConfig,
    kind: EventKind,
    ordinal: usize,
) -> String {
    let span = cfg.epoch_end.saturating_sub(cfg.epoch_start).max(1);
    let normalised = (kind.topo_rank() as f32 / 70.0).clamp(0.0, 1.0);
    let epoch = cfg.epoch_start + (normalised * span as f32).round() as u32;
    // ddd spans 000..999 with a slight jitter for visual variety.
    let base = (kind.topo_rank() % 70) * 14;
    let jitter: u32 = rng.gen_range(0..40);
    let ddd = (base + jitter + ordinal as u32 * 5).min(999);
    format!("M{epoch}.{ddd:03}")
}

fn synthesise_era(
    rng: &mut impl rand::Rng,
    cfg: &HistoryConfig,
    kind: EventKind,
) -> (String, String, i32) {
    let era = cfg
        .eras
        .iter()
        .find(|e| e.allowed_events.is_empty() || e.allowed_events.contains(&kind))
        .or_else(|| cfg.eras.first());
    let Some(era) = era else {
        return ("unlabelled".into(), "Unlabelled".into(), 0);
    };
    let lo = era.relative_start.min(era.relative_end);
    let hi = era.relative_start.max(era.relative_end);
    let year = if lo == hi { lo } else { rng.gen_range(lo..=hi) };
    (era.id.clone(), era.label.clone(), year)
}

fn event_id(anchor: &HistoryAnchor, kind: EventKind, ordinal: usize) -> String {
    match anchor {
        HistoryAnchor::Sector => format!("evt-sector-{}-{ordinal}", kind_slug(kind)),
        HistoryAnchor::System { system_id } => {
            format!("evt-{system_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Route { route_id, .. } => {
            format!("evt-{route_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Subsector { subsector_id } => {
            format!("evt-{subsector_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::Region { region_id } => {
            format!("evt-{region_id}-{}-{ordinal}", kind_slug(kind))
        }
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("evt-{system_id}-{world_id}-{}-{ordinal}", kind_slug(kind)),
    }
}

fn entities_for_anchor(anchor: &HistoryAnchor) -> Vec<HistoryEntityRef> {
    match anchor {
        HistoryAnchor::Sector => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Sector,
            id: "sector".into(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::System { system_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::System,
            id: system_id.to_string(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Route {
            route_id,
            from_system_id,
            to_system_id,
        } => vec![
            HistoryEntityRef {
                kind: HistoryEntityKind::Route,
                id: route_id.to_string(),
                role: Some("anchor".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: from_system_id.to_string(),
                role: Some("route_endpoint".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: to_system_id.to_string(),
                role: Some("route_endpoint".into()),
            },
        ],
        HistoryAnchor::Subsector { subsector_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Subsector,
            id: subsector_id.clone(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::Region { region_id } => vec![HistoryEntityRef {
            kind: HistoryEntityKind::Region,
            id: region_id.clone(),
            role: Some("anchor".into()),
        }],
        HistoryAnchor::World {
            system_id,
            world_id,
        } => vec![
            HistoryEntityRef {
                kind: HistoryEntityKind::System,
                id: system_id.to_string(),
                role: Some("parent_system".into()),
            },
            HistoryEntityRef {
                kind: HistoryEntityKind::World,
                id: world_id.to_string(),
                role: Some("anchor".into()),
            },
        ],
    }
}

fn consequences_for(kind: EventKind) -> Vec<HistoryConsequence> {
    use EventKind::*;
    let (ck, text, severity) = match kind {
        Foundation => (
            HistoryConsequenceKind::WorldSettled,
            "settlement record became part of the sector rolls",
            20,
        ),
        Discovery | AeldariActivity | TauContact => (
            HistoryConsequenceKind::RegionRecorded,
            "restricted charts gained a new strategic note",
            35,
        ),
        ImperialMandateGranted | Consecration | CommercialCharter | DynasticClaim => (
            HistoryConsequenceKind::ClaimEstablished,
            "legal memory now affects present claims",
            45,
        ),
        Annexation | Reconquest => (
            HistoryConsequenceKind::ControlShift,
            "present control diverged from older settlement patterns",
            70,
        ),
        Secession | Uprising | Purge | CultExposed | ChaosIncursion | OrkWaaagh
        | NecronAwakening | TyranidContact => (
            HistoryConsequenceKind::ConflictEscalated,
            "local grudges and emergency powers persist",
            80,
        ),
        QuarantineDeclared => (
            HistoryConsequenceKind::QuarantineDeclared,
            "access restrictions and sealed records persist",
            75,
        ),
        Blockade => (
            HistoryConsequenceKind::BlockadeCreated,
            "void-control imbalance shaped later route politics",
            75,
        ),
        WarpStormSurge => (
            HistoryConsequenceKind::RouteHazard,
            "navigation risk entered formal route doctrine",
            70,
        ),
    };
    vec![HistoryConsequence {
        kind: ck,
        description: text.into(),
        severity,
        entity_id: None,
    }]
}

fn event_kind_from_str(s: &str) -> Option<EventKind> {
    let key: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    use EventKind::*;
    match key.as_str() {
        "foundation" | "founding" => Some(Foundation),
        "discovery" | "disappearance" => Some(Discovery),
        "annexation" => Some(Annexation),
        "compliance" | "imperialmandate" | "imperialmandategranted" => Some(ImperialMandateGranted),
        "consecration" => Some(Consecration),
        "treaty" | "commercialcharter" => Some(CommercialCharter),
        "dynasticclaim" | "dynasty" => Some(DynasticClaim),
        "schism" | "secession" => Some(Secession),
        "rebellion" | "uprising" => Some(Uprising),
        "war" | "reconquest" | "crusade" => Some(Reconquest),
        "purge" => Some(Purge),
        "cultexposed" | "cult" => Some(CultExposed),
        "awakening" | "necronawakening" | "necron" => Some(NecronAwakening),
        "tyranidcontact" | "tyranid" => Some(TyranidContact),
        "orkwaaagh" | "waaagh" | "ork" => Some(OrkWaaagh),
        "quarantinedeclared" | "quarantine" => Some(QuarantineDeclared),
        "blockade" => Some(Blockade),
        "plague" | "warpstormsurge" | "warpstorm" => Some(WarpStormSurge),
        "taucontact" | "tau" => Some(TauContact),
        "aeldariactivity" | "aeldari" => Some(AeldariActivity),
        "chaosincursion" | "chaos" => Some(ChaosIncursion),
        _ => None,
    }
}

fn rule_matches_system(rule: &HistoryEventRule, sys: &GeneratedSystem) -> bool {
    let Some(want) = rule.when_system_state.as_deref() else {
        return true;
    };
    let Some(state) = sys.control.state else {
        return false;
    };
    normalize_key(want) == normalize_key(&format!("{state:?}"))
}

fn normalize_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn rule_event_text(
    kind: EventKind,
    sys: &GeneratedSystem,
    faction_names: &BTreeMap<&str, &str>,
) -> String {
    match kind {
        EventKind::Reconquest | EventKind::Purge => {
            let actors: Vec<&str> = sys
                .conflict
                .attacker
                .iter()
                .chain(sys.conflict.defender.iter())
                .map(|f| faction_names.get(f.as_str()).copied().unwrap_or(f.as_str()))
                .collect();
            if actors.is_empty() {
                format!("{} entered the chronicle as a sustained warzone.", sys.name)
            } else {
                format!(
                    "{} entered the chronicle as a sustained warzone involving {}.",
                    sys.name,
                    actors.join(" and ")
                )
            }
        }
        EventKind::Blockade => format!(
            "Route ledgers preserve the blockade crisis at {}.",
            sys.name
        ),
        EventKind::QuarantineDeclared => {
            format!(
                "{} was sealed by rule-triggered quarantine memory.",
                sys.name
            )
        }
        EventKind::Uprising => format!("{} retains records of a precursor rebellion.", sys.name),
        _ => format!(
            "{} gained a rule-mandated {:?} entry in the sector chronicle.",
            sys.name, kind
        ),
    }
}

fn kind_slug(k: EventKind) -> &'static str {
    use EventKind::*;
    match k {
        Foundation => "foundation",
        Discovery => "discovery",
        Annexation => "annexation",
        ImperialMandateGranted => "mandate",
        Consecration => "consecration",
        CommercialCharter => "charter",
        DynasticClaim => "dynasty",
        Secession => "secession",
        Uprising => "uprising",
        Reconquest => "reconquest",
        Purge => "purge",
        CultExposed => "cult",
        NecronAwakening => "necron",
        TyranidContact => "tyranid",
        OrkWaaagh => "waaagh",
        QuarantineDeclared => "quarantine",
        Blockade => "blockade",
        WarpStormSurge => "warpstorm",
        TauContact => "tau",
        AeldariActivity => "aeldari",
        ChaosIncursion => "chaos",
    }
}

fn article_phrase(s: &str) -> String {
    if s.is_empty() {
        return "an outpost".into();
    }
    let lower = s.to_ascii_lowercase();
    let first = lower.chars().next().unwrap_or('x');
    let article = if matches!(first, 'a' | 'e' | 'i' | 'o' | 'u') {
        "an"
    } else {
        "a"
    };
    format!("{article} {s}")
}

fn gsc_stage_label(s: GscStage) -> &'static str {
    match s {
        GscStage::None => "no contact",
        GscStage::Rumor => "rumour stage",
        GscStage::HiddenCell => "hidden cell",
        GscStage::DistrictControl => "district control",
        GscStage::ParallelGovernment => "parallel government",
        GscStage::Uprising => "open uprising",
        GscStage::PlanetarySeizure => "planetary seizure",
    }
}

fn tau_band_label(b: TauSphereBand) -> &'static str {
    use TauSphereBand::*;
    match b {
        None => "no contact",
        Core => "core",
        Client => "client",
        Fringe => "fringe",
        Contact => "first-contact",
    }
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &HistoryReport, cfg: &HistoryConfig) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Sector Chronicle — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal events: **{}**", report.events.len());

    // Key events digest.
    let mut keyed: Vec<&HistoryEvent> = report.events.iter().collect();
    keyed.sort_by(|a, b| {
        b.weight
            .cmp(&a.weight)
            .then_with(|| a.date.cmp(&b.date))
            .then_with(|| a.id.cmp(&b.id))
    });
    if !keyed.is_empty() {
        let _ = writeln!(s, "\n## Key events");
        let n = (cfg.key_events_top_n as usize).min(keyed.len());
        for e in keyed.iter().take(n) {
            let _ = writeln!(
                s,
                "- **{}** · _{}_ ({:?}, weight {}): {}",
                e.date, e.era_label, e.kind, e.weight, e.narrative
            );
        }
    }

    // Group remaining events by anchor for the chronicle proper.
    let mut by_system: BTreeMap<crate::ids::SystemId, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_route: BTreeMap<crate::ids::RouteId, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_subsector: BTreeMap<String, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_region: BTreeMap<String, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_world: BTreeMap<(crate::ids::SystemId, crate::ids::WorldId), Vec<&HistoryEvent>> =
        BTreeMap::new();
    let mut sector_events: Vec<&HistoryEvent> = Vec::new();
    for e in &report.events {
        match &e.anchor {
            HistoryAnchor::Sector => sector_events.push(e),
            HistoryAnchor::System { system_id } => {
                by_system.entry(system_id.clone()).or_default().push(e)
            }
            HistoryAnchor::Route { route_id, .. } => {
                by_route.entry(route_id.clone()).or_default().push(e)
            }
            HistoryAnchor::Subsector { subsector_id } => by_subsector
                .entry(subsector_id.clone())
                .or_default()
                .push(e),
            HistoryAnchor::Region { region_id } => {
                by_region.entry(region_id.clone()).or_default().push(e)
            }
            HistoryAnchor::World {
                system_id,
                world_id,
            } => by_world
                .entry((system_id.clone(), world_id.clone()))
                .or_default()
                .push(e),
        }
    }

    if !sector_events.is_empty() {
        let _ = writeln!(s, "\n## Sector-wide events");
        for e in &sector_events {
            let _ = writeln!(s, "- **{}** — {}", e.date, e.narrative);
        }
    }

    if !by_system.is_empty() {
        let _ = writeln!(s, "\n## System chronicles");
        for (sys_id, evs) in &by_system {
            let _ = writeln!(s, "\n### {sys_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_route.is_empty() {
        let _ = writeln!(s, "\n## Route chronicles");
        for (route_id, evs) in &by_route {
            let _ = writeln!(s, "\n### {route_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_subsector.is_empty() {
        let _ = writeln!(s, "\n## Subsector chronicles");
        for (subsector_id, evs) in &by_subsector {
            let _ = writeln!(s, "\n### {subsector_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_region.is_empty() {
        let _ = writeln!(s, "\n## Region chronicles");
        for (region_id, evs) in &by_region {
            let _ = writeln!(s, "\n### {region_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    if !by_world.is_empty() {
        let _ = writeln!(s, "\n## World chronicles");
        for ((sys_id, world_id), evs) in &by_world {
            let _ = writeln!(s, "\n### {sys_id} · {world_id}");
            for e in evs {
                write_event_line(&mut s, e);
            }
        }
    }

    s
}

fn write_event_line(s: &mut String, e: &HistoryEvent) {
    let entity_ids: Vec<&str> = e.entities.iter().map(|x| x.id.as_str()).collect();
    let consequences: Vec<&str> = e
        .consequences
        .iter()
        .map(|x| x.description.as_str())
        .collect();
    let _ = writeln!(
        s,
        "- **{}** · _{}_ ({:?}): {}",
        e.date, e.era_label, e.kind, e.narrative
    );
    if !entity_ids.is_empty() {
        let _ = writeln!(s, "  - refs: `{}`", entity_ids.join("`, `"));
    }
    if !consequences.is_empty() {
        let _ = writeln!(s, "  - consequences: {}", consequences.join("; "));
    }
}

/// Write `history.md` + `history.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written, and
/// [`SectorError::ExportFailed`] if the report cannot be serialised.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &HistoryReport,
    cfg: &HistoryConfig,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "history", &render_markdown(report, cfg), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        FactionClaim, FactionInfluence, GeneratedFaction, GeneratedRoute, GeneratedStar,
        GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord, PowerProfile,
        PresenceDimensions, RouteStability, RouteType, SystemControlSummary, WorldControlSummary,
        WorldDto, WorldFactionPresence,
    };
    use std::collections::BTreeMap as Map;

    fn empty_sector() -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "history-seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "history-seed".into(),
                seed_hash: "h".into(),
                base_seed: None,
                candidate_index: None,
                constraints_digest: None,
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
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
            ..Default::default()
        }
    }

    fn world(id: &str, name: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: id.into(),
            index: 1,
            name: name.into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "Y".into(),
                star_colour_code: "Y".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Standard".into(),
                population: "Massive".into(),
                tech_level: "Imperial".into(),
                government: "ImperialCommander".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
        }
    }

    fn system(id: &str) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            kind: crate::sector_model::SystemKind::Star,
            coord: HexCoord { q: 0, r: 0 },
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    #[test]
    fn derive_is_deterministic() {
        let mut sec = empty_sector();
        let mut sys = system("sys-0001");
        let mut w = world("wrld-0001-1", "Alpha Prime");
        w.claims.push(FactionClaim {
            faction_id: "imp".into(),
            claim_type: ClaimType::ImperialMandate,
            strength: 80,
        });
        w.claims.push(FactionClaim {
            faction_id: "chaos".into(),
            claim_type: ClaimType::MilitaryOccupation,
            strength: 70,
        });
        sys.worlds.push(w);
        sec.systems.push(sys);
        sec.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "Imperial".into(),
            disposition: "Order".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile::default(),
        });

        let a = derive(&sec);
        let b = derive(&sec);
        let ja = serde_json::to_string(&a).unwrap();
        let jb = serde_json::to_string(&b).unwrap();
        assert_eq!(ja, jb);
        assert!(!a.events.is_empty());
        // Foundation must precede Annexation in the same world chronicle.
        let evs: Vec<&HistoryEvent> = a
            .events
            .iter()
            .filter(|e| matches!(&e.anchor, HistoryAnchor::World { .. }))
            .collect();
        let pos_foundation = evs.iter().position(|e| e.kind == EventKind::Foundation);
        let pos_annexation = evs.iter().position(|e| e.kind == EventKind::Annexation);
        if let (Some(f), Some(a)) = (pos_foundation, pos_annexation) {
            assert!(f < a, "foundation must precede annexation");
        }
    }

    #[test]
    fn empty_sector_yields_empty_report() {
        let sec = empty_sector();
        let r = derive(&sec);
        assert!(r.events.is_empty());
    }

    #[test]
    fn world_with_no_claims_still_gets_foundation() {
        let mut sec = empty_sector();
        let mut sys = system("sys-0001");
        sys.worlds.push(world("wrld-0001-1", "Lonely"));
        sec.systems.push(sys);
        let r = derive(&sec);
        assert!(r.events.iter().any(|e| e.kind == EventKind::Foundation));
    }

    #[test]
    fn route_events_have_refs_and_consequences() {
        let mut sec = empty_sector();
        sec.systems.push(system("sys-0001"));
        sec.systems.push(system("sys-0002"));
        sec.routes.push(GeneratedRoute {
            id: "route-sys-0001-sys-0002".into(),
            from_system_id: "sys-0001".into(),
            to_system_id: "sys-0002".into(),
            distance: 1,
            route_type: RouteType::ChartedPassage,
            stability: RouteStability::Perilous,
            tags: Vec::new(),
            controls: Vec::new(),
        });
        let r = derive(&sec);
        let ev = r
            .events
            .iter()
            .find(|e| matches!(e.anchor, HistoryAnchor::Route { .. }))
            .expect("route event");
        assert!(ev
            .entities
            .iter()
            .any(|x| { x.kind == HistoryEntityKind::Route && x.id == "route-sys-0001-sys-0002" }));
        assert!(ev
            .consequences
            .iter()
            .any(|x| x.kind == HistoryConsequenceKind::RouteHazard));
    }

    #[test]
    fn presence_dims_smoke() {
        // Exercise the unused-elsewhere PresenceDimensions/FactionInfluence
        // imports to keep the test module honest about dependencies.
        let _ = WorldFactionPresence {
            faction_id: "x".into(),
            subfaction_id: None,
            subfaction_name: None,
            force_id: None,
            force_name: None,
            influence: FactionInfluence::Minor,
            relationship_to_government: "neutral".into(),
            dimensions: PresenceDimensions::default(),
            dominance: Default::default(),
            intel_confidence: 100,
        };
    }
}
