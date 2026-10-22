//! Deterministic named-character generator (§3 NEW.md).
//!
//! Each significant faction presence on a world (Dominant / Significant tier)
//! and each per-system sovereign / orbital-controller / hidden-master slot
//! anchors a single named persona: a title appropriate to the faction kind,
//! 1–3 traits biased by world tags + faction disposition, and a one-line
//! agenda derived from the world's actual claims.
//!
//! Personae are a pure overlay over the finished sector: same sector ⇒ same
//! cast. The stage RNG is seeded from
//! `blake3("sectorforge:{seed}:personae:{faction_id}:{anchor_id}")` so two
//! personae drawn from the same faction in different systems never collide
//! on the same RNG stream.
//!
//! Name pools, title pools, and trait pools default to a built-in faction-kind
//! library; users can override them per-kind via an optional `personae.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use camino::Utf8Path;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{
    DominanceState, FactionInfluence, GeneratedFaction, GeneratedForce, GeneratedSector,
    GeneratedSubfaction, GeneratedSystem,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PersonaeConfig {
    /// Minimum dominance tier on a world that anchors a persona.
    /// Defaults to "Influence" — Influence/Contested/Controlled/Stronghold.
    #[serde(default = "default_min_dominance")]
    pub min_world_dominance: DominanceTier,
    /// Maximum personae anchored to a single world.
    #[serde(default = "default_per_world")]
    pub max_per_world: u32,
    /// Maximum personae anchored to a single system (across system slots).
    #[serde(default = "default_per_system")]
    pub max_per_system: u32,
    /// Per-faction-kind overrides. When a kind is missing here the built-in
    /// pools (see [`default_kind_pools`]) are used.
    #[serde(default)]
    pub kinds: BTreeMap<String, KindPools>,
    /// §3 NEW.md: manual personae imported from file. Appended to derived set.
    #[serde(default)]
    pub manual: Vec<Persona>,
    /// Emit organizational-leadership personae — the overall head of each
    /// faction's sector arm, the head of each sub-faction, and the commander of
    /// each force — in addition to the geographic (system/world) personae.
    #[serde(default = "default_faction_leaders")]
    pub faction_leaders: bool,
    /// Minimum number of systems-or-worlds an org node (faction / sub-faction /
    /// force) must be present in to anchor a leadership persona. Gate metric is
    /// `max(system_presence.len(), world_presence.len())`. A node with no
    /// placed presence never anchors a leader.
    #[serde(default = "default_min_faction_presence")]
    pub min_faction_presence: u32,
}

impl Default for PersonaeConfig {
    fn default() -> Self {
        Self {
            min_world_dominance: default_min_dominance(),
            max_per_world: default_per_world(),
            max_per_system: default_per_system(),
            kinds: BTreeMap::new(),
            manual: Vec::new(),
            faction_leaders: default_faction_leaders(),
            min_faction_presence: default_min_faction_presence(),
        }
    }
}

fn default_min_dominance() -> DominanceTier {
    DominanceTier::Influence
}
fn default_per_world() -> u32 {
    3
}
fn default_per_system() -> u32 {
    4
}
fn default_faction_leaders() -> bool {
    true
}
fn default_min_faction_presence() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DominanceTier {
    Presence,
    Influence,
    Contested,
    Controlled,
    Stronghold,
}

enum_slug!(DominanceTier {
    Presence => "presence",
    Influence => "influence",
    Contested => "contested",
    Controlled => "controlled",
    Stronghold => "stronghold",
}, display);

impl DominanceTier {
    fn rank(self) -> u8 {
        match self {
            Self::Presence => 1,
            Self::Influence => 2,
            Self::Contested => 3,
            Self::Controlled => 4,
            Self::Stronghold => 5,
        }
    }
    fn meets(self, d: DominanceState) -> bool {
        let r = match d {
            DominanceState::Rumored => 0,
            DominanceState::Presence => 1,
            DominanceState::Influence => 2,
            DominanceState::Contested => 3,
            DominanceState::Controlled => 4,
            DominanceState::Stronghold => 5,
        };
        r >= self.rank()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct KindPools {
    #[serde(default)]
    pub name_prefixes: Vec<String>,
    #[serde(default)]
    pub name_roots: Vec<String>,
    #[serde(default)]
    pub name_suffixes: Vec<String>,
    /// Optional single-name pool used in preference when non-empty.
    #[serde(default)]
    pub single_names: Vec<String>,
    #[serde(default)]
    pub titles: Vec<String>,
    #[serde(default)]
    pub traits: Vec<String>,
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonaeReport {
    pub sector_id: String,
    pub seed: String,
    pub personae: Vec<Persona>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub id: crate::ids::PersonaId,
    pub faction_id: crate::ids::FactionId,
    pub faction_kind: String,
    pub anchor: PersonaAnchor,
    pub name: String,
    pub title: String,
    pub traits: Vec<String>,
    pub agenda: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PersonaAnchor {
    System {
        system_id: crate::ids::SystemId,
        slot: SystemSlot,
    },
    World {
        system_id: crate::ids::SystemId,
        world_id: crate::ids::WorldId,
    },
    /// Organizational-leadership anchor: the head of a faction's whole sector
    /// presence (`Overall`), of a sub-faction, or of a single force. The id
    /// fields narrow from top to bottom — `subfaction_id`/`force_id` are
    /// populated only at their tier and below.
    Faction {
        faction_id: crate::ids::FactionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subfaction_id: Option<crate::ids::FactionId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        force_id: Option<crate::ids::FactionId>,
        tier: FactionTier,
    },
}

/// Tier of an organizational-leadership persona (see [`PersonaAnchor::Faction`]).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FactionTier {
    /// Head of a top-level faction's entire sector presence.
    Overall,
    /// Head of a middle-level sub-faction.
    Subfaction,
    /// Commander of a force-level catalogue entry.
    Force,
}

enum_slug!(FactionTier {
    Overall => "overall",
    Subfaction => "subfaction",
    Force => "force",
}, display);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SystemSlot {
    Sovereign,
    OrbitalController,
    EconomicHegemon,
    HiddenMaster,
}

enum_slug!(SystemSlot {
    Sovereign => "sovereign",
    OrbitalController => "orbital_controller",
    EconomicHegemon => "economic_hegemon",
    HiddenMaster => "hidden_master",
}, display);

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &PersonaeConfig) -> PersonaeReport {
    let faction_kind: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.kind.as_slug()))
        .collect();
    let mut out: Vec<Persona> = Vec::new();
    let mut used_names: BTreeSet<String> = BTreeSet::new();

