//! Trade & resource economy layer (§12 NEW.md).
//!
//! Pure read-only derivation over the finished sector: each world declares a
//! production/consumption vector (keyed by `world_type` × `tech_level` ×
//! population scale, mapped from the existing `tags`), routes carry derived
//! trade volume = function of endpoint surplus/deficit gradient × distance
//! falloff × `RouteStability` × per-faction `RouteControl` interference.
//! No new RNG draws — same sector ⇒ same numbers.
//!
//! Default production tables ship in this module; users may override or
//! extend them in `economy.toml` (referenced by `inputs.economy` in
//! `sectorforge.toml`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::errors::SectorError;
use crate::sector_model::{GeneratedRoute, GeneratedSector, RouteStability};

// ── Resource categories ────────────────────────────────────────────────────────

pub const RESOURCE_KEYS: &[&str] = &[
    "ore",
    "promethium",
    "foodstuffs",
    "manufactured",
    "archeotech",
    "recruits",
];

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EconomyFile {
    #[serde(default)]
    pub economy: EconomyConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EconomyConfig {
    /// Whether the derivation runs. Defaults to `false` so legacy projects
    /// keep byte-identical sector JSON output.
    #[serde(default)]
    pub enabled: bool,
    /// Whether shortfall worlds nudge `stability.famine_or_resource_stress`
    /// upward. Read-only nudge; conflict tick is not affected.
    #[serde(default)]
    pub feed_stability: bool,
    /// Production/consumption table additions or overrides keyed by world_type.
    #[serde(default)]
    pub by_world_type: BTreeMap<String, ResourceVector>,
    /// Tech-level multipliers applied to the world-type vector.
    #[serde(default)]
    pub by_tech_level: BTreeMap<String, f32>,
    /// Population scale multipliers applied after tech.
    #[serde(default)]
    pub by_population: BTreeMap<String, f32>,
}

impl Default for EconomyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            feed_stability: false,
            by_world_type: BTreeMap::new(),
            by_tech_level: BTreeMap::new(),
            by_population: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ResourceVector {
    #[serde(default)]
    pub ore: f32,
    #[serde(default)]
    pub promethium: f32,
    #[serde(default)]
    pub foodstuffs: f32,
    #[serde(default)]
    pub manufactured: f32,
    #[serde(default)]
    pub archeotech: f32,
    #[serde(default)]
    pub recruits: f32,
}

impl ResourceVector {
    fn get(&self, key: &str) -> f32 {
        match key {
            "ore" => self.ore,
            "promethium" => self.promethium,
            "foodstuffs" => self.foodstuffs,
            "manufactured" => self.manufactured,
            "archeotech" => self.archeotech,
            "recruits" => self.recruits,
            _ => 0.0,
        }
    }
    fn scale(mut self, f: f32) -> Self {
        self.ore *= f;
        self.promethium *= f;
        self.foodstuffs *= f;
        self.manufactured *= f;
        self.archeotech *= f;
        self.recruits *= f;
        self
    }
}

// ── Built-in vectors (positive = surplus, negative = deficit) ──────────────────

fn default_world_type_vector(world_type: &str) -> ResourceVector {
    match world_type {
        "HiveWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -40.0,
            manufactured: 40.0,
            archeotech: 0.0,
            recruits: 60.0,
        },
        "ForgeWorld" => ResourceVector {
            ore: -20.0,
            promethium: 0.0,
            foodstuffs: -20.0,
            manufactured: 80.0,
            archeotech: 10.0,
            recruits: 5.0,
        },
        "AgriWorld" | "AgriculturalWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 80.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 10.0,
        },
        "MiningWorld" => ResourceVector {
            ore: 70.0,
            promethium: 20.0,
            foodstuffs: -10.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 5.0,
        },
        "DeathWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 5.0,
            recruits: 40.0,
        },
        "KnightWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 10.0,
            manufactured: 10.0,
            archeotech: 0.0,
            recruits: 30.0,
        },
        "CivilisedWorld" | "CivilizedWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 10.0,
            manufactured: 15.0,
            archeotech: 0.0,
            recruits: 15.0,
        },
        "FeudalWorld" | "FeralWorld" => ResourceVector {
            ore: 5.0,
            promethium: 0.0,
            foodstuffs: 20.0,
            manufactured: -20.0,
            archeotech: 0.0,
            recruits: 25.0,
        },
        "FortressWorld" => ResourceVector {
            ore: 0.0,
            promethium: 5.0,
            foodstuffs: -10.0,
            manufactured: 5.0,
            archeotech: 0.0,
            recruits: 30.0,
        },
        "Shrine" | "ShrineWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 20.0,
        },
        "PleasureWorld" => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: -10.0,
            manufactured: -10.0,
            archeotech: 0.0,
            recruits: 5.0,
        },
        "DeadWorld" | "WarpLostWorld" | "QuarantinedWorld" | "Uninhabited" => {
            ResourceVector::default()
        }
        _ => ResourceVector {
            ore: 0.0,
            promethium: 0.0,
            foodstuffs: 0.0,
            manufactured: 0.0,
            archeotech: 0.0,
            recruits: 0.0,
        },
    }
}

