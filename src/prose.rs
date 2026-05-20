//! Narrative prose / gazetteer generator (§6 NEW.md).
//!
//! Deterministic template grammar — not an LLM — that produces in-universe
//! prose paragraphs for the sector overview and each system. Variation
//! comes from seeded selection among synonym lists keyed by id, so two
//! systems read differently while the same system always reads the same.
//!
//! Templates are strictly data-bound. Slots that point at missing model
//! fields render as empty fragments rather than placeholder text, so prose
//! can never claim a fact the JSON doesn't carry.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::Utf8Path;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::rng::stage_rng;
use crate::sector_model::{GeneratedSector, GeneratedSystem, SystemState};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProseConfig {
    /// Tone preset: "gazetteer" (florid) or "dispatch" (terse Administratum).
    #[serde(default = "default_tone")]
    pub tone: ProseTone,
    /// Include sector-wide overview paragraph.
    #[serde(default = "default_true")]
    pub include_overview: bool,
    /// Include per-system gazetteer entries.
    #[serde(default = "default_true")]
    pub include_per_system: bool,
}

impl Default for ProseConfig {
    fn default() -> Self {
        Self {
            tone: default_tone(),
            include_overview: true,
            include_per_system: true,
        }
    }
}

fn default_tone() -> ProseTone {
    ProseTone::Gazetteer
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProseTone {
    Gazetteer,
    Dispatch,
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProseReport {
    pub sector_id: String,
    pub seed: String,
    pub tone: String,
    pub overview: String,
    pub system_entries: Vec<SystemProse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProse {
    pub system_id: String,
    pub name: String,
    pub paragraphs: Vec<String>,
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> ProseReport {
    derive_with(sector, &ProseConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &ProseConfig) -> ProseReport {
    let faction_names: BTreeMap<&str, &str> = sector
        .factions
        .iter()
        .map(|f| (f.id.as_str(), f.name.as_str()))
        .collect();
    let overview = if cfg.include_overview {
        sector_overview(sector, cfg, &faction_names)
    } else {
        String::new()
    };

    let mut entries: Vec<SystemProse> = Vec::new();
    if cfg.include_per_system {
        for sys in &sector.systems {
            entries.push(system_prose(sys, sector, cfg, &faction_names));
        }
    }

    ProseReport {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        tone: format!("{:?}", cfg.tone),
        overview,
        system_entries: entries,
    }
}

fn sector_overview(
    sector: &GeneratedSector,
    cfg: &ProseConfig,
    faction_names: &BTreeMap<&str, &str>,
) -> String {
    let mut rng = stage_rng(&sector.seed, "prose", "sector");
    let n_sys = sector.systems.len();
    let n_worlds: usize = sector.systems.iter().map(|s| s.worlds.len()).sum();
    let n_factions = sector.factions.len();
    let top_faction = sector
        .factions
        .iter()
        .max_by(|a, b| {
            a.power
                .total_projection()
                .partial_cmp(&b.power.total_projection())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|f| {
            faction_names
                .get(f.id.as_str())
                .copied()
                .unwrap_or(f.name.as_str())
        });

    let opener_pool: &[&str] = match cfg.tone {
        ProseTone::Gazetteer => &[
            "The {sector} extends across",
            "Charts of the {sector} record",
            "Mariners speak of the {sector} as",
        ],
        ProseTone::Dispatch => &["REPORT: {sector} —", "STATUS, {sector}:", "MEMO, {sector}:"],
    };
    let opener = opener_pool
        .choose(&mut rng)
        .copied()
        .unwrap_or("The {sector}")
        .replace("{sector}", &sector.title);

    let mut s = String::new();
    let _ = write!(s, "{opener} ");
    let _ = write!(
        s,
        "{n_sys} systems and {n_worlds} worlds, contested by {n_factions} known powers."
    );
    if let Some(top) = top_faction {
        let _ = write!(
            s,
            " The dominant projection belongs to {top}, though the balance shifts year to year."
        );
    }
    s
}

fn system_prose(
    sys: &GeneratedSystem,
    sector: &GeneratedSector,
    cfg: &ProseConfig,
    faction_names: &BTreeMap<&str, &str>,
) -> SystemProse {
    let mut rng = stage_rng(&sector.seed, "prose", &sys.id);

    let mut paragraphs: Vec<String> = Vec::new();

    // Paragraph 1: geographic / population framing.
    let n_worlds = sys.worlds.len();
    let pop_words: &[&str] = match cfg.tone {
        ProseTone::Gazetteer => &["holding", "supporting", "burdened with", "graced with"],
        ProseTone::Dispatch => &["count", "rated", "logged at"],
    };
    let pop_word = pop_words.choose(&mut rng).copied().unwrap_or("supporting");
    let star = &sys.star.colour_name;
    let p1 = match cfg.tone {
        ProseTone::Gazetteer => format!(
            "{name} circles a {star_lc} star, {pop_word} {n} world{plural}.",
            name = sys.name,
            star_lc = star.to_ascii_lowercase(),
            pop_word = pop_word,
            n = n_worlds,
            plural = if n_worlds == 1 { "" } else { "s" },
        ),
        ProseTone::Dispatch => format!(
            "{name}: {star} primary; {n} world{plural} on registry.",
            name = sys.name,
            star = star,
            n = n_worlds,
            plural = if n_worlds == 1 { "" } else { "s" },
        ),
    };
    paragraphs.push(p1);

    // Paragraph 2: political framing.
    if let Some(dom_id) = &sys.control.dominant {
        let dom_name = faction_names
            .get(dom_id.as_str())
            .copied()
            .unwrap_or(dom_id.as_str());
        let state_text = match sys.control.state {
            Some(SystemState::Pacified) => "Pacified, by present register.",
            Some(SystemState::Fragmented) => "Authority within the system is fragmented.",
            Some(SystemState::Blockaded) => "A void blockade strangles the approaches.",
            Some(SystemState::Warzone) => "The system burns as an open warzone.",
            Some(SystemState::Infiltrated) => "Inquisitorial probes report deep infiltration.",
            Some(SystemState::Quarantined) => "The system stands under interdict.",
            Some(SystemState::Uncharted) => "Cartography remains incomplete; transit at risk.",
            None => "",
        };
        let intro_pool: &[&str] = match cfg.tone {
            ProseTone::Gazetteer => &[
                "Control rests with",
                "Dominion is held by",
                "The seat of power belongs to",
            ],
            ProseTone::Dispatch => &["DOMINANT:", "PRIMARY POWER:", "HEGEMON:"],
        };
        let intro = intro_pool
            .choose(&mut rng)
            .copied()
            .unwrap_or("Control rests with");
        let mut p2 = format!("{intro} {dom_name}.");
        if !state_text.is_empty() {
            p2.push(' ');
            p2.push_str(state_text);
        }
        paragraphs.push(p2);
    }

    // Paragraph 3: archetype + conflict colour.
    let mut colour: Vec<String> = Vec::new();
    let a = &sys.archetype;
    if a.chaos_corruption >= 30 {
        colour.push(format!(
            "Chaos corruption is logged at {}.",
            a.chaos_corruption
        ));
    }
    if a.necron_phase != crate::archetypes::NecronPhase::default() {
        colour.push(format!("Necron phase: {:?}.", a.necron_phase));
    }
    if a.tyranid_stage != crate::archetypes::TyranidStage::default() {
        colour.push(format!("Tyranid pressure: {:?}.", a.tyranid_stage));
    }
    if a.ork_waaagh >= 30 {
        colour.push(format!("Waaagh! intensity {}.", a.ork_waaagh));
    }
    if a.gsc_stage != crate::archetypes::GscStage::default() {
        colour.push(format!("Cultist stage: {:?}.", a.gsc_stage));
    }
    if sys.conflict.intensity >= 30 {
        colour.push(format!(
            "Active conflict intensity {}.",
            sys.conflict.intensity
        ));
    }
    if !colour.is_empty() {
        paragraphs.push(colour.join(" "));
    }

    SystemProse {
        system_id: sys.id.clone(),
        name: sys.name.clone(),
        paragraphs,
    }
}

// ── Markdown ───────────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &ProseReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Sector Gazetteer — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`  ·  Tone: {}", report.seed, report.tone);
    if !report.overview.is_empty() {
        let _ = writeln!(s, "\n## Overview");
        let _ = writeln!(s, "\n{}", report.overview);
    }
    if !report.system_entries.is_empty() {
        let _ = writeln!(s, "\n## Systems");
        for e in &report.system_entries {
            let _ = writeln!(s, "\n### {} — {}", e.system_id, e.name);
            for p in &e.paragraphs {
                let _ = writeln!(s, "\n{p}");
            }
        }
    }
    s
}

/// Write `gazetteer.md` + `gazetteer.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(output_dir: &Utf8Path, report: &ProseReport) -> Result<(), SectorError> {
    crate::export::write_md_and_json(output_dir, "gazetteer", &render_markdown(report), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GenerationManifest, HexCoord, SystemControlSummary,
    };
    use std::collections::BTreeMap as Map;

    fn empty_sector() -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "The Test Sector".into(),
            seed: "prose-seed".into(),
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
            regions: Vec::new(),
            economy: Default::default(),
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
    fn deterministic_prose() {
        let mut sec = empty_sector();
        sec.systems.push(system("sys-0001"));
        let a = derive(&sec);
        let b = derive(&sec);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert!(!a.system_entries.is_empty());
        assert!(!a.overview.is_empty());
    }
}
