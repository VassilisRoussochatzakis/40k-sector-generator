//! Shared CLI helpers: progress logging, output formatting, parsers.

use camino::Utf8PathBuf;
use serde::Serialize;

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

pub fn log_progress(message: impl std::fmt::Display) {
    eprintln!("[sectorforge] {message}");
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
        } => {
            if should_log_progress(current, total) {
                log_progress(format_args!(
                    "{prefix}: generated systems {current}/{total} (last {worlds} world(s))"
                ));
            }
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
        } => {
            if should_log_progress(current, total) {
                log_progress(format_args!(
                    "{prefix}: region route effects scanned {current}/{total} route(s), affected {affected_routes}, changed {changed_routes}, bridge checks {bridge_checks}, preserved {bridges_preserved}"
                ));
            }
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
        SectorProgress::RouteControlsProgress { current, total } => {
            if should_log_progress(current, total) {
                log_progress(format_args!(
                    "{prefix}: derived route control {current}/{total}"
                ));
            }
        }
        SectorProgress::RouteControlsDerived { routes } => {
            log_progress(format_args!("{prefix}: derived control for {routes} route(s)"));
        }
        SectorProgress::SystemStateDerived { current, total } => {
            if should_log_progress(current, total) {
                log_progress(format_args!("{prefix}: derived system state {current}/{total}"));
            }
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
        } => {
            if should_log_progress(current, total) {
                log_progress(format_args!(
                    "{prefix}: chronicle scanned systems {current}/{total} ({events} event(s))"
                ));
            }
        }
        SectorProgress::ChronicleRoutesScanned {
            current,
            total,
            events,
        } => {
            if should_log_progress(current, total) {
                log_progress(format_args!(
                    "{prefix}: chronicle scanned routes {current}/{total} ({events} event(s))"
                ));
            }
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
