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

#[derive(Debug)]
pub struct ProjectInput {
    pub root_dir: Utf8PathBuf,
    pub config: AppConfig,
    pub world_tables: crate::worlds::KeyTables,
    pub world_rows: Vec<crate::worlds::GenerationRow>,
    pub names: NameTables,
    pub factions: Vec<FactionDef>,
    pub route_rules: RouteRules,
    /// Project-relative path -> "blake3:<hex>" digest of input file bytes.
    pub input_digests: BTreeMap<String, String>,
}

pub fn load_project(project_dir: &Utf8Path) -> Result<ProjectInput, SectorError> {
    let root_dir = project_dir.to_path_buf();
    let config_path = root_dir.join("sectorforge.toml");

    let config_text =
        fs::read_to_string(&config_path).map_err(|e| SectorError::io(config_path.as_str(), e))?;
    let config: AppConfig = toml::from_str(&config_text)
        .map_err(|e| SectorError::config_parse(config_path.as_str(), e.to_string()))?;

    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    digests.insert("sectorforge.toml".to_string(), blake3_of(&config_text));

    let workbook_rel = config.inputs.world_workbook.clone();
    let workbook_path = root_dir.join(&workbook_rel);
    let workbook_bytes =
        fs::read(&workbook_path).map_err(|e| SectorError::io(workbook_path.as_str(), e))?;
    digests.insert(workbook_rel.clone(), blake3_of_bytes(&workbook_bytes));

    let (world_tables, world_rows) = crate::worlds::load_generation_rows(workbook_path.as_str())
        .map_err(|message| SectorError::WorldWorkbookLoad {
            path: workbook_path.to_string(),
            message,
        })?;

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

    Ok(ProjectInput {
        root_dir,
        config,
        world_tables,
        world_rows,
        names,
        factions,
        route_rules,
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
