//! Project file I/O for the GUI builder (docs/BUILDER_REQS §P1–§P3).
//!
//! Three responsibilities:
//!
//!  * `new_project`   — scaffold a brand-new project directory on disk
//!    (blank or via a preset overlay) and populate a [`BuilderState`] that
//!    points at it.
//!  * `open_project`  — parse an existing project's `sectorforge.toml` and
//!    every referenced config file via [`sectorforge::input::load_project`],
//!    surfacing parse errors with line numbers in [`BuilderError::ParseFailed`].
//!  * `save_project`  — write every catalog the builder owns back to disk
//!    using atomic `tmp + rename` (no new crate; R9), then update the
//!    sector's manifest digests so the manifest mirrors the on-disk files.
//!
//! Atomic write strategy: a sibling temp file is written next to the target
//! path, then [`std::fs::rename`] swaps it in. On crash the worst outcome is
//! a stray `<file>.tmp.<pid>` that the user can delete; the canonical path
//! is never observed mid-write. This matches the `tempfile` strategy named
//! by §P3 without taking on the dependency that R9 forbids.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};

use sectorforge::config::AppConfig;
use sectorforge::factions::{FactionDef, FactionsFile};
use sectorforge::history::HistoryFile;
use sectorforge::ids::FactionId;
use sectorforge::input::{load_project, ProjectInput};
use sectorforge::regions::RegionsFile;
use sectorforge::relations::RelationsFile;
use sectorforge::routes::RouteRulesFile;
use sectorforge::sector_model::GeneratedSector;

use super::data_catalogs::DataCatalogs;
use super::derivation_cache::{DepClass, DerivationKind};
use super::errors::BuilderError;
use super::index::BuilderIndex;
use super::state::BuilderState;

/// Default `sectorforge.toml` written next to a brand-new blank project.
fn default_app_config(id: &str, title: &str, seed: &str, width: u32, height: u32) -> AppConfig {
    use sectorforge::config::{
        GenerationConfig, InputConfig, OutputConfig, OutputFormat, PlacementConfig, ProjectConfig,
        RelationsGenerationConfig, RouteGenerationConfig, WorldSelectionConfig,
    };
    AppConfig {
        schema_version: None,
        project: ProjectConfig {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            version: Some("0.1.0".to_string()),
        },
        inputs: InputConfig {
            world_data_dir: "data/worlds".to_string(),
            system_names: None,
            world_names: None,
            factions: None,
            route_rules: None,
            generation_profiles: None,
            relations: None,
            regions: None,
            economy: None,
            history: None,
            personae: None,
            sites: None,
            hooks: None,
            missions: None,
            prose: None,
        },
        generation: GenerationConfig {
            seed: seed.to_string(),
            sector_width: width,
            sector_height: height,
            subsector_width: None,
            subsector_height: None,
            system_count: 0,
            min_worlds_per_system: 2,
            max_worlds_per_system: 4,
            allow_empty_hexes: true,
            world_feature_count: 1,
            strict_world_rows: true,
            placement: PlacementConfig::default(),
            world_selection: WorldSelectionConfig::default(),
            routes: RouteGenerationConfig::default(),
            relations: RelationsGenerationConfig::default(),
            search_base_seed: None,
            search_candidate_index: None,
            search_constraints_digest: None,
        },
        outputs: OutputConfig {
            directory: "out".to_string(),
            formats: vec![OutputFormat::Json],
            pretty_json: true,
            write_per_system_files: false,
            write_manifest: true,
            write_diagnostics: false,
            bitmap: Default::default(),
            html: Default::default(),
        },
        map_theme: None,
        analyze: Default::default(),
        search: Default::default(),
        diff: Default::default(),
        history: Default::default(),
    }
}

/// Parameters captured by the §P1 new-project wizard.
#[derive(Debug, Clone)]
pub struct NewProjectOptions {
    pub dest: Utf8PathBuf,
    pub id: String,
    pub title: String,
    pub seed: String,
    pub width: u32,
    pub height: u32,
    /// Optional preset overlay (id under `presets/`). When `None` the wizard
    /// writes a blank project: default `sectorforge.toml` plus
    /// `data/worlds/worlds.toml` seeded from `presets/_base` when reachable
    /// (so the world pool is non-empty), falling back to an empty default if
    /// the `_base` preset is unavailable.
    pub preset: Option<String>,
}

/// §P1: scaffold a new project on disk, then load it into a [`BuilderState`].
///
/// When `preset` is `Some`, delegates to
/// [`sectorforge::presets::scaffold_to_dir`] and optionally rewrites the
/// generation seed. When `preset` is `None`, writes a fresh project tree
/// (`sectorforge.toml` + every catalog) and seeds `data/worlds/worlds.toml`
/// from `presets/_base` when reachable so "Regenerate this system" finds a
/// non-empty world pool on a brand-new project; then loads it back through
/// the standard `open_project` path so the in-memory state is identical to a
/// fresh open.
pub fn new_project(opts: NewProjectOptions) -> Result<BuilderState, BuilderError> {
    let dest = opts.dest.clone();
    if dest.exists() {
        return Err(BuilderError::ParseFailed {
            file: dest.to_string(),
            message: "destination already exists; choose a fresh path".to_string(),
        });
    }

    if let Some(preset_id) = opts.preset.as_deref() {
        sectorforge::presets::scaffold_to_dir(preset_id, &dest, Some(&opts.seed)).map_err(|e| {
            BuilderError::ParseFailed {
                file: dest.to_string(),
                message: e.to_string(),
            }
        })?;
    } else {
        scaffold_blank(&opts)?;
    }

    open_project(&dest)
}

