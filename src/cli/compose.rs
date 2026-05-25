//! `compose` — multi-sector segmentum composition.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{log_progress, log_segmentum_progress, print_json};

pub fn run_compose(
    segmentum_path: &Utf8PathBuf,
    out: &Utf8PathBuf,
    stitch_seed: Option<String>,
    json: bool,
) -> Result<ExitCode, sectorforge::SectorError> {
    log_progress(format_args!("segmentum: loading config {segmentum_path}"));
    let mut file = sectorforge::load_segmentum_file(segmentum_path)?;
    if let Some(s) = stitch_seed {
        file.segmentum.stitch_seed = s;
    }
    let base_dir = segmentum_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Utf8PathBuf::from("."));
    log_progress(format_args!(
        "segmentum: composing '{}' ({} child sector(s))",
        file.segmentum.id,
        file.children.len()
    ));
    let seg = sectorforge::compose_segmentum_with_progress(
        &file,
        &base_dir,
        out,
        log_segmentum_progress,
    )?;
    if json {
        log_progress("segmentum: emitting JSON to stdout");
        print_json(&seg)?;
    } else {
        log_progress(format_args!("segmentum: writing reports to {out}"));
        sectorforge::write_segmentum(out, &seg)?;
        println!(
            "Composed segmentum '{}' — {} children, {} inter-sector links, {} systems",
            seg.id,
            seg.children.len(),
            seg.inter_sector_links.len(),
            seg.manifest.system_count
        );
        println!("Output written to: {out}");
    }
    Ok(ExitCode::SUCCESS)
}
