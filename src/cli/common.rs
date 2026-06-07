//! Shared CLI helpers: progress logging, output formatting, parsers.

use camino::Utf8PathBuf;
use serde::Serialize;

use sectorforge::config::OutputFormat;
use sectorforge::validation::Severity;
use sectorforge::{SectorProgress, SegmentumProgress};

pub fn print_json<T: Serialize>(value: &T) -> Result<(), sectorforge::SectorError> {
    let text = to_json_pretty(value)?;
    println!("{text}");
    Ok(())
}

pub fn to_json_pretty<T: Serialize>(value: &T) -> Result<String, sectorforge::SectorError> {
    serde_json::to_string_pretty(value).map_err(|e| sectorforge::SectorError::ExportFailed {
        path: "<stdout>".to_string(),
        message: e.to_string(),
    })
}

/// Emit a derived report to one of the three CLI sinks (C3): write a `--out`
/// directory, print JSON to stdout (`--json`), or print rendered Markdown to
/// stdout (the default). Exactly one branch runs.
///
/// `write_dir` performs the directory write **and** prints its own `"Wrote …"`
/// confirmation — the written filenames differ per report, so that line stays
/// with the caller. `render_md` produces the Markdown for the default sink.
pub fn emit_report<R: Serialize>(
    out: Option<&Utf8PathBuf>,
    json: bool,
    report: &R,
    write_dir: impl FnOnce(&Utf8PathBuf) -> Result<(), sectorforge::SectorError>,
    render_md: impl FnOnce() -> String,
) -> Result<(), sectorforge::SectorError> {
    if let Some(dir) = out {
        write_dir(dir)?;
    } else if json {
        print_json(report)?;
    } else {
        print!("{}", render_md());
    }
    Ok(())
}

pub fn print_validation_report(report: &sectorforge::ValidationReport) {
    println!("Validation: {}", if report.ok { "OK" } else { "FAILED" });
    println!(
        "  Workbook rows:        {}",
        report.world_workbook.row_count
    );
    println!(
        "  Usable candidates:    {}",
        report.world_workbook.usable_candidate_count
    );
    println!(
        "  Excluded rows:        {}",
        report.world_workbook.excluded_row_count
    );
    if !report.world_workbook.exclusion_reasons.is_empty() {
        for (reason, count) in &report.world_workbook.exclusion_reasons {
            println!("    - {reason}: {count}");
        }
    }
    println!("  Errors:               {}", report.errors.len());
    println!("  Warnings:             {}", report.warnings.len());
    for issue in &report.errors {
        print_issue(issue);
    }
    for issue in &report.warnings {
        print_issue(issue);
    }
}

fn print_issue(issue: &sectorforge::ValidationIssue) {
    match &issue.path {
        Some(path) => println!(
            "  [{}] {}: {} ({})",
            severity_tag(issue.severity),
            issue.code,
            issue.message,
            path
        ),
        None => println!(
            "  [{}] {}: {}",
            severity_tag(issue.severity),
            issue.code,
            issue.message
        ),
    }
}

pub fn print_invariant_report(report: &sectorforge::InvariantReport) {
    println!(
        "Sector invariants: {}",
        if report.ok { "OK" } else { "FAILED" }
    );
    println!("  Violations: {}", report.violations.len());
    for v in &report.violations {
        match &v.path {
            Some(path) => println!("  [{}] {} ({})", v.code, v.message, path),
            None => println!("  [{}] {}", v.code, v.message),
        }
    }
}

pub fn print_workbook_stats(stats: &sectorforge::world_pool::WorkbookStats) {
    println!("World data dir: {}", stats.data_dir);
    println!("Key tables:");
    for (name, n) in &stats.key_table_counts {
        println!("  {name:<18} {n}");
    }
    println!("Generator rows:    {}", stats.generator_rows);
    println!("Usable candidates: {}", stats.usable_candidates);
    println!("Excluded rows:     {}", stats.excluded_rows);
    println!("\nTop star colours by total weight:");
    for (n, w) in &stats.top_star_colours {
        println!("  {n:<14} {w:.2}");
    }
    println!("\nTop world types by total weight:");
    for (n, w) in &stats.top_world_types {
        println!("  {n:<22} {w:.2}");
    }
    println!("\nTop notable features by row count:");
    for (n, c) in &stats.top_features {
        println!("  {n:<28} {c}");
    }
}

