//! `.sgforge` session file (D6).
//!
//! The session is a JSON envelope. The sector + command log + side-tables
//! live as native serialised values.

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
    pub snapshots: Vec<Snapshot>,
    pub pinned_systems: BTreeSet<SystemId>,
    pub pinned_worlds: BTreeSet<WorldId>,
    pub project_path: Option<Utf8PathBuf>,
}

impl SessionFile {
    pub fn from_state(state: &BuilderState) -> Self {
        Self {
            version: SESSION_VERSION,
            sector: (*state.sector).clone(),
            config: state.config.clone(),
            command_log: state.command_log.clone(),
            command_cursor: state.command_cursor,
            snapshots: state.snapshots.clone(),
            pinned_systems: state.pinned_systems.clone(),
            pinned_worlds: state.pinned_worlds.clone(),
            project_path: state.project_path.clone(),
        }
    }

    pub fn into_state(self) -> BuilderState {
        use super::index::BuilderIndex;

        // Everything not persisted in the envelope keeps its `new_blank`
        // default; only the loaded document fields below are overwritten.
        let mut state = BuilderState::new_blank("", "", "", 0, 0);
        state.index = BuilderIndex::rebuild(&self.sector);
        state.sector = self.sector.into();
        state.project_path = self.project_path;
        state.config = self.config;
        state.command_log = self.command_log;
        state.command_cursor = self.command_cursor;
        state.snapshots = self.snapshots;
        state.pinned_systems = self.pinned_systems;
        state.pinned_worlds = self.pinned_worlds;
        state
    }
}

pub fn save_session(path: &Path, state: &BuilderState) -> Result<(), BuilderError> {
    let file = SessionFile::from_state(state);
    let text = serde_json::to_string_pretty(&file)?;
    fs::write(path, text).map_err(BuilderError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty_state() {
        let state = BuilderState::new_blank("t", "T", "seed", 4, 4);
        let file = SessionFile::from_state(&state);
        let text = serde_json::to_string(&file).unwrap();
        let back: SessionFile = serde_json::from_str(&text).unwrap();
        let restored = back.into_state();
        assert_eq!(restored.sector.id.as_ref(), "t");
        assert_eq!(restored.sector.width, 4);
        assert_eq!(restored.command_log.len(), 0);
    }
}
