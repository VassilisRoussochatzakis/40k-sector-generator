//! Loads a project directory from disk: sectorforge.toml + all referenced inputs.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::config::AppConfig;
use crate::errors::SectorError;
use crate::factions::{FactionDef, FactionsFile};
use crate::names::NameTables;
use crate::rng;
use crate::routes::{RouteRules, RouteRulesFile};

#[derive(Debug, Clone)]
pub struct ProjectInput {
    pub root_dir: Utf8PathBuf,
    pub config: AppConfig,
    pub world_tables: crate::worlds::KeyTables,
    pub world_rows: Vec<crate::worlds::GenerationRow>,
    /// §45 WD3: structured feature pool authored in `worlds.toml`, when
    /// present. Callers building a `WorldCandidatePool` should overlay
    /// this via `world_pool::apply_authored_features`.
    pub authored_features: Option<crate::worlds_toml::ResolvedFeaturePool>,
    pub names: NameTables,
    pub factions: Vec<FactionDef>,
    pub route_rules: RouteRules,
    /// §5 NEW2.md: parsed `relations.toml`, default empty when `inputs.relations`
    /// is unset.
    pub relations: crate::relations::RelationsConfig,
    /// §5 NEW.md: parsed `regions.toml`, default disabled when unset.
    pub regions: crate::regions::RegionsConfig,
    /// §12 NEW.md: parsed `economy.toml`, default disabled when unset.
    pub economy: crate::economy::EconomyConfig,
    /// §1 NEW2.md: parsed chronicle config, default eras/rules when unset.
    pub history: crate::history::HistoryConfig,
    /// Project-relative path -> "blake3:<hex>" digest of input file bytes.
    pub input_digests: BTreeMap<String, String>,
}