fn severity_tag(s: Severity) -> &'static str {
    match s {
        Severity::Error => "ERROR",
        Severity::Warning => "WARN",
        Severity::Info => "INFO",
        _ => "UNKNOWN",
    }
}

pub fn parse_heatmap(
    s: &str,
) -> Result<sectorforge::heatmap::HeatmapMode, sectorforge::SectorError> {
    use sectorforge::heatmap::HeatmapMode;
    match s.to_ascii_lowercase().as_str() {
        "off" => Ok(HeatmapMode::Off),
        "control" => Ok(HeatmapMode::Control),
        "military" => Ok(HeatmapMode::Military),
        "trade" => Ok(HeatmapMode::Trade),
        "industrial" | "industry" => Ok(HeatmapMode::Industrial),
        "covert" => Ok(HeatmapMode::Covert),
        "faith" => Ok(HeatmapMode::Faith),
        "threat" => Ok(HeatmapMode::Threat),
        "intel" => Ok(HeatmapMode::Intel),
        "tension" => Ok(HeatmapMode::Tension),
        "trade_volume" | "trade-volume" | "tradevol" => Ok(HeatmapMode::TradeVolume),
        "food" | "food_output" | "food-output" => Ok(HeatmapMode::FoodOutput),
        "tithe" | "tithe_stress" | "tithe-stress" => Ok(HeatmapMode::TitheStress),
        "supply" | "supply_vulnerability" | "supply-vulnerability" => {
            Ok(HeatmapMode::SupplyVulnerability)
        }
        other => Err(sectorforge::SectorError::InvalidConfig(format!(
            "unknown heatmap mode '{other}' (expected off|control|military|trade|industrial|covert|faith|threat|intel|tension|trade_volume|food|tithe|supply)"
        ))),
    }
}

/// Render-only export artifacts. Unlike `Json`, none of these is ever read
/// back in: HTML inlines its own data copy and PNG/SVG/Markdown are terminal.
/// So the `--light` / `--exclude` opt-outs may freely drop them.
const RENDER_FORMATS: [OutputFormat; 4] = [
    OutputFormat::Html,
    OutputFormat::Bitmap,
    OutputFormat::Svg,
    OutputFormat::Markdown,
];

/// Resolve the effective export-format set from a project's base formats plus
/// the CLI overrides shared by `generate` and `random`:
///
/// * `formats` — when given, *replaces* the base set (positive selection).
/// * `light` — drop every render artifact (html/png/svg/markdown), keeping the
///   machine-readable `json`.
/// * `exclude` — remove the listed formats from whatever remains.
///
/// `json` is load-bearing: the viewer (`viewer/src/main.rs`) and segmentum
/// compose load `out/sector.json`, so it can never be excluded — passing it to
/// `--exclude` is a hard error rather than a silent viewer break.
pub fn resolve_formats(
    base: Vec<OutputFormat>,
    formats: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    light: bool,
) -> Result<Vec<OutputFormat>, sectorforge::SectorError> {
    let mut set = match formats {
        Some(tokens) => parse_format_tokens(&tokens, "--formats")?,
        None => base,
    };

    if light {
        set.retain(|f| !RENDER_FORMATS.contains(f));
    }

    if let Some(tokens) = exclude {
        let drop = parse_format_tokens(&tokens, "--exclude")?;
        if drop.contains(&OutputFormat::Json) {
            return Err(sectorforge::SectorError::InvalidConfig(
                "json cannot be excluded: the viewer and segmentum compose load out/sector.json. \
                 Drop html, png, svg, or markdown instead (or use --light)."
                    .into(),
            ));
        }
        set.retain(|f| !drop.contains(f));
    }

    if set.is_empty() {
        return Err(sectorforge::SectorError::InvalidConfig(
            "no output formats remain after --formats/--light/--exclude".into(),
        ));
    }
    Ok(set)
}

/// Parse + dedup a list of CLI format tokens, naming the originating flag in any
/// error. Shared by `--formats` and `--exclude`.
fn parse_format_tokens(
    tokens: &[String],
    flag: &str,
) -> Result<Vec<OutputFormat>, sectorforge::SectorError> {
    let mut out: Vec<OutputFormat> = Vec::with_capacity(tokens.len());
    for token in tokens {
        match OutputFormat::parse_token(token) {
            Some(f) if !out.contains(&f) => out.push(f),
            Some(_) => {}
            None => {
                return Err(sectorforge::SectorError::InvalidConfig(format!(
                    "unknown {flag} token '{token}' (expected json|markdown|png|svg|html)"
                )))
            }
        }
    }
    Ok(out)
}