    for sys in sector.systems.iter() {
        // System-slot personae.
        let max_per_system = cfg.max_per_system as usize;
        for (sys_count, (slot, faction_id)) in system_slot_factions(sys).into_iter().enumerate() {
            if sys_count >= max_per_system {
                break;
            }
            let kind = faction_kind.get(faction_id.as_str()).copied().unwrap_or("");
            let p = build_persona(
                PersonaParams {
                    sector,
                    cfg,
                    kind,
                    faction_id: &faction_id,
                    anchor: PersonaAnchor::System {
                        system_id: sys.id.clone(),
                        slot,
                    },
                    sys,
                    world: None,
                },
                &mut used_names,
            );
            out.push(p);
        }

        // World personae.
        for world in sys.worlds.iter() {
            let mut world_count = 0u32;
            for p in &world.factions {
                if world_count >= cfg.max_per_world {
                    break;
                }
                if !cfg.min_world_dominance.meets(p.dominance) {
                    continue;
                }
                // Skip merely-hidden presences unless they're the hidden master.
                if p.influence == FactionInfluence::Hidden
                    && world.control.hidden_master.as_deref() != Some(p.faction_id.as_str())
                {
                    continue;
                }
                let kind = faction_kind
                    .get(p.faction_id.as_str())
                    .copied()
                    .unwrap_or("");
                let persona = build_persona(
                    PersonaParams {
                        sector,
                        cfg,
                        kind,
                        faction_id: &p.faction_id,
                        anchor: PersonaAnchor::World {
                            system_id: sys.id.clone(),
                            world_id: world.id.clone(),
                        },
                        sys,
                        world: Some(world),
                    },
                    &mut used_names,
                );
                out.push(persona);
                world_count += 1;
            }
        }
    }

    // Organizational-leadership personae (overall / sub-faction / force heads).
    if cfg.faction_leaders {
        out.extend(derive_faction_leaders(sector, cfg, &mut used_names));
    }

    out.extend(cfg.manual.clone());

    PersonaeReport {
        sector_id: sector.id.to_string(),
        seed: sector.seed.to_string(),
        personae: out,
    }
}

fn system_slot_factions(sys: &GeneratedSystem) -> Vec<(SystemSlot, crate::ids::FactionId)> {
    let mut out: Vec<(SystemSlot, crate::ids::FactionId)> = Vec::new();
    if let Some(id) = &sys.control.sovereign {
        out.push((SystemSlot::Sovereign, id.clone()));
    }
    if let Some(id) = &sys.control.orbital_controller {
        if !out.iter().any(|(_, x)| x == id) {
            out.push((SystemSlot::OrbitalController, id.clone()));
        }
    }
    if let Some(id) = &sys.control.economic_hegemon {
        if !out.iter().any(|(_, x)| x == id) {
            out.push((SystemSlot::EconomicHegemon, id.clone()));
        }
    }
    if let Some(id) = &sys.control.hidden_master {
        if !out.iter().any(|(_, x)| x == id) {
            out.push((SystemSlot::HiddenMaster, id.clone()));
        }
    }
    out
}

// ── Organizational-leadership personae ──────────────────────────────────────────

/// Derive one leader per qualifying org node: overall faction, then each
/// sub-faction, then each force. Nodes below `cfg.min_faction_presence` are
/// skipped. Iteration is id-sorted at every level for determinism.
fn derive_faction_leaders(
    sector: &GeneratedSector,
    cfg: &PersonaeConfig,
    used: &mut BTreeSet<String>,
) -> Vec<Persona> {
    let min = cfg.min_faction_presence as usize;
    let mut out: Vec<Persona> = Vec::new();

    let mut factions: Vec<&GeneratedFaction> = sector.factions.iter().collect();
    factions.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    for f in factions {
        let kind = f.kind.as_slug();
        let fac_id = f.id.as_str();

        if f.system_presence.len().max(f.world_presence.len()) >= min {
            out.push(build_faction_leader(
                FactionLeaderParams {
                    sector,
                    cfg,
                    kind,
                    faction_id: fac_id,
                    node_name: f.name.as_ref(),
                    system_count: f.system_presence.len(),
                    world_count: f.world_presence.len(),
                    anchor: PersonaAnchor::Faction {
                        faction_id: f.id.clone(),
                        subfaction_id: None,
                        force_id: None,
                        tier: FactionTier::Overall,
                    },
                },
                used,
            ));
        }

        let mut subs: Vec<&GeneratedSubfaction> = f.subfactions.iter().collect();
        subs.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        for sf in subs {
            if sf.system_presence.len().max(sf.world_presence.len()) >= min {
                out.push(build_faction_leader(
                    FactionLeaderParams {
                        sector,
                        cfg,
                        kind,
                        faction_id: fac_id,
                        node_name: sf.name.as_ref(),
                        system_count: sf.system_presence.len(),
                        world_count: sf.world_presence.len(),
                        anchor: PersonaAnchor::Faction {
                            faction_id: f.id.clone(),
                            subfaction_id: Some(sf.id.clone()),
                            force_id: None,
                            tier: FactionTier::Subfaction,
                        },
                    },
                    used,
                ));
            }

            let mut forces: Vec<&GeneratedForce> = sf.forces.iter().collect();
            forces.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
            for force in forces {
                if force.system_presence.len().max(force.world_presence.len()) >= min {
                    out.push(build_faction_leader(
                        FactionLeaderParams {
                            sector,
                            cfg,
                            kind,
                            faction_id: fac_id,
                            node_name: force.name.as_ref(),
                            system_count: force.system_presence.len(),
                            world_count: force.world_presence.len(),
                            anchor: PersonaAnchor::Faction {
                                faction_id: f.id.clone(),
                                subfaction_id: Some(sf.id.clone()),
                                force_id: Some(force.id.clone()),
                                tier: FactionTier::Force,
                            },
                        },
                        used,
                    ));
                }
            }
        }
    }
    out
}

struct FactionLeaderParams<'a> {
    sector: &'a GeneratedSector,
    cfg: &'a PersonaeConfig,
    kind: &'a str,
    faction_id: &'a str,
    node_name: &'a str,
    system_count: usize,
    world_count: usize,
    anchor: PersonaAnchor,
}