fn scaffold_blank(opts: &NewProjectOptions) -> Result<(), BuilderError> {
    let dest = &opts.dest;
    fs::create_dir_all(Path::new(dest.as_str()))?;

    // Build the config with `[inputs]` already pointing at every catalog the
    // wizard is about to scaffold, so the loader sees a complete project the
    // moment it reopens.
    // Geometry invariant: sectors must be square. Normalize to the larger side
    // so no New-Project entry point (even a stale non-square default) can
    // scaffold a non-square sector — the UI already locks width == height.
    let dim = opts.width.max(opts.height);
    let mut cfg = default_app_config(&opts.id, &opts.title, &opts.seed, dim, dim);
    cfg.inputs.factions = Some("data/factions/factions.toml".to_string());
    cfg.inputs.route_rules = Some("data/routes/route_rules.toml".to_string());
    cfg.inputs.relations = Some("data/factions/relations.toml".to_string());
    cfg.inputs.regions = Some("data/regions/regions.toml".to_string());
    cfg.inputs.economy = Some("data/worlds/economy.toml".to_string());
    cfg.inputs.history = Some("data/history.toml".to_string());

    let toml_text = toml::to_string_pretty(&cfg).map_err(|e| BuilderError::ParseFailed {
        file: "sectorforge.toml".to_string(),
        message: e.to_string(),
    })?;
    atomic_write(&dest.join("sectorforge.toml"), toml_text.as_bytes())?;

    // Seed worlds.toml from the `_base` preset workbook. Prefer the on-disk
    // copy under `./presets` or next to the executable so users can edit it,
    // but fall back to the workbook embedded at compile time so the wizard
    // always produces a non-empty world pool regardless of CWD or install
    // layout. A blank `WorldsConfig::default()` would round-trip to empty
    // `[[generation]]`/`[features]` sections — valid for the loader, but the
    // world-pool builder then yields zero candidates, so every "Regenerate
    // this system" call fails with `NoWorldCandidates`.
    const EMBEDDED_BASE_WORLDS: &str =
        include_str!("../../../presets/_base/data/worlds/worlds.toml");
    let worlds_dir = dest.join("data/worlds");
    fs::create_dir_all(Path::new(worlds_dir.as_str()))?;
    let presets_dir = sectorforge::presets::default_presets_dir();
    let base_workbook = presets_dir.join("_base/data/worlds/worlds.toml");
    let worlds_text = if base_workbook.is_file() {
        fs::read_to_string(base_workbook.as_path())?
    } else {
        EMBEDDED_BASE_WORLDS.to_string()
    };
    atomic_write(&worlds_dir.join("worlds.toml"), worlds_text.as_bytes())?;

    // factions.toml + relations.toml share data/factions/.
    let factions_dir = dest.join("data/factions");
    fs::create_dir_all(Path::new(factions_dir.as_str()))?;
    let factions = FactionsFile {
        factions: default_starter_roster(),
    };
    let text =
        toml::to_string_pretty(&factions).map_err(parse_err("data/factions/factions.toml"))?;
    atomic_write(&factions_dir.join("factions.toml"), text.as_bytes())?;
    let relations = RelationsFile {
        relations: Default::default(),
    };
    let text =
        toml::to_string_pretty(&relations).map_err(parse_err("data/factions/relations.toml"))?;
    atomic_write(&factions_dir.join("relations.toml"), text.as_bytes())?;

    // route_rules.toml under data/routes/.
    let routes_dir = dest.join("data/routes");
    fs::create_dir_all(Path::new(routes_dir.as_str()))?;
    let route_rules_file = RouteRulesFile {
        routes: Default::default(),
    };
    let text = toml::to_string_pretty(&route_rules_file)
        .map_err(parse_err("data/routes/route_rules.toml"))?;
    atomic_write(&routes_dir.join("route_rules.toml"), text.as_bytes())?;

    // regions.toml under data/regions/.
    let regions_dir = dest.join("data/regions");
    fs::create_dir_all(Path::new(regions_dir.as_str()))?;
    let regions_file = RegionsFile {
        regions: Default::default(),
    };
    let text =
        toml::to_string_pretty(&regions_file).map_err(parse_err("data/regions/regions.toml"))?;
    atomic_write(&regions_dir.join("regions.toml"), text.as_bytes())?;

    // economy.toml shares data/worlds/.
    let economy_file = sectorforge::economy::EconomyFile {
        economy: Default::default(),
        resources: sectorforge::economy::ResourceModelConfig::default(),
    };
    let text =
        toml::to_string_pretty(&economy_file).map_err(parse_err("data/worlds/economy.toml"))?;
    atomic_write(&worlds_dir.join("economy.toml"), text.as_bytes())?;

    // history.toml at data/ root.
    fs::create_dir_all(Path::new(dest.join("data").as_str()))?;
    let history_file = HistoryFile {
        history: Default::default(),
    };
    let text = toml::to_string_pretty(&history_file).map_err(parse_err("data/history.toml"))?;
    atomic_write(&dest.join("data/history.toml"), text.as_bytes())?;

    // Empty `out/sector.json` so the project loads with a baseline sector
    // and the EXPORT/TREE views show the canonical output file.
    let out_dir = dest.join("out");
    fs::create_dir_all(Path::new(out_dir.as_str()))?;
    let empty_sector =
        GeneratedSector::empty(&opts.id, &opts.title, &opts.seed, opts.width, opts.height);
    let sector_text = serde_json::to_string_pretty(&empty_sector)?;
    atomic_write(&out_dir.join("sector.json"), sector_text.as_bytes())?;

    Ok(())
}

