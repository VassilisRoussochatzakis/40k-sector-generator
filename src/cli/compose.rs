//! `compose` — multi-sector segmentum composition.

use std::process::ExitCode;

use camino::Utf8PathBuf;

use super::common::{log_progress, log_segmentum_progress, print_json};

#[derive(Debug, clap::Args)]
pub(super) struct ComposeArgs {
    /// Path to `segmentum.toml`.
    #[arg(long)]
    pub segmentum: Utf8PathBuf,
    /// Output directory for `segmentum.md`, `segmentum.json`,
    /// `super_manifest.json`, and per-child sector subdirectories.
    #[arg(long)]
    pub out: Utf8PathBuf,
    /// Override the stitch seed from the segmentum file.
    #[arg(long)]
    pub stitch_seed: Option<String>,
    /// Emit JSON to stdout instead of writing files.
    #[arg(long)]
    pub json: bool,
}

pub(crate) fn run_compose(args: ComposeArgs) -> Result<ExitCode, sectorforge::SectorError> {
    let ComposeArgs {
        segmentum: segmentum_path,
        out,
        stitch_seed,
        json,
    } = args;
    let segmentum_path = &segmentum_path;
    let out = &out;

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
