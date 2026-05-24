use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use image::{ImageFormat, Rgba, RgbaImage};
use sectorforge::ids::{route_id, FactionId, RouteId, SystemId, WorldId};
use sectorforge::regions::{RegionConditionKind, WarpRegion};
use sectorforge::sector_model::{
    hex_distance, GeneratedFaction, GeneratedRoute, GeneratedSector, GeneratedStar,
    GeneratedSystem, GeneratedWorld, HexCoord, PowerProfile, RouteStability, RouteType,
    RouteViewMode, SystemKind,
};
use sectorforge::subsectors::{Subsector, SubsectorBounds, SubsectorSummary};
use sectorforge_gui_core::map_theme::MapTheme;
use sectorforge_gui_core::sector_view::{SectorGeom, SectorMapCache, SectorView};

const UPDATE_ENV: &str = "UPDATE_MAP_SNAPSHOTS";
const GOLDEN_DIR: &str = "tests/goldens/map";

#[test]
fn map_snapshots_match_goldens() {
    for case in snapshot_cases() {
        assert_snapshot(&case);
    }
}

struct SnapshotCase {
    name: &'static str,
    sector: GeneratedSector,
    subsectors: Vec<Subsector>,
    hex_size: f32,
    selected_system: Option<SystemId>,
    selected_route: Option<RouteId>,
    path_route_ids: HashSet<RouteId>,
    path_waypoints: HashSet<SystemId>,
    pinned: BTreeSet<SystemId>,
    multi_selected: BTreeSet<SystemId>,
    rect_select: Option<(HexCoord, HexCoord)>,
}

fn snapshot_cases() -> Vec<SnapshotCase> {
    vec![
        SnapshotCase {
            name: "hex_tile",
            sector: GeneratedSector::empty("snap-hex", "Hex Tile Snapshot", "snap", 3, 2),
            subsectors: Vec::new(),
            hex_size: 30.0,
            selected_system: None,
            selected_route: None,
            path_route_ids: HashSet::new(),
            path_waypoints: HashSet::new(),
            pinned: BTreeSet::new(),
            multi_selected: BTreeSet::new(),
            rect_select: None,
        },
        route_segments_case(),
        system_glyphs_case(),
        region_overlays_labels_case(),
        full_fixture_case(),
    ]
}

fn route_segments_case() -> SnapshotCase {
    let mut sector = GeneratedSector::empty("snap-routes", "Route Segment Snapshot", "snap", 13, 3);
    sector
        .factions
        .push(faction("imperium", "Imperium", "imperial"));
    let route_types = [
        RouteType::StableWarpLane,
        RouteType::ChartedPassage,
        RouteType::SecretPassage,
        RouteType::Webway,
        RouteType::BlackShip,
        RouteType::SmugglingLane,
    ];
    let stabilities = [
        RouteStability::Stable,
        RouteStability::Unstable,
        RouteStability::Hazardous,
        RouteStability::Perilous,
        RouteStability::Stable,
        RouteStability::Hazardous,
    ];
    for (idx, route_type) in route_types.into_iter().enumerate() {
        let a = system(
            &format!("r{}a", idx + 1),
            idx * 2,
            HexCoord {
                q: (idx * 2) as i32,
                r: 1,
            },
            SystemKind::Star,
            Some("G"),
            1,
        );
        let b = system(
            &format!("r{}b", idx + 1),
            idx * 2 + 1,
            HexCoord {
                q: (idx * 2 + 1) as i32,
                r: 1,
            },
            SystemKind::Star,
            Some("A"),
            1,
        );
        let from = a.id.clone();
        let to = b.id.clone();
        sector.systems.push(a);
        sector.systems.push(b);
        let mut route = route(from.clone(), to.clone(), route_type, stabilities[idx]);
        if idx < 4 {
            route.controls.push(control("imperium", idx));
        }
        sector.routes.push(route);
    }
    refresh_counts(&mut sector);
    SnapshotCase {
        name: "route_segments",
        sector,
        subsectors: Vec::new(),
        hex_size: 24.0,
        selected_system: None,
        selected_route: None,
        path_route_ids: HashSet::new(),
        path_waypoints: HashSet::new(),
        pinned: BTreeSet::new(),
        multi_selected: BTreeSet::new(),
        rect_select: None,
    }
}

