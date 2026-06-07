//! Smoke test for the SVG exporter against the bundled m42 fixture.

use std::path::PathBuf;

use camino::Utf8PathBuf;

use crate::shared::fixture_sector;

const PIN_ENV: &str = "UPDATE_GOLDEN_SVG";

fn pinned_hash_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/svg_m42_default.blake3")
}

#[test]
fn renders_m42_sector_as_well_formed_svg() {
    let sector = fixture_sector();
    let opts = sectorforge::bitmap::RenderOptions::default();
    let svg = sectorforge::svg_export::render_sector_svg(sector, None, &opts);

    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(
        svg.contains("<polygon"),
        "expected at least one hex polygon"
    );
    assert!(svg.contains("<circle"), "expected at least one star disc");
    assert!(svg.contains("<line"), "expected at least one route line");
    assert!(
        svg.contains("SECTOR:"),
        "expected legend title to be present"
    );
}

#[test]
fn writes_svg_file_to_disk() {
    let sector = fixture_sector();
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("sector.svg")).unwrap();
    sectorforge::svg_export::write_sector_svg_to(sector, &path, None).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.len() > 4096, "expected non-trivial SVG output");
}

// G10: pin the SVG bytes with a committed blake3 golden, mirroring
// `golden_png.rs`. `RenderOptions::default()` has `heatmap: Off`, so the
// heat/heat_tints maps are lookup-only and the writer iterates in deterministic
// grid order — capturing the current bytes is safe. Pass `UPDATE_GOLDEN_SVG=1`
// to (re)bless the pin.
#[test]
fn svg_export_matches_pinned_blake3_hash() {
    let sector = fixture_sector();
    let opts = sectorforge::bitmap::RenderOptions::default();
    let svg = sectorforge::svg_export::render_sector_svg(sector, None, &opts);
    let hash = blake3::hash(svg.as_bytes()).to_hex().to_string();
    let pin = pinned_hash_path();
    if std::env::var_os(PIN_ENV).is_some() {
        std::fs::create_dir_all(pin.parent().unwrap()).unwrap();
        std::fs::write(&pin, format!("{hash}\n")).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&pin).unwrap_or_else(|_| {
        panic!("missing pinned hash; run `{PIN_ENV}=1 cargo test --test it -- svg_export` to bless")
    });
    assert_eq!(
        expected.trim(),
        hash,
        "SVG bytes drifted from pinned hash; if intentional, rerun with `{PIN_ENV}=1` to refresh"
    );
}

// GAP 141: with a non-`Off` heatmap, the renderers consume a `HashMap` of
// per-system heat. The point of this test is to prove that consumption iterates
// the grid deterministically (not in HashMap order): repeated renders must be
// byte-identical, and turning the heatmap on must measurably change the output
// vs. `Off` (so we know the heat branch actually ran).
#[test]
fn heatmap_render_is_byte_stable_and_distinct_from_off() {
    let sector = fixture_sector();

    let opts = sectorforge::bitmap::RenderOptions {
        heatmap: sectorforge::heatmap::HeatmapMode::Control,
        ..Default::default()
    };

    // SVG byte-stability under heatmap.
    let svg1 = sectorforge::svg_export::render_sector_svg(sector, None, &opts);
    let svg2 = sectorforge::svg_export::render_sector_svg(sector, None, &opts);
    assert_eq!(svg1, svg2, "heatmap SVG must be byte-identical across renders");

    // PNG byte-stability under heatmap (blake3 over encoded bytes).
    let img1 = sectorforge::bitmap::render_sector_image(sector, 2, None, opts.clone());
    let img2 = sectorforge::bitmap::render_sector_image(sector, 2, None, opts.clone());
    let png1 = sectorforge::bitmap::encode_png_bytes(&img1).unwrap();
    let png2 = sectorforge::bitmap::encode_png_bytes(&img2).unwrap();
    assert_eq!(
        blake3::hash(&png1),
        blake3::hash(&png2),
        "heatmap PNG must be byte-identical across renders"
    );

    // on != off — proves the Control heat branch had an effect.
    let off = sectorforge::bitmap::RenderOptions::default(); // heatmap: Off
    let svg_off = sectorforge::svg_export::render_sector_svg(sector, None, &off);
    assert_ne!(svg1, svg_off, "Control heatmap must change the SVG vs Off");

    let img_off = sectorforge::bitmap::render_sector_image(sector, 2, None, off);
    let png_off = sectorforge::bitmap::encode_png_bytes(&img_off).unwrap();
    assert_ne!(
        blake3::hash(&png1),
        blake3::hash(&png_off),
        "Control heatmap must change the PNG vs Off"
    );
}