fn build_faction_leader(params: FactionLeaderParams, used: &mut BTreeSet<String>) -> Persona {
    let FactionLeaderParams {
        sector,
        cfg,
        kind,
        faction_id,
        node_name,
        system_count,
        world_count,
        anchor,
    } = params;
    let tier = match &anchor {
        PersonaAnchor::Faction { tier, .. } => *tier,
        _ => FactionTier::Overall,
    };
    let disc = anchor_id(&anchor);
    let mut rng = stage_rng(&sector.seed, "personae", &format!("{faction_id}:{disc}"));

    let pools = resolve_pools(cfg, kind);
    let name = generate_name(&mut rng, &pools, used);
    let title = match tier_titles(kind, tier).choose(&mut rng) {
        Some(t) => (*t).to_string(),
        None => pick(&pools.titles, &mut rng).unwrap_or_default(),
    };
    let traits = pick_traits(&pools.traits, &mut rng, None);
    let agenda = build_faction_agenda(kind, tier, node_name, system_count, world_count);

    Persona {
        id: format!("persona-{faction_id}-{disc}").into(),
        faction_id: crate::ids::FactionId::new(faction_id),
        faction_kind: kind.to_string(),
        anchor,
        name,
        title,
        traits,
        agenda,
    }
}

/// Deterministic org-leader agenda: tier-appropriate framing over the node's
/// footprint, closed with the faction-kind verb shared with geographic agendas.
fn build_faction_agenda(
    kind: &str,
    tier: FactionTier,
    node_name: &str,
    system_count: usize,
    world_count: usize,
) -> String {
    let verb = agenda_verb(kind);
    match tier {
        FactionTier::Overall => format!(
            "Commands all {node_name} forces across {system_count} system(s); seeks to {verb}."
        ),
        FactionTier::Subfaction => {
            format!("Leads {node_name} across {world_count} world(s); seeks to {verb}.")
        }
        FactionTier::Force => {
            format!("Field commander of {node_name} ({world_count} world(s)); seeks to {verb}.")
        }
    }
}

/// Fold a faction-kind string — in any of the spellings the generator / catalog
/// can produce — to the single canonical key the persona pools below are keyed
/// on. This is the one place that knows the full alias set, so every lookup
/// into [`KIND_TABLE`] (`tier_titles`, `agenda_verb`, `default_pool`) routes
/// through it and no producible kind falls through to the generic placeholder.
///
/// The canonical keys returned here are exactly the [`KIND_TABLE`] keys
/// (Finding #20). Inputs that genuinely aren't a known faction kind return
/// `""`, which deliberately lands on [`PLACEHOLDER_KIND`].
fn canonical_kind(kind: &str) -> &'static str {
    // Normalise apostrophes (T'au / t'au), case, and surrounding whitespace so
    // the match below only has to enumerate the underscore spellings.
    let k = kind.trim().to_ascii_lowercase().replace('\'', "");
    match k.as_str() {
        // Imperial civil authority + Imperial military / elite arms.
        "imperial"
        | "imperial_guard"
        | "imperial_knight"
        | "adeptus_astartes"
        | "deathwatch"
        | "grey_knights"
        | "talons_of_the_emperor"
        | "collegia_titanica" => "imperial",
        // Militant faith.
        "adepta_sororitas" | "ecclesiarchy" => "ecclesiarchy",
        "inquisition" | "inquisitorial" => "inquisition",
        // Adeptus Mechanicus + its corrupted mirror.
        "mechanicus" | "dark_mechanicum" => "mechanicus",
        // Chaos / traitor / warp, including generic and heretic cults.
        "chaos"
        | "chaos_space_marine"
        | "chaos_knight"
        | "traitor_guard"
        | "traitor_titan_legion"
        | "daemon"
        | "cult"
        | "heretic"
        | "renegade" => "chaos",
        // Genestealer cults (all spellings).
        "genestealer_cult" | "genestealer" | "gsc" => "genestealer",
        "tau" => "tau",
        "aeldari" | "eldar" => "aeldari",
        "harlequin" => "harlequin",
        "drukhari" => "drukhari",
        "ork" | "orks" => "ork",
        "tyranid" => "tyranid",
        "necron" => "necron",
        // Commercial / criminal mercantile powers (incl. rogue-trader dynasties).
        "merchant" | "mercantile" | "rogue_trader" | "criminal" => "merchant",
        "rebel" | "separatist" => "rebel",
        // Other xenos polities with no bespoke pool (incl. the Votann).
        "xenos" | "minor_xenos" | "leagues_of_votann" => "xenos",
        _ => "",
    }
}

/// Tier-flavored title pools. `Overall` returns sector-supreme titles and
/// `Force` returns field-command titles; `Subfaction` returns empty so the
/// caller falls back to the mid-level [`KindPools::titles`]. Empty result for
/// an unknown kind also falls back.
fn tier_titles(kind: &str, tier: FactionTier) -> &'static [&'static str] {
    let row = kind_row(kind);
    match tier {
        FactionTier::Subfaction => &[],
        FactionTier::Overall => row.overall,
        FactionTier::Force => row.force,
    }
}

struct PersonaParams<'a> {
    sector: &'a GeneratedSector,
    cfg: &'a PersonaeConfig,
    kind: &'a str,
    faction_id: &'a str,
    anchor: PersonaAnchor,
    sys: &'a GeneratedSystem,
    world: Option<&'a crate::sector_model::GeneratedWorld>,
}

fn build_persona(params: PersonaParams, used: &mut BTreeSet<String>) -> Persona {
    let PersonaParams {
        sector,
        cfg,
        kind,
        faction_id,
        anchor,
        sys,
        world,
    } = params;
    let anchor_disc = match &anchor {
        PersonaAnchor::System { system_id, slot } => format!("{system_id}:{slot:?}"),
        PersonaAnchor::World {
            system_id,
            world_id,
        } => format!("{system_id}:{world_id}"),
        // `build_persona` only handles geographic anchors; org-leadership
        // anchors go through `build_faction_leader`. Kept for exhaustiveness.
        PersonaAnchor::Faction { .. } => anchor_id(&anchor),
    };
    let mut rng = stage_rng(
        &sector.seed,
        "personae",
        &format!("{faction_id}:{anchor_disc}"),
    );

    let pools = resolve_pools(cfg, kind);

    let name = generate_name(&mut rng, &pools, used);
    let title = pick(&pools.titles, &mut rng).unwrap_or_default();
    let traits = pick_traits(&pools.traits, &mut rng, world);
    let agenda = build_agenda(kind, faction_id, &anchor, world, sys);

    let pid = format!("persona-{}-{}", faction_id, anchor_id(&anchor));
    Persona {
        id: pid.into(),
        faction_id: crate::ids::FactionId::new(faction_id),
        faction_kind: kind.to_string(),
        anchor,
        name,
        title,
        traits,
        agenda,
    }
}

