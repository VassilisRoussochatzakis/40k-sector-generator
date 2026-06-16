//! #28: pinned byte-stability goldens for four export writers that previously
//! lacked them — `heatmap`, `system_map`, `html_export`, and `segmentum`.
//!
//! These mirror the self-blessing pattern already used by `golden_png.rs` /
//! `svg_export_tests.rs` (blake3 hash committed under `tests/goldens/`) and
//! `golden_generation.rs` (full committed file). On first run under the
//! per-writer `UPDATE_GOLDEN_*` env var the golden is (re)written; thereafter
//! the test asserts the bytes have not drifted.
//!
//! INVARIANT (CLAUDE.md "Output writers must be byte-stable"): each writer below
//! is rendered from a single fixed-seed sector (the shared m42 fixture, or the
//! segmentum example children) and must produce byte-identical output run to
//! run. None of these iterate an `FxHashMap`/`FxHashSet` for emission — the
//! producers already emit in deterministic (grid / BTreeMap) order.

use std::path::PathBuf;

use camino::Utf8PathBuf;

use crate::shared::fixture_sector;

fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// Self-blessing blake3 pin (mirrors `golden_png.rs` /
/// `svg_export_tests.rs`). With `env` set, (re)writes `file_name` and returns;
/// otherwise asserts the freshly-computed hash matches the committed one.
fn assert_blake3_golden(file_name: &str, env: &str, bytes: &[u8]) {
    let hash = blake3::hash(bytes).to_hex().to_string();
    let pin = goldens_dir().join(file_name);
    if std::env::var_os(env).is_some() {
        std::fs::create_dir_all(pin.parent().unwrap()).unwrap();
        std::fs::write(&pin, format!("{hash}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&pin).unwrap_or_else(|_| {
        panic!("missing pinned hash {file_name}; bless with `{env}=1 cargo test --test it -- export_byte_goldens`")
    });
    assert_eq!(
        expected.trim(),
        hash,
        "{file_name} drifted from the pinned hash ({} bytes); if intentional, rerun with `{env}=1`",
        bytes.len(),
    );
}

// ── heatmap ──────────────────────────────────────────────────────────────────
//
// The heatmap writer surface has two byte-stable faces, both pinned here:
//   1. the pure-data `score_sector` reduction (serialised JSON), which is the
//      direct output of the `heatmap` module; and
//   2. the heatmap *rendered into a PNG* via the bitmap pipeline (the consumer
//      of `compute_rgb`), which is what actually reaches disk for a heatmap
//      export. `HeatmapMode::Control` is used so the heat branch is live (the
//      default `Off` would make this identical to the plain PNG golden).

#[test]
fn heatmap_score_sector_matches_pinned_blake3_hash() {
    use std::fmt::Write as _;
    let sector = fixture_sector();
    let scores =
        sectorforge::heatmap::score_sector(sector, sectorforge::heatmap::HeatmapMode::Control);
    // `score_sector` walks `sector.systems` in order, so the Vec is already
    // deterministic. `SystemScore` is not `Serialize` (a production type we must
    // not modify in an additive test), so format each row by hand into a stable
    // line-oriented byte string — capturing ordering, the f32 score, and the
    // dominant-faction payload. `{:.6}` pins the float text deterministically.
    let mut blob = String::new();
    for s in &scores {
        let _ = writeln!(
            blob,
            "{}|{:.6}|{}",
            s.system_id,
            s.score,
            s.dominant.as_deref().unwrap_or("-")
        );
    }
    assert_blake3_golden(
        "heatmap_score_m42_control.blake3",
        "UPDATE_GOLDEN_HEATMAP",
        blob.as_bytes(),
    );
}

#[test]
fn heatmap_png_matches_pinned_blake3_hash() {
    let sector = fixture_sector();
    let opts = sectorforge::bitmap::RenderOptions {
        heatmap: sectorforge::heatmap::HeatmapMode::Control,
        ..Default::default()
    };
    let img = sectorforge::bitmap::render_sector_image(sector, 2, None, opts);
    let png = sectorforge::bitmap::encode_png_bytes(&img).unwrap();
    assert_blake3_golden(
        "heatmap_png_m42_control.blake3",
        "UPDATE_GOLDEN_HEATMAP_PNG",
        &png,
    );
}

// ── system_map ───────────────────────────────────────────────────────────────
//
// `render_system` rasterises a single system to RGBA; `encode_png_bytes` is the
// same fast/no-filter encoder the on-disk `write_one_system_png` uses, so the
// hashed bytes equal what would land on disk. Pinned on the first fixture
// system with the default `SystemRenderOptions` (faction_fill on, gm_dark).

#[test]
fn system_map_png_matches_pinned_blake3_hash() {
    let sector = fixture_sector();
    let sys = sector
        .systems
        .first()
        .expect("m42 fixture must place at least one system");
    let img = sectorforge::system_map::render_system(
        sys,
        &sector.factions,
        2,
        sectorforge::system_map::SystemRenderOptions::default(),
    );
    let png = sectorforge::bitmap::encode_png_bytes(&img).unwrap();
    assert_blake3_golden(
        "system_map_png_m42_first.blake3",
        "UPDATE_GOLDEN_SYSTEM_MAP",
        &png,
    );
}

// ── html_export ──────────────────────────────────────────────────────────────
//
// `render_html` is the pure transform behind `write_html`/`write_html_to`
// (which write exactly its bytes — see `export_parity_tests.rs`). With the
// default `HtmlConfig` (`player_observer: None`, no timestamp) it is fully
// determined by the sector, so pinning the byte hash guards the embedded JSON,
// faction palette, and theme CSS against drift.

#[test]
fn html_export_matches_pinned_blake3_hash() {
    let sector = fixture_sector();
    let html =
        sectorforge::html_export::render_html(sector, &sectorforge::config::HtmlConfig::default())
            .expect("render_html");
    assert_blake3_golden(
        "html_m42_default.blake3",
        "UPDATE_GOLDEN_HTML",
        html.as_bytes(),
    );
}

// ── segmentum ────────────────────────────────────────────────────────────────
//
// `compose_segmentum` runs full per-child generation, so — exactly like the
// other composition tests in `segmentum_tests.rs` — this is gated behind
// `#[ignore]` (slow). The serialised `Segmentum` is byte-deterministic for a
// fixed config (proven by `compose_is_byte_deterministic`, which compares two
// runs from *different* output dirs, so the serialised form carries no absolute
// paths). A full committed JSON golden (mirroring `sector_m42_default.json`)
// surfaces the exact field that drifted under `git diff tests/goldens/`.

fn segmentum_fixture_project() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project")
}