fn parse_err(rel: &'static str) -> impl Fn(toml::ser::Error) -> BuilderError {
    move |e| BuilderError::ParseFailed {
        file: rel.to_string(),
        message: e.to_string(),
    }
}

/// Small Imperium/Chaos/xenos/cult/trader roster the §P1 wizard drops into a
/// blank project so the FACTIONS tab and downstream derivations have
/// something to bind to. Matches the starter set described in BUILDER.md §5.
fn default_starter_roster() -> Vec<FactionDef> {
    #[allow(clippy::too_many_arguments)]
    fn def(
        id: &str,
        name: &str,
        kind: &str,
        weight: f64,
        disposition: &str,
        world_types: &[&str],
        governments: &[&str],
        features: &[&str],
    ) -> FactionDef {
        FactionDef {
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            id: FactionId::new(id),
            name: name.to_string(),
            kind: kind.to_string(),
            weight,
            default_disposition: disposition.to_string(),
            preferred_world_types: world_types.iter().map(|s| (*s).to_string()).collect(),
            preferred_governments: governments.iter().map(|s| (*s).to_string()).collect(),
            preferred_notable_features: features.iter().map(|s| (*s).to_string()).collect(),
            style_fill: None,
            style_accent: None,
            style_glyph: None,
            style_border: None,
            legend_visible: None,
        }
    }

    vec![
        def(
            "imperial_administration",
            "Imperial Administration",
            "imperial",
            10.0,
            "lawful",
            &["HiveWorld", "BastionWorld", "IndustrialWorld"],
            &[
                "MilitaryGovernor",
                "MagistrateCouncil",
                "EcclesiarchicalAppointee",
            ],
            &["AdministrativeHub", "PoliceState", "ImportantShrine"],
        ),
        def(
            "mechanicus",
            "Adeptus Mechanicus",
            "mechanicus",
            6.0,
            "insular",
            &["ForgeWorld", "ResearchStation", "Orbital"],
            &["MechanicusForgeLord", "ExploratorAuthority"],
            &[
                "ArchaeotechRuins",
                "ForbiddenTech",
                "MajorSpaceyard",
                "LocalTech",
            ],
        ),
        def(
            "free_traders",
            "Free Trader Compact",
            "merchant",
            4.0,
            "opportunistic",
            &["FrontierWorld", "AgriWorld"],
            &["RogueTraderDynasty", "Megacorporations", "GuildsCombine"],
            &["Freeport", "TradeHub", "Prosperous"],
        ),
        def(
            "word_bearers",
            "Word Bearers Warband",
            "chaos_space_marine",
            3.0,
            "hostile",
            &["HiveWorld", "ShrineWorld", "DeathWorld"],
            &[],
            &["DaemonicCorruption", "ChaosCultists", "WitchHunt"],
        ),
        def(
            "goff_klanz",
            "Goff Ork Klanz",
            "ork",
            3.0,
            "hostile",
            &["FeralWorld", "DeathWorld", "FrontierWorld"],
            &["Warlords"],
            &["HostileXenos", "WarZone"],
        ),
        def(
            "hive_fleet_hydra",
            "Hive Fleet Hydra Splinter",
            "tyranid",
            2.0,
            "hostile",
            &["DeathWorld", "FeralWorld", "AgriWorld"],
            &[],
            &["HostileXenos", "Quarantined"],
        ),
        def(
            "cult_of_the_four_armed_emperor",
            "Cult of the Four-Armed Emperor",
            "genestealer_cult",
            2.0,
            "secretive",
            &["HiveWorld", "ExtractiveColony", "IndustrialWorld"],
            &[],
            &["ChaosCultists", "PsykerCult", "PopularUprising"],
        ),
    ]
}

/// §P2: load an existing project from disk into a [`BuilderState`].
///
/// Walks the project tree via [`sectorforge::input::load_project`], populates
/// every catalog the builder edits, and surfaces parse errors per file. The
/// `toml` crate's error message already carries `line:col` information so
/// the [`BuilderError::ParseFailed`] payload meets the §P2 requirement.
///
/// If the project has an `out/sector.json` (or `<outputs.directory>/sector.json`)
/// the sector is loaded so the user reopens at the last generated state.
/// Otherwise the sector starts blank but matched to the config's dimensions.
pub fn open_project(project_dir: &Utf8Path) -> Result<BuilderState, BuilderError> {
    let input: ProjectInput = load_project(project_dir).map_err(map_load_err)?;
    let catalogs = catalogs_from_input(&input);
    let sector = load_or_blank_sector(&input)?;

    let index = BuilderIndex::rebuild(&sector);
    let mut state = BuilderState::new_blank(
        sector.id.as_ref(),
        sector.title.as_ref(),
        sector.seed.as_ref(),
        sector.width,
        sector.height,
    );
    state.sector = sector.into();
    state.index = index;
    state.config = input.config;
    state.data_catalogs = catalogs;
    let root = input.root_dir.clone();
    state.project_path = Some(root.clone());
    state.dirty = false;
    state.invariant_report = Some(sectorforge::invariants::check_sector(&state.sector));

    // §P4 + §P5: snapshot mtimes for every file the loader hashed and start
    // the polling watcher so external edits surface as ConflictResolver or
    // silent reloads.
    let mtimes = collect_mtimes(&root, input.input_digests.keys());
    state.file_mtimes = mtimes.clone();
    state.file_watcher = Some(super::file_watcher::FileWatcher::spawn(
        root.clone(),
        mtimes,
    ));

    // §P6: push the open path onto the MRU.
    push_recent_silent(&root);

    Ok(state)
}

