#![no_main]
//! TF-T-10: fuzz target for `AppConfig` TOML parsing. Catches panics on
//! malformed input that the validation layer assumes well-formed.

use libfuzzer_sys::fuzz_target;
use sectorforge::config::AppConfig;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = toml::from_str::<AppConfig>(s);
});