// GAP 148: `render_sector_svg` with a non-empty `&[Subsector]`. The default
// theme (`gm_dark`) has `show_subsector_borders: true` and `label_density: All`,
// so passing `Some(&subs)` activates the subsector border + label branches.
#[test]
fn subsector_branches_render_and_are_stable() {
    let sector = fixture_sector();
    let subs = sectorforge::build_subsectors(
        sector,
        sectorforge::subsectors::SubsectorConfig::default(),
    )
    .expect("build_subsectors");
    assert!(!subs.is_empty(), "fixture should yield >=1 subsector");

    let opts = sectorforge::bitmap::RenderOptions::default();

    let with_subs = sectorforge::svg_export::render_sector_svg(sector, Some(subs.as_slice()), &opts);

    // Structural framing.
    assert!(with_subs.starts_with("<?xml"));
    assert!(with_subs.contains("<svg"));
    assert!(with_subs.trim_end().ends_with("</svg>"));

    // The subsector-label branch uppercases names and prefixes `SUBSECTOR `.
    // If labels happen to be placement-gated off for this fixture, fall back to
    // proving the border branch ran by showing it added markup vs a
    // no-subsector render.
    let without_subs = sectorforge::svg_export::render_sector_svg(sector, None, &opts);
    if !with_subs.contains("SUBSECTOR") {
        assert!(
            with_subs.len() > without_subs.len(),
            "subsector borders should add markup vs a no-subsector render"
        );
    }
    // In all cases the Some(subs) output must differ from None.
    assert_ne!(
        with_subs, without_subs,
        "subsector branch must change the SVG vs None"
    );

    // Byte-identical across re-render (guards subsector-path HashMap order).
    assert_eq!(
        with_subs,
        sectorforge::svg_export::render_sector_svg(sector, Some(subs.as_slice()), &opts),
        "subsector SVG must be byte-stable"
    );
}

// GAP 142 (well-formedness via a real tag-depth walk). NOTE: `quick-xml` is only
// a transitive dependency, not a root `[dev-dependencies]` entry, so a write-only
// test author cannot `use quick_xml::...`. This is the documented fallback: a
// hand-rolled tag-depth counter over the `<...>` tokens. It confirms the tree is
// balanced (every open tag closes, depth never goes negative, returns to 0) and
// — crucially for the escaping contract — that a system name containing `&`
// serialises to a still-balanced document (the `&` is escaped to `&amp;`, never
// breaking a tag boundary).
#[test]
fn svg_is_tag_depth_balanced_including_ampersand_name() {
    // (a) the real m42 fixture.
    let sector = fixture_sector();
    let svg = sectorforge::svg_export::render_sector_svg(
        sector,
        None,
        &sectorforge::bitmap::RenderOptions::default(),
    );
    assert_eq!(tag_depth_balance(&svg), Some(0), "m42 SVG must be balanced");

    // (b) a minimal sector whose single system name carries a raw `&`. After
    // escaping the `<text>` body contains `&amp;`; the document must stay
    // balanced (the escaped entity is not a tag).
    use sectorforge::sector_model::{GeneratedStar, GeneratedSystem, HexCoord, SystemKind};
    let sys = GeneratedSystem {
        id: "s1".into(),
        index: 0,
        name: "A & B".into(),
        coord: HexCoord { q: 1, r: 1 },
        kind: SystemKind::Star,
        star: Some(GeneratedStar {
            colour_code: "O".into(),
            colour_name: "orange dwarf".into(),
            spectral_type: None,
            source_row_index: None,
        }),
        worlds: vec![],
        primary_factions: vec![],
        tags: vec![],
        notes: vec![],
        control: Default::default(),
        stability: Default::default(),
        orbital_assets: Vec::new(),
        blockade: Default::default(),
        conflict: Default::default(),
        intel: Default::default(),
        archetype: Default::default(),
    };
    let small = sectorforge::sector_model::GeneratedSector {
        width: 4,
        height: 4,
        systems: vec![sys],
        ..Default::default()
    };
    let svg2 = sectorforge::svg_export::render_sector_svg(
        &small,
        None,
        &sectorforge::bitmap::RenderOptions::default(),
    );
    assert!(svg2.contains("&amp;"), "ampersand should be escaped to &amp;");
    assert!(!svg2.contains("A & B"), "raw `A & B` must not appear unescaped");
    assert_eq!(
        tag_depth_balance(&svg2),
        Some(0),
        "ampersand-name SVG must remain balanced"
    );
}

/// Hand-rolled XML tag-depth walker (the quick-xml fallback). Scans `<...>`
/// tokens; `<?...?>` declarations and `<tag .../>` self-closers don't change
/// depth, `</tag>` decrements, `<tag>` increments. Returns the final depth
/// (`Some(0)` iff balanced) or `None` if depth ever goes negative.
fn tag_depth_balance(svg: &str) -> Option<i64> {
    let bytes = svg.as_bytes();
    let mut depth: i64 = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Find the matching '>'.
        let Some(rel_end) = svg[i..].find('>') else {
            break;
        };
        let tag = &svg[i..i + rel_end + 1]; // includes '<' and '>'
        if tag.starts_with("<?") || tag.starts_with("<!") {
            // declaration / comment: no depth change
        } else if tag.starts_with("</") {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        } else if tag.ends_with("/>") {
            // self-closing: no depth change
        } else {
            depth += 1;
        }
        i += rel_end + 1;
    }
    Some(depth)
}