fn anchor_id(a: &PersonaAnchor) -> String {
    match a {
        PersonaAnchor::System { system_id, slot } => format!("{system_id}-{slot:?}").to_lowercase(),
        PersonaAnchor::World {
            system_id,
            world_id,
        } => format!("{system_id}-{world_id}"),
        PersonaAnchor::Faction {
            faction_id,
            subfaction_id,
            force_id,
            tier,
        } => {
            let sub = subfaction_id
                .as_ref()
                .map(|s| format!("-{s}"))
                .unwrap_or_default();
            let force = force_id
                .as_ref()
                .map(|s| format!("-{s}"))
                .unwrap_or_default();
            format!("faction-{}-{faction_id}{sub}{force}", tier.as_slug()).to_lowercase()
        }
    }
}

fn generate_name(rng: &mut impl Rng, pools: &KindPools, used: &mut BTreeSet<String>) -> String {
    // Prefer single names, then a personal name (root + suffix, with rank
    // honorifics dropped so they don't echo the title), then — only to keep
    // the name unique — the honorific-prefixed form, then a numeral fallback.
    if !pools.single_names.is_empty() {
        if let Some(s) = pick_unique(&pools.single_names, rng, used) {
            return s;
        }
    }
    if !pools.name_roots.is_empty() || !pools.name_prefixes.is_empty() {
        // Pass 1: personal name only (honorific prefixes suppressed, name
        // particles such as T'au "Shas'" kept). Pass 2: re-admit the honorific
        // to recover uniqueness before resorting to a numeral.
        for allow_honorific in [false, true] {
            for _ in 0..8 {
                let pre = pick(&pools.name_prefixes, rng).unwrap_or_default();
                let root = pick(&pools.name_roots, rng).unwrap_or_default();
                let suf = pick(&pools.name_suffixes, rng).unwrap_or_default();
                let candidate = assemble_name(&pre, &root, &suf, allow_honorific);
                if candidate.is_empty() {
                    break;
                }
                if used.insert(candidate.clone()) {
                    return candidate;
                }
            }
        }
    }
    // Numeral fallback — guaranteed unique.
    let mut n = 1u32;
    loop {
        let candidate = format!("Unnamed Persona {n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// Assemble a persona name from its pool parts.
///
/// A prefix ending in `'` or `-` is treated as a name *particle* and attaches
/// directly to the root (e.g. T'au `"Shas'" + "ui"` ⇒ `"Shas'ui"`). Any other
/// prefix is a rank *honorific* (`"Farseer"`, `"Magos"`, `"Lord"`); it is
/// dropped unless `allow_honorific` is set, because the persona's title already
/// carries rank — keeping it produces redundancies like
/// "Farseer Macha, High Farseer".
fn assemble_name(prefix: &str, root: &str, suffix: &str, allow_honorific: bool) -> String {
    let is_particle = prefix.ends_with('\'') || prefix.ends_with('-');
    let mut name = String::new();
    if is_particle {
        name.push_str(prefix);
        name.push_str(root);
    } else {
        if allow_honorific && !prefix.is_empty() {
            name.push_str(prefix);
            if !root.is_empty() {
                name.push(' ');
            }
        }
        name.push_str(root);
    }
    if !suffix.is_empty() {
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(suffix);
    }
    name.trim().to_string()
}

fn pick<T: Clone>(pool: &[T], rng: &mut impl Rng) -> Option<T> {
    pool.choose(rng).cloned()
}

fn pick_unique(pool: &[String], rng: &mut impl Rng, used: &mut BTreeSet<String>) -> Option<String> {
    let mut shuffled: Vec<&String> = pool.iter().collect();
    shuffled.shuffle(rng);
    for s in shuffled {
        if used.insert(s.clone()) {
            return Some(s.clone());
        }
    }
    None
}

fn pick_traits(
    pool: &[String],
    rng: &mut impl Rng,
    world: Option<&crate::sector_model::GeneratedWorld>,
) -> Vec<String> {
    if pool.is_empty() {
        return Vec::new();
    }
    let count = rng.gen_range(1..=3.min(pool.len()));
    let mut shuffled: Vec<&String> = pool.iter().collect();
    shuffled.shuffle(rng);
    let mut out: Vec<String> = shuffled.iter().take(count).map(|s| (*s).clone()).collect();

    // Light contextual bias: if the anchor world has notable_features
    // matching trait keywords, push that trait to the front (deterministic).
    if let Some(w) = world {
        let feature_join: String = w
            .world
            .notable_features
            .iter()
            .map(|s| s.as_ref().to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        for trait_name in pool {
            let lower = trait_name.to_ascii_lowercase();
            if (feature_join.contains("police") && lower.contains("iron"))
                || (feature_join.contains("shrine") && lower.contains("zealous"))
                || (feature_join.contains("cult") && lower.contains("paranoid"))
            {
                if !out.iter().any(|s| s == trait_name) {
                    out.insert(0, trait_name.clone());
                    if out.len() > 3 {
                        out.truncate(3);
                    }
                }
                break;
            }
        }
    }
    out
}

fn build_agenda(
    kind: &str,
    faction_id: &str,
    anchor: &PersonaAnchor,
    world: Option<&crate::sector_model::GeneratedWorld>,
    sys: &GeneratedSystem,
) -> String {
    let where_phrase = match anchor {
        PersonaAnchor::World { world_id, .. } => world
            .map(|w| w.name.clone())
            .unwrap_or_else(|| world_id.as_str().to_string().into()),
        _ => sys.name.clone(),
    };
    // Inspect competing claims for color.
    let rival = world.and_then(|w| {
        w.claims
            .iter()
            .find(|c| c.faction_id != faction_id)
            .map(|c| (c.faction_id.clone(), c.claim_type))
    });
    if let Some((rival_id, claim)) = rival {
        return format!(
            "Seeks to {} on {} against {} (claim: {:?}).",
            agenda_verb(kind),
            where_phrase,
            rival_id,
            claim,
        );
    }
    format!("Seeks to {} on {}.", agenda_verb(kind), where_phrase,)
}

fn agenda_verb(kind: &str) -> &'static str {
    kind_row(kind).verb
}

// ── Built-in pools ─────────────────────────────────────────────────────────────

fn resolve_pools(cfg: &PersonaeConfig, kind: &str) -> KindPools {
    // Honour a user override under the exact spelling first (backward compat),
    // then under the canonical key, so e.g. an `adeptus_astartes` faction still
    // picks up a `[kinds.imperial]` override block. Built-in pools fall back
    // through `default_pool`, which canonicalises internally.
    if let Some(p) = cfg.kinds.get(kind) {
        return merge_with_defaults(p, kind);
    }
    let canon = canonical_kind(kind);
    if !canon.is_empty() {
        if let Some(p) = cfg.kinds.get(canon) {
            return merge_with_defaults(p, kind);
        }
    }
    default_pool(kind)
}

fn merge_with_defaults(user: &KindPools, kind: &str) -> KindPools {
    let base = default_pool(kind);
    let or_base = |u: &Vec<String>, b: Vec<String>| if u.is_empty() { b } else { u.clone() };
    KindPools {
        name_prefixes: or_base(&user.name_prefixes, base.name_prefixes),
        name_roots: or_base(&user.name_roots, base.name_roots),
        name_suffixes: or_base(&user.name_suffixes, base.name_suffixes),
        single_names: or_base(&user.single_names, base.single_names),
        titles: or_base(&user.titles, base.titles),
        traits: or_base(&user.traits, base.traits),
    }
}

fn default_pool(kind: &str) -> KindPools {
    let row = kind_row(kind);
    KindPools {
        name_prefixes: svec(row.prefixes),
        name_roots: svec(row.roots),
        name_suffixes: svec(row.suffixes),
        single_names: vec![],
        titles: svec(row.titles),
        traits: svec(row.traits),
    }
}

fn svec(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

/// Built-in persona library for one canonical kind (see [`canonical_kind`]):
/// leadership-tier titles, agenda verb, and the default name / title / trait
/// pools.
struct KindRow {
    key: &'static str,
    overall: &'static [&'static str],
    force: &'static [&'static str],
    verb: &'static str,
    prefixes: &'static [&'static str],
    roots: &'static [&'static str],
    suffixes: &'static [&'static str],
    titles: &'static [&'static str],
    traits: &'static [&'static str],
}

