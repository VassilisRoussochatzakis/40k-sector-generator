//! OPTIMIZE.txt G3 (rust_sectorforge_existing_app_optimization_prompt_v4 §3F,
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
//! A pinned hash is intentionally NOT baked in: any cosmetic theme tweak
//! would force a hash bump and become busywork. The "two independent renders
//! agree" property is the load-bearing one — it would catch the genuine
//! determinism bugs the optimisation spec is concerned about.

use camino::Utf8PathBuf;
use sectorforge::bitmap::{encode_png_bytes, render_sector_image, RenderOptions};

#[test]
fn png_export_is_deterministic_for_fixed_seed() {
    let project_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project");

    // Two independent loads + generations: catches RNG / iteration-order leaks
    // that survive a single in-process run.
    let proj_a = sectorforge::load_project(&project_dir).expect("load A");
    let sector_a = sectorforge::generate_sector(proj_a).expect("generate A");
    let img_a = render_sector_image(&sector_a, 2, None, RenderOptions::default());
    let png_a = encode_png_bytes(&img_a).expect("encode A");

    let proj_b = sectorforge::load_project(&project_dir).expect("load B");
    let sector_b = sectorforge::generate_sector(proj_b).expect("generate B");
    let img_b = render_sector_image(&sector_b, 2, None, RenderOptions::default());
    let png_b = encode_png_bytes(&img_b).expect("encode B");

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
    let project_dir = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project");

    let mut proj_a = sectorforge::load_project(&project_dir).expect("load A");
    let mut proj_b = proj_a.clone();
    proj_a.config.generation.seed = "golden-png-seed-A".into();
    proj_b.config.generation.seed = "golden-png-seed-B".into();

    let sector_a = sectorforge::generate_sector(proj_a).expect("generate A");
    let sector_b = sectorforge::generate_sector(proj_b).expect("generate B");
    let img_a = render_sector_image(&sector_a, 2, None, RenderOptions::default());
    let img_b = render_sector_image(&sector_b, 2, None, RenderOptions::default());
    let png_a = encode_png_bytes(&img_a).expect("encode A");
    let png_b = encode_png_bytes(&img_b).expect("encode B");

    assert_ne!(
        blake3::hash(&png_a),
        blake3::hash(&png_b),
        "different seeds should produce different PNG output"
    );
}
