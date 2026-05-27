//! Project loading + serialisation.
//!
//! `input` parses a project directory (`sectorforge.toml` + every referenced
//! data file). `config` is the top-level config schema. `presets` resolves the
//! built-in preset library. `sector_save` is the IDs-only runtime save format
//! (§13).

pub mod config;
pub mod input;
pub mod presets;
pub mod sector_save;