/// §P3: write every catalog and the sector itself back to disk under
/// `project_path`, then update the in-memory manifest digests so they match
/// what is actually on disk. Each file write is atomic (tmp + rename).
pub fn save_project(state: &mut BuilderState) -> Result<(), BuilderError> {
    let root = state.project_path.clone().ok_or_else(|| {
        BuilderError::IoFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no project_path set; use `save_project_as` first",
        ))
    })?;
    save_project_as(state, &root)
}

/// §P3: write the project to a specific root. Updates `state.project_path` and
/// clears the dirty flag on success.
pub fn save_project_as(state: &mut BuilderState, root: &Utf8Path) -> Result<(), BuilderError> {
    fs::create_dir_all(Path::new(root.as_str()))?;
    let mut digests: BTreeMap<String, String> = BTreeMap::new();

    // sectorforge.toml — always rewritten from the live config.
    let cfg_text =
        toml::to_string_pretty(&state.config).map_err(|e| BuilderError::ParseFailed {
            file: "sectorforge.toml".to_string(),
            message: e.to_string(),
        })?;
    write_and_digest(
        root,
        Utf8Path::new("sectorforge.toml"),
        cfg_text.as_bytes(),
        &mut digests,
    )?;

    // Worlds catalog (mandatory).
    if let Some(worlds) = &state.data_catalogs.worlds {
        let text = worlds
            .to_toml_string()
            .map_err(|e| BuilderError::ParseFailed {
                file: "worlds.toml".to_string(),
                message: e.to_string(),
            })?;
        let rel_dir = state.config.inputs.world_data_dir.trim_end_matches('/');
        let rel = format!("{rel_dir}/worlds.toml");
        write_and_digest(root, Utf8Path::new(&rel), text.as_bytes(), &mut digests)?;
    }

    // Optional referenced catalogs — only written when `[inputs]` actually
    // points at them. This matches the load path in `input::load_project`.
    if let (Some(rel), Some(tbl)) = (
        state.config.inputs.system_names.as_deref(),
        state.data_catalogs.names.as_ref(),
    ) {
        let text = toml::to_string_pretty(tbl).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(tbl)) = (
        state.config.inputs.world_names.as_deref(),
        state.data_catalogs.names.as_ref(),
    ) {
        // Skip if it points at the same file as `system_names`; the merged
        // table was already written above.
        if Some(rel) != state.config.inputs.system_names.as_deref() {
            let text = toml::to_string_pretty(tbl).map_err(toml_err(rel))?;
            write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
        }
    }
    if let (Some(rel), Some(file)) = (
        state.config.inputs.factions.as_deref(),
        state.data_catalogs.factions.as_ref(),
    ) {
        let text = toml::to_string_pretty(file).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(rules)) = (
        state.config.inputs.route_rules.as_deref(),
        state.data_catalogs.route_rules.as_ref(),
    ) {
        let wrapper = RouteRulesFile {
            routes: rules.clone(),
        };
        let text = toml::to_string_pretty(&wrapper).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.relations.as_deref(),
        state.data_catalogs.relations.as_ref(),
    ) {
        let wrapper = RelationsFile {
            relations: cfg.clone(),
        };
        let text = toml::to_string_pretty(&wrapper).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.regions.as_deref(),
        state.data_catalogs.regions.as_ref(),
    ) {
        let wrapper = RegionsFile {
            regions: cfg.clone(),
        };
        let text = toml::to_string_pretty(&wrapper).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.economy.as_deref(),
        state.data_catalogs.economy.as_ref(),
    ) {
        // Round-trip via `EconomyFile` to mirror the load path.
        let wrapper = sectorforge::economy::EconomyFile {
            economy: cfg.clone(),
            resources: sectorforge::economy::ResourceModelConfig::default(),
        };
        let text = toml::to_string_pretty(&wrapper).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.history.as_deref(),
        state.data_catalogs.history.as_ref(),
    ) {
        let wrapper = HistoryFile {
            history: cfg.clone(),
        };
        let text = toml::to_string_pretty(&wrapper).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.personae.as_deref(),
        state.data_catalogs.personae.as_ref(),
    ) {
        let text = toml::to_string_pretty(cfg).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.hooks.as_deref(),
        state.data_catalogs.hooks.as_ref(),
    ) {
        let text = toml::to_string_pretty(cfg).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.sites.as_deref(),
        state.data_catalogs.sites.as_ref(),
    ) {
        let text = toml::to_string_pretty(cfg).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.missions.as_deref(),
        state.data_catalogs.missions.as_ref(),
    ) {
        let text = toml::to_string_pretty(cfg).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }
    if let (Some(rel), Some(cfg)) = (
        state.config.inputs.prose.as_deref(),
        state.data_catalogs.prose.as_ref(),
    ) {
        let text = toml::to_string_pretty(cfg).map_err(toml_err(rel))?;
        write_and_digest(root, Utf8Path::new(rel), text.as_bytes(), &mut digests)?;
    }

    // Sector JSON — under `outputs.directory` (default `out/`).
    let out_dir_rel = state.config.outputs.directory.trim_end_matches('/');
    let out_dir = root.join(out_dir_rel);
    fs::create_dir_all(Path::new(out_dir.as_str()))?;

    // Update the manifest digests to mirror what we just wrote before
    // serialising the sector. This keeps `sector.manifest.input_digests`
    // honest after a builder save.
    let s = state.sector_mut();
    s.manifest.input_digests = digests
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    s.manifest.system_count = s.systems.len();
    s.manifest.world_count = s.systems.iter().map(|sys| sys.worlds.len()).sum();
    s.manifest.route_count = s.routes.len();

    let sector_text = serde_json::to_string_pretty(&*state.sector)?;
    atomic_write(&out_dir.join("sector.json"), sector_text.as_bytes())?;

    if state.config.outputs.write_manifest {
        let manifest_text = serde_json::to_string_pretty(&state.sector.manifest)?;
        atomic_write(&out_dir.join("manifest.json"), manifest_text.as_bytes())?;
    }

    state.project_path = Some(root.to_path_buf());
    state.dirty = false;
    state.dirty_files.clear();

    // Refresh the mtime snapshot + restart the watcher so its baseline does
    // not immediately fire on our own writes.
    let mtimes = collect_mtimes(root, digests.keys());
    state.file_mtimes = mtimes.clone();
    state.file_watcher = Some(super::file_watcher::FileWatcher::spawn(
        root.to_path_buf(),
        mtimes,
    ));
    push_recent_silent(root);

    Ok(())
}