fn system_glyphs_case() -> SnapshotCase {
    let mut sector = GeneratedSector::empty("snap-glyphs", "System Glyph Snapshot", "snap", 6, 3);
    let systems = [
        ("star", "G", SystemKind::Star, 2),
        ("special", "", SystemKind::SpecialLocation, 0),
        ("black-hole", "", SystemKind::BlackHole, 0),
        ("anomaly", "", SystemKind::WarpAnomaly, 0),
        ("station", "", SystemKind::SpaceStation, 1),
    ];
    for (idx, (id, star, kind, worlds)) in systems.into_iter().enumerate() {
        sector.systems.push(system(
            id,
            idx,
            HexCoord {
                q: idx as i32,
                r: 1,
            },
            kind,
            (!star.is_empty()).then_some(star),
            worlds,
        ));
    }
    refresh_counts(&mut sector);
    let subs = vec![subsector(
        "sub-glyph",
        "Glyphs",
        0,
        0,
        0,
        5,
        0,
        2,
        Some("star"),
    )];
    SnapshotCase {
        name: "system_glyphs",
        sector,
        subsectors: subs,
        hex_size: 30.0,
        selected_system: Some(SystemId::new("star")),
        selected_route: None,
        path_route_ids: HashSet::new(),
        path_waypoints: HashSet::from([SystemId::new("station")]),
        pinned: BTreeSet::from([SystemId::new("special")]),
        multi_selected: BTreeSet::from([SystemId::new("anomaly")]),
        rect_select: None,
    }
}

fn region_overlays_labels_case() -> SnapshotCase {
    let mut sector =
        GeneratedSector::empty("snap-regions", "Region Overlay Snapshot", "snap", 8, 4);
    sector.regions = Arc::new(vec![
        region("warp", "Warp Storm", RegionConditionKind::WarpStorm, 0, 0),
        region("turb", "Turbulence", RegionConditionKind::Turbulence, 2, 0),
        region(
            "calm",
            "Calm Corridor",
            RegionConditionKind::CalmCorridor,
            4,
            0,
        ),
        region("black", "Blackout", RegionConditionKind::Blackout, 6, 0),
        region("anom", "Anomaly", RegionConditionKind::Anomaly, 0, 2),
        region(
            "necro",
            "Necropolis Drift",
            RegionConditionKind::NecropolisDrift,
            2,
            2,
        ),
        region(
            "beacon",
            "Beacon Chain",
            RegionConditionKind::BeaconChain,
            4,
            2,
        ),
        region(
            "bleed",
            "Empyric Bleed",
            RegionConditionKind::EmpyricBleed,
            6,
            2,
        ),
    ]);
    SnapshotCase {
        name: "region_overlays_labels",
        sector,
        subsectors: Vec::new(),
        hex_size: 28.0,
        selected_system: None,
        selected_route: None,
        path_route_ids: HashSet::new(),
        path_waypoints: HashSet::new(),
        pinned: BTreeSet::new(),
        multi_selected: BTreeSet::new(),
        rect_select: None,
    }
}

