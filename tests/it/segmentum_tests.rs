//! §14 NEW.md — segmentum composition integration tests.

use camino::Utf8PathBuf;
use sectorforge::segmentum::{
    ChildEntry, FactionMode, SegmentumConfig, SegmentumFile, StitchConfig,
};

fn fixture_project() -> Utf8PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Utf8PathBuf::from(manifest_dir).join("examples/m42_project")
}

fn base_file() -> SegmentumFile {
    SegmentumFile {
        segmentum: SegmentumConfig {
            id: "seg-test".into(),
            title: "Test Segmentum".into(),
            stitch_seed: "stitch-test".into(),
            columns: 2,
            rows: 1,
            faction_mode: FactionMode::Shared,
        },
        stitch: StitchConfig {
            max_links_per_pair: 2,
            border_depth: 2,
            ..Default::default()
        },
        children: vec![
            ChildEntry {
                id: "alpha".into(),
                project: fixture_project(),
                column: 0,
                row: 0,
                seed: Some("alpha-seed".into()),
                title: Some("Alpha".into()),
            },
            ChildEntry {
                id: "beta".into(),
                project: fixture_project(),
                column: 1,
                row: 0,
                seed: Some("beta-seed".into()),
                title: Some("Beta".into()),
            },
        ],
    }
}

#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it segmentum -- --ignored`"]
fn compose_produces_children_and_links() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let file = base_file();
    let base = Utf8PathBuf::from(".");
    let seg = sectorforge::compose_segmentum(&file, &base, &out).expect("compose");
    assert_eq!(seg.children.len(), 2);
    assert!(seg.manifest.system_count > 0);
    // East-west adjacency with both children at q-border systems should yield
    // at least one link with the m42 fixture (24 systems on 8x10 grid).
    assert!(!seg.inter_sector_links.is_empty());
    // Per-child outputs exist.
    let alpha_sector = out.join("children/alpha/sector.json");
    assert!(alpha_sector.exists(), "{} not written", alpha_sector);
}

#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it segmentum -- --ignored`"]
fn compose_is_byte_deterministic() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let out_a = Utf8PathBuf::from_path_buf(tmp_a.path().to_path_buf()).unwrap();
    let out_b = Utf8PathBuf::from_path_buf(tmp_b.path().to_path_buf()).unwrap();
    let file = base_file();
    let base = Utf8PathBuf::from(".");
    let a = sectorforge::compose_segmentum(&file, &base, &out_a).unwrap();
    let b = sectorforge::compose_segmentum(&file, &base, &out_b).unwrap();
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "byte-stable composition expected for identical config"
    );
}

#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it segmentum -- --ignored`"]
fn different_stitch_seed_can_change_links() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let out_a = Utf8PathBuf::from_path_buf(tmp_a.path().to_path_buf()).unwrap();
    let out_b = Utf8PathBuf::from_path_buf(tmp_b.path().to_path_buf()).unwrap();
    let mut file_a = base_file();
    let mut file_b = base_file();
    file_a.segmentum.stitch_seed = "stitch-A".into();
    file_b.segmentum.stitch_seed = "stitch-B".into();
    let base = Utf8PathBuf::from(".");
    let a = sectorforge::compose_segmentum(&file_a, &base, &out_a).unwrap();
    let b = sectorforge::compose_segmentum(&file_b, &base, &out_b).unwrap();
    // Children identical (seeds the same), only the stitch derivation differs.
    assert_eq!(a.manifest.child_digests, b.manifest.child_digests);
    // Manifest differs by stitch_seed_hash.
    assert_ne!(a.manifest.stitch_seed_hash, b.manifest.stitch_seed_hash);
}

#[test]
fn duplicate_child_slot_is_rejected() {
    let mut file = base_file();
    file.children[1].column = 0; // collide with alpha
    let tmp = tempfile::tempdir().unwrap();
    let out = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let err = sectorforge::compose_segmentum(&file, &Utf8PathBuf::from("."), &out)
        .expect_err("expected error");
    let msg = format!("{err}");
    assert!(msg.contains("super-grid"), "{msg}");
}

#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it segmentum -- --ignored`"]
fn writers_emit_expected_artifacts() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let file = base_file();
    let base = Utf8PathBuf::from(".");
    let seg = sectorforge::compose_segmentum(&file, &base, &out).unwrap();
    sectorforge::write_segmentum(&out, &seg).unwrap();
    for name in ["segmentum.md", "segmentum.json", "super_manifest.json"] {
        let p = out.join(name);
        assert!(p.exists(), "{} missing", p);
    }
}
