//! §9 NEW2.md: Briefing profiles & player-facing redaction packs.
//!
//! Wraps the existing intel-redaction primitives ([`crate::intel`],
//! [`crate::html_export`]) with named *profiles* that describe an audience:
//! GM full truth, Imperial Navy captain, Inquisitorial cell, Rogue Trader
//! dynasty, local governor, public atlas. A profile is applied to a finished
//! [`GeneratedSector`] to produce a redacted clone that can be re-exported
//! through the normal Markdown / JSON paths.
//!
//! Determinism: profile application is a pure transform over the sector.
//! No RNG, no I/O.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::ids::{FactionId, RouteId};
use crate::sector_model::{FactionInfluence, GeneratedSector};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BriefingFile {
    #[serde(default)]
    pub profiles: Vec<BriefingProfile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BriefingProfile {
    /// Profile slug — used as file prefix and report header.
    pub id: String,
    /// One-line description for the digest header.
    #[serde(default)]
    pub label: Option<String>,
    /// Built-in audience preset; explicit fields below override it.
    #[serde(default)]
    pub preset: Option<AudiencePreset>,
    /// Observer faction id. When set, every Hidden-tier presence with
    /// `vis * observer.vis / 100 < minimum_intel_confidence` is dropped, and
    /// every other faction's intel sub-record on systems is hidden.
    #[serde(default)]
    pub observer_faction: Option<String>,
    /// Cutoff for the redaction helper. 0 keeps everything visible; 100 keeps
    /// only directly-observable presences.
    #[serde(default = "default_min_conf")]
    pub minimum_intel_confidence: u8,
    /// When false (default), the three hidden route classes
    /// ([`RouteType::Webway`], [`RouteType::BlackShip`],
    /// [`RouteType::SmugglingLane`]) are stripped from the sector view.
    #[serde(default)]
    pub include_hidden_routes: bool,
    /// When false (default for player-facing profiles), `sector.relations`
    /// is cleared from the briefing-side clone.
    #[serde(default)]
    pub show_relations: bool,
    /// When set, only routes / worlds touching these faction ids survive.
    /// Empty = no faction-presence filter.
    #[serde(default)]
    pub restrict_to_factions: Vec<String>,
    /// When true, factions absent from the observer's known world set are
    /// stripped from the top-level `factions` list. Useful for the public
    /// atlas profile so unknown xenos don't appear.
    #[serde(default)]
    pub redact_unknown_factions: bool,
    /// When false, `worlds[*].claims` is cleared. Public atlases often hide
    /// disputed sovereignty.
    #[serde(default = "default_true")]
    pub show_claims: bool,
    /// When false, per-system orbital asset detail is dropped.
    #[serde(default = "default_true")]
    pub show_orbital_assets: bool,
    /// When false, per-system blockade snapshot is dropped (kept for naval
    /// audiences; hidden for civilian).
    #[serde(default = "default_true")]
    pub show_blockades: bool,
    /// When false, archetype state (warp corruption / Tyranid stage etc.) is
    /// scrubbed — kept for Inquisition / Navy, hidden for civilian audiences.
    #[serde(default = "default_true")]
    pub show_archetype_state: bool,
}

impl Default for BriefingProfile {
    fn default() -> Self {
        Self {
            id: "gm_full_truth".to_string(),
            label: None,
            preset: Some(AudiencePreset::GmFullTruth),
            observer_faction: None,
            minimum_intel_confidence: default_min_conf(),
            include_hidden_routes: true,
            show_relations: true,
            restrict_to_factions: Vec::new(),
            redact_unknown_factions: false,
            show_claims: true,
            show_orbital_assets: true,
            show_blockades: true,
            show_archetype_state: true,
        }
    }
}

fn default_min_conf() -> u8 {
    30
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudiencePreset {
    /// No redaction — everything visible.
    GmFullTruth,
    /// Imperial Navy captain: routes + orbital assets visible; ground
    /// politics partially redacted; hidden routes hidden.
    ImperialNavy,
    /// Inquisitorial cell: hidden presences kept above a confidence floor,
    /// secret relations and intel sub-records visible.
    Inquisition,
    /// Rogue Trader dynasty: trade-oriented; routes always visible, hidden
    /// presences mostly absent, orbital assets visible.
    RogueTrader,
    /// Local governor: own faction + nearby presences only.
    LocalGovernor,
    /// Public atlas: redact unknown factions, hide claims, strip relations,
    /// no hidden routes, no archetype state.
    PublicAtlas,
}

impl AudiencePreset {
    fn apply(self, p: &mut BriefingProfile) {
        match self {
            Self::GmFullTruth => { /* defaults */ }
            Self::ImperialNavy => {
                p.include_hidden_routes = false;
                p.show_relations = true;
                p.show_archetype_state = true;
                p.minimum_intel_confidence = p.minimum_intel_confidence.max(20);
            }
            Self::Inquisition => {
                p.include_hidden_routes = true;
                p.show_relations = true;
                p.show_archetype_state = true;
                p.minimum_intel_confidence = p.minimum_intel_confidence.min(20);
            }
            Self::RogueTrader => {
                p.include_hidden_routes = false;
                p.show_relations = false;
                p.show_archetype_state = false;
                p.show_blockades = true;
                p.minimum_intel_confidence = p.minimum_intel_confidence.max(40);
            }
            Self::LocalGovernor => {
                p.include_hidden_routes = false;
                p.show_relations = false;
                p.show_archetype_state = false;
                p.show_blockades = false;
                p.show_orbital_assets = false;
                p.minimum_intel_confidence = 60;
                p.redact_unknown_factions = true;
            }
            Self::PublicAtlas => {
                p.include_hidden_routes = false;
                p.show_relations = false;
                p.show_archetype_state = false;
                p.show_blockades = false;
                p.show_claims = false;
                p.show_orbital_assets = false;
                p.redact_unknown_factions = true;
                p.minimum_intel_confidence = 80;
            }
        }
    }
}

// ── Output ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingPack {
    pub profile_id: String,
    pub sector_id: String,
    pub seed: String,
    pub sector: GeneratedSector,
    /// IDs of factions that were stripped from the pack.
    pub redacted_factions: Vec<FactionId>,
    /// IDs of routes that were stripped from the pack.
    pub redacted_routes: Vec<RouteId>,
}

// ── Apply ──────────────────────────────────────────────────────────────────────

/// Bake the audience preset into the explicit fields, then return a copy of
/// `sector` with the redactions applied. Pure — no I/O, no RNG.
#[must_use]
pub fn apply(sector: &GeneratedSector, profile: &BriefingProfile) -> BriefingPack {
    let mut p = profile.clone();
    if let Some(preset) = p.preset {
        preset.apply(&mut p);
    }
    let mut redacted_factions: BTreeSet<FactionId> = BTreeSet::new();
    let mut redacted_routes: BTreeSet<RouteId> = BTreeSet::new();

    let mut out = sector.clone();

    // Per-world presence redaction.
    for sys in &mut out.systems {
        if let Some(observer) = p.observer_faction.as_deref() {
            sys.intel.by_observer.retain(|k, _| k == observer);
        } else {
            sys.intel.by_observer.clear();
        }
        if !p.show_orbital_assets {
            sys.orbital_assets.clear();
        }
        if !p.show_blockades {
            sys.blockade = Default::default();
        }
        if !p.show_archetype_state {
            sys.archetype = Default::default();
        }
        for w in &mut sys.worlds {
            redact_world_presences(w, p.observer_faction.as_deref(), p.minimum_intel_confidence);
            if !p.show_claims {
                w.claims.clear();
            }
        }
    }

    // Faction-restrict filter.
    if !p.restrict_to_factions.is_empty() {
        let keep: BTreeSet<&str> = p.restrict_to_factions.iter().map(String::as_str).collect();
        for sys in &mut out.systems {
            for w in &mut sys.worlds {
                w.factions
                    .retain(|fp| keep.contains(fp.faction_id.as_str()));
            }
        }
    }

    // Hidden routes.
    if !p.include_hidden_routes {
        let before: BTreeSet<RouteId> = out.routes.iter().map(|r| r.id.clone()).collect();
        out.routes.retain(|r| !r.route_type.is_hidden());
        let after: BTreeSet<RouteId> = out.routes.iter().map(|r| r.id.clone()).collect();
        redacted_routes.extend(before.difference(&after).cloned());
    }

    // Relations.
    if !p.show_relations {
        out.relations = Default::default();
    }

    // Drop unknown factions from top-level list.
    if p.redact_unknown_factions {
        let known: BTreeSet<FactionId> = out
            .systems
            .iter()
            .flat_map(|s| s.worlds.iter())
            .flat_map(|w| w.factions.iter())
            .map(|fp| fp.faction_id.clone())
            .collect();
        let before: BTreeSet<FactionId> = out.factions.iter().map(|f| f.id.clone()).collect();
        out.factions.retain(|f| known.contains(&f.id));
        let after: BTreeSet<FactionId> = out.factions.iter().map(|f| f.id.clone()).collect();
        redacted_factions.extend(before.difference(&after).cloned());
    }

    BriefingPack {
        profile_id: p.id.clone(),
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        sector: out,
        redacted_factions: redacted_factions.into_iter().collect(),
        redacted_routes: redacted_routes.into_iter().collect(),
    }
}

fn redact_world_presences(
    w: &mut crate::sector_model::GeneratedWorld,
    observer: Option<&str>,
    min_conf: u8,
) {
    let observer_visibility = match observer {
        Some(id) => w
            .factions
            .iter()
            .find(|p| p.faction_id == id)
            .map(|p| p.dimensions.visibility)
            .unwrap_or(20.0),
        None => 100.0,
    };
    w.factions.retain(|p| {
        if let Some(id) = observer {
            if p.faction_id == id {
                return true;
            }
        }
        let raw = (p.dimensions.visibility * observer_visibility) / 100.0;
        if matches!(p.influence, FactionInfluence::Hidden) {
            return (raw as u8) >= min_conf;
        }
        (raw as u8) >= min_conf.saturating_sub(20)
    });
}

// ── Built-in profile catalog ───────────────────────────────────────────────────

#[must_use]
pub fn preset(id: AudiencePreset) -> BriefingProfile {
    let mut p = BriefingProfile {
        id: format!("{id:?}").to_lowercase(),
        label: Some(label_for(id).to_string()),
        preset: Some(id),
        ..Default::default()
    };
    id.apply(&mut p);
    p
}

fn label_for(id: AudiencePreset) -> &'static str {
    match id {
        AudiencePreset::GmFullTruth => "GM full truth — no redaction",
        AudiencePreset::ImperialNavy => "Imperial Navy captain's briefing",
        AudiencePreset::Inquisition => "Inquisitorial cell dossier",
        AudiencePreset::RogueTrader => "Rogue Trader trade ledger",
        AudiencePreset::LocalGovernor => "Local Governor's situational map",
        AudiencePreset::PublicAtlas => "Public-edition sector atlas",
    }
}