pub fn load_or_regenerate(
    project: Option<Utf8PathBuf>,
    sector: Option<Utf8PathBuf>,
) -> Result<sectorforge::GeneratedSector, sectorforge::SectorError> {
    match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(&project)?;
            sectorforge::generate_sector(input)
        }
        (None, Some(sector)) => sectorforge::load_sector_json(&sector),
        (Some(_), Some(_)) | (None, None) => Err(sectorforge::SectorError::InvalidConfig(
            "pass exactly one of --project <dir> or --sector <path>".into(),
        )),
    }
}

/// Resolve a derived-report runner's sector plus its per-runner config `C` from
/// exactly one of `--project` / `--sector` (C2 / C-S1). On `--project`,
/// `extract` pulls the runner's config out of the loaded `ProjectInput`
/// **before** `generate_sector` consumes it; on `--sector` there is no project
/// config, so `C::default()` is used. Centralises the "pass exactly one of …"
/// error that was inlined across six runners. (`load_or_regenerate` above is the
/// no-config variant for runners that need only the sector.)
pub fn resolve_sector_with_cfg<C: Default>(
    project: Option<&Utf8PathBuf>,
    sector: Option<&Utf8PathBuf>,
    extract: impl FnOnce(&sectorforge::ProjectInput) -> C,
) -> Result<(sectorforge::GeneratedSector, C), sectorforge::SectorError> {
    match (project, sector) {
        (Some(project), None) => {
            let input = sectorforge::load_project(project)?;
            let cfg = extract(&input);
            let sec = sectorforge::generate_sector(input)?;
            Ok((sec, cfg))
        }
        (None, Some(sector)) => Ok((sectorforge::load_sector_json(sector)?, C::default())),
        (Some(_), Some(_)) | (None, None) => Err(sectorforge::SectorError::InvalidConfig(
            "pass exactly one of --project <dir> or --sector <path>".into(),
        )),
    }
}

pub fn log_progress(message: impl std::fmt::Display) {
    eprintln!("[sectorforge] {message}");
}

/// Render export progress to stderr. The `JsonProgress` ticks animate the
/// otherwise-silent multi-second write of a large `sector.json`.
pub fn log_export_progress(event: sectorforge::ExportProgress) {
    use sectorforge::ExportProgress as E;
    match event {
        E::Started { formats } => {
            log_progress(format_args!("export: writing {formats} format(s)"));
        }
        E::FormatStarted { format } => log_progress(format_args!("export: writing {format}…")),
        E::JsonProgress { bytes_written } => log_progress(format_args!(
            "export: sector.json {} written…",
            human_bytes(bytes_written)
        )),
        E::PerSystemJson { current, total } => {
            log_progress(format_args!("export: per-system json {current}/{total}"));
        }
        E::FormatComplete { format, bytes } => {
            log_progress(format_args!(
                "export: wrote {format} ({})",
                human_bytes(bytes)
            ));
        }
        E::Complete => log_progress("export: all formats written"),
        _ => {}
    }
}

/// Human-readable byte size (binary units) for progress lines.
fn human_bytes(n: u64) -> String {
    const UNIT: f64 = 1024.0;
    let b = n as f64;
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", b / UNIT / UNIT / UNIT)
    } else if n >= 1024 * 1024 {
        format!("{:.1} MiB", b / UNIT / UNIT)
    } else if n >= 1024 {
        format!("{:.1} KiB", b / UNIT)
    } else {
        format!("{n} B")
    }
}

pub fn log_sector_progress(event: SectorProgress) {
    log_sector_progress_with_prefix("sector", event);
}