fn full_fixture_case() -> SnapshotCase {
    let mut sector = GeneratedSector::empty("snap-full", "Full Map Snapshot", "snap", 7, 5);
    sector
        .factions
        .push(faction("imperium", "Imperium", "imperial"));
    sector.systems = vec![
        system(
            "alpha",
            0,
            HexCoord { q: 0, r: 1 },
            SystemKind::Star,
            Some("G"),
            2,
        ),
        system(
            "beta",
            1,
            HexCoord { q: 2, r: 1 },
            SystemKind::Star,
            Some("M"),
            1,
        ),
        system(
            "gamma",
            2,
            HexCoord { q: 4, r: 1 },
            SystemKind::Star,
            Some("B"),
            3,
        ),
        system(
            "delta",
            3,
            HexCoord { q: 1, r: 3 },
            SystemKind::SpecialLocation,
            None,
            0,
        ),
        system(
            "epsilon",
            4,
            HexCoord { q: 5, r: 3 },
            SystemKind::SpaceStation,
            None,
            1,
        ),
    ];
    sector.routes = vec![
        route(
            SystemId::new("alpha"),
            SystemId::new("beta"),
            RouteType::StableWarpLane,
            RouteStability::Stable,
        ),
        route(
            SystemId::new("beta"),
            SystemId::new("gamma"),
            RouteType::ChartedPassage,
            RouteStability::Unstable,
        ),
        route(
            SystemId::new("delta"),
            SystemId::new("epsilon"),
            RouteType::Webway,
            RouteStability::Hazardous,
        ),
    ];
    sector.routes[1].controls.push(control("imperium", 0));
    sector.regions = Arc::new(vec![
        region(
            "maelstrom",
            "Maelstrom",
            RegionConditionKind::WarpStorm,
            2,
            0,
        ),
        region(
            "beacons",
            "Beacon Chain",
            RegionConditionKind::BeaconChain,
            4,
            3,
        ),
    ]);
    refresh_counts(&mut sector);
    let mut path_route_ids = HashSet::new();
    path_route_ids.insert(route_id(&SystemId::new("alpha"), &SystemId::new("beta")));
    let selected_route = route_id(&SystemId::new("beta"), &SystemId::new("gamma"));
    SnapshotCase {
        name: "full_fixture_sector",
        sector,
        subsectors: vec![
            subsector("sub-a", "Aurelia", 0, 0, 0, 3, 0, 4, Some("alpha")),
            subsector("sub-b", "Borealis", 1, 0, 1, 6, 0, 4, Some("gamma")),
        ],
        hex_size: 28.0,
        selected_system: Some(SystemId::new("beta")),
        selected_route: Some(selected_route),
        path_route_ids,
        path_waypoints: HashSet::from([SystemId::new("alpha"), SystemId::new("beta")]),
        pinned: BTreeSet::from([SystemId::new("delta")]),
        multi_selected: BTreeSet::from([SystemId::new("epsilon")]),
        rect_select: Some((HexCoord { q: 0, r: 0 }, HexCoord { q: 2, r: 2 })),
    }
}

fn assert_snapshot(case: &SnapshotCase) {
    let rendered = render_case(case);
    let hash = raw_hash(&rendered);
    let golden_png = golden_path(case.name, "png");
    let golden_hash = golden_path(case.name, "blake3");
    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::create_dir_all(golden_png.parent().unwrap()).unwrap();
        rendered.save(&golden_png).unwrap();
        std::fs::write(&golden_hash, format!("{hash}\n")).unwrap();
        return;
    }
    let expected_hash = std::fs::read_to_string(&golden_hash)
        .unwrap_or_else(|_| panic!("missing golden hash for {}", case.name));
    assert_eq!(
        expected_hash.trim(),
        hash,
        "map snapshot {} changed; inspect {} then run `{UPDATE_ENV}=1 cargo test -p sectorforge-gui-core map_snapshots_match_goldens --quiet` to bless",
        case.name,
        write_current(case.name, &rendered).display()
    );
    let expected_png = std::fs::read(&golden_png)
        .unwrap_or_else(|_| panic!("missing golden PNG for {}", case.name));
    let expected = image::load_from_memory_with_format(&expected_png, ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    assert_eq!(
        expected.dimensions(),
        rendered.dimensions(),
        "map snapshot {} dimensions drifted",
        case.name
    );
    assert_eq!(
        raw_hash(&expected),
        hash,
        "map snapshot {} golden PNG does not match committed hash",
        case.name
    );
}

