//! Named save points (U3/U4). A snapshot freezes the sector + the position
//! in the command log so the user can fall back to it.

use serde::{Deserialize, Serialize};

use sectorforge::sector_model::GeneratedSector;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub sector: GeneratedSector,
    pub command_log_position: usize,
}

impl Snapshot {
    pub fn new(name: impl Into<String>, sector: GeneratedSector, position: usize) -> Self {
        Self {
            name: name.into(),
            sector,
            command_log_position: position,
        }
    }
}
