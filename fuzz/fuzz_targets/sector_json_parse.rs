#![no_main]
//! #26: fuzz target for `GeneratedSector` JSON deserialization — the richest
//! untested deserialization surface. `serde_json::from_slice` must never panic
//! on arbitrary bytes; malformed input is discarded.

use libfuzzer_sys::fuzz_target;
use sectorforge::GeneratedSector;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<GeneratedSector>(data);
});
