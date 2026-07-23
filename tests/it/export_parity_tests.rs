//! Disk-I/O parity for the export writers: the in-memory render must equal the
//! bytes written to disk, byte-for-byte.
//!
//! INVARIANTS exercised here:
//! - Output writers are byte-stable: same sector + cfg ⇒ identical bytes.
//! - No FxHashMap/FxHashSet iteration is asserted on; equality is over full
//!   serialised byte streams (HTML strings / PNG byte buffers), which the
//!   producers already emit in deterministic (BTreeMap / grid) order.

use sectorforge::config::HtmlConfig;

use crate::shared::fixture_sector;

// GAP 145: `write_html` returns `output_dir/"sector.html"` and writes exactly the
// `render_html` bytes; `write_html_to` is deterministic across paths.
#[test]
fn write_html_matches_render_html_and_is_path_stable() {
    let sector = fixture_sector();
    let cfg = HtmlConfig::default();

    let dir = tempfile::tempdir().unwrap();
    let out_dir = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    // write_html returns the path it wrote.
    let p = sectorforge::html_export::write_html(sector, &out_dir, &cfg).unwrap();
    assert_eq!(
        p,
        out_dir.join("sector.html"),
        "write_html must write sector.html under output_dir"
    );

    // On-disk bytes == in-memory render.
    let on_disk = std::fs::read_to_string(p.as_std_path()).unwrap();
    let in_mem = sectorforge::html_export::render_html(sector, &cfg).unwrap();
    assert_eq!(on_disk, in_mem, "file bytes must equal render_html output");

    // write_html_to twice (to two distinct paths) yields identical bytes.
    let p1 = out_dir.join("a.html");
    let p2 = out_dir.join("b.html");
    sectorforge::html_export::write_html_to(sector, &p1, &cfg).unwrap();
    sectorforge::html_export::write_html_to(sector, &p2, &cfg).unwrap();
    assert_eq!(
        std::fs::read(p1.as_std_path()).unwrap(),
        std::fs::read(p2.as_std_path()).unwrap(),
        "two write_html_to calls must produce byte-identical files"
    );
}

// GAP 146: `encode_png_bytes` (in-memory) vs `write_sector_png_to_with` (disk).
// Both use the same fast/no-filter PNG encoder, so for the same image
// (rendered with the same scale + `RenderOptions::default()`) the byte
// streams are identical.
#[test]
fn in_memory_png_equals_on_disk_png() {
    let sector = fixture_sector();
    let scale = 2u32;

    // In-memory: render then encode with default options (what the disk writer
    // uses internally).
    let img = sectorforge::bitmap::render_sector_image(
        sector,
        scale,
        None,
        sectorforge::bitmap::RenderOptions::default(),
    );
    let mem_png = sectorforge::bitmap::encode_png_bytes(&img).unwrap();

    // On-disk: write_sector_png_to_with, same scale + default RenderOptions.
    let dir = tempfile::tempdir().unwrap();
    let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("s.png")).unwrap();
    sectorforge::bitmap::write_sector_png_to_with(
        sector,
        &path,
        scale,
        None,
        sectorforge::bitmap::RenderOptions::default(),
    )
    .unwrap();
    let disk_png = std::fs::read(path.as_std_path()).unwrap();

    assert_eq!(
        blake3::hash(&mem_png),
        blake3::hash(&disk_png),
        "in-memory PNG must equal the on-disk PNG for the same image"
    );
    // Stronger: exact byte equality.
    assert_eq!(mem_png, disk_png, "PNG byte streams must be identical");
}