/// §PF5: re-snapshot the mtimes of every tracked file and restart the watcher.
///
/// The §PF2 raw editor writes individual config files directly (not through
/// `save_project`), so without this the watcher's stale baseline would fire a
/// spurious [`super::ModalKind::ConflictResolver`] on the builder's own write.
/// Mirrors the watcher refresh `save_project_as` performs after a full save.
pub fn refresh_watcher_baseline(state: &mut BuilderState) {
    let Some(root) = state.project_path.clone() else {
        return;
    };
    let keys: Vec<String> = state.file_mtimes.keys().cloned().collect();
    let mtimes = collect_mtimes(&root, keys.iter());
    state.file_mtimes = mtimes.clone();
    state.file_watcher = Some(super::file_watcher::FileWatcher::spawn(root, mtimes));
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn catalogs_from_input(input: &ProjectInput) -> DataCatalogs {
    use sectorforge::worlds_toml::WorldsConfig;

    // Re-read the worlds.toml directly so the catalog mirrors the on-disk
    // file (the loader otherwise normalises into `KeyTables` + rows).
    let worlds_path = input
        .root_dir
        .join(&input.config.inputs.world_data_dir)
        .join(sectorforge::worlds_toml::DEFAULT_FILENAME);
    let worlds = match fs::read_to_string(Path::new(worlds_path.as_str())) {
        Ok(text) => WorldsConfig::from_str(&text).ok(),
        Err(_) => None,
    };

    DataCatalogs {
        worlds,
        names: Some(input.catalogs.names.clone()),
        factions: Some(FactionsFile {
            factions: input.catalogs.factions.clone(),
        }),
        relations: Some(input.catalogs.relations.clone()),
        route_rules: Some(input.catalogs.route_rules.clone()),
        regions: Some(input.catalogs.regions.clone()),
        economy: Some(input.catalogs.economy.clone()),
        history: Some(input.catalogs.history.clone()),
        personae: Some(input.catalogs.personae.clone()),
        hooks: Some(input.catalogs.hooks.clone()),
        sites: Some(input.catalogs.sites.clone()),
        missions: Some(input.catalogs.missions.clone()),
        prose: Some(input.catalogs.prose.clone()),
    }
}

/// Load the bundled `_base` preset's full data-catalog set (worlds, factions,
/// names, regions, route rules, …) together with its `[inputs]` path block, for
/// seeding a generator launched with **no project open** (so the live
/// `data_catalogs` has no world pool and a prefix/commit run would dead-end on
/// the missing worlds catalog). Returns `None` when `_base` can't be located /
/// parsed (e.g. an install with no `presets/` dir) — the caller then leaves the
/// session uninitialised and the panel surfaces the missing-catalog message.
///
/// This is the in-memory analogue of what the one-shot random generator gets by
/// scaffolding `_base` to disk and `load_project`-ing it, so the wizard preview
/// / commit run on exactly the same world pool as a one-shot roll.
#[must_use]
pub fn load_base_catalogs() -> Option<(DataCatalogs, sectorforge::config::InputConfig)> {
    load_base_catalogs_from(&sectorforge::presets::default_presets_dir().join("_base"))
}

/// Inner form of [`load_base_catalogs`] taking an explicit `_base` directory, so
/// tests can point at the workspace preset regardless of the process CWD (the
/// public form resolves it CWD-relative via [`sectorforge::presets::default_presets_dir`]).
#[must_use]
pub(crate) fn load_base_catalogs_from(
    base: &Utf8Path,
) -> Option<(DataCatalogs, sectorforge::config::InputConfig)> {
    let input = sectorforge::load_project(base).ok()?;
    let catalogs = catalogs_from_input(&input);
    Some((catalogs, input.config.inputs))
}

fn load_or_blank_sector(input: &ProjectInput) -> Result<GeneratedSector, BuilderError> {
    let candidate = input
        .root_dir
        .join(input.config.outputs.directory.trim_end_matches('/'))
        .join("sector.json");
    if candidate.is_file() {
        let text = fs::read_to_string(Path::new(candidate.as_str()))?;
        let sector: GeneratedSector =
            serde_json::from_str(&text).map_err(|e| BuilderError::ParseFailed {
                file: candidate.to_string(),
                message: e.to_string(),
            })?;
        return Ok(sector);
    }
    Ok(GeneratedSector::empty(
        &input.config.project.id,
        &input.config.project.title,
        &input.config.generation.seed,
        input.config.generation.sector_width,
        input.config.generation.sector_height,
    ))
}

fn map_load_err(err: sectorforge::errors::SectorError) -> BuilderError {
    use sectorforge::errors::SectorError;
    match err {
        SectorError::ConfigParse { path, message } => BuilderError::ParseFailed {
            file: path,
            message,
        },
        SectorError::Io { path, source } => BuilderError::ParseFailed {
            file: path,
            message: source.to_string(),
        },
        other => BuilderError::ParseFailed {
            file: "(project)".to_string(),
            message: other.to_string(),
        },
    }
}

fn toml_err(rel: &str) -> impl Fn(toml::ser::Error) -> BuilderError + '_ {
    move |e| BuilderError::ParseFailed {
        file: rel.to_string(),
        message: e.to_string(),
    }
}