pub fn log_segmentum_progress(event: SegmentumProgress) {
    match event {
        SegmentumProgress::Started {
            children,
            output_dir,
        } => log_progress(format_args!(
            "segmentum: started ({children} child sector(s), output {output_dir})"
        )),
        SegmentumProgress::ChildLoading {
            index,
            total,
            id,
            project,
        } => log_progress(format_args!(
            "segmentum: child {index}/{total} {id}: loading {project}"
        )),
        SegmentumProgress::ChildValidated {
            index,
            total,
            id,
            warnings,
        } => log_progress(format_args!(
            "segmentum: child {index}/{total} {id}: validation OK ({warnings} warning(s))"
        )),
        SegmentumProgress::ChildGenerating { index, total, id } => log_progress(format_args!(
            "segmentum: child {index}/{total} {id}: generating sector"
        )),
        SegmentumProgress::ChildSectorProgress {
            index,
            total,
            id,
            event,
        } => {
            let prefix = format!("segmentum: child {index}/{total} {id}");
            log_sector_progress_with_prefix(&prefix, event);
        }
        SegmentumProgress::ChildInvariantsChecked { index, total, id } => log_progress(
            format_args!("segmentum: child {index}/{total} {id}: invariants OK"),
        ),
        SegmentumProgress::ChildExporting {
            index,
            total,
            id,
            output_dir,
        } => log_progress(format_args!(
            "segmentum: child {index}/{total} {id}: exporting to {output_dir}"
        )),
        SegmentumProgress::ChildComplete {
            index,
            total,
            id,
            systems,
            worlds,
            routes,
        } => log_progress(format_args!(
            "segmentum: child {index}/{total} {id}: complete ({systems} systems, {worlds} worlds, {routes} routes)"
        )),
        SegmentumProgress::Stitching { children } => log_progress(format_args!(
            "segmentum: stitching borders across {children} child sector(s)"
        )),
        SegmentumProgress::Complete {
            children,
            links,
            systems,
            worlds,
            routes,
        } => log_progress(format_args!(
            "segmentum: compose complete ({children} children, {links} links, {systems} systems, {worlds} worlds, {routes} routes)"
        )),
        _ => {}
    }
}