fn kind_row(kind: &str) -> &'static KindRow {
    let k = canonical_kind(kind);
    KIND_TABLE
        .iter()
        .find(|r| r.key == k)
        .unwrap_or(&PLACEHOLDER_KIND)
}

/// Generic entry for kinds [`canonical_kind`] doesn't know.
static PLACEHOLDER_KIND: KindRow = KindRow {
    key: "",
    overall: &["Paramount Leader", "Sector Warlord"],
    force: &["Commander", "Captain"],
    verb: "secure faction interests",
    prefixes: &[],
    roots: &["Anonymous", "Unknown", "Faceless"],
    suffixes: &["One", "Two", "Three"],
    titles: &["Faction Lead", "Senior Operative"],
    traits: &["Discreet", "Stoic"],
};

const KIND_TABLE: &[KindRow] = &[
    KindRow {
        key: "imperial",
        overall: &["Lord Sector", "Lord Commander Militant", "Sector Governor"],
        force: &["Colonel", "Lord Captain", "Knight Commander"],
        verb: "enforce Imperial writ",
        prefixes: &["Lord", "Lady", "High"],
        roots: &[
            "Valerian",
            "Aurelius",
            "Constance",
            "Septimia",
            "Marius",
            "Cassia",
            "Cornelius",
            "Drusilla",
            "Octavian",
            "Petronius",
        ],
        suffixes: &["Vex", "Karn", "Voll", "Halen", "Marsden", "Drachma"],
        titles: &[
            "Planetary Governor",
            "Lord Militant",
            "Sector Lord",
            "Subsector Praetor",
            "High Steward",
        ],
        traits: &[
            "Iron-Fisted",
            "Paranoid",
            "Pious",
            "Diplomatic",
            "Severe",
            "Pragmatic",
            "Indolent",
        ],
    },
    KindRow {
        key: "mechanicus",
        overall: &["Fabricator-General", "Archmagos Prime"],
        force: &["Tech-Priest Dominus", "Myrmidon Secutor"],
        verb: "expand the cogitator covenant",
        prefixes: &["Magos", "Genetor", "Artisan", "Logis"],
        roots: &[
            "Cryptus", "Theta", "Omikron", "Vex", "Volta", "Caliban", "Hyperion", "Strados",
        ],
        suffixes: &["-4", "-VII", "-XXI", "Prime", "Secundus", "Mu", "Tertius"],
        titles: &[
            "Magos Dominus",
            "Forge Lord",
            "Archmagos Veneratus",
            "Logis Strategos",
        ],
        traits: &[
            "Logic-Bound",
            "Tech-Heretic-Curious",
            "Schematic",
            "Hoarding",
            "Detached",
            "Inquisitive",
        ],
    },
    KindRow {
        key: "ecclesiarchy",
        overall: &["Cardinal Astral", "Arch-Cardinal"],
        force: &["Palatine", "Canoness"],
        verb: "extend the dominion of the Faith",
        prefixes: &["Father", "Mother", "Cardinal", "Confessor", "Deacon"],
        roots: &[
            "Ambrose",
            "Cassian",
            "Theonia",
            "Vespera",
            "Severus",
            "Penance",
            "Sabbatine",
        ],
        suffixes: &["of the Sword", "the Pure", "the Stern", "the Anointed"],
        titles: &[
            "Cardinal",
            "Arch-Confessor",
            "Sister Superior",
            "Missionary Vexilla",
            "Episcopa",
        ],
        traits: &[
            "Zealous",
            "Mortified",
            "Eloquent",
            "Iron-Faithed",
            "Ascetic",
            "Vengeful",
        ],
    },
    KindRow {
        key: "inquisition",
        overall: &["Lord Inquisitor", "Inquisitorial Representative"],
        force: &["Interrogator", "Throne Agent"],
        verb: "hunt heresy in shadow",
        prefixes: &["Inquisitor", "Interrogator"],
        roots: &[
            "Eisenhorn",
            "Ravenor",
            "Karamanzov",
            "Quixos",
            "Sand",
            "Greyfax",
            "Drazyari",
            "Voll",
        ],
        suffixes: &["", "-Vekt", "-Kael", "-Mordant"],
        titles: &[
            "Inquisitor (Ordo Malleus)",
            "Inquisitor (Ordo Hereticus)",
            "Inquisitor (Ordo Xenos)",
            "Lord Inquisitor",
        ],
        traits: &[
            "Paranoid",
            "Radical",
            "Puritan",
            "Calculating",
            "Cold",
            "Ruthless",
            "Patient",
        ],
    },
    KindRow {
        key: "merchant",
        overall: &["Warrant-Holder Paramount", "Lord-Captain Dynast"],
        force: &["Captain", "Voidmaster"],
        verb: "press a commercial advantage",
        prefixes: &["Lord-Captain", "Captain", "Dame", "Dynast"],
        roots: &[
            "Vrede",
            "Halcyon",
            "Sigismund",
            "Karnak",
            "Verrin",
            "Calderra",
            "Surak",
            "Tessera",
        ],
        suffixes: &["Caligari", "Voss", "Drask", "Hallow", "Tannhauser"],
        titles: &[
            "Rogue Trader",
            "Free Captain",
            "Dynast Primus",
            "Lord of the Charter",
        ],
        traits: &[
            "Acquisitive",
            "Charming",
            "Indebted",
            "Bold",
            "Calculating",
            "Glittering",
        ],
    },
    KindRow {
        key: "chaos",
        overall: &["Warmaster", "Dark Apostle Ascendant"],
        force: &["Champion", "Aspiring Lord"],
        verb: "spread the touch of the Dark Gods",
        prefixes: &["Lord", "Champion", "Sorcerer"],
        roots: &[
            "Vraxis",
            "Mortarion",
            "Khar'kos",
            "Tzeneth",
            "Slathar",
            "Bel'kor",
            "Khel'gar",
            "Maleth",
        ],
        suffixes: &[
            "the Damned",
            "the Eight-Eyed",
            "of the Black Tongue",
            "the Defiler",
        ],
        titles: &[
            "Chaos Lord",
            "Daemon Prince",
            "Dark Apostle",
            "Sorcerer-Captain",
        ],
        traits: &[
            "Vainglorious",
            "Tainted",
            "Cunning",
            "Wrathful",
            "Patient",
            "Mad",
        ],
    },
    KindRow {
        key: "rebel",
        overall: &["Liberation Marshal", "Supreme Commander"],
        force: &["Field Commander", "Cell Leader"],
        verb: "break Imperial chains",
        prefixes: &["Commander", "Captain", "Speaker", "Headsman"],
        roots: &[
            "Toran", "Vada", "Selka", "Kosh", "Mira", "Petros", "Yarrick", "Aren",
        ],
        suffixes: &["", "Lien", "Vesh", "Karn"],
        titles: &[
            "Rebel Commander",
            "Council Speaker",
            "Revolutionary Marshal",
            "Demagogue",
        ],
        traits: &[
            "Charismatic",
            "Embittered",
            "Idealistic",
            "Ruthless",
            "Hounded",
        ],
    },
    KindRow {
        key: "necron",
        overall: &["Phaeron", "Supreme Overlord"],
        force: &["Lord", "Royal Warden"],
        verb: "reclaim ancient dominion",
        prefixes: &["Overlord", "Phaeron", "Cryptek"],
        roots: &[
            "Sahmek",
            "Trazyn",
            "Imotekh",
            "Kutlakh",
            "Anrakyr",
            "Khaybar",
            "Zahndrekh",
        ],
        suffixes: &["", "of the Black Pyramid", "the Silent"],
        titles: &[
            "Necron Overlord",
            "Phaeron",
            "Cryptek Plasmancer",
            "Lord of the Tomb",
        ],
        traits: &["Implacable", "Patient", "Disdainful", "Curious", "Vengeful"],
    },
    KindRow {
        key: "tyranid",
        overall: &["Hive Tyrant Prime", "Norn-Queen Locus"],
        force: &["Alpha Warrior", "Synapse Node"],
        verb: "feed the Hive",
        prefixes: &[],
        roots: &["Hive Tyrant", "Norn-Queen Echo", "Synapse Node", "Tendril"],
        suffixes: &["Alpha", "Beta", "Gamma", "Delta", "Epsilon"],
        titles: &["Hive Tyrant", "Synapse Beast", "Norn-Queen Echo"],
        traits: &["Hungry", "Adaptive", "Patient", "Implacable"],
    },
    KindRow {
        key: "ork",
        overall: &["Warlord", "Big Boss of Bosses"],
        force: &["Nob", "Boss"],
        verb: "spread the Waaagh!",
        prefixes: &[],
        roots: &[
            "Skarsnik",
            "Grimskull",
            "Bonebreaka",
            "Throgg",
            "Garglug",
            "Skullkrump",
            "Urghuz",
        ],
        suffixes: &["Big-Mek", "Da Brutal", "Wartoof", "Da Boss", "Krumpa"],
        titles: &["Warboss", "Big Mek", "Painboy", "Nob Boss"],
        traits: &[
            "Brutal",
            "Cunning",
            "Loud",
            "Inventive",
            "Massive",
            "Reckless",
        ],
    },
    KindRow {
        key: "tau",
        overall: &["High Ethereal", "Supreme Commander"],
        force: &["Shas'o", "Shas'el"],
        verb: "expand the Greater Good",
        prefixes: &["Aun'", "Shas'", "Por'"],
        roots: &["vre", "ui", "el", "o", "la", "su"],
        suffixes: &["Mont'yr", "Tash'var", "Sa'cea", "Bork'an", "Vior'la"],
        titles: &[
            "Ethereal",
            "Shas'O Commander",
            "Por'el Diplomat",
            "Aun'vre Aide",
        ],
        traits: &[
            "Patient",
            "Idealistic",
            "Disciplined",
            "Inquisitive",
            "Naive",
        ],
    },
    KindRow {
        key: "aeldari",
        overall: &["Autarch Supreme", "High Farseer"],
        force: &["Exarch", "Autarch"],
        verb: "thread the Skein toward victory",
        prefixes: &["Farseer", "Autarch", "Spiritseer"],
        roots: &["Eldrad", "Asurmen", "Macha", "Yvraine", "Yriel", "Saerith"],
        suffixes: &["", "the Cold", "Lin'doril"],
        titles: &["Farseer", "Autarch", "Warlock", "Spiritseer"],
        traits: &["Cold", "Cryptic", "Patient", "Haughty", "Foreboding"],
    },
    KindRow {
        key: "drukhari",
        overall: &["Supreme Archon"],
        force: &["Dracon", "Sybarite"],
        verb: "harvest fresh suffering",
        prefixes: &["Archon", "Dracon", "Succubus"],
        roots: &["Vect", "Yllith", "Sliscus", "Aestred", "Drazhar"],
        suffixes: &["the Cruel", "Pain-Touched"],
        titles: &["Archon", "Dracon", "Succubus", "Haemonculus"],
        traits: &["Cruel", "Decadent", "Cunning", "Vindictive", "Aesthete"],
    },
    KindRow {
        key: "harlequin",
        overall: &["Great Harlequin"],
        force: &["Troupe Master"],
        verb: "play out the Black Library",
        prefixes: &["Troupe Master", "Shadowseer"],
        roots: &["Sylandri", "Veilwalker", "Death-Mask"],
        suffixes: &["", "the Laughing", "of the Crone"],
        titles: &["Troupe Master", "Shadowseer", "Death Jester"],
        traits: &["Theatrical", "Cryptic", "Lethal"],
    },
    KindRow {
        key: "genestealer",
        overall: &["Patriarch"],
        force: &["Primus", "Acolyte Iconward"],
        verb: "incubate the awakening",
        prefixes: &["Patriarch", "Magus", "Primus"],
        roots: &["Saint", "First", "Whisper", "Old", "Quiet"],
        suffixes: &["Hraxon", "Velt", "Marek", "Selas"],
        titles: &["Cult Patriarch", "Cult Magus", "Cult Primus"],
        traits: &["Patient", "Soothing", "Insidious", "Familial", "Concealed"],
    },
    KindRow {
        key: "xenos",
        overall: &["Paramount Warleader", "Supreme Voice"],
        force: &["Warleader", "Hivespeaker"],
        verb: "press alien interests",
        prefixes: &[],
        roots: &["Vesh'Ka", "Mor'tek", "Zhuun", "Pak'thar", "Ssuln"],
        suffixes: &["", "the Many-Eyed", "of the Deep"],
        titles: &["Warleader", "Hivespeaker", "High Voice"],
        traits: &["Inscrutable", "Hostile", "Cunning"],
    },
];