fn render_case(case: &SnapshotCase) -> RgbaImage {
    let geom = SectorGeom::new(case.hex_size, Pos2::ZERO);
    let size = geom.map_size_px(case.sector.width, case.sector.height);
    let width = size.x.ceil() as u32;
    let height = size.y.ceil() as u32;
    let cache = SectorMapCache::new(&case.sector, &case.subsectors);
    let theme = MapTheme::default();
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(width as f32, height as f32),
        )),
        ..Default::default()
    };
    let output = ctx.run(raw, |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(width as f32, height as f32));
                SectorView {
                    sector: &case.sector,
                    selected_system: case.selected_system.as_ref().map(SystemId::as_str),
                    selected_route: case.selected_route.as_ref().map(RouteId::as_str),
                    hex_size: case.hex_size,
                    path_route_ids: (!case.path_route_ids.is_empty())
                        .then_some(&case.path_route_ids),
                    path_waypoints: (!case.path_waypoints.is_empty())
                        .then_some(&case.path_waypoints),
                    subsectors: (!case.subsectors.is_empty()).then_some(case.subsectors.as_slice()),
                    cache: Some(&cache),
                    selected_subsector: None,
                    heatmap: None,
                    empty_hex_clicks: false,
                    route_view_mode: RouteViewMode::Detailed,
                    origin: Pos2::ZERO,
                    multi_selected: (!case.multi_selected.is_empty())
                        .then_some(&case.multi_selected),
                    pinned: (!case.pinned.is_empty()).then_some(&case.pinned),
                    drag_override: None,
                    pending_route_preview: None,
                    rect_select: case.rect_select,
                    sense: Sense::hover(),
                    disable_internal_click_dispatch: true,
                    theme: Some(&theme),
                }
                .show(ui);
            });
    });
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    rasterize(width, height, &primitives)
}

fn rasterize(width: u32, height: u32, primitives: &[egui::ClippedPrimitive]) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    for primitive in primitives {
        let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else {
            continue;
        };
        let clip = primitive.clip_rect;
        for tri in mesh.indices.chunks_exact(3) {
            let v0 = mesh.vertices[tri[0] as usize];
            let v1 = mesh.vertices[tri[1] as usize];
            let v2 = mesh.vertices[tri[2] as usize];
            rasterize_triangle(&mut img, clip, v0, v1, v2);
        }
    }
    img
}

fn rasterize_triangle(
    img: &mut RgbaImage,
    clip: Rect,
    v0: egui::epaint::Vertex,
    v1: egui::epaint::Vertex,
    v2: egui::epaint::Vertex,
) {
    let min_x = v0
        .pos
        .x
        .min(v1.pos.x)
        .min(v2.pos.x)
        .floor()
        .max(clip.min.x)
        .max(0.0) as u32;
    let max_x = v0
        .pos
        .x
        .max(v1.pos.x)
        .max(v2.pos.x)
        .ceil()
        .min(clip.max.x)
        .min(img.width() as f32) as u32;
    let min_y = v0
        .pos
        .y
        .min(v1.pos.y)
        .min(v2.pos.y)
        .floor()
        .max(clip.min.y)
        .max(0.0) as u32;
    let max_y = v0
        .pos
        .y
        .max(v1.pos.y)
        .max(v2.pos.y)
        .ceil()
        .min(clip.max.y)
        .min(img.height() as f32) as u32;
    let denom = edge(v0.pos, v1.pos, v2.pos);
    if denom.abs() <= f32::EPSILON {
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(v1.pos, v2.pos, p) / denom;
            let w1 = edge(v2.pos, v0.pos, p) / denom;
            let w2 = edge(v0.pos, v1.pos, p) / denom;
            if w0 >= -0.001 && w1 >= -0.001 && w2 >= -0.001 {
                let src = mix_color(v0.color, v1.color, v2.color, w0, w1, w2);
                over(img.get_pixel_mut(x, y), src);
            }
        }
    }
}

fn edge(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

fn mix_color(c0: Color32, c1: Color32, c2: Color32, w0: f32, w1: f32, w2: f32) -> [u8; 4] {
    let a0 = c0.to_array();
    let a1 = c1.to_array();
    let a2 = c2.to_array();
    let ch = |idx: usize| {
        (a0[idx] as f32 * w0 + a1[idx] as f32 * w1 + a2[idx] as f32 * w2).clamp(0.0, 255.0) as u8
    };
    [ch(0), ch(1), ch(2), ch(3)]
}

fn over(dst: &mut Rgba<u8>, src: [u8; 4]) {
    let sa = src[3] as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    for idx in 0..3 {
        let sc = src[idx] as f32 / 255.0;
        let dc = dst[idx] as f32 / 255.0;
        dst[idx] = (((sc * sa + dc * da * (1.0 - sa)) / out_a) * 255.0).round() as u8;
    }
    dst[3] = (out_a * 255.0).round() as u8;
}

fn raw_hash(img: &RgbaImage) -> String {
    blake3::hash(img.as_raw()).to_hex().to_string()
}

fn golden_path(name: &str, ext: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(format!("{name}.{ext}"))
}

fn write_current(name: &str, img: &RgbaImage) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/map_snapshots/current")
        .join(format!("{name}.png"));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = img.save(&path);
    path
}