fn write_and_digest(
    root: &Utf8Path,
    rel: &Utf8Path,
    bytes: &[u8],
    digests: &mut BTreeMap<String, String>,
) -> Result<(), BuilderError> {
    let target = root.join(rel);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(Path::new(parent.as_str()))?;
    }
    atomic_write(&target, bytes)?;
    digests.insert(rel.to_string(), blake3_digest(bytes));
    Ok(())
}

/// Write `bytes` to `target` atomically (write to a sibling temp file, then
/// rename). Shared with the §PF6 / §SG1 `segmentum.toml` writer.
pub(crate) fn atomic_write(target: &Utf8Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = target.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target has no parent directory",
        )
    })?;
    fs::create_dir_all(Path::new(parent.as_str()))?;
    let file_name = target.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "target has no file name")
    })?;
    let tmp = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));
    {
        fs::write(Path::new(tmp.as_str()), bytes)?;
    }
    fs::rename(Path::new(tmp.as_str()), Path::new(target.as_str()))
}

fn blake3_digest(bytes: &[u8]) -> String {
    format!("blake3:{}", sectorforge::rng::digest_bytes(bytes))
}

fn collect_mtimes<'a, I>(root: &Utf8Path, rels: I) -> BTreeMap<String, std::time::SystemTime>
where
    I: IntoIterator<Item = &'a String>,
{
    let mut out = BTreeMap::new();
    for rel in rels {
        let abs = root.join(rel);
        if let Ok(meta) = fs::metadata(Path::new(abs.as_str())) {
            if let Ok(t) = meta.modified() {
                out.insert(rel.clone(), t);
            }
        }
    }
    out
}

/// §P6: best-effort MRU push. Failures are swallowed because preferences are
/// non-critical — a missing `$HOME` or unreadable file must not block a
/// successful open/save.
fn push_recent_silent(root: &Utf8Path) {
    let mut prefs = super::preferences::Preferences::load();
    prefs.push_recent(root.to_path_buf());
    let _ = prefs.save();
}

/// §P5: drain pending file changes from the active watcher and merge them
/// into `state`. When the in-memory buffer is clean (`!state.dirty`) the
/// affected catalog is silently reloaded; otherwise the first dirty
/// collision raises [`ModalKind::ConflictResolver`] and remaining events
/// stay queued for next frame.
///
/// The caller wires this into the GUI's per-frame update.
pub fn drain_watcher_events(state: &mut BuilderState) {
    // Collect all pending events first so we don't hold an immutable borrow of
    // `state.file_watcher` while mutating other fields below.
    let mut events: Vec<super::file_watcher::FileChange> = Vec::new();
    if let Some(watcher) = state.file_watcher.as_ref() {
        while let Some(ev) = watcher.try_recv() {
            events.push(ev);
        }
    }
    for ev in events {
        // Update the recorded mtime so the next tick doesn't refire.
        state.file_mtimes.insert(ev.rel_path.clone(), ev.mtime);
        if state.dirty || state.dirty_files.contains(&ev.rel_path) {
            // User has unsaved changes — defer to the resolver dialog.
            if state.feedback.modal.is_none() {
                state.feedback.modal = Some(super::ModalKind::ConflictResolver {
                    rel_path: ev.rel_path,
                });
            }
            // Leave further events on the queue; they'll be drained after
            // the user resolves this one.
            return;
        }
        // Clean buffer: silently re-read the catalog from disk.
        let Some(root) = state.project_path.clone() else {
            continue;
        };
        let abs = root.join(&ev.rel_path);
        if let Ok(text) = fs::read_to_string(Path::new(abs.as_str())) {
            let rel = ev.rel_path.clone();
            match reload_catalog(state, &rel, &text) {
                Ok(()) => {
                    state.feedback.last_catalog_error = None;
                }
                Err(e) => {
                    state.feedback.last_catalog_error = Some(e.to_string());
                }
            }
        }
    }
}

/// Error returned by [`reload_catalog`] when an on-disk catalog edit
/// triggered by the file watcher can no longer be parsed. Stored in
/// `BuilderState::feedback.last_catalog_error` so the status bar surfaces it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogReloadError {
    #[error("parse {kind} from {rel}: {message}")]
    Parse {
        kind: &'static str,
        rel: String,
        message: String,
    },
}

fn reload_err(kind: &'static str, rel: &str, e: impl std::fmt::Display) -> CatalogReloadError {
    CatalogReloadError::Parse {
        kind,
        rel: rel.to_string(),
        message: e.to_string(),
    }
}