// ── Markdown rendering ─────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &PersonaeReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Dramatis Personae — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal personae: **{}**", report.personae.len());

    // Group by faction.
    let mut by_faction: BTreeMap<crate::ids::FactionId, Vec<&Persona>> = BTreeMap::new();
    for p in &report.personae {
        by_faction.entry(p.faction_id.clone()).or_default().push(p);
    }
    for (faction_id, group) in &by_faction {
        let _ = writeln!(s, "\n## {faction_id}");
        for p in group {
            let traits = if p.traits.is_empty() {
                String::new()
            } else {
                format!(" — _{}_", p.traits.join(", "))
            };
            let anchor = match &p.anchor {
                PersonaAnchor::System { system_id, slot } => {
                    format!("({system_id}, {slot:?})")
                }
                PersonaAnchor::World {
                    system_id,
                    world_id,
                } => format!("({system_id}/{world_id})"),
                PersonaAnchor::Faction {
                    subfaction_id,
                    force_id,
                    tier,
                    ..
                } => match (subfaction_id, force_id) {
                    (_, Some(force)) => format!("(force: {force})"),
                    (Some(sub), None) => format!("(sub-faction: {sub})"),
                    (None, None) => format!("({})", tier.as_slug()),
                },
            };
            let _ = writeln!(
                s,
                "- **{}**, {} {} {}\n  - {}",
                p.name, p.title, anchor, traits, p.agenda
            );
        }
    }
    s
}

