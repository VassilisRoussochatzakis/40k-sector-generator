//! Route detail render (`route_summary`) + endpoint-label helper. Split
//! verbatim from `info_panel.rs` (AREA_F F8, by section).

use egui::Ui;

use sectorforge::sector_model::{GeneratedRoute, GeneratedSector};

use crate::palette::{faction_style_by_id, stability_color};
use crate::sector_view::SectorMapCache;

use super::{dim, kv, legend_route_row, legend_row, section, short, title};

pub fn route_summary(
    ui: &mut Ui,
    route: &GeneratedRoute,
    sector: &GeneratedSector,
    mode: sectorforge::sector_model::RouteViewMode,
    cache: Option<&SectorMapCache>,
) {
    title(ui, "ROUTE");
    kv(ui, "ID", route.id.as_str());
    kv(
        ui,
        "FROM",
        &route_endpoint_label(sector, &route.from_system_id),
    );
    kv(ui, "TO", &route_endpoint_label(sector, &route.to_system_id));
    match mode {
        sectorforge::sector_model::RouteViewMode::Detailed => {
            kv(ui, "TYPE", route.route_type.label());
        }
        sectorforge::sector_model::RouteViewMode::TopLevel => {
            kv(ui, "TYPE", route.route_type.kind().label());
        }
        _ => {}
    }
    kv(
        ui,
        "STABILITY",
        &format!("{}", route.stability).to_uppercase(),
    );
    kv(ui, "DISTANCE", &route.distance.to_string());
    ui.add_space(8.0);
    legend_route_row(
        ui,
        stability_color(route.stability),
        route.route_type.pattern(mode),
        match mode {
            sectorforge::sector_model::RouteViewMode::Detailed => route.route_type.label(),
            sectorforge::sector_model::RouteViewMode::TopLevel => route.route_type.kind().label(),
            _ => route.route_type.label(),
        },
    );

    if !route.tags.is_empty() {
        ui.add_space(8.0);
        section(ui, "TAGS");
        for t in &route.tags {
            dim(ui, &t.to_uppercase());
        }
    }

    if !route.controls.is_empty() {
        ui.add_space(8.0);
        section(ui, &format!("ROUTE CONTROL ({})", route.controls.len()));
        for c in &route.controls {
            let style = cache
                .and_then(|mc| mc.faction_style(&c.faction_id).copied())
                .unwrap_or_else(|| faction_style_by_id(&sector.factions, &c.faction_id));
            legend_row(
                ui,
                style.fill,
                &format!(
                    "{}  PTRL{:.0} TOLL{:.0} INTR{:.0} PIRC{:.0} SCRC{:.0} CONF{:.0}",
                    short(&c.faction_id.to_uppercase(), 12),
                    c.patrol,
                    c.toll,
                    c.interdiction,
                    c.piracy,
                    c.secrecy,
                    c.confidence,
                ),
            );
        }
    }
}

fn route_endpoint_label(sector: &GeneratedSector, id: &sectorforge::ids::SystemId) -> String {
    sector
        .systems
        .iter()
        .find(|s| s.id.as_str() == id.as_str())
        .map(|s| {
            format!(
                "{} ({})",
                short(&s.name.to_uppercase(), 18),
                id.to_uppercase()
            )
        })
        .unwrap_or_else(|| id.to_uppercase())
}