/// Reload a single catalog from `text`. Each branch matches the file path
/// against `state.config.inputs.<key>` so renames in `sectorforge.toml`
/// are honoured. Public so the conflict-resolver panel can call it after
/// the user picks "Reload from disk".
///
/// Returns `Ok(())` on success or when `rel` does not match any known
/// catalog (the caller usually doesn't care). Returns `Err` with the
/// underlying TOML parse message when the file *did* match a catalog but
/// failed to parse — previously these errors were silently swallowed.
pub fn reload_catalog(
    state: &mut BuilderState,
    rel: &str,
    text: &str,
) -> Result<(), CatalogReloadError> {
    if rel == "sectorforge.toml" {
        state.config =
            toml::from_str::<AppConfig>(text).map_err(|e| reload_err("AppConfig", rel, e))?;
        // Audit finding #21: a config reload can change any generation knob the
        // derivation fingerprints fold in (e.g. relations `min_world_presence`),
        // so stale every already-derived overlay so it re-derives against the
        // new config.
        state.invalidate_all_derivations();
        return Ok(());
    }
    let worlds_rel = format!(
        "{}/{}",
        state.config.inputs.world_data_dir.trim_end_matches('/'),
        sectorforge::worlds_toml::DEFAULT_FILENAME
    );
    if rel == worlds_rel {
        let cfg = sectorforge::worlds_toml::WorldsConfig::from_str(text)
            .map_err(|e| reload_err("WorldsConfig", rel, e))?;
        state.data_catalogs.worlds = Some(cfg);
        // Audit finding #21: the worlds catalog is the §39 `SystemsWorlds`
        // input, which fans out to every overlay — stale them all.
        state.invalidate_all_derivations();
        return Ok(());
    }
    if state.config.inputs.factions.as_deref() == Some(rel) {
        let file =
            toml::from_str::<FactionsFile>(text).map_err(|e| reload_err("FactionsFile", rel, e))?;
        state.data_catalogs.factions = Some(file);
        // Audit finding #21: mirror a `Factions`-class command's invalidation.
        state.invalidate_derivations(&[DepClass::Factions]);
        return Ok(());
    }
    if state.config.inputs.route_rules.as_deref() == Some(rel) {
        let file = toml::from_str::<RouteRulesFile>(text)
            .map_err(|e| reload_err("RouteRulesFile", rel, e))?;
        state.data_catalogs.route_rules = Some(file.routes);
        // Audit finding #21: mirror a `Routes`-class command's invalidation.
        state.invalidate_derivations(&[DepClass::Routes]);
        return Ok(());
    }
    if state.config.inputs.relations.as_deref() == Some(rel) {
        let file = toml::from_str::<RelationsFile>(text)
            .map_err(|e| reload_err("RelationsFile", rel, e))?;
        state.data_catalogs.relations = Some(file.relations);
        // Audit finding #21: mirror a `RelationsCfg`-class catalog edit.
        state.invalidate_derivations(&[DepClass::RelationsCfg]);
        return Ok(());
    }
    if state.config.inputs.regions.as_deref() == Some(rel) {
        let file =
            toml::from_str::<RegionsFile>(text).map_err(|e| reload_err("RegionsFile", rel, e))?;
        state.data_catalogs.regions = Some(file.regions);
        // Audit finding #21: mirror a `Regions`-class command's invalidation.
        state.invalidate_derivations(&[DepClass::Regions]);
        return Ok(());
    }
    if state.config.inputs.economy.as_deref() == Some(rel) {
        let file = toml::from_str::<sectorforge::economy::EconomyFile>(text)
            .map_err(|e| reload_err("EconomyFile", rel, e))?;
        state.data_catalogs.economy = Some(file.into_config());
        // Audit finding #21: mirror an `EconomyCfg`-class catalog edit.
        state.invalidate_derivations(&[DepClass::EconomyCfg]);
        return Ok(());
    }
    if state.config.inputs.history.as_deref() == Some(rel) {
        let file =
            toml::from_str::<HistoryFile>(text).map_err(|e| reload_err("HistoryFile", rel, e))?;
        state.data_catalogs.history = Some(file.history);
        // Audit finding #21: history has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::History]);
        return Ok(());
    }
    if state.config.inputs.personae.as_deref() == Some(rel) {
        let cfg = toml::from_str::<sectorforge::personae::PersonaeConfig>(text)
            .map_err(|e| reload_err("PersonaeConfig", rel, e))?;
        state.data_catalogs.personae = Some(cfg);
        // Audit finding #21: personae has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::Personae]);
        return Ok(());
    }
    if state.config.inputs.hooks.as_deref() == Some(rel) {
        let cfg = toml::from_str::<sectorforge::hooks::HooksConfig>(text)
            .map_err(|e| reload_err("HooksConfig", rel, e))?;
        state.data_catalogs.hooks = Some(cfg);
        // Audit finding #21: hooks has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::Hooks]);
        return Ok(());
    }
    if state.config.inputs.sites.as_deref() == Some(rel) {
        let cfg = toml::from_str::<sectorforge::sites::SitesConfig>(text)
            .map_err(|e| reload_err("SitesConfig", rel, e))?;
        state.data_catalogs.sites = Some(cfg);
        // Audit finding #21: sites has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::Sites]);
        return Ok(());
    }
    if state.config.inputs.prose.as_deref() == Some(rel) {
        let cfg = toml::from_str::<sectorforge::prose::ProseConfig>(text)
            .map_err(|e| reload_err("ProseConfig", rel, e))?;
        state.data_catalogs.prose = Some(cfg);
        // Audit finding #21: prose has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::Prose]);
        return Ok(());
    }
    if state.config.inputs.missions.as_deref() == Some(rel) {
        let cfg = toml::from_str::<sectorforge::missions::MissionsConfig>(text)
            .map_err(|e| reload_err("MissionsConfig", rel, e))?;
        state.data_catalogs.missions = Some(cfg);
        // Audit finding #21: missions has no DepClass — stale the kind directly.
        state.invalidate_derivation_kinds(&[DerivationKind::Missions]);
        return Ok(());
    }
    if state.config.inputs.system_names.as_deref() == Some(rel)
        || state.config.inputs.world_names.as_deref() == Some(rel)
    {
        let file = toml::from_str::<sectorforge::names::NameTables>(text)
            .map_err(|e| reload_err("NameTables", rel, e))?;
        state.data_catalogs.names = Some(file);
        // Audit finding #21: name tables feed generation broadly with no single
        // DepClass/kind — stale every already-derived overlay conservatively.
        state.invalidate_all_derivations();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    #[test]
    fn new_project_blank_round_trips() {
        let dir = td();
        let dest = Utf8PathBuf::from_path_buf(dir.path().join("proj")).unwrap();
        let opts = NewProjectOptions {
            dest: dest.clone(),
            id: "test".into(),
            title: "Test".into(),
            seed: "seed-1".into(),
            width: 6,
            height: 6,
            preset: None,
        };
        let state = new_project(opts).expect("scaffold");
        assert_eq!(state.sector.width, 6);
        assert_eq!(state.sector.id.as_ref(), "test");
        assert!(dest.join("sectorforge.toml").exists());
        assert!(dest.join("data/worlds/worlds.toml").exists());
        assert!(dest.join("data/factions/factions.toml").exists());
        assert!(dest.join("data/factions/relations.toml").exists());
        assert!(dest.join("data/routes/route_rules.toml").exists());
        assert!(dest.join("data/regions/regions.toml").exists());
        assert!(dest.join("data/worlds/economy.toml").exists());
        assert!(dest.join("data/history.toml").exists());
        assert!(dest.join("out/sector.json").exists());
        assert!(state.data_catalogs.worlds.is_some());
        assert!(state.data_catalogs.factions.is_some());
        // Wizard ships a 7-faction starter roster so the FACTIONS tab and
        // downstream derivations have something to bind to.
        assert_eq!(
            state
                .data_catalogs
                .factions
                .as_ref()
                .unwrap()
                .factions
                .len(),
            7
        );
        assert!(state.data_catalogs.relations.is_some());
        assert!(state.data_catalogs.route_rules.is_some());
        assert!(state.data_catalogs.regions.is_some());
        assert!(state.data_catalogs.economy.is_some());
        assert!(state.data_catalogs.history.is_some());

        // The scaffold should seed `worlds.toml` from the `_base` preset when
        // it is reachable (it is during `cargo test` from the workspace root)
        // so "Regenerate this system" finds a non-empty world pool. The
        // fallback `WorldsConfig::default()` path leaves zero rows; guard
        // against that regression while still tolerating it on installs that
        // ship without presets.
        if sectorforge::presets::default_presets_dir()
            .join("_base/data/worlds/worlds.toml")
            .is_file()
        {
            assert!(
                !state
                    .data_catalogs
                    .worlds
                    .as_ref()
                    .unwrap()
                    .generation
                    .is_empty(),
                "blank scaffold should copy generation rows from presets/_base"
            );
        }

        // Reopening the project must succeed — confirms every scaffolded
        // catalogue parses cleanly through `load_project`.
        let reopened = open_project(&dest).expect("reopen scaffolded project");
        assert_eq!(reopened.sector.width, 6);
        assert!(reopened.data_catalogs.regions.is_some());
    }

    #[test]
    fn open_then_save_creates_files_and_digests() {
        let dir = td();
        let dest = Utf8PathBuf::from_path_buf(dir.path().join("proj")).unwrap();
        let opts = NewProjectOptions {
            dest: dest.clone(),
            id: "s".into(),
            title: "S".into(),
            seed: "seed".into(),
            width: 4,
            height: 4,
            preset: None,
        };
        let mut state = new_project(opts).unwrap();
        save_project(&mut state).unwrap();
        assert!(dest.join("out/sector.json").exists());
        assert!(dest.join("out/manifest.json").exists());
        // Digests now mirror the on-disk files.
        let m = &state.sector.manifest.input_digests;
        assert!(m.contains_key("sectorforge.toml"));
        assert!(m.values().all(|v| v.starts_with("blake3:")));
    }

    #[test]
    fn open_project_surfaces_toml_parse_errors_with_line() {
        let dir = td();
        let proj = Utf8PathBuf::from_path_buf(dir.path().join("bad")).unwrap();
        fs::create_dir_all(proj.as_std_path()).unwrap();
        fs::write(
            proj.join("sectorforge.toml").as_std_path(),
            "[project]\nid = \nbroken\n",
        )
        .unwrap();
        let result = open_project(&proj);
        match result {
            Err(BuilderError::ParseFailed { message, .. }) => {
                assert!(
                    message.contains("line") || message.contains("expected"),
                    "expected line/col info, got: {message}"
                );
            }
            Err(other) => panic!("expected ParseFailed, got {other:?}"),
            Ok(_) => panic!("expected open_project to fail on bad TOML"),
        }
    }
}
