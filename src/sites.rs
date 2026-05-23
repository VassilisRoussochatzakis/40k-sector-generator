//! §7 NEW2.md: Planetary site / point-of-interest generator.
//!
//! Sits between whole worlds and battlefields: per-world named sites with a
//! type, controlling faction, secrecy level, and one-line hook. Derives from
//! [`GeneratedWorld::world`] (type + features), [`GeneratedWorld::factions`],
//! and [`GeneratedWorld::regions`]. Pure read-only — no RNG draws beyond a
//! deterministic naming stream per-world.
//!
//! Determinism: stage seed `blake3("sectorforge:{seed}:sites:{world_id}")`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use camino::Utf8Path;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{FactionInfluence, GeneratedSector, GeneratedSystem, GeneratedWorld};
use crate::surface_region::{RegionKind, SurfaceRegion};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SitesConfig {
    /// Maximum sites generated per world.
    #[serde(default = "default_per_world")]
    pub max_per_world: u32,
    /// Skip uninhabited worlds entirely.
    #[serde(default = "default_true")]
    pub skip_uninhabited: bool,
    /// When true, hide sites whose `public_status` differs from
    /// `actual_status` (player edition).
    #[serde(default)]
    pub player_edition: bool,
    /// §7 NEW2.md: manual sites imported from file. Appended to derived set.
    #[serde(default)]
    pub manual: Vec<WorldSite>,
}

impl Default for SitesConfig {
    fn default() -> Self {
        Self {
            max_per_world: default_per_world(),
            skip_uninhabited: true,
            player_edition: false,
            manual: Vec::new(),
        }
    }
}

