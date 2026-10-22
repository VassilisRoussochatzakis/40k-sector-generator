//! docs/OPTIMIZE.txt G3 (rust_sectorforge_existing_app_optimization_prompt_v4 §3F,
//! §8 "Image/output anti-patterns"): byte-stable PNG output for a fixed
//! project + seed.
//!
//! Approach: render twice from independent generation runs and compare blake3
//! hashes of the in-memory PNG bytes. This tests two things at once:
//!   1. `generate_sector` is deterministic (same seed ⇒ same sector).
//!   2. The bitmap rasteriser + PNG encoder are deterministic (same sector ⇒
//!      same bytes; no HashMap iteration order leakage, no timestamp in the
//!      PNG, no floating-point nondeterminism in coordinate rounding).
//!
//! TF-T-5 also pins a blake3 hash to disk so unintended drift in the bitmap
//! pipeline trips the test (the run-to-run check alone could not catch a
//! deterministic-but-changed render). Pass `UPDATE_GOLDEN_PNG=1` to refresh.

use sectorforge::bitmap::{encode_png_bytes, render_sector_image, RenderOptions};

use crate::shared::{assert_blake3_golden, fixture_dir};

/// Load the m42 fixture (optionally re-seeded), generate, and encode the 2×
/// bitmap as PNG bytes. Deliberately not memoised: every call is an independent
/// load + generation.
fn m42_png(seed: Option<&str>) -> Vec<u8> {
    let mut proj = sectorforge::load_project(fixture_dir()).expect("load");
    if let Some(seed) = seed {
        proj.config.generation.seed = seed.into();
    }
    let sector = sectorforge::generate_sector(proj).expect("generate");
    let img = render_sector_image(&sector, 2, None, RenderOptions::default());
    encode_png_bytes(&img).expect("encode")
}

#[test]
fn png_export_is_deterministic_for_fixed_seed() {
    // Two independent loads + generations: catches RNG / iteration-order leaks
    // that survive a single in-process run.
    let png_a = m42_png(None);
    let png_b = m42_png(None);

    let hash_a = blake3::hash(&png_a);
    let hash_b = blake3::hash(&png_b);

    assert_eq!(
        hash_a,
        hash_b,
        "PNG export must be byte-stable for same project + seed \
         (len_a = {}, len_b = {}, hash_a = {hash_a}, hash_b = {hash_b})",
        png_a.len(),
        png_b.len()
    );

    // Sanity: hash is over a non-empty buffer.
    assert!(
        png_a.len() > 1024,
        "PNG buffer suspiciously small: {} bytes",
        png_a.len()
    );
}

#[test]
fn png_export_changes_when_seed_changes() {
    let png_a = m42_png(Some("golden-png-seed-A"));
    let png_b = m42_png(Some("golden-png-seed-B"));

    assert_ne!(
        blake3::hash(&png_a),
        blake3::hash(&png_b),
        "different seeds should produce different PNG output"
    );
}

#[test]
fn png_export_matches_pinned_blake3_hash() {
    let png = m42_png(None);
    assert_blake3_golden("png_m42_default.blake3", "UPDATE_GOLDEN_PNG", &png);
}
