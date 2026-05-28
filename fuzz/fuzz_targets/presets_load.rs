#![no_main]
//! TF-T-10: fuzz target for preset metadata parsing.
//! We fuzz the `preset.toml` schema deserialiser instead of `presets::load`
//! itself because the latter requires a directory tree (libfuzzer hands us
//! raw bytes, not filesystem state).

use libfuzzer_sys::fuzz_target;
use sectorforge::presets::PresetMeta;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = toml::from_str::<PresetMeta>(s);
});