pub fn load_project(project_dir: &Utf8Path) -> Result<ProjectInput, SectorError> {
    let root_dir = project_dir.to_path_buf();
    let config_path = root_dir.join("sectorforge.toml");

    let config_text =
        fs::read_to_string(&config_path).map_err(|e| SectorError::io(config_path.as_str(), e))?;
    let mut config: AppConfig = toml::from_str(&config_text)
        .map_err(|e| SectorError::config_parse(config_path.as_str(), e.to_string()))?;

    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    digests.insert("sectorforge.toml".to_string(), blake3_of(&config_text));

    if let Some(theme) = config.map_theme.take() {
        config.outputs.bitmap.theme = config.outputs.bitmap.theme.clone().overlay(theme);
    }
    if let Some(rel) = config.outputs.bitmap.theme_file.clone() {
        let text = read_relative(&root_dir, &rel, &mut digests)?;
        let theme = crate::map_theme::parse_map_theme_file(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        config.outputs.bitmap.theme = config.outputs.bitmap.theme.clone().overlay(theme);
    }

    let data_dir_rel = config.inputs.world_data_dir.clone();
    let data_dir = root_dir.join(&data_dir_rel);
    let trimmed_rel = data_dir_rel.trim_end_matches('/');
    let toml_rel = format!("{}/{}", trimmed_rel, crate::worlds_toml::DEFAULT_FILENAME);
    let toml_path = data_dir.join(crate::worlds_toml::DEFAULT_FILENAME);
    if toml_path.exists() {
        // Native TOML path: a single digestable artifact.
        let bytes = fs::read(&toml_path).map_err(|e| SectorError::io(toml_path.as_str(), e))?;
        digests.insert(toml_rel, blake3_of_bytes(&bytes));
    } else {
        // Legacy CSV path: digest both files (key.csv optional).
        let key_rel = format!("{}/key.csv", trimmed_rel);
        let gen_rel = format!("{}/generator.csv", trimmed_rel);
        let key_path = data_dir.join("key.csv");
        let gen_path = data_dir.join("generator.csv");
        if key_path.exists() {
            let key_bytes =
                fs::read(&key_path).map_err(|e| SectorError::io(key_path.as_str(), e))?;
            digests.insert(key_rel, blake3_of_bytes(&key_bytes));
        }
        let gen_bytes = fs::read(&gen_path).map_err(|e| SectorError::io(gen_path.as_str(), e))?;
        digests.insert(gen_rel, blake3_of_bytes(&gen_bytes));
    }

    let worlds_load = crate::worlds::load_worlds_data(data_dir.as_std_path()).map_err(|e| {
        SectorError::WorldDataLoad {
            path: data_dir.to_string(),
            message: e.to_string(),
        }
    })?;
    let crate::worlds::WorldsLoad {
        tables: world_tables,
        rows: world_rows,
        authored_features,
    } = worlds_load;

    let mut names = NameTables::default();
    if let Some(rel) = &config.inputs.system_names {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let table: NameTables = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        merge_name_tables(&mut names, table);
    }
    if let Some(rel) = &config.inputs.world_names {
        // Already loaded if system_names points to the same file.
        if Some(rel) != config.inputs.system_names.as_ref() {
            let text = read_relative(&root_dir, rel, &mut digests)?;
            let table: NameTables = toml::from_str(&text)
                .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
            merge_name_tables(&mut names, table);
        }
    }

    let factions = if let Some(rel) = &config.inputs.factions {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: FactionsFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.factions
    } else {
        Vec::new()
    };

    let route_rules = if let Some(rel) = &config.inputs.route_rules {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: RouteRulesFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.routes
    } else {
        RouteRules::default()
    };

    if let Some(rel) = &config.inputs.generation_profiles {
        // Profiles parsing intentionally minimal: digest only, content reserved for future use.
        let _ = read_relative(&root_dir, rel, &mut digests)?;
    }

    let relations = if let Some(rel) = &config.inputs.relations {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: crate::relations::RelationsFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.relations
    } else {
        crate::relations::RelationsConfig::default()
    };

    let regions = if let Some(rel) = &config.inputs.regions {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: crate::regions::RegionsFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.regions
    } else {
        crate::regions::RegionsConfig::default()
    };

    let economy = if let Some(rel) = &config.inputs.economy {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: crate::economy::EconomyFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.into_config()
    } else {
        crate::economy::EconomyConfig::default()
    };

    let history = if let Some(rel) = &config.inputs.history {
        let text = read_relative(&root_dir, rel, &mut digests)?;
        let parsed: crate::history::HistoryFile = toml::from_str(&text)
            .map_err(|e| SectorError::config_parse(rel.clone(), e.to_string()))?;
        parsed.history
    } else {
        config.history.clone()
    };

    Ok(ProjectInput {
        root_dir,
        config,
        world_tables,
        world_rows,
        authored_features,
        names,
        factions,
        route_rules,
        relations,
        regions,
        economy,
        history,
        input_digests: digests,
    })
}

fn read_relative(
    root: &Utf8Path,
    rel: &str,
    digests: &mut BTreeMap<String, String>,
) -> Result<String, SectorError> {
    let abs = root.join(rel);
    let text = fs::read_to_string(&abs).map_err(|e| SectorError::io(abs.as_str(), e))?;
    digests.insert(rel.to_string(), blake3_of(&text));
    Ok(text)
}

fn blake3_of(text: &str) -> String {
    blake3_of_bytes(text.as_bytes())
}

fn blake3_of_bytes(bytes: &[u8]) -> String {
    let h = blake3::hash(bytes);
    format!("blake3:{}", rng::hex(h.as_bytes()))
}

fn merge_name_tables(target: &mut NameTables, source: NameTables) {
    if !source.system_names.prefixes.is_empty() {
        target.system_names.prefixes = source.system_names.prefixes;
    }
    if !source.system_names.suffixes.is_empty() {
        target.system_names.suffixes = source.system_names.suffixes;
    }
    if !source.system_names.single_names.is_empty() {
        target.system_names.single_names = source.system_names.single_names;
    }
    target.location_names = source.location_names;
    if !source.world_names.prefixes.is_empty() {
        target.world_names.prefixes = source.world_names.prefixes;
    }
    if !source.world_names.roots.is_empty() {
        target.world_names.roots = source.world_names.roots;
    }
    if !source.world_names.suffixes.is_empty() {
        target.world_names.suffixes = source.world_names.suffixes;
    }
}