fn default_per_world() -> u32 {
    4
}
fn default_true() -> bool {
    true
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SitesReport {
    pub sector_id: String,
    pub seed: String,
    pub sites: Vec<WorldSite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSite {
    pub id: String,
    pub world_id: crate::ids::WorldId,
    pub system_id: crate::ids::SystemId,
    pub region_kind: Option<RegionKind>,
    pub kind: SiteKind,
    pub name: String,
    pub controlling_faction: Option<crate::ids::FactionId>,
    pub known_to: Vec<crate::ids::FactionId>,
    pub public_status: SiteStatus,
    pub actual_status: SiteStatus,
    pub tags: Vec<String>,
    pub hook: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SiteKind {
    GovernorsPalace,
    CathedralSpire,
    Manufactorum,
    UnderhiveSumpCity,
    VoidElevator,
    StarFortDockyard,
    QuarantineZone,
    XenosRuin,
    PilgrimNecropolis,
    AstropathicChoir,
    ArbitesPrecinct,
    DataVault,
    DisputedShrine,
    PenalMine,
    BlackMarketEnclave,
    CultSafehouse,
    CrashedVoidship,
    AgriBeltGranary,
    ForgeReactor,
    TombComplex,
    NavalAnchorage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SiteStatus {
    Active,
    Restricted,
    Abandoned,
    Quarantined,
    Sealed,
    Contested,
    UnderConstruction,
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> SitesReport {
    derive_with(sector, &SitesConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &SitesConfig) -> SitesReport {
    let mut out: Vec<WorldSite> = Vec::new();
    for sys in &sector.systems {
        for w in sector.get_worlds_for_system(sys) {
            if cfg.skip_uninhabited && w.world.population.as_ref() == "Uninhabited" {
                // Skip unless the world type still has interest (tomb / dead).
                if !matches!(
                    w.world.world_type.as_ref(),
                    "TombWorld" | "DeadWorld" | "WarpLostWorld" | "DaemonWorld"
                ) {
                    continue;
                }
            }
            emit_world_sites(sector, sys, w, cfg, &mut out);
        }
    }
    if cfg.player_edition {
        out.retain(|s| s.public_status == s.actual_status);
    }
    out.extend(cfg.manual.clone());
    out.sort_unstable_by(|a, b| a.world_id.cmp(&b.world_id).then_with(|| a.id.cmp(&b.id)));
    SitesReport {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        sites: out,
    }
}

// ── Generation per world ───────────────────────────────────────────────────────

fn emit_world_sites(
    sector: &GeneratedSector,
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    cfg: &SitesConfig,
    out: &mut Vec<WorldSite>,
) {
    let mut rng = stage_rng(&sector.seed, "sites", &w.id);
    let kinds = candidate_kinds(&w.world.world_type, &w.world.notable_features);
    if kinds.is_empty() {
        return;
    }
    let regions_by_kind: Vec<(RegionKind, &SurfaceRegion)> =
        w.regions.iter().map(|r| (r.kind, r)).collect();

    for (idx, kind) in kinds.into_iter().enumerate() {
        if idx >= cfg.max_per_world as usize {
            break;
        }
        let region_kind = preferred_region(kind);
        let region = region_kind.and_then(|rk| {
            regions_by_kind
                .iter()
                .find(|(k, _)| *k == rk)
                .map(|(_, r)| *r)
        });
        let controlling = region
            .and_then(|r| r.dominant.clone())
            .or_else(|| w.control.dominant.clone());
        let hidden = HiddenPresence::from_flag(
            w.factions
                .iter()
                .any(|p| matches!(p.influence, FactionInfluence::Hidden)),
        );
        let actual_status = status_from_world(kind, w, hidden, &mut rng);
        let public_status = mask_status(actual_status, kind, hidden);
        let name = site_name(kind, w, &mut rng);
        let known_to = if matches!(actual_status, SiteStatus::Sealed | SiteStatus::Quarantined) {
            vec![]
        } else {
            w.factions
                .iter()
                .filter(|p| !matches!(p.influence, FactionInfluence::Hidden))
                .map(|p| p.faction_id.clone())
                .collect()
        };
        let tags = tags_for(kind, w);
        let hook = hook_for(kind, w, controlling.as_deref(), &mut rng);
        out.push(WorldSite {
            id: format!("site-{}-{}-{:02}", sys.id, w.id, idx + 1),
            world_id: w.id.clone(),
            system_id: sys.id.clone(),
            region_kind,
            kind,
            name,
            controlling_faction: controlling,
            known_to,
            public_status,
            actual_status,
            tags,
            hook,
        });
    }
}

// ── Candidate sites per world type ─────────────────────────────────────────────

fn candidate_kinds(world_type: &str, features: &[Arc<str>]) -> Vec<SiteKind> {
    use SiteKind::*;
    let mut v = match world_type {
        "HiveWorld" => vec![
            GovernorsPalace,
            Manufactorum,
            UnderhiveSumpCity,
            ArbitesPrecinct,
            BlackMarketEnclave,
            CultSafehouse,
            VoidElevator,
        ],
        "ForgeWorld" => vec![
            ForgeReactor,
            Manufactorum,
            DataVault,
            UnderhiveSumpCity,
            VoidElevator,
            CultSafehouse,
        ],
        "ShrineWorld" | "CardinalWorld" => vec![
            CathedralSpire,
            PilgrimNecropolis,
            DisputedShrine,
            AstropathicChoir,
            ArbitesPrecinct,
            CultSafehouse,
        ],
        "AgriWorld" => vec![
            AgriBeltGranary,
            GovernorsPalace,
            ArbitesPrecinct,
            PenalMine,
            CultSafehouse,
        ],
        "CivilisedWorld" | "Civilised" => vec![
            GovernorsPalace,
            Manufactorum,
            CathedralSpire,
            ArbitesPrecinct,
            BlackMarketEnclave,
            DataVault,
            CultSafehouse,
        ],
        "DeathWorld" => vec![QuarantineZone, XenosRuin, NavalAnchorage, PenalMine],
        "FeudalWorld" | "FeralWorld" => {
            vec![GovernorsPalace, XenosRuin, CathedralSpire, CrashedVoidship]
        }
        "TombWorld" => vec![TombComplex, XenosRuin, QuarantineZone],
        "DeadWorld" | "WarpLostWorld" => vec![CrashedVoidship, XenosRuin, QuarantineZone],
        "DaemonWorld" => vec![DisputedShrine, XenosRuin, QuarantineZone],
        "KnightWorld" => vec![GovernorsPalace, Manufactorum, CathedralSpire],
        "FortressWorld" => vec![NavalAnchorage, StarFortDockyard, ArbitesPrecinct, PenalMine],
        "MiningWorld" => vec![Manufactorum, PenalMine, BlackMarketEnclave],
        _ => vec![GovernorsPalace, Manufactorum, ArbitesPrecinct],
    };
    for f in features {
        match f.as_ref() {
            "OrbitalDock" | "Shipyard" => v.push(StarFortDockyard),
            "VoidElevator" => v.push(VoidElevator),
            "Pilgrimage" | "PilgrimSite" => v.push(PilgrimNecropolis),
            "AstropathicChoir" => v.push(AstropathicChoir),
            "PenalColony" => v.push(PenalMine),
            "XenosRuins" | "AlienRuins" => v.push(XenosRuin),
            "WrecksiteShipwreck" | "CrashedVoidship" => v.push(CrashedVoidship),
            "BlackSite" | "QuarantineZone" => v.push(QuarantineZone),
            _ => {}
        }
    }
    // Stable dedup preserving order.
    let mut seen: std::collections::BTreeSet<SiteKind> = Default::default();
    v.retain(|k| seen.insert(*k));
    v
}

fn preferred_region(kind: SiteKind) -> Option<RegionKind> {
    use SiteKind::*;
    Some(match kind {
        GovernorsPalace | DataVault | ArbitesPrecinct => RegionKind::Capital,
        Manufactorum | ForgeReactor => RegionKind::ForgeComplex,
        UnderhiveSumpCity | BlackMarketEnclave | CultSafehouse => RegionKind::Underhive,
        CathedralSpire | PilgrimNecropolis | DisputedShrine => RegionKind::ShrineContinent,
        AstropathicChoir => RegionKind::CardinalSpire,
        AgriBeltGranary => RegionKind::AgriBelt,
        QuarantineZone | XenosRuin | CrashedVoidship | NavalAnchorage | VoidElevator
        | StarFortDockyard | PenalMine => RegionKind::Wilderness,
        TombComplex => RegionKind::TombComplex,
    })
}

// ── Status / mask ──────────────────────────────────────────────────────────────

/// Whether a Hidden-tier faction presence sits on a world. Replaces positional
/// `bool` arguments so call sites read self-documenting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HiddenPresence {
    Absent,
    Present,
}

impl HiddenPresence {
    fn from_flag(present: bool) -> Self {
        if present {
            Self::Present
        } else {
            Self::Absent
        }
    }

    fn is_present(self) -> bool {
        matches!(self, Self::Present)
    }
}

fn status_from_world(
    kind: SiteKind,
    w: &GeneratedWorld,
    hidden: HiddenPresence,
    rng: &mut impl Rng,
) -> SiteStatus {
    use SiteKind::*;
    if w.world.population.as_ref() == "Uninhabited" {
        if matches!(
            kind,
            TombComplex | XenosRuin | CrashedVoidship | QuarantineZone
        ) {
            return if rng.gen_bool(0.3) {
                SiteStatus::Sealed
            } else {
                SiteStatus::Abandoned
            };
        }
        return SiteStatus::Abandoned;
    }
    if w.control.contested {
        return SiteStatus::Contested;
    }
    if matches!(kind, CultSafehouse | BlackMarketEnclave) && hidden.is_present() {
        return SiteStatus::Active;
    }
    if matches!(kind, QuarantineZone) {
        return SiteStatus::Quarantined;
    }
    if matches!(kind, TombComplex | XenosRuin) {
        return SiteStatus::Sealed;
    }
    if matches!(kind, PenalMine | DataVault) {
        return SiteStatus::Restricted;
    }
    SiteStatus::Active
}

/// Player-side "public_status" — covers up hidden activity.
fn mask_status(actual: SiteStatus, kind: SiteKind, hidden: HiddenPresence) -> SiteStatus {
    use SiteKind::*;
    if matches!(kind, CultSafehouse | BlackMarketEnclave) && hidden.is_present() {
        return SiteStatus::Abandoned;
    }
    if matches!(kind, TombComplex) {
        return SiteStatus::Sealed;
    }
    actual
}

// ── Names + hooks ──────────────────────────────────────────────────────────────

fn site_name(kind: SiteKind, w: &GeneratedWorld, rng: &mut impl Rng) -> String {
    let base = &w.name;
    let prefix = pick(name_prefixes(kind), rng);
    let suffix = pick(name_suffixes(kind), rng);
    match (prefix, suffix) {
        (Some(p), Some(s)) => format!("{p} {base} {s}"),
        (Some(p), None) => format!("{p} {base}"),
        (None, Some(s)) => format!("{base} {s}"),
        (None, None) => format!("{base} Site"),
    }
}

fn name_prefixes(kind: SiteKind) -> &'static [&'static str] {
    use SiteKind::*;
    match kind {
        GovernorsPalace => &["Sapphire Court of", "Magistrate's Hall of", "Spire of"],
        CathedralSpire => &["Cathedral-Spire of", "Basilica of", "Spire-Reliquary of"],
        Manufactorum => &["Manufactorum", "Forgeworks of", "Atelier of"],
        UnderhiveSumpCity => &["Sump-City of", "Tower-Below", "Drowned Wards of"],
        VoidElevator => &["Voidship Lift", "Orbital Spine of", "Stairway of"],
        StarFortDockyard => &["Star-Fort", "Anchorage of", "Voidyard of"],
        QuarantineZone => &["Quarantine-Hold", "Sealed Pales of", "Black Zone of"],
        XenosRuin => &["Ruin of", "Tomb-Cluster of", "Echo-Vaults of"],
        PilgrimNecropolis => &["Necropolis of", "Reliquary Fields of", "Pilgrim Roads of"],
        AstropathicChoir => &["Choir-Hold of", "Black-Light Choir of", "Whisper-Spire of"],
        ArbitesPrecinct => &["Arbites Precinct", "Court-Fortress of", "Magistratum of"],
        DataVault => &["Data-Vault of", "Codicierium of", "Scribe-Tower of"],
        DisputedShrine => &["Shrine of", "Heretic-Reliquary of", "Twin-Altar of"],
        PenalMine => &["Penal Mine", "Iron Workings of", "Bonded Pit of"],
        BlackMarketEnclave => &["Bazaar of", "Crooked Hold of", "Smoke-Quarter of"],
        CultSafehouse => &[
            "Quiet House at",
            "Ninth Sump Chapel of",
            "Hidden Door under",
        ],
        CrashedVoidship => &["Wreck of the", "Crash-Hulk", "Bonefield of the"],
        AgriBeltGranary => &["Granary of", "Vat-Belt of", "Harvest Hall of"],
        ForgeReactor => &["Reactor", "Plasma Core of", "Iron Sun of"],
        TombComplex => &["Tomb-Complex of", "Sealed Necropolis of", "Crypt-City of"],
        NavalAnchorage => &["Naval Anchorage", "Fleet-Roads of", "Convoy Marshal of"],
    }
}

fn name_suffixes(kind: SiteKind) -> &'static [&'static str] {
    use SiteKind::*;
    match kind {
        GovernorsPalace => &["Prime", "Ascendant", "the Bright"],
        CathedralSpire => &["the Throne", "Hallowed", "Pre-Eminent"],
        Manufactorum => &["Alpha", "Septima", "the Roaring"],
        UnderhiveSumpCity => &["Bottom", "Mire", "the Drowned"],
        VoidElevator => &["the Cable", "Pillar", "Lift"],
        StarFortDockyard => &["Indomitus", "Anvil", "Bulwark"],
        QuarantineZone => &["Black-Walk", "the Veil", "Sigil-Locked"],
        XenosRuin => &["the Forgotten", "Glass-Throne", "Bone-Strand"],
        PilgrimNecropolis => &["Sanctus", "Ossuary", "the Quiet"],
        AstropathicChoir => &["Choir-Veil", "the Stilled Voice", "Hollow"],
        ArbitesPrecinct => &["Lex", "Decimus", "the Iron Code"],
        DataVault => &["the Sealed Scroll", "Cogitator-Vault", "Lex Mechanicus"],
        DisputedShrine => &["the Doubled Saint", "the Quarrel", "Wound-of-Faith"],
        PenalMine => &["the Long Sentence", "Bone Pit", "Iron Bond"],
        BlackMarketEnclave => &["the Lantern", "Crook-Way", "Sump-Bazaar"],
        CultSafehouse => &["the Calling", "Beneath", "the Quiet Door"],
        CrashedVoidship => &["the Argent Maw", "Long Fall", "Burnt Keel"],
        AgriBeltGranary => &["Surplus", "Tithe-Hall", "Sub-Harvest"],
        ForgeReactor => &["Omega-Cell", "Anvil-Heart", "Iron Sun"],
        TombComplex => &["the Silent Dynast", "Geometric Sleep", "the Unwoken"],
        NavalAnchorage => &["the Convoy", "Anchor-Roads", "Fleet Vigil"],
    }
}

fn pick<'a>(xs: &'a [&'a str], rng: &mut impl Rng) -> Option<&'a str> {
    xs.choose(rng).copied()
}

fn tags_for(kind: SiteKind, w: &GeneratedWorld) -> Vec<String> {
    use SiteKind::*;
    let mut t: Vec<String> = match kind {
        CultSafehouse => vec!["hidden", "ritual", "smuggling_access"],
        BlackMarketEnclave => vec!["hidden", "criminal", "trade"],
        UnderhiveSumpCity => vec!["dense", "criminal", "service"],
        Manufactorum | ForgeReactor => vec!["industrial", "strategic"],
        GovernorsPalace => vec!["political", "fortified"],
        CathedralSpire | DisputedShrine | PilgrimNecropolis => vec!["religious"],
        AstropathicChoir => vec!["psychic", "comms"],
        ArbitesPrecinct => vec!["enforcement", "fortified"],
        DataVault => vec!["archive", "restricted"],
        PenalMine => vec!["servitude", "industrial"],
        QuarantineZone => vec!["sealed", "hazard"],
        XenosRuin => vec!["archaeology", "hazard"],
        TombComplex => vec!["xenos", "hazard"],
        StarFortDockyard | NavalAnchorage | VoidElevator => vec!["void", "logistics"],
        AgriBeltGranary => vec!["supply", "economic"],
        CrashedVoidship => vec!["salvage", "hazard"],
    }
    .into_iter()
    .map(String::from)
    .collect();
    if w.control.contested {
        t.push("contested".to_string());
    }
    t.sort_unstable();
    t
}

fn hook_for(
    kind: SiteKind,
    w: &GeneratedWorld,
    controller: Option<&str>,
    rng: &mut impl Rng,
) -> String {
    use SiteKind::*;
    let owner = controller.unwrap_or("local interests");
    let pool: Vec<&str> = match kind {
        GovernorsPalace => vec![
            "The governor's seal is no longer trusted off-world.",
            "A succession debate plays out behind closed shutters.",
            "An off-world auditor has just arrived without invitation.",
        ],
        CathedralSpire => vec![
            "A relic has stopped responding to invocation.",
            "Confession backlog is now measured in years.",
            "The cardinal has not been seen in nine days.",
        ],
        Manufactorum => vec![
            "Output figures arrive on time but the foremen do not.",
            "An off-pattern weapon was shipped last quarter; nobody admits to it.",
            "Strike rumours among the labour castes.",
        ],
        UnderhiveSumpCity => vec![
            "Body-finders won't go below the eighteenth level.",
            "A new gang controls the chem-lifts.",
            "Children speak with one voice when asked about the dark.",
        ],
        VoidElevator => vec![
            "A cargo car arrived loaded with empty crates and a single passenger.",
            "The cable sings now even when nothing climbs.",
        ],
        StarFortDockyard => vec![
            "A dockyard slip is closed 'for refit' two years past schedule.",
            "Tariff disputes have shut three berths.",
        ],
        QuarantineZone => vec![
            "The seal is breached on one segment.",
            "Vapours leak after rain.",
        ],
        XenosRuin => vec![
            "The geometry shifts in dreams.",
            "Off-world scholars vanish on approach.",
        ],
        PilgrimNecropolis => vec![
            "A new tomb has appeared without record.",
            "The procession route was diverted by edict.",
        ],
        AstropathicChoir => vec![
            "Choristers report a 'pulling' on the warp side.",
            "Outgoing dispatches are received with one extra word.",
        ],
        ArbitesPrecinct => vec![
            "The reporter of confiscated heretic literature vanished last week.",
            "A magistratum trial has been delayed sine die.",
        ],
        DataVault => vec![
            "An index is now missing one volume.",
            "Two scribes have been replaced without comment.",
        ],
        DisputedShrine => vec![
            "Two sects publicly hold the same feast on different days here.",
            "The reliquary lock turns the wrong way once a month.",
        ],
        PenalMine => vec![
            "Prisoner manifests omit the new arrivals.",
            "Excavated artefacts go missing en route to assay.",
        ],
        BlackMarketEnclave => vec![
            "A new fixer has displaced the established Brokers.",
            "Trade in forbidden relics has spiked.",
        ],
        CultSafehouse => vec![
            "The lay-priest of the local chapel is now of the cult.",
            "Members memorise an Imperial benediction backwards.",
        ],
        CrashedVoidship => vec![
            "The wreck's transponder still ticks.",
            "Salvage crews suffer recurring nightmares.",
        ],
        AgriBeltGranary => vec![
            "Last cycle's surplus did not arrive at the spaceport.",
            "Inspectors have been turned away politely.",
        ],
        ForgeReactor => vec![
            "Plasma readings are off by exactly the right amount.",
            "A magos is requisitioning servitors above quota.",
        ],
        TombComplex => vec![
            "Seismic instruments record breath where there is no air.",
            "A scout team transmitted a single coherent word, then silence.",
        ],
        NavalAnchorage => vec![
            "Convoys form on schedule but light short.",
            "A new admiral has issued contradictory ordnance writs.",
        ],
    };
    let line = pool.choose(rng).copied().unwrap_or("Something is amiss.");
    format!("{} (controlled by `{owner}` on {})", line, w.name)
}

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &SitesReport, cfg: &SitesConfig) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Planetary Sites — {}", report.sector_id);
    let _ = writeln!(
        s,
        "\nSeed: `{}`  ·  Mode: {}\nTotal: **{}**",
        report.seed,
        if cfg.player_edition { "player" } else { "GM" },
        report.sites.len()
    );
    let mut by_world: BTreeMap<crate::ids::WorldId, Vec<&WorldSite>> = BTreeMap::new();
    for site in &report.sites {
        by_world
            .entry(site.world_id.clone())
            .or_default()
            .push(site);
    }
    for (world_id, group) in &by_world {
        let _ = writeln!(s, "\n## {world_id}");
        for site in group {
            let region = site
                .region_kind
                .map(|r| format!(" ({r:?})"))
                .unwrap_or_default();
            let owner = site.controlling_faction.as_deref().unwrap_or("(unowned)");
            let _ = writeln!(
                s,
                "- **{}** — {:?}{region}, public: {:?}{}, controller: `{owner}`\n  - {}",
                site.name,
                site.kind,
                site.public_status,
                if !cfg.player_edition && site.public_status != site.actual_status {
                    format!(" (actual: {:?})", site.actual_status)
                } else {
                    String::new()
                },
                site.hook,
            );
        }
    }
    s
}

/// Write `sites.md` + `sites.json` into `output_dir`.
///
/// # Errors
///
/// [`SectorError::Io`] on write failure and [`SectorError::ExportFailed`] on
/// serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &SitesReport,
    cfg: &SitesConfig,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "sites", &render_markdown(report, cfg), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedFaction, GeneratedStar, GeneratedSystem,
        GeneratedWorld, GenerationManifest, HexCoord, PresenceDimensions, SystemControlSummary,
        WorldControlSummary, WorldDto, WorldFactionPresence,
    };
    use std::collections::BTreeMap as Map;

    fn empty() -> GeneratedSector {
        GeneratedSector {
            id: "t".into(),
            title: "t".into(),
            seed: "s".into(),
            generator_name: "sf".into(),
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
                seed: "s".into(),
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

    fn hive_world(id: &str, name: &str, hidden: bool) -> GeneratedWorld {
        let mut presences = vec![WorldFactionPresence {
            faction_id: "imp".into(),
            subfaction_id: None,
            subfaction_name: None,
            force_id: None,
            force_name: None,
            influence: FactionInfluence::Dominant,
            relationship_to_government: "lawful".into(),
            dimensions: PresenceDimensions::default(),
            dominance: DominanceState::Controlled,
            intel_confidence: 100,
        }];
        if hidden {
            presences.push(WorldFactionPresence {
                faction_id: "cult".into(),
                subfaction_id: None,
                subfaction_name: None,
                force_id: None,
                force_name: None,
                influence: FactionInfluence::Hidden,
                relationship_to_government: "outlaw".into(),
                dimensions: PresenceDimensions::default(),
                dominance: DominanceState::default(),
                intel_confidence: 5,
            });
        }
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
            factions: presences,
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary {
                dominant: Some("imp".into()),
                ..Default::default()
            },
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
        }
    }

    #[test]
    fn deterministic_sites() {
        let mut s = empty();
        s.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        });
        s.systems.push(GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Test".into(),
            coord: HexCoord { q: 0, r: 0 },
            kind: crate::sector_model::SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![hive_world("sys-0001-w1", "Karthax", false)],
            primary_factions: vec!["imp".into()],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        });

        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert!(!a.sites.is_empty());
    }

    #[test]
    fn cult_safehouse_masks_status() {
        let mut s = empty();
        s.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: Default::default(),
        });
        s.systems.push(GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Test".into(),
            coord: HexCoord { q: 0, r: 0 },
            kind: crate::sector_model::SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![hive_world("sys-0001-w1", "Karthax", true)],
            primary_factions: vec!["imp".into()],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        });
        let r = derive(&s);
        let safe = r.sites.iter().find(|s| s.kind == SiteKind::CultSafehouse);
        if let Some(site) = safe {
            assert_eq!(site.public_status, SiteStatus::Abandoned);
            assert_eq!(site.actual_status, SiteStatus::Active);
        }
    }
}