/// Write `personae.md` + `personae.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] if either file cannot be written and
/// [`SectorError::ExportFailed`] if the report cannot be serialised.
pub fn write_report(output_dir: &Utf8Path, report: &PersonaeReport) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "personae", &render_markdown(report), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        FactionInfluence, GeneratedFaction, GeneratedStar, GeneratedSystem, GeneratedWorld,
        GenerationManifest, HexCoord, PowerProfile, PresenceDimensions, SystemControlSummary,
        WorldControlSummary, WorldDto, WorldFactionPresence,
    };
    use std::collections::BTreeMap as Map;

    fn empty_sector() -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "personae-seed".into(),
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

    #[test]
    fn deterministic_personae() {
        let mut sec = empty_sector();
        sec.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: vec![],
            world_presence: vec![],
            power: PowerProfile::default(),
        });
        sec.systems.push(GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Alpha".into(),
            coord: HexCoord { q: 0, r: 0 },
            kind: crate::sector_model::SystemKind::Star,
            star: Some(GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            }),
            worlds: vec![GeneratedWorld {
                id: "wrld-0001-1".into(),
                index: 1,
                name: "Alpha Prime".into(),
                orbit: 1,
                source_row_index: 0,
                world: WorldDto {
                    star_colour: crate::worlds::StarColour::Yellow,
                    world_type: crate::worlds::WorldType::HiveWorld,
                    atmosphere: crate::worlds::Atmosphere::Breathable,
                    temperature: crate::worlds::Temperature::Temperate,
                    biosphere: crate::worlds::Biosphere::Thriving,
                    population: crate::worlds::Population::ExtremelyDense,
                    tech_level: crate::worlds::TechLevel::High,
                    government: crate::worlds::Government::MilitaryGovernor,
                    notable_features: vec![],
                },
                factions: vec![WorldFactionPresence {
                    faction_id: "imp".into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: FactionInfluence::Dominant,
                    relationship_to_government: "loyal".into(),
                    dimensions: PresenceDimensions::default(),
                    dominance: DominanceState::Controlled,
                    intel_confidence: 100,
                }],
                tags: vec![],
                notes: vec![],
                claims: vec![],
                control: WorldControlSummary {
                    dominant: Some("imp".into()),
                    sovereign: Some("imp".into()),
                    ..Default::default()
                },
                stability: Default::default(),
                regions: vec![],
                conflict: Default::default(),
                intel: Default::default(),
            }],
            primary_factions: vec!["imp".into()],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary {
                sovereign: Some("imp".into()),
                ..Default::default()
            },
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        });

        let a = derive_with(&sec, &PersonaeConfig::default());
        let b = derive_with(&sec, &PersonaeConfig::default());
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert!(!a.personae.is_empty());
        assert!(a.personae.iter().any(|p| p.faction_id == "imp"));
    }

    /// A faction with a populated sub-faction/force hierarchy anchors one
    /// leader per tier; a zero-presence force is gated out.
    #[test]
    fn faction_leaders_span_all_tiers_and_honour_presence() {
        let mut sec = empty_sector();
        sec.factions.push(GeneratedFaction {
            id: "imp".into(),
            name: "Imperium".into(),
            kind: "imperial".into(),
            disposition: "lawful".into(),
            subfactions: vec![crate::sector_model::GeneratedSubfaction {
                id: "imp-astartes".into(),
                name: "Adeptus Astartes".into(),
                disposition: "lawful".into(),
                forces: vec![
                    GeneratedForce {
                        id: "imp-astartes-3rd".into(),
                        name: "3rd Company".into(),
                        disposition: "lawful".into(),
                        system_presence: vec!["sys-0001".into()],
                        world_presence: vec!["wrld-0001-1".into()],
                    },
                    // Zero presence: must be gated out at the default min=1.
                    GeneratedForce {
                        id: "imp-astartes-recon".into(),
                        name: "Recon Detachment".into(),
                        disposition: "lawful".into(),
                        system_presence: vec![],
                        world_presence: vec![],
                    },
                ],
                system_presence: vec!["sys-0001".into()],
                world_presence: vec!["wrld-0001-1".into()],
            }],
            system_presence: vec!["sys-0001".into()],
            world_presence: vec!["wrld-0001-1".into()],
            power: PowerProfile::default(),
        });

        let report = derive_with(&sec, &PersonaeConfig::default());
        let tiers: Vec<FactionTier> = report
            .personae
            .iter()
            .filter_map(|p| match &p.anchor {
                PersonaAnchor::Faction { tier, .. } => Some(*tier),
                _ => None,
            })
            .collect();
        assert!(
            tiers.contains(&FactionTier::Overall),
            "missing overall leader"
        );
        assert!(
            tiers.contains(&FactionTier::Subfaction),
            "missing sub-faction leader"
        );
        assert!(tiers.contains(&FactionTier::Force), "missing force leader");
        // Exactly one force leader — the zero-presence force is skipped.
        let force_leaders = report.personae.iter().filter(|p| {
            matches!(
                &p.anchor,
                PersonaAnchor::Faction {
                    force_id: Some(_),
                    ..
                }
            )
        });
        assert_eq!(force_leaders.count(), 1, "zero-presence force not gated");

        // Toggling the feature off drops every org-leadership persona.
        let cfg = PersonaeConfig {
            faction_leaders: false,
            ..Default::default()
        };
        let off = derive_with(&sec, &cfg);
        assert!(
            !off.personae
                .iter()
                .any(|p| matches!(p.anchor, PersonaAnchor::Faction { .. })),
            "faction_leaders=false still emitted org leaders"
        );
    }

    /// Finding #20: every faction kind the generator / control derivation can
    /// produce must, after [`canonical_kind`] folding, land on a *real* persona
    /// entry in each dispatch table — never the generic placeholder. The list
    /// below is the authoritative producer vocabulary: it mirrors the
    /// `kind_profile` match arms in `crate::analysis::control` (the canonical set
    /// of kinds `control.rs` understands) plus every alias spelling personae
    /// itself accepts. If a new kind is added there, add it here too.
    #[test]
    fn every_producible_kind_resolves_to_non_placeholder_persona() {
        // The placeholder outputs each table emits for an unknown kind. Defining
        // them via the empty key keeps the test honest if the `_` arms change.
        let placeholder_pool = default_pool("");
        let placeholder_verb = agenda_verb("");
        let placeholder_overall = tier_titles("", FactionTier::Overall);
        let placeholder_force = tier_titles("", FactionTier::Force);

        // Authoritative producer set: the `kind_profile` keys in `control.rs`
        // (what generation stores on `GeneratedFaction.kind`, directly or via
        // `legacy_top_faction_id`) plus personae's own accepted alias spellings.
        let producible: &[&str] = &[
            // control.rs kind_profile arms.
            "imperial",
            "adepta_sororitas",
            "inquisition",
            "adeptus_astartes",
            "imperial_guard",
            "imperial_knight",
            "collegia_titanica",
            "deathwatch",
            "grey_knights",
            "talons_of_the_emperor",
            "mechanicus",
            "dark_mechanicum",
            "chaos_space_marine",
            "chaos_knight",
            "traitor_guard",
            "traitor_titan_legion",
            "daemon",
            "cult",
            "tau",
            "aeldari",
            "drukhari",
            "harlequin",
            "leagues_of_votann",
            "ork",
            "tyranid",
            "necron",
            "minor_xenos",
            "xenos",
            "merchant",
            "criminal",
            "rebel",
            "genestealer_cult",
            // legacy_top_faction_id outputs not otherwise listed.
            "chaos",
            // Alias spellings personae accepts.
            "inquisitorial",
            "heretic",
            "renegade",
            "rogue_trader",
            "mercantile",
            "separatist",
            "orks",
            "eldar",
            "t'au",
            "gsc",
            "genestealer",
        ];

        for kind in producible {
            let canon = canonical_kind(kind);
            assert!(
                !canon.is_empty(),
                "kind {kind:?} folds to the empty canonical key (placeholder)"
            );

            // Pools and agenda verb must differ from the placeholder's.
            let pool = default_pool(kind);
            assert_ne!(
                pool.name_roots, placeholder_pool.name_roots,
                "kind {kind:?} (canon {canon:?}) falls through to placeholder name pool"
            );
            assert_ne!(
                pool.titles, placeholder_pool.titles,
                "kind {kind:?} (canon {canon:?}) falls through to placeholder title pool"
            );
            assert_ne!(
                agenda_verb(kind),
                placeholder_verb,
                "kind {kind:?} (canon {canon:?}) falls through to placeholder agenda verb"
            );

            // Both flavored leadership tiers must resolve too (Subfaction
            // deliberately returns empty and falls back to KindPools::titles,
            // so it is exempt).
            assert_ne!(
                tier_titles(kind, FactionTier::Overall),
                placeholder_overall,
                "kind {kind:?} (canon {canon:?}) falls through to placeholder Overall titles"
            );
            assert_ne!(
                tier_titles(kind, FactionTier::Force),
                placeholder_force,
                "kind {kind:?} (canon {canon:?}) falls through to placeholder Force titles"
            );
        }
    }

    /// Genuinely unknown kinds still resolve to the placeholder (the normalizer
    /// must not accidentally start claiming arbitrary strings).
    #[test]
    fn unknown_kind_stays_placeholder() {
        assert_eq!(canonical_kind("definitely-not-a-faction"), "");
        assert_eq!(canonical_kind(""), "");
        assert_eq!(agenda_verb(""), "secure faction interests");
    }
}