fn segmentum_base_file() -> sectorforge::segmentum::SegmentumFile {
    use sectorforge::segmentum::{
        ChildEntry, FactionMode, SegmentumConfig, SegmentumFile, StitchConfig,
    };
    SegmentumFile {
        segmentum: SegmentumConfig {
            id: "seg-golden".into(),
            title: "Golden Segmentum".into(),
            stitch_seed: "stitch-golden".into(),
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
                project: segmentum_fixture_project(),
                column: 0,
                row: 0,
                seed: Some("alpha-seed".into()),
                title: Some("Alpha".into()),
            },
            ChildEntry {
                id: "beta".into(),
                project: segmentum_fixture_project(),
                column: 1,
                row: 0,
                seed: Some("beta-seed".into()),
                title: Some("Beta".into()),
            },
        ],
    }
}

#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it export_byte_goldens -- --ignored`"]
fn segmentum_composition_matches_committed_golden() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let file = segmentum_base_file();
    let base = Utf8PathBuf::from(".");
    let seg = sectorforge::compose_segmentum(&file, &base, &out).expect("compose");
    let json = serde_json::to_string_pretty(&seg).unwrap();

    // Full committed-file golden (mirrors `golden_generation.rs`).
    let path = goldens_dir().join("segmentum_golden.json");
    if std::env::var_os("UPDATE_GOLDEN_SEGMENTUM").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &json).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing segmentum golden; bless with `UPDATE_GOLDEN_SEGMENTUM=1 \
             cargo test --test it segmentum_composition -- --ignored`"
        )
    });
    assert_eq!(
        expected,
        json,
        "segmentum composition drifted from the committed golden ({} vs {} bytes); \
         if intentional, rerun with `UPDATE_GOLDEN_SEGMENTUM=1` and review \
         `git diff tests/goldens/`",
        expected.len(),
        json.len(),
    );
}

// The human-readable Markdown super-map (`segmentum::render_markdown`) is the
// exact byte string `write_segmentum`'s `.md` writer puts on disk, and is fully
// determined by the composed `Segmentum`. It is pinned here as a committed file
// golden (so `git diff tests/goldens/segmentum.md` shows any drift verbatim)
// over the SAME fixed-seed children as the JSON golden above. Same `#[ignore]`
// gating because it shares the slow full-composition step.
#[test]
#[ignore = "slow: full m42 composition; run with `cargo test --test it export_byte_goldens -- --ignored`"]
fn segmentum_markdown_matches_committed_golden() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf()).unwrap();
    let file = segmentum_base_file();
    let base = Utf8PathBuf::from(".");
    let seg = sectorforge::compose_segmentum(&file, &base, &out).expect("compose");
    let md = sectorforge::segmentum::render_markdown(&seg);

    let path = goldens_dir().join("segmentum.md");
    if std::env::var_os("UPDATE_GOLDEN_SEGMENTUM_MD").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &md).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing segmentum markdown golden; bless with `UPDATE_GOLDEN_SEGMENTUM_MD=1 \
             cargo test --test it segmentum_markdown -- --ignored`"
        )
    });
    assert_eq!(
        expected,
        md,
        "segmentum markdown drifted from the committed golden ({} vs {} bytes); \
         if intentional, rerun with `UPDATE_GOLDEN_SEGMENTUM_MD=1` and review \
         `git diff tests/goldens/`",
        expected.len(),
        md.len(),
    );
}
