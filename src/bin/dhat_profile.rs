//! docs/OPTIMIZE.txt G4 (rust_sectorforge_existing_app_optimization_prompt_v4 §5C):
//! heap-allocation profiler driver. Compiled only when the `dhat-heap` feature
//! is enabled so production builds stay allocator-free.
//!
//! Usage:
//! ```text
//! cargo run --release --features dhat-heap --bin dhat-profile -- examples/m42_project
//! ```
//!
//! Output goes to `dhat-heap.json` in the current directory; open with
//! <https://nnethercote.github.io/dh_view/dh_view.html>.

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::env;

use camino::Utf8PathBuf;

fn main() {
    let _profiler = dhat::Profiler::new_heap();

    let project_dir = env::args()
        .nth(1)
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|| Utf8PathBuf::from("examples/m42_project"));

    let project = sectorforge::load_project(&project_dir).expect("load_project");
    let sector = sectorforge::generate_sector(project).expect("generate_sector");

    // Touch downstream consumers so their allocation costs land in the report
    // too — generation alone is only half the story for repeat regenerate
    // cycles in the GUI.
    let img = sectorforge::bitmap::render_sector_image(
        &sector,
        2,
        None,
        sectorforge::bitmap::RenderOptions::default(),
    );
    let png = sectorforge::bitmap::encode_png_bytes(&img).expect("encode_png_bytes");
    let json = serde_json::to_string(&sector).expect("serialise sector");

    eprintln!(
        "dhat-profile: systems={}, png_bytes={}, json_bytes={}",
        sector.systems.len(),
        png.len(),
        json.len(),
    );
}
