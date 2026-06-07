#![no_main]
//! #26: fuzz target for the factions catalog TOML. `factions.toml`
//! deserializes into `sectorforge::factions::FactionsFile` (see
//! `src/loading/input.rs`); parsing must never panic on arbitrary input.

use libfuzzer_sys::fuzz_target;
use sectorforge::factions::FactionsFile;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = toml::from_str::<FactionsFile>(s);
});
