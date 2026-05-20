//! Deterministic sector chronicle generator (§1 NEW.md).
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
    ClaimType, GeneratedSector, GeneratedSystem, GeneratedWorld, SystemState,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryConfig {
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
    /// Cap on the sector-wide "Key events" digest in the Markdown output.
    #[serde(default = "default_key_events")]
    pub key_events_top_n: u32,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            epoch_start: default_epoch_start(),
            epoch_end: default_epoch_end(),
            max_events_per_world: default_per_world(),
            max_events_per_system: default_per_system(),
            key_events_top_n: default_key_events(),
        }
    }
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
fn default_key_events() -> u32 {
    20
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryReport {
    pub sector_id: String,
    pub seed: String,
    pub events: Vec<HistoryEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub id: String,
    /// Synthetic in-universe date in `M{epoch}.{ddd}` notation. Strictly
    /// monotonic within a single anchor (foundation before annexation
    /// before any later reconquest).
    pub date: String,
    pub anchor: HistoryAnchor,
    pub kind: EventKind,
    pub narrative: String,
    pub factions: Vec<String>,
    /// 0..=100. Higher = more dramatically central. Drives the sector-wide
    /// "Key events" digest ordering.
    pub weight: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum HistoryAnchor {
    Sector,
    System { system_id: String },
    World { system_id: String, world_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Foundation,
    Discovery,
    Annexation,
    ImperialMandateGranted,
    Consecration,
    CommercialCharter,
    DynasticClaim,
    Secession,
    Uprising,
    Reconquest,
    Purge,
    CultExposed,
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

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &HistoryConfig) -> HistoryReport {
    let faction_names: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();

    let mut events: Vec<HistoryEvent> = Vec::new();

    // Per-system + per-world events.
    for sys in &sector.systems {
        emit_system_events(sys, &faction_names, cfg, sector, &mut events);
        for world in &sys.worlds {
            emit_world_events(sys, world, &faction_names, cfg, sector, &mut events);
        }
    }

    // Stable sort: epoch date then anchor then kind rank. Dates were chosen
    // so that the topo rank already orders events within an anchor; sorting
    // by date alone yields the final chronology.
    events.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then_with(|| anchor_key(&a.anchor).cmp(&anchor_key(&b.anchor)))
            .then_with(|| a.kind.topo_rank().cmp(&b.kind.topo_rank()))
            .then_with(|| a.id.cmp(&b.id))
    });

    HistoryReport {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        events,
    }
}

fn anchor_key(a: &HistoryAnchor) -> String {
    match a {
        HistoryAnchor::Sector => "0:sector".into(),
        HistoryAnchor::System { system_id } => format!("1:{system_id}"),
        HistoryAnchor::World {
            system_id,
            world_id,
        } => format!("2:{system_id}:{world_id}"),
    }
}

// ── Event emission ─────────────────────────────────────────────────────────────

fn emit_world_events(
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    faction_names: &BTreeMap<&str, &str>,
    cfg: &HistoryConfig,
    sector: &GeneratedSector,
    out: &mut Vec<HistoryEvent>,
) {
    let mut rng = stage_rng(&sector.seed, "history", &format!("{}:{}", sys.id, w.id));

    let mut buf: Vec<(EventKind, String, Vec<String>, u8)> = Vec::new();

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
        let fname = faction_names
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
                let dom_n = faction_names.get(dom.as_str()).copied().unwrap_or(dom);
                let sov_n = faction_names.get(sov.as_str()).copied().unwrap_or(sov);
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
            let n = faction_names
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
        let an = faction_names
            .get(attacker.as_str())
            .copied()
            .unwrap_or(attacker.as_str());
        let dn = faction_names
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
    if buf.len() as u32 > cfg.max_events_per_world {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(cfg.max_events_per_world as usize);
    }

    // Resort by topological rank so the chronicle reads forward.
    buf.sort_by(|a, b| a.0.topo_rank().cmp(&b.0.topo_rank()));

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let date = synthesise_date(&mut rng, cfg, kind, i);
        out.push(HistoryEvent {
            id: format!("evt-{}-{}-{}-{i}", sys.id, w.id, kind_slug(kind)),
            date,
            anchor: HistoryAnchor::World {
                system_id: sys.id.clone(),
                world_id: w.id.clone(),
            },
            kind,
            narrative: text,
            factions,
            weight,
        });
    }
}

fn emit_system_events(
    sys: &GeneratedSystem,
    faction_names: &BTreeMap<&str, &str>,
    cfg: &HistoryConfig,
    sector: &GeneratedSector,
    out: &mut Vec<HistoryEvent>,
) {
    let mut rng = stage_rng(&sector.seed, "history", &sys.id);
    let mut buf: Vec<(EventKind, String, Vec<String>, u8)> = Vec::new();

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
                    let bn = faction_names.get(b.as_str()).copied().unwrap_or(b.as_str());
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

    if buf.len() as u32 > cfg.max_events_per_system {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(cfg.max_events_per_system as usize);
    }
    buf.sort_by(|a, b| a.0.topo_rank().cmp(&b.0.topo_rank()));

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let date = synthesise_date(&mut rng, cfg, kind, i);
        out.push(HistoryEvent {
            id: format!("evt-{}-{}-{i}", sys.id, kind_slug(kind)),
            date,
            anchor: HistoryAnchor::System {
                system_id: sys.id.clone(),
            },
            kind,
            narrative: text,
            factions,
            weight,
        });
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
                "- **{}** ({:?}, weight {}): {}",
                e.date, e.kind, e.weight, e.narrative
            );
        }
    }

    // Group remaining events by anchor for the chronicle proper.
    let mut by_system: BTreeMap<String, Vec<&HistoryEvent>> = BTreeMap::new();
    let mut by_world: BTreeMap<(String, String), Vec<&HistoryEvent>> = BTreeMap::new();
    let mut sector_events: Vec<&HistoryEvent> = Vec::new();
    for e in &report.events {
        match &e.anchor {
            HistoryAnchor::Sector => sector_events.push(e),
            HistoryAnchor::System { system_id } => {
                by_system.entry(system_id.clone()).or_default().push(e)
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
                let _ = writeln!(s, "- **{}** ({:?}): {}", e.date, e.kind, e.narrative);
            }
        }
    }

    if !by_world.is_empty() {
        let _ = writeln!(s, "\n## World chronicles");
        for ((sys_id, world_id), evs) in &by_world {
            let _ = writeln!(s, "\n### {sys_id} · {world_id}");
            for e in evs {
                let _ = writeln!(s, "- **{}** ({:?}): {}", e.date, e.kind, e.narrative);
            }
        }
    }

    s
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

#[allow(dead_code)]
fn cmp_date(a: &str, b: &str) -> Ordering {
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        FactionClaim, FactionInfluence, GeneratedFaction, GeneratedStar, GeneratedSystem,
        GeneratedWorld, GenerationManifest, HexCoord, PowerProfile, PresenceDimensions,
        SystemControlSummary, WorldControlSummary, WorldDto, WorldFactionPresence,
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
            regions: Vec::new(),
            economy: Default::default(),
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
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            },
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
    fn presence_dims_smoke() {
        // Exercise the unused-elsewhere PresenceDimensions/FactionInfluence
        // imports to keep the test module honest about dependencies.
        let _ = WorldFactionPresence {
            faction_id: "x".into(),
            influence: FactionInfluence::Minor,
            relationship_to_government: "neutral".into(),
            dimensions: PresenceDimensions::default(),
            dominance: Default::default(),
            intel_confidence: 100,
        };
    }
}
