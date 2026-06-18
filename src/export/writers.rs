//! Sector export: JSON, Markdown, manifest. All file-creating code lives here.

use std::fs;
use std::io::{self, BufWriter, Write as _};

use camino::Utf8Path;
use serde::Serialize;

use crate::config::{OutputConfig, OutputFormat};
use crate::errors::SectorError;
use crate::render;
use crate::sector_model::GeneratedSector;

/// How to render JSON. Replaces positional `pretty: bool` arguments so call
/// sites read self-documenting (`JsonFormat::Pretty` instead of `true`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonFormat {
    Pretty,
    Compact,
}

impl JsonFormat {
    fn render<T: Serialize>(self, value: &T) -> serde_json::Result<String> {
        match self {
            JsonFormat::Pretty => serde_json::to_string_pretty(value),
            JsonFormat::Compact => serde_json::to_string(value),
        }
    }

    /// Streaming counterpart of [`JsonFormat::render`]: serialize straight into
    /// `writer` instead of building the whole `String` first. Byte-for-byte
    /// identical output (`to_writer*` and `to_string*` share one serializer) —
    /// used for the large `sector.json` so the write can report live progress.
    fn render_to_writer<W: io::Write, T: Serialize>(
        self,
        writer: W,
        value: &T,
    ) -> serde_json::Result<()> {
        match self {
            JsonFormat::Pretty => serde_json::to_writer_pretty(writer, value),
            JsonFormat::Compact => serde_json::to_writer(writer, value),
        }
    }

    /// Pick a format from an existing `OutputConfig::pretty_json` flag at the
    /// boundary so the bool stays in config-land only.
    pub(crate) fn from_flag(pretty: bool) -> Self {
        if pretty {
            Self::Pretty
        } else {
            Self::Compact
        }
    }
}

/// Shared writer for sub-system reports: create dir, write `<base_name>.md`
/// from `md`, then write `<base_name>.json` from a pretty serialization of
/// `json_payload`. Used by every `<module>::write_report` to eliminate the
/// near-identical 8-line bodies they used to carry.
pub(crate) fn write_md_and_json<T: Serialize>(
    output_dir: &Utf8Path,
    base_name: &str,
    md: &str,
    json_payload: &T,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md_path = output_dir.join(format!("{base_name}.md"));
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join(format!("{base_name}.json"));
    let file = fs::File::create(&json_path).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, json_payload)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    writer
        .flush()
        .map_err(|e| SectorError::io(json_path.as_str(), e))
}

/// Progress events emitted by [`export_all_with_progress`] and the public
/// [`crate::export_sector_with_progress`]. Lets a caller surface the otherwise
/// silent multi-second write of a large `sector.json` (and the other format
/// writers). Purely informational — never part of any generated artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportProgress {
    /// Export began; `formats` format writers will run.
    Started { formats: usize },
    /// A format writer started.
    FormatStarted { format: OutputFormat },
    /// Periodic byte counter while streaming `sector.json` to disk.
    JsonProgress { bytes_written: u64 },
    /// Per-system JSON files, when `write_per_system_files` is enabled.
    PerSystemJson { current: usize, total: usize },
    /// A format writer finished; `bytes` is its primary file size on disk.
    FormatComplete { format: OutputFormat, bytes: u64 },
    /// All format writers finished.
    Complete,
}

/// Emit a [`ExportProgress::JsonProgress`] roughly every this many bytes while
/// streaming `sector.json`. Coarse enough to stay cheap on small sectors, fine
/// enough to animate a 100 MB+ write.
const JSON_PROGRESS_STRIDE: u64 = 8 * 1024 * 1024;

/// Emit a [`ExportProgress::PerSystemJson`] every this many per-system files.
const PER_SYSTEM_PROGRESS_STRIDE: usize = 200;

/// `io::Write` adapter that counts bytes passing through and fires `on_bytes`
/// every `stride` bytes. Wraps the `sector.json` file (under a `BufWriter`) so
/// the streaming serialize reports live progress without buffering the whole
/// document in memory.
struct ProgressWriter<'a, W: io::Write> {
    inner: W,
    written: u64,
    last_reported: u64,
    stride: u64,
    on_bytes: &'a mut dyn FnMut(u64),
}

impl<'a, W: io::Write> ProgressWriter<'a, W> {
    fn new(inner: W, stride: u64, on_bytes: &'a mut dyn FnMut(u64)) -> Self {
        Self {
            inner,
            written: 0,
            last_reported: 0,
            stride,
            on_bytes,
        }
    }
}