#[must_use]
pub fn all_presets() -> Vec<BriefingProfile> {
    use AudiencePreset::*;
    [
        GmFullTruth,
        ImperialNavy,
        Inquisition,
        RogueTrader,
        LocalGovernor,
        PublicAtlas,
    ]
    .into_iter()
    .map(preset)
    .collect()
}

#[must_use]
pub fn parse_preset(s: &str) -> Option<AudiencePreset> {
    match s.to_ascii_lowercase().as_str() {
        "gm" | "gm_full_truth" | "gmfulltruth" => Some(AudiencePreset::GmFullTruth),
        "navy" | "imperial_navy" | "imperialnavy" => Some(AudiencePreset::ImperialNavy),
        "inquisition" | "inquisitorial" => Some(AudiencePreset::Inquisition),
        "rogue_trader" | "roguetrader" | "trader" => Some(AudiencePreset::RogueTrader),
        "governor" | "local_governor" | "localgovernor" => Some(AudiencePreset::LocalGovernor),
        "public" | "atlas" | "public_atlas" | "publicatlas" => Some(AudiencePreset::PublicAtlas),
        _ => None,
    }
}

// ── Markdown header ────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(pack: &BriefingPack, profile: &BriefingProfile) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Briefing — {}", pack.sector_id);
    let _ = writeln!(
        s,
        "\nProfile: **{}**{}\nSeed: `{}`",
        profile.id,
        profile
            .label
            .as_ref()
            .map(|l| format!(" ({l})"))
            .unwrap_or_default(),
        pack.seed
    );
    if let Some(obs) = &profile.observer_faction {
        let _ = writeln!(
            s,
            "\nObserver faction: `{obs}` (min intel confidence {})",
            profile.minimum_intel_confidence
        );
    }
    let sec = &pack.sector;
    let _ = writeln!(
        s,
        "\n## Pack contents\n- Systems: **{}**\n- Worlds: **{}**\n- Routes: **{}** (hidden routes {})\n- Factions: **{}**",
        sec.systems.len(),
        sec.systems.iter().map(|s| s.worlds.len()).sum::<usize>(),
        sec.routes.len(),
        if profile.include_hidden_routes { "visible" } else { "redacted" },
        sec.factions.len()
    );
    if !pack.redacted_factions.is_empty() {
        let _ = writeln!(
            s,
            "\n_Redacted factions ({}): {}_",
            pack.redacted_factions.len(),
            join_ids(&pack.redacted_factions)
        );
    }
    if !pack.redacted_routes.is_empty() {
        let _ = writeln!(
            s,
            "\n_Redacted routes ({}): {}_",
            pack.redacted_routes.len(),
            join_ids(&pack.redacted_routes)
        );
    }
    let _ = writeln!(s, "\n## Systems");
    for sys in &sec.systems {
        let _ = writeln!(s, "\n### {} (`{}`)", sys.name, sys.id);
        if !sys.primary_factions.is_empty() {
            let _ = writeln!(s, "- Primary factions: {}", join_ids(&sys.primary_factions));
        }
        for w in &sys.worlds {
            let claims = if profile.show_claims && !w.claims.is_empty() {
                format!(
                    " — claims: {}",
                    w.claims
                        .iter()
                        .map(|c| format!("{:?}({})", c.claim_type, c.faction_id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                String::new()
            };
            let _ = writeln!(
                s,
                "- **{}** ({}, {} factions visible){}",
                w.name,
                w.world.world_type,
                w.factions.len(),
                claims
            );
        }
    }
    s
}

fn join_ids<T: AsRef<str>>(ids: &[T]) -> String {
    ids.iter()
        .map(|id| id.as_ref())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Write `briefing-<profile>.md` + `briefing-<profile>.json` into `output_dir`.
///
/// # Errors
///
/// [`SectorError::Io`] on write failure and [`SectorError::ExportFailed`] on
/// serialisation failure.
pub fn write_pack(
    output_dir: &Utf8Path,
    pack: &BriefingPack,
    profile: &BriefingProfile,
) -> Result<(), SectorError> {
    crate::export::write_md_and_json(
        output_dir,
        &format!("briefing-{}", profile.id),
        &render_markdown(pack, profile),
        pack,
    )
}

/// Read a `briefing.toml` describing one or more profiles.
///
/// # Errors
///
/// [`SectorError::Io`] if the file cannot be read; [`SectorError::ConfigParse`]
/// on syntax errors.
pub fn load_briefing_file(path: &Utf8Path) -> Result<BriefingFile, SectorError> {
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    toml::from_str::<BriefingFile>(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, GeneratedFaction, GeneratedRoute, GeneratedStar, GeneratedSystem,
        GeneratedWorld, GenerationManifest, HexCoord, PresenceDimensions, RouteStability,
        RouteType, WorldDto, WorldFactionPresence,
    };
    use std::collections::BTreeMap as Map;

    fn make_sector() -> GeneratedSector {
        let world = GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
                notable_features: vec![],
            },
            factions: vec![
                WorldFactionPresence {
                    faction_id: "imp".into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    influence: FactionInfluence::Dominant,
                    relationship_to_government: "lawful".into(),
                    dimensions: PresenceDimensions {
                        visibility: 90.0,
                        ..Default::default()
                    },
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                },
                WorldFactionPresence {
                    faction_id: "cult".into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    influence: FactionInfluence::Hidden,
                    relationship_to_government: "outlaw".into(),
                    dimensions: PresenceDimensions {
                        visibility: 5.0,
                        covert: 90.0,
                        ..Default::default()
                    },
                    dominance: DominanceState::default(),
                    intel_confidence: 5,
                },
            ],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: Default::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
        };
        let sys = GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Aurelia".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![world],
            primary_factions: vec!["imp".into()],
            tags: vec![],
            notes: vec![],
            control: Default::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        };
        GeneratedSector {
            id: "demo".into(),
            title: "Demo".into(),
            seed: "abc".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems: vec![sys],
            routes: vec![
                GeneratedRoute {
                    id: "r1".into(),
                    from_system_id: "sys-0001".into(),
                    to_system_id: "sys-0002".into(),
                    distance: 1,
                    route_type: RouteType::ChartedPassage,
                    stability: RouteStability::Stable,
                    tags: vec![],
                    controls: vec![],
                },
                GeneratedRoute {
                    id: "r2".into(),
                    from_system_id: "sys-0001".into(),
                    to_system_id: "sys-0003".into(),
                    distance: 1,
                    route_type: RouteType::Webway,
                    stability: RouteStability::Stable,
                    tags: vec![],
                    controls: vec![],
                },
            ],
            factions: vec![
                GeneratedFaction {
                    id: "imp".into(),
                    name: "Imperium".into(),
                    kind: "imperial".into(),
                    disposition: "lawful".into(),
                    subfactions: Vec::new(),
                    system_presence: vec!["sys-0001".into()],
                    world_presence: vec!["sys-0001-w1".into()],
                    power: Default::default(),
                },
                GeneratedFaction {
                    id: "cult".into(),
                    name: "Cult".into(),
                    kind: "chaos".into(),
                    disposition: "outlaw".into(),
                    subfactions: Vec::new(),
                    system_presence: vec![],
                    world_presence: vec!["sys-0001-w1".into()],
                    power: Default::default(),
                },
            ],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 1,
                world_count: 1,
                route_count: 2,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    #[test]
    fn public_atlas_strips_hidden_route_and_cult() {
        let s = make_sector();
        let p = preset(AudiencePreset::PublicAtlas);
        let pack = apply(&s, &p);
        assert!(pack.sector.routes.iter().all(|r| !r.route_type.is_hidden()));
        assert!(pack.redacted_routes.iter().any(|r| r.as_str() == "r2"));
        assert!(!pack
            .sector
            .systems
            .iter()
            .flat_map(|s| s.worlds.iter())
            .flat_map(|w| w.factions.iter())
            .any(|fp| fp.faction_id == "cult"));
    }

    #[test]
    fn gm_full_truth_preserves_everything() {
        let s = make_sector();
        let p = preset(AudiencePreset::GmFullTruth);
        let pack = apply(&s, &p);
        assert_eq!(pack.sector.routes.len(), 2);
        assert_eq!(pack.sector.factions.len(), 2);
    }

    #[test]
    fn inquisition_keeps_cult_visible() {
        let s = make_sector();
        let mut p = preset(AudiencePreset::Inquisition);
        p.observer_faction = Some("imp".into());
        // Lower the floor so the cult survives the redact filter.
        p.minimum_intel_confidence = 1;
        let pack = apply(&s, &p);
        assert!(pack
            .sector
            .systems
            .iter()
            .flat_map(|s| s.worlds.iter())
            .flat_map(|w| w.factions.iter())
            .any(|fp| fp.faction_id == "cult"));
    }
}
