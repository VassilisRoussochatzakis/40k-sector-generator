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
use crate::sector_model::{DominanceState, FactionInfluence, GeneratedSector, GeneratedSystem};

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
}

impl Default for PersonaeConfig {
    fn default() -> Self {
        Self {
            min_world_dominance: default_min_dominance(),
            max_per_world: default_per_world(),
            max_per_system: default_per_system(),
            kinds: BTreeMap::new(),
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DominanceTier {
    Presence,
    Influence,
    Contested,
    Controlled,
    Stronghold,
}

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
    pub id: String,
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
pub enum PersonaAnchor {
    System {
        system_id: crate::ids::SystemId,
        slot: SystemSlot,
    },
    World {
        system_id: crate::ids::SystemId,
        world_id: crate::ids::WorldId,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemSlot {
    Sovereign,
    OrbitalController,
    EconomicHegemon,
    HiddenMaster,
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> PersonaeReport {
    derive_with(sector, &PersonaeConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &PersonaeConfig) -> PersonaeReport {
    let faction_kind: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.kind.as_ref()))
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
        for world in sector.get_worlds_for_system(sys) {
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

struct PersonaParams<'a> {
    sector: &'a GeneratedSector,
    cfg: &'a PersonaeConfig,
    kind: &'a str,
    faction_id: &'a str,
    anchor: PersonaAnchor,
    sys: &'a GeneratedSystem,
    world: Option<&'a crate::sector_model::GeneratedWorld>,
}

fn build_persona(
    params: PersonaParams,
    used: &mut BTreeSet<String>,
) -> Persona {
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
    };
    let mut rng = stage_rng(
        &sector.seed,
        "personae",
        &format!("{faction_id}:{anchor_disc}"),
    );

    let pools = resolve_pools(cfg, kind);

    let name = generate_name(&mut rng, &pools, used);
    let title = pick(&pools.titles, &mut rng).unwrap_or_else(|| default_title(kind));
    let traits = pick_traits(&pools.traits, &mut rng, world);
    let agenda = build_agenda(kind, faction_id, &anchor, world, sys);

    let pid = format!("persona-{}-{}", faction_id, anchor_id(&anchor));
    Persona {
        id: pid,
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
    }
}

fn generate_name(rng: &mut impl Rng, pools: &KindPools, used: &mut BTreeSet<String>) -> String {
    // Prefer single names, then prefix+root+suffix, then a numeral fallback.
    if !pools.single_names.is_empty() {
        if let Some(s) = pick_unique(&pools.single_names, rng, used) {
            return s;
        }
    }
    if !pools.name_roots.is_empty() || !pools.name_prefixes.is_empty() {
        for _ in 0..8 {
            let pre = pick(&pools.name_prefixes, rng).unwrap_or_default();
            let root = pick(&pools.name_roots, rng).unwrap_or_default();
            let suf = pick(&pools.name_suffixes, rng).unwrap_or_default();
            let parts: Vec<&str> = [pre.as_str(), root.as_str(), suf.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            if parts.is_empty() {
                break;
            }
            let candidate = parts.join(" ");
            if used.insert(candidate.clone()) {
                return candidate;
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
            .map(|s| s.to_ascii_lowercase())
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
        PersonaAnchor::System { .. } => sys.name.clone(),
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
    match kind.to_ascii_lowercase().as_str() {
        "imperial" => "enforce Imperial writ",
        "mechanicus" => "expand the cogitator covenant",
        "ecclesiarchy" => "extend the dominion of the Faith",
        "inquisition" | "inquisitorial" => "hunt heresy in shadow",
        "rogue_trader" | "merchant" | "mercantile" => "press a commercial advantage",
        "chaos" | "heretic" | "renegade" => "spread the touch of the Dark Gods",
        "rebel" | "separatist" => "break Imperial chains",
        "xenos" => "press alien interests",
        "necron" => "reclaim ancient dominion",
        "tyranid" => "feed the Hive",
        "ork" | "orks" => "spread the Waaagh!",
        "tau" | "t'au" => "expand the Greater Good",
        "aeldari" | "eldar" => "thread the Skein toward victory",
        "drukhari" => "harvest fresh suffering",
        "harlequin" => "play out the Black Library",
        "genestealer" | "gsc" => "incubate the awakening",
        _ => "secure faction interests",
    }
}

fn default_title(kind: &str) -> String {
    match kind.to_ascii_lowercase().as_str() {
        "imperial" => "Planetary Governor".into(),
        "mechanicus" => "Magos Dominus".into(),
        "ecclesiarchy" => "Cardinal".into(),
        "inquisition" | "inquisitorial" => "Inquisitor".into(),
        "rogue_trader" | "merchant" | "mercantile" => "Lord-Captain".into(),
        "chaos" | "heretic" | "renegade" => "Champion of Ruin".into(),
        "rebel" | "separatist" => "Rebel Commander".into(),
        "necron" => "Overlord".into(),
        "tyranid" => "Synapse Beast".into(),
        "ork" | "orks" => "Warboss".into(),
        "tau" | "t'au" => "Ethereal".into(),
        "aeldari" | "eldar" => "Farseer".into(),
        "drukhari" => "Dracon".into(),
        "harlequin" => "Shadowseer".into(),
        "genestealer" | "gsc" => "Cult Patriarch".into(),
        _ => "Faction Lead".into(),
    }
}

// ── Built-in pools ─────────────────────────────────────────────────────────────

fn resolve_pools(cfg: &PersonaeConfig, kind: &str) -> KindPools {
    if let Some(p) = cfg.kinds.get(kind) {
        return merge_with_defaults(p, kind);
    }
    default_pool(kind)
}

fn merge_with_defaults(user: &KindPools, kind: &str) -> KindPools {
    let base = default_pool(kind);
    KindPools {
        name_prefixes: if user.name_prefixes.is_empty() {
            base.name_prefixes
        } else {
            user.name_prefixes.clone()
        },
        name_roots: if user.name_roots.is_empty() {
            base.name_roots
        } else {
            user.name_roots.clone()
        },
        name_suffixes: if user.name_suffixes.is_empty() {
            base.name_suffixes
        } else {
            user.name_suffixes.clone()
        },
        single_names: if user.single_names.is_empty() {
            base.single_names
        } else {
            user.single_names.clone()
        },
        titles: if user.titles.is_empty() {
            base.titles
        } else {
            user.titles.clone()
        },
        traits: if user.traits.is_empty() {
            base.traits
        } else {
            user.traits.clone()
        },
    }
}

fn default_pool(kind: &str) -> KindPools {
    match kind.to_ascii_lowercase().as_str() {
        "imperial" => KindPools {
            name_prefixes: svec(&["Lord", "Lady", "High"]),
            name_roots: svec(&[
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
            ]),
            name_suffixes: svec(&["Vex", "Karn", "Voll", "Halen", "Marsden", "Drachma"]),
            single_names: vec![],
            titles: svec(&[
                "Planetary Governor",
                "Lord Militant",
                "Sector Lord",
                "Subsector Praetor",
                "High Steward",
            ]),
            traits: svec(&[
                "Iron-Fisted",
                "Paranoid",
                "Pious",
                "Diplomatic",
                "Severe",
                "Pragmatic",
                "Indolent",
            ]),
        },
        "mechanicus" => KindPools {
            name_prefixes: svec(&["Magos", "Genetor", "Artisan", "Logis"]),
            name_roots: svec(&[
                "Cryptus", "Theta", "Omikron", "Vex", "Volta", "Caliban", "Hyperion", "Strados",
            ]),
            name_suffixes: svec(&["-4", "-VII", "-XXI", "Prime", "Secundus", "Mu", "Tertius"]),
            single_names: vec![],
            titles: svec(&[
                "Magos Dominus",
                "Forge Lord",
                "Archmagos Veneratus",
                "Logis Strategos",
            ]),
            traits: svec(&[
                "Logic-Bound",
                "Tech-Heretic-Curious",
                "Schematic",
                "Hoarding",
                "Detached",
                "Inquisitive",
            ]),
        },
        "ecclesiarchy" => KindPools {
            name_prefixes: svec(&["Father", "Mother", "Cardinal", "Confessor", "Deacon"]),
            name_roots: svec(&[
                "Ambrose",
                "Cassian",
                "Theonia",
                "Vespera",
                "Severus",
                "Penance",
                "Sabbatine",
            ]),
            name_suffixes: svec(&["of the Sword", "the Pure", "the Stern", "the Anointed"]),
            single_names: vec![],
            titles: svec(&[
                "Cardinal",
                "Arch-Confessor",
                "Sister Superior",
                "Missionary Vexilla",
                "Episcopa",
            ]),
            traits: svec(&[
                "Zealous",
                "Mortified",
                "Eloquent",
                "Iron-Faithed",
                "Ascetic",
                "Vengeful",
            ]),
        },
        "inquisition" | "inquisitorial" => KindPools {
            name_prefixes: svec(&["Inquisitor", "Interrogator"]),
            name_roots: svec(&[
                "Eisenhorn",
                "Ravenor",
                "Karamanzov",
                "Quixos",
                "Sand",
                "Greyfax",
                "Drazyari",
                "Voll",
            ]),
            name_suffixes: svec(&["", "-Vekt", "-Kael", "-Mordant"]),
            single_names: vec![],
            titles: svec(&[
                "Inquisitor (Ordo Malleus)",
                "Inquisitor (Ordo Hereticus)",
                "Inquisitor (Ordo Xenos)",
                "Lord Inquisitor",
            ]),
            traits: svec(&[
                "Paranoid",
                "Radical",
                "Puritan",
                "Calculating",
                "Cold",
                "Ruthless",
                "Patient",
            ]),
        },
        "rogue_trader" | "merchant" | "mercantile" => KindPools {
            name_prefixes: svec(&["Lord-Captain", "Captain", "Dame", "Dynast"]),
            name_roots: svec(&[
                "Vrede",
                "Halcyon",
                "Sigismund",
                "Karnak",
                "Verrin",
                "Calderra",
                "Surak",
                "Tessera",
            ]),
            name_suffixes: svec(&["Caligari", "Voss", "Drask", "Hallow", "Tannhauser"]),
            single_names: vec![],
            titles: svec(&[
                "Rogue Trader",
                "Free Captain",
                "Dynast Primus",
                "Lord of the Charter",
            ]),
            traits: svec(&[
                "Acquisitive",
                "Charming",
                "Indebted",
                "Bold",
                "Calculating",
                "Glittering",
            ]),
        },
        "chaos" | "heretic" | "renegade" => KindPools {
            name_prefixes: svec(&["Lord", "Champion", "Sorcerer"]),
            name_roots: svec(&[
                "Vraxis",
                "Mortarion",
                "Khar'kos",
                "Tzeneth",
                "Slathar",
                "Bel'kor",
                "Khel'gar",
                "Maleth",
            ]),
            name_suffixes: svec(&[
                "the Damned",
                "the Eight-Eyed",
                "of the Black Tongue",
                "the Defiler",
            ]),
            single_names: vec![],
            titles: svec(&[
                "Chaos Lord",
                "Daemon Prince",
                "Dark Apostle",
                "Sorcerer-Captain",
            ]),
            traits: svec(&[
                "Vainglorious",
                "Tainted",
                "Cunning",
                "Wrathful",
                "Patient",
                "Mad",
            ]),
        },
        "rebel" | "separatist" => KindPools {
            name_prefixes: svec(&["Commander", "Captain", "Speaker", "Headsman"]),
            name_roots: svec(&[
                "Toran", "Vada", "Selka", "Kosh", "Mira", "Petros", "Yarrick", "Aren",
            ]),
            name_suffixes: svec(&["", "Lien", "Vesh", "Karn"]),
            single_names: vec![],
            titles: svec(&[
                "Rebel Commander",
                "Council Speaker",
                "Revolutionary Marshal",
                "Demagogue",
            ]),
            traits: svec(&[
                "Charismatic",
                "Embittered",
                "Idealistic",
                "Ruthless",
                "Hounded",
            ]),
        },
        "necron" => KindPools {
            name_prefixes: svec(&["Overlord", "Phaeron", "Cryptek"]),
            name_roots: svec(&[
                "Sahmek",
                "Trazyn",
                "Imotekh",
                "Kutlakh",
                "Anrakyr",
                "Khaybar",
                "Zahndrekh",
            ]),
            name_suffixes: svec(&["", "of the Black Pyramid", "the Silent"]),
            single_names: vec![],
            titles: svec(&[
                "Necron Overlord",
                "Phaeron",
                "Cryptek Plasmancer",
                "Lord of the Tomb",
            ]),
            traits: svec(&["Implacable", "Patient", "Disdainful", "Curious", "Vengeful"]),
        },
        "tyranid" => KindPools {
            name_prefixes: svec(&[]),
            name_roots: svec(&["Hive Tyrant", "Norn-Queen Echo", "Synapse Node", "Tendril"]),
            name_suffixes: svec(&["Alpha", "Beta", "Gamma", "Delta", "Epsilon"]),
            single_names: vec![],
            titles: svec(&["Hive Tyrant", "Synapse Beast", "Norn-Queen Echo"]),
            traits: svec(&["Hungry", "Adaptive", "Patient", "Implacable"]),
        },
        "ork" | "orks" => KindPools {
            name_prefixes: svec(&[]),
            name_roots: svec(&[
                "Skarsnik",
                "Grimskull",
                "Bonebreaka",
                "Throgg",
                "Garglug",
                "Skullkrump",
                "Urghuz",
            ]),
            name_suffixes: svec(&["Big-Mek", "Da Brutal", "Wartoof", "Da Boss", "Krumpa"]),
            single_names: vec![],
            titles: svec(&["Warboss", "Big Mek", "Painboy", "Nob Boss"]),
            traits: svec(&[
                "Brutal",
                "Cunning",
                "Loud",
                "Inventive",
                "Massive",
                "Reckless",
            ]),
        },
        "tau" | "t'au" => KindPools {
            name_prefixes: svec(&["Aun'", "Shas'", "Por'"]),
            name_roots: svec(&["vre", "ui", "el", "o", "la", "su"]),
            name_suffixes: svec(&["Mont'yr", "Tash'var", "Sa'cea", "Bork'an", "Vior'la"]),
            single_names: vec![],
            titles: svec(&[
                "Ethereal",
                "Shas'O Commander",
                "Por'el Diplomat",
                "Aun'vre Aide",
            ]),
            traits: svec(&[
                "Patient",
                "Idealistic",
                "Disciplined",
                "Inquisitive",
                "Naive",
            ]),
        },
        "aeldari" | "eldar" => KindPools {
            name_prefixes: svec(&["Farseer", "Autarch", "Spiritseer"]),
            name_roots: svec(&["Eldrad", "Asurmen", "Macha", "Yvraine", "Yriel", "Saerith"]),
            name_suffixes: svec(&["", "the Cold", "Lin'doril"]),
            single_names: vec![],
            titles: svec(&["Farseer", "Autarch", "Warlock", "Spiritseer"]),
            traits: svec(&["Cold", "Cryptic", "Patient", "Haughty", "Foreboding"]),
        },
        "drukhari" => KindPools {
            name_prefixes: svec(&["Archon", "Dracon", "Succubus"]),
            name_roots: svec(&["Vect", "Yllith", "Sliscus", "Aestred", "Drazhar"]),
            name_suffixes: svec(&["the Cruel", "Pain-Touched"]),
            single_names: vec![],
            titles: svec(&["Archon", "Dracon", "Succubus", "Haemonculus"]),
            traits: svec(&["Cruel", "Decadent", "Cunning", "Vindictive", "Aesthete"]),
        },
        "harlequin" => KindPools {
            name_prefixes: svec(&["Troupe Master", "Shadowseer"]),
            name_roots: svec(&["Sylandri", "Veilwalker", "Death-Mask"]),
            name_suffixes: svec(&["", "the Laughing", "of the Crone"]),
            single_names: vec![],
            titles: svec(&["Troupe Master", "Shadowseer", "Death Jester"]),
            traits: svec(&["Theatrical", "Cryptic", "Lethal"]),
        },
        "genestealer" | "gsc" => KindPools {
            name_prefixes: svec(&["Patriarch", "Magus", "Primus"]),
            name_roots: svec(&["Saint", "First", "Whisper", "Old", "Quiet"]),
            name_suffixes: svec(&["Hraxon", "Velt", "Marek", "Selas"]),
            single_names: vec![],
            titles: svec(&["Cult Patriarch", "Cult Magus", "Cult Primus"]),
            traits: svec(&["Patient", "Soothing", "Insidious", "Familial", "Concealed"]),
        },
        "xenos" => KindPools {
            name_prefixes: svec(&[]),
            name_roots: svec(&["Vesh'Ka", "Mor'tek", "Zhuun", "Pak'thar", "Ssuln"]),
            name_suffixes: svec(&["", "the Many-Eyed", "of the Deep"]),
            single_names: vec![],
            titles: svec(&["Warleader", "Hivespeaker", "High Voice"]),
            traits: svec(&["Inscrutable", "Hostile", "Cunning"]),
        },
        _ => KindPools {
            name_prefixes: svec(&[]),
            name_roots: svec(&["Anonymous", "Unknown", "Faceless"]),
            name_suffixes: svec(&["One", "Two", "Three"]),
            single_names: vec![],
            titles: svec(&["Faction Lead", "Senior Operative"]),
            traits: svec(&["Discreet", "Stoic"]),
        },
    }
}

fn svec(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

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
            star: GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![GeneratedWorld {
                id: "wrld-0001-1".into(),
                index: 1,
                name: "Alpha Prime".into(),
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
                regions: vec![].into(),
                conflict: Default::default(),
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

        let a = derive(&sec);
        let b = derive(&sec);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert!(!a.personae.is_empty());
        assert!(a.personae.iter().any(|p| p.faction_id == "imp"));
    }
}