fn faction(id: &str, name: &str, kind: &str) -> GeneratedFaction {
    GeneratedFaction {
        id: FactionId::new(id),
        name: name.into(),
        kind: kind.into(),
        disposition: "lawful".into(),
        subfactions: Vec::new(),
        system_presence: Vec::new(),
        world_presence: Vec::new(),
        power: PowerProfile::default(),
    }
}

fn system(
    id: &str,
    index: usize,
    coord: HexCoord,
    kind: SystemKind,
    star_code: Option<&str>,
    world_count: usize,
) -> GeneratedSystem {
    let mut sys = GeneratedSystem::new_at(SystemId::new(id), index, coord, id);
    sys.kind = kind;
    sys.star = star_code.map(|code| GeneratedStar {
        colour_code: code.into(),
        colour_name: code.into(),
        spectral_type: None,
        source_row_index: None,
    });
    sys.worlds = (0..world_count)
        .map(|idx| GeneratedWorld::new(WorldId::new(format!("{id}-w{}", idx + 1)), idx, "World"))
        .collect();
    sys
}

fn route(
    from_system_id: SystemId,
    to_system_id: SystemId,
    route_type: RouteType,
    stability: RouteStability,
) -> GeneratedRoute {
    let distance = hex_distance(HexCoord { q: 0, r: 0 }, HexCoord { q: 1, r: 0 });
    GeneratedRoute {
        id: route_id(&from_system_id, &to_system_id),
        from_system_id,
        to_system_id,
        distance,
        route_type,
        stability,
        tags: Vec::new(),
        controls: Vec::new(),
    }
}

fn control(faction_id: &str, kind: usize) -> sectorforge::route_control::RouteControl {
    let (patrol, toll, interdiction, piracy) = match kind {
        0 => (80.0, 0.0, 0.0, 0.0),
        1 => (0.0, 80.0, 0.0, 0.0),
        2 => (0.0, 0.0, 80.0, 0.0),
        _ => (0.0, 0.0, 0.0, 80.0),
    };
    sectorforge::route_control::RouteControl {
        faction_id: FactionId::new(faction_id),
        patrol,
        toll,
        interdiction,
        piracy,
        secrecy: 0.0,
        confidence: 100.0,
    }
}

fn region(id: &str, name: &str, kind: RegionConditionKind, q: i32, r: i32) -> WarpRegion {
    let hexes = vec![
        HexCoord { q, r },
        HexCoord { q: q + 1, r },
        HexCoord { q, r: r + 1 },
    ];
    WarpRegion {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        hexes,
        centre: HexCoord { q, r },
    }
}

#[allow(clippy::too_many_arguments)]
fn subsector(
    id: &str,
    name: &str,
    index: u32,
    row: u32,
    col: u32,
    q_max: u32,
    r_min: u32,
    r_max: u32,
    capital: Option<&str>,
) -> Subsector {
    let hex_cells = (r_min..=r_max)
        .flat_map(|r| {
            let q_start = if col == 0 { 0 } else { 4 };
            let q_end = q_max;
            (q_start..=q_end).map(move |q| (q, r))
        })
        .collect();
    Subsector {
        id: id.into(),
        sector_id: "snap".into(),
        label: id.into(),
        name: name.into(),
        index,
        row,
        col,
        bounds: SubsectorBounds {
            q_min: if col == 0 { 0 } else { 4 },
            q_max,
            r_min,
            r_max,
        },
        system_ids: Vec::new(),
        hex_cells,
        route_ids_internal: Vec::new(),
        route_ids_border: Vec::new(),
        neighboring_subsector_ids: Vec::new(),
        connected_subsector_ids: Vec::new(),
        summary: SubsectorSummary {
            subsector_capital_system_id: capital.map(SystemId::new),
            ..Default::default()
        },
        tags: Vec::new(),
        notes: Vec::new(),
    }
}

fn refresh_counts(sector: &mut GeneratedSector) {
    sector.manifest.system_count = sector.systems.len();
    sector.manifest.world_count = sector.systems.iter().map(|s| s.worlds.len()).sum();
    sector.manifest.route_count = sector.routes.len();
}