fn log_sector_progress_with_prefix(prefix: &str, event: SectorProgress) {
    match event {
        SectorProgress::WorldPoolBuilt {
            rows,
            candidates,
            excluded,
        } => log_progress(format_args!(
            "{prefix}: world pool ready ({candidates} candidates, {excluded} excluded, {rows} source rows)"
        )),
        SectorProgress::SystemsPlaced {
            total,
            width,
            height,
        } => log_progress(format_args!(
            "{prefix}: placed {total} system slot(s) on {width}x{height} grid"
        )),
        SectorProgress::RegionsBuilt { count } => {
            log_progress(format_args!("{prefix}: built {count} warp region(s)"));
        }
        SectorProgress::SystemBuilt {
            current,
            total,
            worlds,
        } if should_log_progress(current, total) => {
            log_progress(format_args!(
                "{prefix}: generated systems {current}/{total} (last {worlds} world(s))"
            ));
        }
        SectorProgress::FactionsAssigned { catalog_rows } => log_progress(format_args!(
            "{prefix}: assigned factions from {catalog_rows} catalog row(s)"
        )),
        SectorProgress::FactionsAggregated { factions } => log_progress(format_args!(
            "{prefix}: aggregated {factions} top-level faction(s)"
        )),
        SectorProgress::StageStarted { name } => {
            log_progress(format_args!("{prefix}: starting {name}"));
        }
        SectorProgress::RoutesGenerated { routes } => {
            log_progress(format_args!("{prefix}: generated {routes} public route(s)"));
        }
        SectorProgress::RegionEffectsStarted {
            regions,
            systems,
            routes,
        } => log_progress(format_args!(
            "{prefix}: region route effects scanning {routes} route(s), {regions} region(s), {systems} system(s)"
        )),
        SectorProgress::RegionEffectsProgress {
            current,
            total,
            affected_routes,
            changed_routes,
            bridge_checks,
            bridges_preserved,
        } if should_log_progress(current, total) => {
            log_progress(format_args!(
                "{prefix}: region route effects scanned {current}/{total} route(s), affected {affected_routes}, changed {changed_routes}, bridge checks {bridge_checks}, preserved {bridges_preserved}"
            ));
        }
        SectorProgress::RegionEffectsBridgeCheckStarted {
            check,
            route_index,
            total_routes,
            route_id,
        } => log_progress(format_args!(
            "{prefix}: region route effects bridge check {check} at route {route_index}/{total_routes} ({route_id})"
        )),
        SectorProgress::RegionEffectsApplied {
            regions,
            affected_routes,
            changed_routes,
            bridge_checks,
            bridges_preserved,
            stable,
            unstable,
            hazardous,
            perilous,
        } => log_progress(format_args!(
            "{prefix}: applied route effects from {regions} warp region(s): affected {affected_routes}, changed {changed_routes}, bridge checks {bridge_checks}, preserved {bridges_preserved}; stability stable={stable}, unstable={unstable}, hazardous={hazardous}, perilous={perilous}"
        )),
        SectorProgress::HiddenRouteLayerStarted { layer, endpoints } => log_progress(
            format_args!("{prefix}: hidden route layer {layer}: {endpoints} endpoint(s)"),
        ),
        SectorProgress::HiddenRouteLayerProgress {
            layer,
            current,
            total,
            pairs,
        } => log_progress(format_args!(
            "{prefix}: hidden route layer {layer}: scanned {current}/{total} endpoint(s), {pairs} candidate pair(s)"
        )),
        SectorProgress::HiddenRouteLayerEmitProgress {
            layer,
            current,
            total,
            added,
        } => log_progress(format_args!(
            "{prefix}: hidden route layer {layer}: emitted {current}/{total} candidate pair(s), {added} route(s) added"
        )),
        SectorProgress::HiddenRouteLayerCompleted {
            layer,
            added,
            routes,
        } => log_progress(format_args!(
            "{prefix}: hidden route layer {layer}: added {added} route(s), total {routes}"
        )),
        SectorProgress::HiddenRoutesApplied { added, routes } => log_progress(format_args!(
            "{prefix}: applied hidden routes (+{added}, total {routes})"
        )),
        SectorProgress::RouteControlsProgress { current, total }
            if should_log_progress(current, total) =>
        {
            log_progress(format_args!(
                "{prefix}: derived route control {current}/{total}"
            ));
        }
        SectorProgress::RouteControlsDerived { routes } => {
            log_progress(format_args!("{prefix}: derived control for {routes} route(s)"));
        }
        SectorProgress::SystemStateDerived { current, total }
            if should_log_progress(current, total) =>
        {
            log_progress(format_args!("{prefix}: derived system state {current}/{total}"));
        }
        SectorProgress::ManifestBuilt {
            systems,
            worlds,
            routes,
        } => log_progress(format_args!(
            "{prefix}: manifest ready ({systems} systems, {worlds} worlds, {routes} routes)"
        )),
        SectorProgress::InfluenceFieldStarted {
            systems,
            anchors,
            cells,
            radius,
        } => log_progress(format_args!(
            "{prefix}: influence field started ({systems} systems, {anchors} anchor(s), {cells} cell(s), radius {radius})"
        )),
        SectorProgress::InfluenceFieldAnchorsProjected {
            current,
            total,
            touched_cells,
        } => log_progress(format_args!(
            "{prefix}: influence field projected anchors {current}/{total}, touched {touched_cells} cell(s)"
        )),
        SectorProgress::InfluenceFieldCellsResolved {
            current,
            total,
            claimed_cells,
        } => log_progress(format_args!(
            "{prefix}: influence field resolved cells {current}/{total}, claimed {claimed_cells} cell(s)"
        )),
        SectorProgress::InfluenceFieldBandsBuilt {
            bands,
            claimed_cells,
        } => log_progress(format_args!(
            "{prefix}: influence field bands ready ({bands} band(s), {claimed_cells} claimed cell(s))"
        )),
        SectorProgress::InfluenceFieldComplete { cells, bands } => log_progress(format_args!(
            "{prefix}: influence field complete ({cells} cell(s), {bands} band(s))"
        )),
        SectorProgress::OverlayDerived { name } => {
            log_progress(format_args!("{prefix}: derived {name} overlay"));
        }
        SectorProgress::ChronicleStarted {
            systems,
            worlds,
            routes,
            max_subsector_events,
        } => log_progress(format_args!(
            "{prefix}: chronicle started ({systems} systems, {worlds} worlds, {routes} routes, max {max_subsector_events} subsector event(s))"
        )),
        SectorProgress::ChronicleSubsectorEventsStarted {
            exact_cluster_count,
            emitted_cap,
            sampled,
        } => {
            let mode = if sampled { "sampled" } else { "exact" };
            log_progress(format_args!(
                "{prefix}: chronicle subsector events {mode} (exact clusters {exact_cluster_count}, cap {emitted_cap})"
            ));
        }
        SectorProgress::ChronicleSubsectorEventsDone { events } => log_progress(format_args!(
            "{prefix}: chronicle subsector events ready ({events} event(s))"
        )),
        SectorProgress::ChronicleSystemsScanned {
            current,
            total,
            events,
        } if should_log_progress(current, total) => {
            log_progress(format_args!(
                "{prefix}: chronicle scanned systems {current}/{total} ({events} event(s))"
            ));
        }
        SectorProgress::ChronicleRoutesScanned {
            current,
            total,
            events,
        } if should_log_progress(current, total) => {
            log_progress(format_args!(
                "{prefix}: chronicle scanned routes {current}/{total} ({events} event(s))"
            ));
        }
        SectorProgress::ChronicleEventRulesApplied { events } => log_progress(format_args!(
            "{prefix}: chronicle event rules applied ({events} event(s))"
        )),
        SectorProgress::ChronicleSortingStarted { events } => log_progress(format_args!(
            "{prefix}: chronicle sorting {events} event(s)"
        )),
        SectorProgress::ChronicleComplete { events } => log_progress(format_args!(
            "{prefix}: chronicle complete ({events} event(s))"
        )),
        SectorProgress::Complete {
            systems,
            worlds,
            routes,
        } => log_progress(format_args!(
            "{prefix}: generation complete ({systems} systems, {worlds} worlds, {routes} routes)"
        )),
        // docs/OPTIMIZE.txt G7: stage-timing events are CLI-logged at a quieter
        // level so they don't drown out structural progress lines.
        SectorProgress::StageElapsed { stage, millis } => {
            log_progress(format_args!("{prefix}: stage `{stage}` took {millis} ms"));
        }
        _ => {}
    }
}