fn default_tech_multiplier(tech: &str) -> f32 {
    match tech {
        "STC" | "Archeotech" => 1.5,
        "Imperial" => 1.0,
        "Mechanicus" => 1.2,
        "PreImperial" | "Industrial" => 0.7,
        "Renaissance" | "Medieval" | "Iron" => 0.4,
        "Stone" | "Primitive" => 0.2,
        _ => 1.0,
    }
}

fn default_population_multiplier(pop_tag: &str) -> f32 {
    // tag form: "population:massive" etc.
    match pop_tag {
        "population:massive" => 1.5,
        "population:large" | "population:huge" => 1.3,
        "population:standard" => 1.0,
        "population:sole_settlement" | "population:lightly_populated" => 0.5,
        "population:minimal" => 0.25,
        "population:uninhabited" => 0.0,
        _ => 1.0,
    }
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EconomyReport {
    /// True when the derivation ran (config.enabled or explicit call).
    #[serde(default)]
    pub enabled: bool,
    pub worlds: Vec<WorldEconomy>,
    pub systems: Vec<SystemEconomy>,
    pub routes: Vec<RouteEconomy>,
    pub sector_balance: ResourceVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEconomy {
    pub system_id: String,
    pub world_id: String,
    pub vector: ResourceVector,
    /// True when net foodstuffs is negative *and* no inbound route can fix it.
    #[serde(default)]
    pub stranded: bool,
    /// Critical shortages by resource key (those with deficit >= 20).
    #[serde(default)]
    pub shortages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEconomy {
    pub system_id: String,
    pub vector: ResourceVector,
    /// `surplus_resources`/`shortage_resources` for quick UI.
    #[serde(default)]
    pub surplus_resources: Vec<String>,
    #[serde(default)]
    pub shortage_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteEconomy {
    pub route_id: String,
    pub from_system_id: String,
    pub to_system_id: String,
    pub volume: f32,
    /// 0..=1 modifier from hazard tier × piracy/interdiction.
    #[serde(default = "default_one")]
    pub friction: f32,
}

fn default_one() -> f32 {
    1.0
}

// ── Loader ─────────────────────────────────────────────────────────────────────

/// Load `economy.toml`. Missing file → defaults (disabled).
///
/// # Errors
///
/// Returns [`SectorError::ConfigParse`] on malformed TOML and
/// [`SectorError::Io`] on read failure.
pub fn load_economy_file(path: &Utf8Path) -> Result<EconomyConfig, SectorError> {
    if !path.exists() {
        return Ok(EconomyConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| SectorError::io(path.as_str(), e))?;
    let parsed: EconomyFile = toml::from_str(&text)
        .map_err(|e| SectorError::config_parse(path.as_str(), e.to_string()))?;
    Ok(parsed.economy)
}

// ── Derivation ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> EconomyReport {
    derive_with(
        sector,
        &EconomyConfig {
            enabled: true,
            ..Default::default()
        },
    )
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &EconomyConfig) -> EconomyReport {
    if !cfg.enabled {
        return EconomyReport::default();
    }
    let mut worlds: Vec<WorldEconomy> = Vec::new();
    let mut systems: Vec<SystemEconomy> = Vec::new();

    for sys in &sector.systems {
        let mut sys_vec = ResourceVector::default();
        for w in &sys.worlds {
            let pop_tag = w
                .tags
                .iter()
                .find(|t| t.starts_with("population:"))
                .cloned()
                .unwrap_or_default();
            let base = cfg
                .by_world_type
                .get(&w.world.world_type)
                .cloned()
                .unwrap_or_else(|| default_world_type_vector(&w.world.world_type));
            let tech = cfg
                .by_tech_level
                .get(&w.world.tech_level)
                .copied()
                .unwrap_or_else(|| default_tech_multiplier(&w.world.tech_level));
            let pop = cfg
                .by_population
                .get(&pop_tag)
                .copied()
                .unwrap_or_else(|| default_population_multiplier(&pop_tag));
            let vector = base.scale(tech * pop);
            sys_vec = add(&sys_vec, &vector);

            let shortages: Vec<String> = RESOURCE_KEYS
                .iter()
                .filter(|k| vector.get(k) <= -20.0)
                .map(|k| (*k).to_string())
                .collect();

            worlds.push(WorldEconomy {
                system_id: sys.id.clone(),
                world_id: w.id.clone(),
                vector,
                stranded: false, // computed below once routes are processed
                shortages,
            });
        }
        let surplus_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) >= 20.0)
            .map(|k| (*k).to_string())
            .collect();
        let shortage_resources: Vec<String> = RESOURCE_KEYS
            .iter()
            .filter(|k| sys_vec.get(k) <= -20.0)
            .map(|k| (*k).to_string())
            .collect();
        systems.push(SystemEconomy {
            system_id: sys.id.clone(),
            vector: sys_vec,
            surplus_resources,
            shortage_resources,
        });
    }

    // Routes: trade volume = endpoint gradient × friction.
    let by_sys: BTreeMap<&str, &SystemEconomy> =
        systems.iter().map(|s| (s.system_id.as_str(), s)).collect();
    let routes: Vec<RouteEconomy> = sector
        .routes
        .iter()
        .map(|r| route_economy(r, &by_sys))
        .collect();

    // Stranded check: a world is stranded if it has any deficit ≥ 20 and the
    // system also nets a deficit there *and* no inbound route from a surplus
    // system on that resource exists.
    let mut stranded_world_idx: Vec<usize> = Vec::new();
    for (idx, we) in worlds.iter().enumerate() {
        let sys = by_sys.get(we.system_id.as_str()).copied();
        if sys.is_none() {
            continue;
        }
        let sys = sys.unwrap();
        // Resources where the system itself is in deficit.
        let resource_deficits: Vec<&str> = RESOURCE_KEYS
            .iter()
            .copied()
            .filter(|k| sys.vector.get(k) <= -20.0 && we.vector.get(k) <= -20.0)
            .collect();
        if resource_deficits.is_empty() {
            continue;
        }
        let mut fix = false;
        for r in &sector.routes {
            if (r.from_system_id == sys.system_id || r.to_system_id == sys.system_id)
                && r.stability != RouteStability::Perilous
            {
                let other = if r.from_system_id == sys.system_id {
                    r.to_system_id.as_str()
                } else {
                    r.from_system_id.as_str()
                };
                if let Some(other_sys) = by_sys.get(other) {
                    if resource_deficits
                        .iter()
                        .any(|k| other_sys.vector.get(k) >= 20.0)
                    {
                        fix = true;
                        break;
                    }
                }
            }
        }
        if !fix {
            stranded_world_idx.push(idx);
        }
    }
    for i in stranded_world_idx {
        worlds[i].stranded = true;
    }

    // Sector totals.
    let mut sector_balance = ResourceVector::default();
    for sy in &systems {
        sector_balance = add(&sector_balance, &sy.vector);
    }

    EconomyReport {
        enabled: true,
        worlds,
        systems,
        routes,
        sector_balance,
    }
}

fn route_economy(r: &GeneratedRoute, by_sys: &BTreeMap<&str, &SystemEconomy>) -> RouteEconomy {
    let a = by_sys.get(r.from_system_id.as_str()).copied();
    let b = by_sys.get(r.to_system_id.as_str()).copied();
    let gradient: f32 = match (a, b) {
        (Some(a), Some(b)) => {
            RESOURCE_KEYS
                .iter()
                .map(|k| (a.vector.get(k) - b.vector.get(k)).abs())
                .sum::<f32>()
                / RESOURCE_KEYS.len() as f32
        }
        _ => 0.0,
    };
    let friction = friction_for(r);
    let distance_falloff = 1.0 / (r.distance.max(1) as f32);
    RouteEconomy {
        route_id: r.id.clone(),
        from_system_id: r.from_system_id.clone(),
        to_system_id: r.to_system_id.clone(),
        volume: (gradient * friction * distance_falloff).max(0.0),
        friction,
    }
}

fn friction_for(r: &GeneratedRoute) -> f32 {
    let mut f = match r.stability {
        RouteStability::Stable => 1.0,
        RouteStability::Unstable => 0.75,
        RouteStability::Hazardous => 0.45,
        RouteStability::Perilous => 0.10,
    };
    let max_piracy: f32 = r.controls.iter().map(|c| c.piracy).fold(0.0_f32, f32::max);
    let max_interdiction: f32 = r
        .controls
        .iter()
        .map(|c| c.interdiction)
        .fold(0.0_f32, f32::max);
    let max_patrol: f32 = r.controls.iter().map(|c| c.patrol).fold(0.0_f32, f32::max);
    f *= 1.0 - (max_piracy / 200.0).clamp(0.0, 0.5);
    f *= 1.0 - (max_interdiction / 200.0).clamp(0.0, 0.6);
    f *= 1.0 + (max_patrol / 400.0).clamp(0.0, 0.25);
    f.clamp(0.0, 1.5)
}

fn add(a: &ResourceVector, b: &ResourceVector) -> ResourceVector {
    ResourceVector {
        ore: a.ore + b.ore,
        promethium: a.promethium + b.promethium,
        foodstuffs: a.foodstuffs + b.foodstuffs,
        manufactured: a.manufactured + b.manufactured,
        archeotech: a.archeotech + b.archeotech,
        recruits: a.recruits + b.recruits,
    }
}

// ── Markdown ───────────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(sector_id: &str, report: &EconomyReport) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Economy — {sector_id}");
    if !report.enabled {
        let _ = writeln!(
            s,
            "\n_Economy derivation disabled. Enable in `economy.toml`._"
        );
        return s;
    }
    let _ = writeln!(s, "\n## Sector balance");
    let _ = writeln!(s, "| Resource | Net |");
    let _ = writeln!(s, "|----------|-----|");
    for k in RESOURCE_KEYS {
        let _ = writeln!(s, "| {k} | {:.1} |", report.sector_balance.get(k));
    }

    let _ = writeln!(s, "\n## Systems");
    let _ = writeln!(s, "| System | Surplus | Shortage |");
    let _ = writeln!(s, "|--------|---------|----------|");
    for sy in &report.systems {
        let _ = writeln!(
            s,
            "| {} | {} | {} |",
            sy.system_id,
            if sy.surplus_resources.is_empty() {
                "—".to_string()
            } else {
                sy.surplus_resources.join(", ")
            },
            if sy.shortage_resources.is_empty() {
                "—".to_string()
            } else {
                sy.shortage_resources.join(", ")
            },
        );
    }

    let stranded: Vec<&WorldEconomy> = report.worlds.iter().filter(|w| w.stranded).collect();
    if !stranded.is_empty() {
        let _ = writeln!(s, "\n## Stranded worlds");
        for w in stranded {
            let _ = writeln!(
                s,
                "- `{}` in `{}` — shortages: {}",
                w.world_id,
                w.system_id,
                if w.shortages.is_empty() {
                    "(systemic)".into()
                } else {
                    w.shortages.join(", ")
                }
            );
        }
    }

    // Top 10 routes by volume.
    let mut top: Vec<&RouteEconomy> = report.routes.iter().collect();
    top.sort_by(|a, b| {
        b.volume
            .partial_cmp(&a.volume)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let _ = writeln!(s, "\n## Top trade lanes");
    for r in top.iter().take(10) {
        let _ = writeln!(
            s,
            "- {} → {} — volume {:.1} (friction {:.2})",
            r.from_system_id, r.to_system_id, r.volume, r.friction
        );
    }
    s
}

/// Write `economy.md` + `economy.json` (+ `economy.csv` summary) into a dir.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    sector_id: &str,
    report: &EconomyReport,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md = render_markdown(sector_id, report);
    let md_path = output_dir.join("economy.md");
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join("economy.json");
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    let csv_path = output_dir.join("economy.csv");
    fs::write(&csv_path, csv_render(report)).map_err(|e| SectorError::io(csv_path.as_str(), e))?;
    Ok(())
}

fn csv_render(report: &EconomyReport) -> String {
    let mut s = String::new();
    s.push_str(
        "system_id,world_id,ore,promethium,foodstuffs,manufactured,archeotech,recruits,stranded\n",
    );
    for w in &report.worlds {
        let _ = writeln!(
            s,
            "{},{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{}",
            w.system_id,
            w.world_id,
            w.vector.ore,
            w.vector.promethium,
            w.vector.foodstuffs,
            w.vector.manufactured,
            w.vector.archeotech,
            w.vector.recruits,
            w.stranded
        );
    }
    s
}

/// §12 stability nudge: increase `famine_or_resource_stress` on every world
/// that is stranded on foodstuffs. Read-only and bounded; no other stability
/// fields are touched, so the conflict tick does not oscillate.
pub fn apply_stability_nudge(report: &EconomyReport, sector: &mut GeneratedSector) {
    if !report.enabled {
        return;
    }
    let stranded: BTreeMap<&str, bool> = report
        .worlds
        .iter()
        .filter(|w| w.stranded)
        .map(|w| (w.world_id.as_str(), true))
        .collect();
    for sys in sector.systems.iter_mut() {
        for w in sys.worlds.iter_mut() {
            if stranded.contains_key(w.id.as_str()) {
                let nudged = (w.stability.famine_or_resource_stress + 20.0).clamp(0.0, 100.0);
                w.stability.famine_or_resource_stress = nudged;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord,
        SystemControlSummary, WorldControlSummary, WorldDto,
    };
    use std::collections::BTreeMap as Map;

    fn world(id: &str, world_type: &str, tech: &str, pop_tag: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: id.into(),
            index: 1,
            name: id.into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "G".into(),
                star_colour_code: "G".into(),
                world_type: world_type.into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Standard".into(),
                population: "Standard".into(),
                tech_level: tech.into(),
                government: "ImperialCommander".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![pop_tag.to_string()],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
        }
    }

    fn sys(id: &str, worlds: Vec<GeneratedWorld>) -> GeneratedSystem {
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
            worlds,
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

    fn sector(systems: Vec<GeneratedSystem>) -> GeneratedSector {
        GeneratedSector {
            id: "econ-test".into(),
            title: "Econ".into(),
            seed: "seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems,
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
            regions: vec![],
            economy: Default::default(),
        }
    }

    #[test]
    fn disabled_yields_empty() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                "HiveWorld",
                "Imperial",
                "population:massive",
            )],
        )]);
        let r = derive_with(&s, &EconomyConfig::default());
        assert!(!r.enabled);
        assert!(r.worlds.is_empty());
    }

    #[test]
    fn hive_world_food_deficit() {
        let s = sector(vec![sys(
            "sys-0001",
            vec![world(
                "wrld-0001-1",
                "HiveWorld",
                "Imperial",
                "population:massive",
            )],
        )]);
        let r = derive(&s);
        assert!(r.enabled);
        assert!(r.worlds[0].vector.foodstuffs < 0.0);
    }

    #[test]
    fn deterministic() {
        let s = sector(vec![
            sys(
                "sys-0001",
                vec![world("a", "HiveWorld", "Imperial", "population:massive")],
            ),
            sys(
                "sys-0002",
                vec![world("b", "AgriWorld", "Imperial", "population:standard")],
            ),
        ]);
        let a = derive(&s);
        let b = derive(&s);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
