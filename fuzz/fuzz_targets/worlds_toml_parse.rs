#![no_main]
//! TF-T-10: fuzz target for `WorldsConfig::from_str`.

use libfuzzer_sys::fuzz_target;
use sectorforge::worlds_toml::WorldsConfig;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = WorldsConfig::from_str(s);
});