fn should_log_progress(current: usize, total: usize) -> bool {
    current == 1 || current == total || current.is_multiple_of(progress_stride(total))
}

fn progress_stride(total: usize) -> usize {
    match total {
        0..=25 => 1,
        26..=100 => 10,
        101..=500 => 25,
        _ => 100,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::heatmap::HeatmapMode;
    use sectorforge::SectorError;

    // ── resolve_formats (Gap 2) ────────────────────────────────────────────

    /// `--exclude json` is a hard error — json can never be dropped because the
    /// viewer and segmentum compose load `out/sector.json`.
    #[test]
    fn resolve_formats_rejects_excluding_json() {
        let err = resolve_formats(
            vec![OutputFormat::Json, OutputFormat::Svg],
            None,
            Some(vec!["json".into()]),
            false,
        )
        .unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("json cannot be excluded")),
            "expected json-exclusion error"
        );
    }

    /// An empty effective set (here `--formats` replaces the base with nothing)
    /// is an error rather than a silent no-op export.
    #[test]
    fn resolve_formats_empty_result_errors() {
        let err = resolve_formats(vec![OutputFormat::Json], Some(vec![]), None, false).unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("no output formats remain")),
            "expected empty-set error"
        );
    }

    /// `--light` drops every render artifact, keeping only the machine-readable
    /// `json` (the only non-render format).
    #[test]
    fn resolve_formats_light_keeps_only_json() {
        let out = resolve_formats(
            vec![
                OutputFormat::Json,
                OutputFormat::Bitmap,
                OutputFormat::Svg,
                OutputFormat::Html,
                OutputFormat::Markdown,
            ],
            None,
            None,
            true,
        )
        .unwrap();
        assert_eq!(out, vec![OutputFormat::Json]);
    }

    /// The `png` token aliases to the `Bitmap` variant (the PNG format is named
    /// `Bitmap`, not `Png`). `--formats` replaces the (empty) base set.
    #[test]
    fn resolve_formats_png_token_maps_to_bitmap() {
        let out = resolve_formats(vec![], Some(vec!["png".into()]), None, false).unwrap();
        assert_eq!(out, vec![OutputFormat::Bitmap]);
    }

    /// Duplicate tokens collapse, preserving first-seen order.
    #[test]
    fn resolve_formats_dedups_preserving_order() {
        let out = resolve_formats(
            vec![],
            Some(vec!["json".into(), "json".into(), "md".into()]),
            None,
            false,
        )
        .unwrap();
        assert_eq!(out, vec![OutputFormat::Json, OutputFormat::Markdown]);
    }

    /// An unknown token names the originating flag (`--formats` / `--exclude`)
    /// and echoes the offending token.
    #[test]
    fn resolve_formats_unknown_token_names_flag() {
        let err =
            resolve_formats(vec![], Some(vec!["bogus".into()]), None, false).unwrap_err();
        match err {
            SectorError::InvalidConfig(m) => {
                assert!(m.contains("--formats"), "message {m:?} missing flag name");
                assert!(m.contains("bogus"), "message {m:?} missing token");
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }

        let err = resolve_formats(
            vec![OutputFormat::Json, OutputFormat::Svg],
            None,
            Some(vec!["bogus".into()]),
            false,
        )
        .unwrap_err();
        match err {
            SectorError::InvalidConfig(m) => {
                assert!(m.contains("--exclude"), "message {m:?} missing flag name");
                assert!(m.contains("bogus"), "message {m:?} missing token");
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    // ── parse_heatmap (Gap 5) ──────────────────────────────────────────────

    #[test]
    fn parse_heatmap_is_case_insensitive() {
        assert_eq!(parse_heatmap("OFF").unwrap(), HeatmapMode::Off);
        assert_eq!(parse_heatmap("Control").unwrap(), HeatmapMode::Control);
    }

    #[test]
    fn parse_heatmap_accepts_aliases() {
        // `industry` is an alias for `industrial`.
        assert_eq!(parse_heatmap("industry").unwrap(), HeatmapMode::Industrial);
        // Both `trade-volume` and `tradevol` resolve to the same variant.
        assert_eq!(
            parse_heatmap("trade-volume").unwrap(),
            HeatmapMode::TradeVolume
        );
        assert_eq!(parse_heatmap("tradevol").unwrap(), HeatmapMode::TradeVolume);
    }

    #[test]
    fn parse_heatmap_unknown_token_errors() {
        let err = parse_heatmap("nonsense").unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m)
                if m.contains("nonsense") && m.contains("unknown heatmap mode")),
            "expected unknown-heatmap error naming the token"
        );
    }

    // ── load_or_regenerate + resolve_sector_with_cfg (Gap 8) ───────────────

    /// Marker config: its `Default` is `MarkerCfg(0)`. Used to prove the
    /// `--sector` arm substitutes `C::default()` and never runs `extract`.
    #[derive(Debug, Default, PartialEq)]
    struct MarkerCfg(u32);

    /// Both fns share the "exactly one of --project / --sector" guard; the
    /// `(None, None)` and `(Some, Some)` arms short-circuit before any I/O.
    #[test]
    fn load_or_regenerate_requires_exactly_one_source() {
        let err = load_or_regenerate(None, None).unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("pass exactly one of")),
            "expected exactly-one error for (None, None)"
        );

        let err =
            load_or_regenerate(Some("a".into()), Some("b".into())).unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("pass exactly one of")),
            "expected exactly-one error for (Some, Some)"
        );
    }

    #[test]
    fn resolve_sector_with_cfg_requires_exactly_one_source() {
        // The closure must never run on these error arms — panic if it does.
        let err = resolve_sector_with_cfg::<MarkerCfg>(None, None, |_| {
            panic!("extract must not run when neither source is given")
        })
        .unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("pass exactly one of")),
            "expected exactly-one error for (None, None)"
        );

        let p = Utf8PathBuf::from("p");
        let err = resolve_sector_with_cfg::<MarkerCfg>(Some(&p), Some(&p), |_| {
            panic!("extract must not run when both sources are given")
        })
        .unwrap_err();
        assert!(
            matches!(err, SectorError::InvalidConfig(m) if m.contains("pass exactly one of")),
            "expected exactly-one error for (Some, Some)"
        );
    }

    /// On the `--sector` arm there is no project config, so `C::default()` is
    /// substituted and `extract` is *not* called. We prove this by passing a
    /// closure that panics if invoked and asserting the returned cfg is the
    /// `MarkerCfg::default()` (== `MarkerCfg(0)`).
    #[test]
    fn resolve_sector_with_cfg_sector_arm_uses_default_without_extract() {
        let proj = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/m42_project");
        let sec = sectorforge::generate_sector(sectorforge::load_project(&proj).unwrap()).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let json_path =
            Utf8PathBuf::from_path_buf(tmp.path().join("sector.json")).unwrap();
        sectorforge::write_sector_json(&json_path, &sec).unwrap();

        let (_sec, cfg) = resolve_sector_with_cfg::<MarkerCfg>(None, Some(&json_path), |_| {
            panic!("extract must NOT run on the --sector arm")
        })
        .unwrap();
        assert_eq!(cfg, MarkerCfg::default());
    }
}
