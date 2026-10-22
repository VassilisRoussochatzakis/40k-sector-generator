use super::*;
use crate::export::render_core::test_fixtures::sample_sector;
use crate::regions::{RegionConditionKind, WarpRegion};
use crate::sector_model::*;

#[test]
fn renders_well_formed_svg() {
    let sector = sample_sector();
    let svg = render_sector_svg(&sector, None, &RenderOptions::default());
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));
    assert!(svg.contains("polygon"));
}

/// Starless systems must render their kind-specific glyph (matching the live
/// egui renderer), not the retired grey square.
#[test]
fn starless_kinds_use_distinct_glyphs_not_grey_square() {
    let mut sector = sample_sector();
    sector.width = 8;
    sector.height = 4;
    let base = sector.systems[0].clone();
    sector.systems.clear();
    for (i, kind) in [
        SystemKind::SpecialLocation,
        SystemKind::BlackHole,
        SystemKind::WarpAnomaly,
        SystemKind::SpaceStation,
    ]
    .into_iter()
    .enumerate()
    {
        let mut sys = base.clone();
        sys.id = format!("s{i}").into();
        sys.kind = kind;
        sys.star = None;
        sys.coord = HexCoord { q: i as i32, r: 0 };
        sector.systems.push(sys);
    }
    let svg = render_sector_svg(&sector, None, &RenderOptions::default());

    // The black hole draws a solid black disk; nothing else in a star-free,
    // subsector-free map emits pure black.
    assert!(svg.contains("#000000"), "black-hole disk missing from SVG");
    // The retired grey-square marker (rgb 140,140,150) must be gone.
    assert!(
        !svg.contains("8c8c96"),
        "old grey-square glyph still emitted for starless systems"
    );
}

// GAP 140: `escape_xml_into` (private) reached through `render_sector_svg`.
// System labels are uppercased before escaping (`sys.name.to_ascii_uppercase()`),
// then `< > & "` are XML-escaped. So `Ix <Prime> & "Co"` lands in the `<text>`
// body as `IX &lt;PRIME&gt; &amp; &quot;CO&quot;` and the raw `<Prime>` token is
// gone (proving the angle brackets were escaped, not passed through as markup).
#[test]
fn system_label_xml_special_chars_are_escaped() {
    let mut sector = sample_sector();
    sector.systems[0].name = "Ix <Prime> & \"Co\"".into();
    let svg = render_sector_svg(&sector, None, &RenderOptions::default());

    assert!(
        svg.contains("IX &lt;PRIME&gt; &amp; &quot;CO&quot;"),
        "escaped + uppercased system label missing"
    );
    // Raw, unescaped tag-like substrings must be absent.
    assert!(!svg.contains("<Prime>"), "raw `<Prime>` leaked unescaped");
    assert!(!svg.contains("<PRIME>"), "raw `<PRIME>` leaked unescaped");
}

// GAP 140 (parallel region case): warp-region labels follow the same
// uppercase-then-escape path (`short(&region.name.to_ascii_uppercase(), 18)`).
// A region named `R<x>&y` (<=18 chars so `short` doesn't truncate) appears as
// `R&lt;X&gt;&amp;Y`.
#[test]
fn region_label_xml_special_chars_are_escaped() {
    let mut sector = sample_sector();
    let region = WarpRegion {
        id: "reg-0001".into(),
        name: "R<x>&y".into(),
        kind: RegionConditionKind::WarpStorm,
        hexes: vec![HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 }],
        centre: HexCoord { q: 0, r: 0 },
    };
    // `sector.regions` is `Arc<Vec<WarpRegion>>`.
    sector.regions = std::sync::Arc::new(vec![region]);

    let svg = render_sector_svg(&sector, None, &RenderOptions::default());

    assert!(
        svg.contains("R&lt;X&gt;&amp;Y"),
        "escaped + uppercased region label missing"
    );
    assert!(
        !svg.contains("R<x>"),
        "raw `R<x>` region name leaked unescaped"
    );
}

/// Lightweight tag-balance check used by the well-formedness tests: count
/// opening tags (`<name`), closing tags (`</name>`) and self-closing tags
/// (`/>`). For a well-formed document, `opening == closing + self_closing`.
fn tag_balance(svg: &str) -> (usize, usize, usize) {
    let self_closing = svg.matches("/>").count();
    let closing = svg.matches("</").count();
    // Every `<` starts a tag except the XML declaration `<?xml ...?>`. Count
    // raw `<` then subtract the closers (`</`) and the declaration so that only
    // genuine element openings remain.
    let total_lt = svg.matches('<').count();
    let decl = svg.matches("<?").count();
    let opening = total_lt - closing - decl;
    (opening, closing, self_closing)
}

// GAP 140 (well-formedness without quick-xml): escaping keeps the SVG
// structurally balanced and the output is byte-stable across re-renders.
#[test]
fn escaped_svg_is_tag_balanced_and_stable() {
    let mut sector = sample_sector();
    sector.systems[0].name = "Ix <Prime> & \"Co\"".into();
    let opts = RenderOptions::default();

    let svg = render_sector_svg(&sector, None, &opts);

    // Required escape entities present; raw angle-bracket markup absent.
    assert!(svg.contains("&lt;"));
    assert!(svg.contains("&gt;"));
    assert!(svg.contains("&amp;"));
    assert!(svg.contains("&quot;"));
    assert!(!svg.contains("<Prime>"));

    // Structural framing.
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("<svg"));
    assert!(svg.trim_end().ends_with("</svg>"));

    // Opening element tags == closing + self-closing tags.
    let (opening, closing, self_closing) = tag_balance(&svg);
    assert_eq!(
        opening,
        closing + self_closing,
        "tag counts unbalanced: {opening} open vs {closing} close + {self_closing} self-close"
    );

    // Byte-identical on re-render (guards HashMap-iteration nondeterminism).
    assert_eq!(svg, render_sector_svg(&sector, None, &opts));
}