impl<W: io::Write> io::Write for ProgressWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.written += n as u64;
        if self.written - self.last_reported >= self.stride {
            self.last_reported = self.written;
            (self.on_bytes)(self.written);
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub fn export_all(
    sector: &GeneratedSector,
    output_config: &OutputConfig,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    export_all_with_progress(sector, output_config, output_dir, &mut |_| {})
}

/// [`export_all`] with a progress callback invoked across the format writers —
/// notably a live byte counter for the large `sector.json`. See
/// [`ExportProgress`].
pub fn export_all_with_progress(
    sector: &GeneratedSector,
    output_config: &OutputConfig,
    output_dir: &Utf8Path,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    on_progress(ExportProgress::Started {
        formats: output_config.formats.len(),
    });

    if output_config.write_manifest {
        let fmt = JsonFormat::from_flag(output_config.pretty_json);
        write_manifest(sector, output_dir, fmt)?;
        write_validation_placeholder(sector, output_dir, fmt)?;
    }

    let bm = &output_config.bitmap;
    let map_theme = crate::map_theme::resolve_map_theme(&bm.theme)
        .map_err(|e| SectorError::InvalidConfig(format!("outputs.bitmap.theme: {e}")))?;
    let mut wrote_image = false;
    for fmt in &output_config.formats {
        on_progress(ExportProgress::FormatStarted { format: *fmt });
        match fmt {
            OutputFormat::Json => write_json(sector, output_dir, output_config, on_progress)?,
            OutputFormat::Markdown => write_markdown(sector, output_dir)?,
            OutputFormat::Bitmap => {
                let opts = crate::bitmap::RenderOptions {
                    faction_fill: bm.faction_fill,
                    heatmap: bm.heatmap,
                    theme: map_theme.clone(),
                    route_view_mode: crate::sector_model::RouteViewMode::default(),
                };
                crate::bitmap::write_bitmap_with(sector, output_dir, bm.sector_scale, None, opts)?;
                wrote_image = true;
            }
            OutputFormat::Svg => {
                let svg_path = output_dir.join("sector.svg");
                crate::svg_export::write_sector_svg_to(sector, &svg_path, None)?;
            }
            OutputFormat::Html => {
                crate::html_export::write_html(sector, output_dir, &output_config.html)?;
            }
        }
        on_progress(ExportProgress::FormatComplete {
            format: *fmt,
            bytes: primary_file_len(*fmt, output_dir),
        });
    }

    // System maps are written whenever any image format is enabled and
    // `bitmap.render_systems` is on.
    if wrote_image && bm.render_systems {
        let sys_opts = crate::system_map::SystemRenderOptions {
            faction_fill: bm.faction_fill,
            theme: map_theme,
        };
        crate::system_map::write_system_maps(sector, output_dir, bm.system_scale, sys_opts)?;
    }

    on_progress(ExportProgress::Complete);
    Ok(())
}

/// Best-effort size of the primary file a format writer produces, for
/// [`ExportProgress::FormatComplete`]. Returns 0 if the file can't be stat'd.
fn primary_file_len(fmt: OutputFormat, output_dir: &Utf8Path) -> u64 {
    let name = match fmt {
        OutputFormat::Json => "sector.json",
        OutputFormat::Markdown => "sector.md",
        OutputFormat::Bitmap => "sector.png",
        OutputFormat::Svg => "sector.svg",
        OutputFormat::Html => "sector.html",
    };
    fs::metadata(output_dir.join(name))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn write_json(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    cfg: &OutputConfig,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), SectorError> {
    let format = JsonFormat::from_flag(cfg.pretty_json);
    write_sector_json_file(sector, output_dir, format, on_progress)?;

    if cfg.write_per_system_files {
        write_per_system_json_files(sector, output_dir, format, on_progress)?;
    } else {
        remove_per_system_json_files(sector, output_dir)?;
    }
    Ok(())
}

fn write_sector_json_file(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), SectorError> {
    let sector_path = output_dir.join("sector.json");
    let file =
        fs::File::create(&sector_path).map_err(|e| SectorError::io(sector_path.as_str(), e))?;
    // Stream the serialize so a 100 MB+ document reports live byte progress
    // instead of building the whole `String` in memory and writing silently.
    let mut on_bytes = |written: u64| {
        on_progress(ExportProgress::JsonProgress {
            bytes_written: written,
        })
    };
    let mut writer = BufWriter::new(ProgressWriter::new(
        file,
        JSON_PROGRESS_STRIDE,
        &mut on_bytes,
    ));
    format
        .render_to_writer(&mut writer, sector)
        .map_err(|e| SectorError::export(sector_path.as_str(), e.to_string()))?;
    writer
        .flush()
        .map_err(|e| SectorError::io(sector_path.as_str(), e))?;
    Ok(())
}

fn write_per_system_json_files(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), SectorError> {
    let systems_dir = output_dir.join("systems");
    fs::create_dir_all(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
    let total = sector.systems.len();
    for (i, sys) in sector.systems.iter().enumerate() {
        let path = systems_dir.join(format!("{}.json", sys.id));
        let text = format
            .render(sys)
            .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
        fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
        let current = i + 1;
        if current == total || current.is_multiple_of(PER_SYSTEM_PROGRESS_STRIDE) {
            on_progress(ExportProgress::PerSystemJson { current, total });
        }
    }
    Ok(())
}

fn remove_per_system_json_files(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
) -> Result<(), SectorError> {
    let systems_dir = output_dir.join("systems");
    for sys in &sector.systems {
        let path = systems_dir.join(format!("{}.json", sys.id));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(SectorError::io(path.as_str(), e)),
        }
    }
    Ok(())
}

fn write_manifest(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    let path = output_dir.join("manifest.json");
    let text = format
        .render(&sector.manifest)
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

fn write_validation_placeholder(
    _sector: &GeneratedSector,
    output_dir: &Utf8Path,
    format: JsonFormat,
) -> Result<(), SectorError> {
    // Validation is run before generation; we just record that it succeeded.
    let path = output_dir.join("validation_report.json");
    let stub = serde_json::json!({
        "ok": true,
        "note": "validation was run successfully before generation",
    });
    let text = format
        .render(&stub)
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

/// Export the canonical `sector.json` to the given directory.
/// Per-system JSON files duplicate `sector.json.systems[]`; use a full
/// [`OutputConfig`] with `write_per_system_files = true` when callers need
/// those convenience files.
pub fn export_json(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    write_sector_json_file(sector, output_dir, JsonFormat::Pretty, &mut |_| {})?;
    remove_per_system_json_files(sector, output_dir)
}

/// Export everything for the sector EXCEPT images: sector JSON,
/// manifest, validation, markdown, plus a recursive copy
/// of the source data folder. Files with image extensions
/// (.png/.bmp/.jpg/.jpeg/.gif/.webp/.tiff/.tif/.svg/.ico) are skipped during
/// the data-folder copy.
///
/// A progress callback is threaded into the (potentially very large)
/// `sector.json` write, so a GUI can animate the otherwise-silent bundle
/// export. See [`ExportProgress`].
pub fn export_bundle_with_progress(
    sector: &GeneratedSector,
    data_dir: Option<&Utf8Path>,
    output_dir: &Utf8Path,
    on_progress: &mut dyn FnMut(ExportProgress),
) -> Result<(), SectorError> {
    let sector_dir = output_dir.join(sanitize_dir_name(&sector.id));
    fs::create_dir_all(&sector_dir).map_err(|e| SectorError::io(sector_dir.as_str(), e))?;

    let out_dir = sector_dir.join("out");
    fs::create_dir_all(&out_dir).map_err(|e| SectorError::io(out_dir.as_str(), e))?;

    // Equivalent to `export_json` but with progress reporting on the big write.
    write_sector_json_file(sector, &out_dir, JsonFormat::Pretty, on_progress)?;
    remove_per_system_json_files(sector, &out_dir)?;
    write_manifest(sector, &out_dir, JsonFormat::Pretty)?;
    write_validation_placeholder(sector, &out_dir, JsonFormat::Pretty)?;
    write_markdown(sector, &out_dir)?;

    if let Some(src) = data_dir {
        let dest = sector_dir.join("data");
        copy_dir_filtered(src, &dest)?;
    }
    Ok(())
}

fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "sector".to_string()
    } else {
        cleaned
    }
}

fn copy_dir_filtered(src: &Utf8Path, dest: &Utf8Path) -> Result<(), SectorError> {
    fs::create_dir_all(dest).map_err(|e| SectorError::io(dest.as_str(), e))?;
    let entries = fs::read_dir(src).map_err(|e| SectorError::io(src.as_str(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| SectorError::io(src.as_str(), e))?;
        let file_name = entry.file_name();
        let name = match file_name.to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let src_path = src.join(&name);
        let dest_path = dest.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|e| SectorError::io(src_path.as_str(), e))?;
        if file_type.is_dir() {
            copy_dir_filtered(&src_path, &dest_path)?;
        } else if file_type.is_file() {
            if is_image_path(&src_path) {
                continue;
            }
            fs::copy(&src_path, &dest_path).map_err(|e| SectorError::io(src_path.as_str(), e))?;
        }
    }
    Ok(())
}

fn is_image_path(path: &Utf8Path) -> bool {
    match path.extension() {
        Some(ext) => matches!(
            ext.to_ascii_lowercase().as_str(),
            "png" | "bmp" | "jpg" | "jpeg" | "gif" | "webp" | "tiff" | "tif" | "svg" | "ico"
        ),
        None => false,
    }
}

// ── Markdown ─────────────────────────────────────────────────────────────────

fn write_markdown(sector: &GeneratedSector, output_dir: &Utf8Path) -> Result<(), SectorError> {
    let text = render::render_sector_markdown(sector);
    let path = output_dir.join("sector.md");
    fs::write(&path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}
