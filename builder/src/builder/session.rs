//! `.sgforge` session file (D6).
//!
//! The session is a JSON envelope. The sector + command log + side-tables
//! live as native serialised values. Project files (worlds.toml, factions.toml,
//! ...) are embedded as `EmbeddedFile` entries — base64-encoded bytes so the
//! envelope round-trips arbitrary byte sequences without requiring a new
//! dependency.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use sectorforge::config::AppConfig;
use sectorforge::ids::{SystemId, WorldId};
use sectorforge::sector_model::GeneratedSector;

use super::command::BuilderCommand;
use super::errors::BuilderError;
use super::snapshot::Snapshot;
use super::state::BuilderState;

const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFile {
    pub version: u32,
    pub sector: GeneratedSector,
    pub config: AppConfig,
    pub command_log: Vec<BuilderCommand>,
    pub command_cursor: usize,
    pub snapshots: Vec<SerializableSnapshot>,
    pub pinned_systems: BTreeSet<SystemId>,
    pub pinned_worlds: BTreeSet<WorldId>,
    pub project_path: Option<Utf8PathBuf>,
    pub stable_ids_on_rename: bool,
    /// Embedded mirrors of every project file the builder was editing.
    pub files: Vec<EmbeddedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableSnapshot {
    pub name: String,
    pub sector: GeneratedSector,
    pub command_log_position: usize,
}

impl From<&Snapshot> for SerializableSnapshot {
    fn from(s: &Snapshot) -> Self {
        Self {
            name: s.name.clone(),
            sector: s.sector.clone(),
            command_log_position: s.command_log_position,
        }
    }
}

impl From<SerializableSnapshot> for Snapshot {
    fn from(s: SerializableSnapshot) -> Self {
        Snapshot {
            name: s.name,
            sector: s.sector,
            command_log_position: s.command_log_position,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedFile {
    /// Path relative to the project root.
    pub path: String,
    /// Base64 (standard alphabet, no padding stripping) of the file's bytes.
    pub data_b64: String,
}

impl SessionFile {
    pub fn from_state(state: &BuilderState, files: Vec<EmbeddedFile>) -> Self {
        Self {
            version: SESSION_VERSION,
            sector: state.sector.clone(),
            config: state.config.clone(),
            command_log: state.command_log.clone(),
            command_cursor: state.command_cursor,
            snapshots: state.snapshots.iter().map(Into::into).collect(),
            pinned_systems: state.pinned_systems.clone(),
            pinned_worlds: state.pinned_worlds.clone(),
            project_path: state.project_path.clone(),
            stable_ids_on_rename: state.stable_ids_on_rename,
            files,
        }
    }

    pub fn into_state(self) -> BuilderState {
        use super::data_catalogs::DataCatalogs;
        use super::derivation_cache::DerivationCache;
        use super::index::BuilderIndex;

        let index = BuilderIndex::rebuild(&self.sector);
        BuilderState {
            sector: self.sector,
            project_path: self.project_path,
            config: self.config,
            data_catalogs: DataCatalogs::new(),
            index,
            command_log: self.command_log,
            command_cursor: self.command_cursor,
            snapshots: self.snapshots.into_iter().map(Into::into).collect(),
            command_log_capacity: super::state::DEFAULT_COMMAND_LOG_CAPACITY,
            pinned_systems: self.pinned_systems,
            pinned_worlds: self.pinned_worlds,
            derivation_cache: DerivationCache::new(),
            dirty: false,
            auto_save_path: None,
            validation_report: None,
            invariant_report: None,
            modal: None,
            pending_jobs: Vec::new(),
            stable_ids_on_rename: self.stable_ids_on_rename,
            dirty_files: std::collections::BTreeSet::new(),
            selected_file: None,
            file_mtimes: std::collections::BTreeMap::new(),
            file_watcher: None,
            validation_dirty_since: None,
            validation_debounce: std::time::Duration::from_millis(
                super::state::DEFAULT_VALIDATION_DEBOUNCE_MS,
            ),
            selected_system_id: None,
            selected_world_id: None,
            selected_route_id: None,
            selected_faction_id: None,
            selected_region_id: None,
            active_tab: super::state::BuilderTab::Project,
            map_tool: super::state::MapTool::Select,
            seed_locked: false,
            seed_reroll_counter: 0,
            preview: super::preview::PreviewState::new(),
            partial_regen_rect: None,
            selected_systems: std::collections::BTreeSet::new(),
            drag_system: None,
            pending_route_start: None,
            pending_place: None,
            pending_rename: None,
            pending_collision: None,
            rect_select: None,
            hex_size: 28.0,
            map_view_cache: None,
            world_reroll_counter: 0,
            route_bulk_filter_type: None,
            route_bulk_filter_stability: None,
            route_bulk_filter_tag: String::new(),
            route_bulk_filter_region: None,
            route_bulk_set_type: sectorforge::sector_model::RouteType::ChartedPassage,
            route_bulk_set_stability: sectorforge::sector_model::RouteStability::Hazardous,
            hidden_route_kind: sectorforge::sector_model::RouteType::Webway,
            hidden_route_k_nearest: sectorforge::hidden_routes::DEFAULT_HIDDEN_K_NEAREST,
            hidden_route_exclude_blackout: true,
            hidden_route_endpoints: std::collections::BTreeSet::new(),
            dominance_locked: std::collections::BTreeSet::new(),
            primary_factions_locked: std::collections::BTreeSet::new(),
            control_overlay: super::state::ControlOverlay::None,
            region_grow_q: 0,
            region_grow_r: 0,
            region_grow_size: 6,
            region_grow_kind: sectorforge::regions::RegionConditionKind::Turbulence,
            selected_subsector_id: None,
            subsector_target_systems: sectorforge::subsectors::DEFAULT_TARGET_SYSTEMS_PER_SUBSECTOR,
            subsector_system_overrides: std::collections::BTreeMap::new(),
            subsector_manual: std::collections::BTreeSet::new(),
            subsector_capital_overrides: std::collections::BTreeMap::new(),
            subsector_colour_overrides: std::collections::BTreeMap::new(),
            world_economy_overrides: std::collections::BTreeMap::new(),
            world_strategic_overrides: std::collections::BTreeMap::new(),
            system_tithe_overrides: std::collections::BTreeMap::new(),
            system_supply_overrides: std::collections::BTreeMap::new(),
            system_priority_overrides: std::collections::BTreeMap::new(),
            map_heatmap_mode: sectorforge::heatmap::HeatmapMode::Off,
            economy_highlight_lifelines: false,
            economy_lifeline_min_score: 35.0,
            relations_selected_pair: None,
            relations_auto_recompute: true,
            selected_history_event: None,
            history_auto_recompute: false,
            history_wizard: None,
            intel_observer: None,
            intel_player_min_confidence: 0,
            archetype_flags: crate::builder::command::ArchetypeApplyFlags::default(),
            system_conflict_override: std::collections::BTreeSet::new(),
            conflict_ticks_to_advance: 1,
            tick_log: std::collections::VecDeque::new(),
            tick_log_capacity: 500,
            nav_back_stack: Vec::new(),
            nav_forward_stack: Vec::new(),
            selected_persona_id: None,
            selected_hook_id: None,
            personae_report: None,
            personae_auto_recompute: true,
            personae_edit_target: None,
            hooks_report: None,
            hooks_auto_recompute: true,
            hooks_player_edition: false,
            hooks_filter_kind: None,
            hooks_edit_target: None,
            sites_report: None,
            sites_auto_recompute: true,
            sites_player_edition: false,
            sites_filter_kind: None,
            selected_site_id: None,
            sites_edit_target: None,
            missions_report: None,
            missions_auto_recompute: true,
            missions_player_edition: false,
            missions_filter_kind: None,
            selected_mission_id: None,
            missions_edit_target: None,
            prose_report: None,
            prose_auto_recompute: true,
            selected_prose_system_id: None,
            briefing_preset: sectorforge::briefing::AudiencePreset::GmFullTruth,
            briefing_observer: None,
            briefing_min_confidence: 30,
            briefing_preview_md: None,
            briefing_preview_pack: None,
            briefing_export_dir: None,
            interestingness_profile: sectorforge::interestingness::ProfileId::PoliticalSandbox,
            interestingness_report: None,
            interestingness_custom_overrides: std::collections::BTreeMap::new(),
            interestingness_custom_pick: String::new(),
        }
    }
}

pub fn save_session(path: &Path, state: &BuilderState) -> Result<(), BuilderError> {
    let file = SessionFile::from_state(state, Vec::new());
    let text = serde_json::to_string_pretty(&file)?;
    fs::write(path, text).map_err(BuilderError::from)
}

pub fn load_session(path: &Path) -> Result<BuilderState, BuilderError> {
    let text = fs::read_to_string(path)?;
    let file: SessionFile = serde_json::from_str(&text).map_err(|e| BuilderError::ParseFailed {
        file: path.display().to_string(),
        message: e.to_string(),
    })?;
    if file.version != SESSION_VERSION {
        return Err(BuilderError::ParseFailed {
            file: path.display().to_string(),
            message: format!(
                "unsupported .sgforge version {} (expected {})",
                file.version, SESSION_VERSION
            ),
        });
    }
    Ok(file.into_state())
}

// ── Base64 (standard alphabet) ─────────────────────────────────────────────
//
// Tiny inline encoder/decoder. Used by [`EmbeddedFile`] so the envelope can
// carry arbitrary bytes — including TOML, JSON, or binaries — without adding
// a `base64` crate (R9: no new crates).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode_base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = u32::from(rem[0]) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf = [0u32; 4];
    let mut pad = [false; 4];
    let mut i = 0;
    for ch in input.bytes() {
        if ch == b'\n' || ch == b'\r' || ch == b' ' {
            continue;
        }
        let (v, is_pad) = match ch {
            b'A'..=b'Z' => (u32::from(ch - b'A'), false),
            b'a'..=b'z' => (26 + u32::from(ch - b'a'), false),
            b'0'..=b'9' => (52 + u32::from(ch - b'0'), false),
            b'+' => (62, false),
            b'/' => (63, false),
            b'=' => (0, true),
            _ => return Err(format!("invalid base64 byte: {ch:#x}")),
        };
        buf[i] = v;
        pad[i] = is_pad;
        i += 1;
        if i == 4 {
            let n = (buf[0] << 18) | (buf[1] << 12) | (buf[2] << 6) | buf[3];
            bytes.push((n >> 16) as u8);
            if !pad[2] {
                bytes.push((n >> 8) as u8);
            }
            if !pad[3] {
                bytes.push(n as u8);
            }
            i = 0;
            pad = [false; 4];
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        let cases: &[&[u8]] = &[b"", b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"];
        let expected = [
            "", "Zg==", "Zm8=", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy",
        ];
        for (i, bytes) in cases.iter().enumerate() {
            let enc = encode_base64(bytes);
            assert_eq!(enc, expected[i], "encode {}", i);
            let dec = decode_base64(&enc).unwrap();
            assert_eq!(&dec, bytes, "decode {}", i);
        }
    }

    #[test]
    fn round_trip_empty_state() {
        let state = BuilderState::new_blank("t", "T", "seed", 4, 4);
        let file = SessionFile::from_state(&state, Vec::new());
        let text = serde_json::to_string(&file).unwrap();
        let back: SessionFile = serde_json::from_str(&text).unwrap();
        let restored = back.into_state();
        assert_eq!(restored.sector.id.as_ref(), "t");
        assert_eq!(restored.sector.width, 4);
        assert_eq!(restored.command_log.len(), 0);
    }
}
