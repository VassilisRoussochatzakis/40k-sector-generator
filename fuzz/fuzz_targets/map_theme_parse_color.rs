#![no_main]
//! TF-T-10: fuzz target for map-theme parsing. We fuzz
//! `parse_map_theme_file` (the public TOML entry point) which internally
//! calls `parse_color` on every colour string — so this also covers the
//! parse_color path the fix-doc named without needing it to be `pub`.

use libfuzzer_sys::fuzz_target;
use sectorforge::map_theme::parse_map_theme_file;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = parse_map_theme_file(s);
});
