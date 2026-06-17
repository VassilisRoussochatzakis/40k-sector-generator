//! Project loading + serialisation.
//!
//! `input` parses a project directory (`sectorforge.toml` + every referenced
//! data file). `config` is the top-level config schema. `presets` resolves the
//! built-in preset library.

pub mod config;
pub mod input;
pub mod presets;
