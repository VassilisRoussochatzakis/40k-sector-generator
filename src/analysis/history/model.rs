//! Output DTOs + `EventKind` taxonomy (no derivation logic).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryReport {
    pub sector_id: String,
    pub seed: String,
    #[serde(default)]
    pub eras: Vec<super::config::HistoryEra>,
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
#[non_exhaustive]
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
#[non_exhaustive]
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

impl EventKind {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::Foundation => "foundation",
            Self::Discovery => "discovery",
            Self::Annexation => "annexation",
            Self::ImperialMandateGranted => "imperial_mandate_granted",
            Self::Consecration => "consecration",
            Self::CommercialCharter => "commercial_charter",
            Self::DynasticClaim => "dynastic_claim",
            Self::Secession => "secession",
            Self::Uprising => "uprising",
            Self::Reconquest => "reconquest",
            Self::Purge => "purge",
            Self::CultExposed => "cult_exposed",
            Self::NecronAwakening => "necron_awakening",
            Self::TyranidContact => "tyranid_contact",
            Self::OrkWaaagh => "ork_waaagh",
            Self::QuarantineDeclared => "quarantine_declared",
            Self::Blockade => "blockade",
            Self::WarpStormSurge => "warp_storm_surge",
            Self::TauContact => "tau_contact",
            Self::AeldariActivity => "aeldari_activity",
            Self::ChaosIncursion => "chaos_incursion",
        }
    }
}

impl core::fmt::Display for EventKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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

impl HistoryEntityKind {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::Sector => "sector",
            Self::System => "system",
            Self::World => "world",
            Self::Route => "route",
            Self::Faction => "faction",
            Self::Claim => "claim",
            Self::Subsector => "subsector",
            Self::Region => "region",
        }
    }
}

impl core::fmt::Display for HistoryEntityKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
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
#[non_exhaustive]
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

impl HistoryConsequenceKind {
    pub fn as_slug(&self) -> &'static str {
        match self {
            Self::WorldSettled => "world_settled",
            Self::ClaimEstablished => "claim_established",
            Self::ControlShift => "control_shift",
            Self::ConflictEscalated => "conflict_escalated",
            Self::RouteHazard => "route_hazard",
            Self::BlockadeCreated => "blockade_created",
            Self::QuarantineDeclared => "quarantine_declared",
            Self::FactionMemory => "faction_memory",
            Self::SubsectorCapitalNamed => "subsector_capital_named",
            Self::RegionRecorded => "region_recorded",
        }
    }
}

impl core::fmt::Display for HistoryConsequenceKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
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
    pub(super) fn topo_rank(self) -> u32 {
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
    pub(super) fn base_weight(self) -> u8 {
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
