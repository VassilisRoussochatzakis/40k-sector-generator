//! PERF2/§42 — map-redraw CPU cost benchmark.
//!
//! The §42 map-redraw target (60 FPS up to 500 systems + 2000 routes) has no
//! automated FPS gate: frame rate depends on the GPU and the windowing stack,
//! neither of which exists in CI. What *is* measurable — and what decides
//! whether the ~16.6 ms/frame budget is reachable on the CPU side — is the
//! per-frame shape-build + tessellation cost: turning a `GeneratedSector` into
//! the clipped egui primitives the renderer uploads each frame.
//!
//! This bench drives the real `SectorView` shape-build path headlessly through
//! a windowless `egui::Context` — the same seam the gui-core map-snapshot
//! tests use — over the two checked-in scale fixtures (both currently below the
//! 500-system target; a larger above-target fixture is no longer shipped):
//!   * `big_sparse_test`  — 80 systems, sparse routes
//!   * `big_test`         — 200 systems, denser routes
//!
//! The whole sector is framed (`screen_rect == map_size_px`), so nothing is
//! viewport-culled: every system, route and hex is built each iteration — the
//! worst-case full redraw. Subsector chrome is excluded; the PERF2 budget is
//! framed in systems + routes, which dominate the shape count.
//!
//! Run: `cargo bench -p sectorforge-gui-core --bench sector_view_render`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use egui::{Pos2, Rect, Vec2};
use sectorforge::sector_model::RouteViewMode;
use sectorforge::{generate_sector, load_project, GeneratedSector};
use sectorforge_gui_core::map_theme::RenderMapTheme;
use sectorforge_gui_core::sector_view::{SectorGeom, SectorMapCache, SectorView};

/// Hex diameter in px. Only affects canvas size and per-label galley layout,
/// not the shape *count* — the whole map is framed at any size, so no culling.
const HEX_SIZE: f32 = 24.0;

/// Load + generate a checked-in example project (untimed setup). Anchored on
/// the crate manifest dir so the bench is working-directory independent.
fn fixture(name: &str) -> GeneratedSector {
    let dir = format!("{}/../examples/{name}", env!("CARGO_MANIFEST_DIR"));
    let project = load_project(dir).unwrap_or_else(|e| panic!("load {name}: {e}"));
    generate_sector(project).unwrap_or_else(|e| panic!("generate {name}: {e}"))
}

fn bench_redraw(c: &mut Criterion) {
    let mut group = c.benchmark_group("map_redraw");
    for name in ["big_sparse_test", "big_test"] {
        let sector = fixture(name);
        let n_sys = sector.systems.len();
        let n_routes = sector.routes.len();
        let cache = SectorMapCache::new(&sector, &[]);
        let theme = RenderMapTheme::default();
        let geom = SectorGeom::new(HEX_SIZE, Pos2::ZERO);
        let size = geom.map_size_px(sector.width, sector.height);
        let (w, h) = (size.x.ceil(), size.y.ceil());
        // Reuse one Context across iterations — fonts/textures stay cached,
        // matching a real app's steady state rather than a cold first frame.
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
            ..Default::default()
        };

        // One frame: run the SectorView shape build in a windowless context,
        // then tessellate to the clipped primitives the GPU would upload.
        let frame = || {
            let output = ctx.run(raw.clone(), |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::none())
                    .show(ctx, |ui| {
                        ui.set_min_size(Vec2::new(w, h));
                        SectorView {
                            hex_size: HEX_SIZE,
                            cache: Some(&cache),
                            theme: Some(&theme),
                            route_view_mode: RouteViewMode::Detailed,
                            disable_internal_click_dispatch: true,
                            ..SectorView::new(&sector)
                        }
                        .show(ui);
                    });
            });
            ctx.tessellate(output.shapes, output.pixels_per_point)
        };

        // Warmup: the first frame lazily loads the embedded fonts — keep that
        // one-off allocation out of the measured samples.
        let _ = frame();

        group.throughput(Throughput::Elements((n_sys + n_routes) as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{name}_{n_sys}sys_{n_routes}rt")),
            |b| b.iter(|| black_box(frame())),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_redraw);
criterion_main!(benches);
