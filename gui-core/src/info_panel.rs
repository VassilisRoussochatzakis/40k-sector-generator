//! Right-side info panel. One pure render fn per entity kind so layout is easy
//! to tweak in isolation.

use std::sync::Arc;

use egui::{Color32, FontId, Pos2, RichText, Ui, Vec2};

use sectorforge::sector_model::{
    GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, RoutePattern,
};
use sectorforge::subsectors::Subsector;

use super::palette::{
    darken, draw_route_line, faction_style_by_id, stability_color, star_color, world_type_color,
    PATH_HIGHLIGHT, TEXT, TEXT_DIM,
};
use sectorforge::importance::{
    compute_display_buckets, DisplayBucket, DEFAULT_DISPLAY_CAP, DEFAULT_MINOR_FRACTION,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectorOverviewCacheKey {
    sector_id: String,
    seed: String,
    width: u32,
    height: u32,
    system_count: usize,
    world_count: usize,
    route_count: usize,
    faction_count: usize,
}

impl SectorOverviewCacheKey {
    fn from_sector(sector: &GeneratedSector) -> Self {
        Self {
            sector_id: sector.id.to_string(),
            seed: sector.seed.to_string(),
            width: sector.width,
            height: sector.height,
            system_count: sector.systems.len(),
            world_count: sector.systems.iter().map(|s| s.worlds.len()).sum(),
            route_count: sector.routes.len(),
            faction_count: sector.factions.len(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SectorOverviewCache {
    key: Option<SectorOverviewCacheKey>,
    buckets: Option<Arc<Vec<DisplayBucket>>>,
}

impl SectorOverviewCache {
    pub fn buckets_for(&mut self, sector: &GeneratedSector) -> Arc<Vec<DisplayBucket>> {
        let key = SectorOverviewCacheKey::from_sector(sector);
        if self.key.as_ref() != Some(&key) || self.buckets.is_none() {
            self.key = Some(key);
            self.buckets = Some(Arc::new(compute_display_buckets(
                sector,
                DEFAULT_MINOR_FRACTION,
                DEFAULT_DISPLAY_CAP,
            )));
        }
        self.buckets.clone().unwrap_or_default()
    }

    pub fn invalidate(&mut self) {
        self.key = None;
        self.buckets = None;
    }
}

pub fn sector_overview(
    ui: &mut Ui,
    sector: &GeneratedSector,
    mode: sectorforge::sector_model::RouteViewMode,
) {
    let buckets = compute_display_buckets(sector, DEFAULT_MINOR_FRACTION, DEFAULT_DISPLAY_CAP);
    sector_overview_with_buckets(ui, sector, &buckets, mode);
}

pub fn sector_overview_with_buckets(
    ui: &mut Ui,
    sector: &GeneratedSector,
    buckets: &[DisplayBucket],
    mode: sectorforge::sector_model::RouteViewMode,
) {
    title(ui, &format!("SECTOR: {}", sector.id.to_uppercase()));
    dim(ui, &format!("SEED: {}", short(&sector.seed, 20)));
    dim(
        ui,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.manifest.world_count,
        ),
    );
    ui.add_space(8.0);

    section(ui, "ROUTE TYPE");
    match mode {
        sectorforge::sector_model::RouteViewMode::Detailed => {
            for rtype in sectorforge::sector_model::RouteType::ALL {
                legend_route_row(ui, TEXT, rtype.pattern(mode), rtype.label());
            }
        }
        sectorforge::sector_model::RouteViewMode::TopLevel => {
            for kind in sectorforge::sector_model::RouteKind::ALL {
                legend_route_row(ui, TEXT, kind.patterns()[0], kind.label());
            }
        }
    }
    legend_route_row(ui, PATH_HIGHLIGHT, RoutePattern::Solid, "PLANNED PATH");
    ui.add_space(8.0);

    section(ui, "ROUTE STABILITY");
    for (stab, name) in [
        (sectorforge::sector_model::RouteStability::Stable, "STABLE"),
        (
            sectorforge::sector_model::RouteStability::Unstable,
            "UNSTABLE",
        ),
        (
            sectorforge::sector_model::RouteStability::Hazardous,
            "HAZARDOUS",
        ),
        (
            sectorforge::sector_model::RouteStability::Perilous,
            "PERILOUS",
        ),
    ] {
        legend_row(ui, stability_color(stab), name);
    }
    ui.add_space(8.0);

    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        section(ui, "ROUTE CONTROL");
        legend_control_row(ui, "PATROL", "PATROL");
        legend_control_row(ui, "TOLL", "TOLL");
        legend_control_row(ui, "INTERDICTION", "INTERDICTION");
        legend_control_row(ui, "PIRACY", "PIRACY");
        ui.add_space(8.0);
    }

    if !sector.factions.is_empty() {
        section(ui, "FACTIONS");
        for b in buckets {
            match b {
                DisplayBucket::Faction {
                    id,
                    name,
                    system_count,
                    world_count,
                    ..
                } => {
                    let style = faction_style_by_id(&sector.factions, id);
                    legend_row(
                        ui,
                        style.fill,
                        &format!(
                            "{}  {}S {}W",
                            short(&name.to_uppercase(), 18),
                            system_count,
                            world_count,
                        ),
                    );
                }
                DisplayBucket::Aggregated {
                    label,
                    system_count,
                    world_count,
                    ..
                } => {
                    legend_row(
                        ui,
                        Color32::from_rgb(140, 140, 150),
                        &format!(
                            "{}  {}S {}W",
                            short(&label.to_uppercase(), 18),
                            system_count,
                            world_count,
                        ),
                    );
                }
            }
        }
        ui.add_space(8.0);
    }
}

pub fn system_summary(ui: &mut Ui, sys: &GeneratedSystem, sector: &GeneratedSector) {
    title(ui, &format!("SYSTEM: {}", sys.id.to_uppercase()));
    body(ui, &short(&sys.name.to_uppercase(), 28));
    dim(ui, &format!("COORD: Q{:+} R{:+}", sys.coord.q, sys.coord.r));
    ui.add_space(8.0);

    if let Some(star) = &sys.star {
        section(ui, "STAR");
        legend_row(
            ui,
            star_color(&star.colour_code),
            &format!(
                "{} - {}",
                star.colour_code.to_uppercase(),
                star.colour_name.to_uppercase()
            ),
        );
        if let Some(s) = star.spectral_type.as_ref() {
            dim(ui, &format!("SPECTRAL: {}", s.to_uppercase()));
        }
    } else {
        section(ui, "KIND");
        body(ui, &format!("{:?}", sys.kind).to_uppercase());
    }
    ui.add_space(8.0);

    section(ui, &format!("WORLDS ({})", sys.worlds.len()));
    for w in &sys.worlds {
        legend_row(
            ui,
            world_type_color(&w.world.world_type),
            &format!(
                "{}  {}  {}",
                w.orbit,
                short(&w.name.to_uppercase(), 16),
                short(&w.world.world_type.to_uppercase(), 14),
            ),
        );
    }

    if !sys.primary_factions.is_empty() {
        ui.add_space(8.0);
        section(ui, "PRIMARY FACTIONS");
        for f in &sys.primary_factions {
            body(ui, &short(&f.to_uppercase(), 28));
        }
    }
    let c = &sys.control;
    if c.state.is_some()
        || c.dominant.is_some()
        || c.sovereign.is_some()
        || c.orbital_controller.is_some()
        || c.economic_hegemon.is_some()
        || c.hidden_master.is_some()
    {
        ui.add_space(8.0);
        section(ui, "CONTROL");
        if let Some(state) = c.state {
            kv(ui, "STATE", &format!("{state:?}").to_uppercase());
        }
        if let Some(v) = &c.dominant {
            kv(ui, "DOMINANT", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.sovereign {
            kv(ui, "SOVEREIGN", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.orbital_controller {
            kv(ui, "ORBITAL", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.economic_hegemon {
            kv(ui, "ECONOMIC", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.hidden_master {
            kv(ui, "HIDDEN", &short(&v.to_uppercase(), 22));
        }
    }
    stability_block(ui, &sys.stability);
    blockade_block(ui, sys);
    conflict_block(ui, &sys.conflict);
    archetype_block(ui, sys);
    orbital_assets_block(ui, sys);
    routes_block(ui, sys, sector);
    system_history(ui, sector, sys.id.as_str());
    if !sys.tags.is_empty() {
        ui.add_space(8.0);
        section(ui, "TAGS");
        for t in &sys.tags {
            dim(ui, &t.to_uppercase());
        }
    }
    if !sys.notes.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTES");
        for n in &sys.notes {
            dim(ui, n);
        }
    }
}

pub fn route_summary(
    ui: &mut Ui,
    route: &GeneratedRoute,
    sector: &GeneratedSector,
    mode: sectorforge::sector_model::RouteViewMode,
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
    }
    kv(
        ui,
        "STABILITY",
        &format!("{:?}", route.stability).to_uppercase(),
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
            let style = faction_style_by_id(&sector.factions, &c.faction_id);
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

pub fn world_history(ui: &mut Ui, sector: &GeneratedSector, world_id: &str) {
    let mut hits: Vec<_> = sector
        .chronicle
        .events
        .iter()
        .filter(|e| event_mentions_world(e, world_id))
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    ui.add_space(8.0);
    section(ui, "HISTORY");
    for e in hits {
        dim(
            ui,
            &format!(
                "{}  {:?}  {}",
                e.date,
                e.kind,
                short(&e.summary.to_uppercase(), 42)
            ),
        );
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

fn system_history(ui: &mut Ui, sector: &GeneratedSector, system_id: &str) {
    let mut hits: Vec<_> = sector
        .chronicle
        .events
        .iter()
        .filter(|e| event_mentions_system(e, system_id))
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    ui.add_space(8.0);
    section(ui, "LOCAL HISTORY");
    for e in hits.iter().take(8) {
        dim(
            ui,
            &format!(
                "{}  {:?}  {}",
                e.date,
                e.kind,
                short(&e.summary.to_uppercase(), 42)
            ),
        );
    }
}

fn event_mentions_world(e: &sectorforge::history::HistoryEvent, world_id: &str) -> bool {
    (match &e.anchor {
        sectorforge::history::HistoryAnchor::World { world_id: wid, .. } => wid == world_id,
        _ => false,
    }) || e.entities.iter().any(|x| {
        matches!(x.kind, sectorforge::history::HistoryEntityKind::World) && x.id == world_id
    })
}

fn event_mentions_system(e: &sectorforge::history::HistoryEvent, system_id: &str) -> bool {
    (match &e.anchor {
        sectorforge::history::HistoryAnchor::System { system_id: sid } => sid == system_id,
        sectorforge::history::HistoryAnchor::World { system_id: sid, .. } => sid == system_id,
        sectorforge::history::HistoryAnchor::Route {
            from_system_id,
            to_system_id,
            ..
        } => from_system_id == system_id || to_system_id == system_id,
        _ => false,
    }) || e.entities.iter().any(|x| {
        matches!(x.kind, sectorforge::history::HistoryEntityKind::System) && x.id == system_id
    })
}

pub fn world_detail(ui: &mut Ui, w: &GeneratedWorld) {
    title(ui, &format!("WORLD: {}", w.name.to_uppercase()));
    dim(ui, &format!("ID: {}", w.id.to_uppercase()));
    dim(ui, &format!("ORBIT: {}", w.orbit));
    ui.add_space(8.0);

    section(ui, "CLASSIFICATION");
    legend_row(
        ui,
        world_type_color(&w.world.world_type),
        &w.world.world_type.to_uppercase(),
    );
    kv(
        ui,
        "STAR COLOUR",
        &format!(
            "{} - {}",
            w.world.star_colour_code.to_uppercase(),
            w.world.star_colour.to_uppercase()
        ),
    );
    ui.add_space(8.0);

    section(ui, "ENVIRONMENT");
    kv(ui, "ATMOSPHERE", &w.world.atmosphere.to_uppercase());
    kv(ui, "TEMPERATURE", &w.world.temperature.to_uppercase());
    kv(ui, "BIOSPHERE", &w.world.biosphere.to_uppercase());
    ui.add_space(8.0);

    section(ui, "SOCIETY");
    kv(ui, "POPULATION", &w.world.population.to_uppercase());
    kv(ui, "TECH LEVEL", &w.world.tech_level.to_uppercase());
    kv(ui, "GOVERNMENT", &w.world.government.to_uppercase());

    if !w.world.notable_features.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTABLE FEATURES");
        for f in &w.world.notable_features {
            dim(ui, &format!("- {f}"));
        }
    }
    if !w.factions.is_empty() {
        ui.add_space(8.0);
        section(ui, "FACTION PRESENCE");
        for fp in &w.factions {
            let sub = fp
                .subfaction_name
                .as_deref()
                .or(fp.subfaction_id.as_deref())
                .unwrap_or("");
            let force = fp
                .force_name
                .as_deref()
                .or(fp.force_id.as_deref())
                .unwrap_or("");
            let label = if sub.is_empty() {
                fp.faction_id.to_uppercase()
            } else if force.is_empty() {
                format!("{} / {}", fp.faction_id.to_uppercase(), sub.to_uppercase())
            } else {
                format!(
                    "{} / {} / {}",
                    fp.faction_id.to_uppercase(),
                    sub.to_uppercase(),
                    force.to_uppercase()
                )
            };
            dim(
                ui,
                &format!(
                    "{} [{:?}/{:?}] ctl {:.0} vis {:.0}",
                    label,
                    fp.influence,
                    fp.dominance,
                    fp.dimensions.local_control_score(),
                    fp.dimensions.visibility,
                ),
            );
        }
    }
    let wc = &w.control;
    if wc.dominant.is_some()
        || wc.sovereign.is_some()
        || wc.occupier.is_some()
        || wc.economic_hegemon.is_some()
        || wc.popular_authority.is_some()
        || wc.hidden_master.is_some()
    {
        ui.add_space(8.0);
        section(ui, "CONTROL");
        if let Some(v) = &wc.dominant {
            kv(ui, "DOMINANT", &v.to_uppercase());
        }
        if let Some(v) = &wc.sovereign {
            kv(ui, "SOVEREIGN", &v.to_uppercase());
        }
        if let Some(v) = &wc.occupier {
            kv(ui, "OCCUPIER", &v.to_uppercase());
        }
        if let Some(v) = &wc.economic_hegemon {
            kv(ui, "ECONOMIC", &v.to_uppercase());
        }
        if let Some(v) = &wc.popular_authority {
            kv(ui, "POPULAR", &v.to_uppercase());
        }
        if let Some(v) = &wc.hidden_master {
            kv(ui, "HIDDEN", &v.to_uppercase());
        }
        if wc.contested {
            dim(ui, "CONTESTED");
        }
        kv(ui, "SCORE", &format!("{:.0}", wc.control_score));
    }
    stability_block(ui, &w.stability);
    if !w.regions.is_empty() {
        ui.add_space(8.0);
        section(ui, &format!("SURFACE REGIONS ({})", w.regions.len()));
        for r in &w.regions {
            dim(
                ui,
                &format!(
                    "{:?} {} - {} ({})",
                    r.kind,
                    short(&r.name.to_uppercase(), 18),
                    r.dominant.as_deref().unwrap_or("—").to_uppercase(),
                    r.control_score
                ),
            );
        }
    }
    if w.conflict.intensity > 0 {
        ui.add_space(8.0);
        section(ui, "CONFLICT");
        kv(ui, "INTENSITY", &w.conflict.intensity.to_string());
        kv(ui, "MOMENTUM", &w.conflict.momentum.to_string());
        if let Some(a) = &w.conflict.attacker {
            kv(ui, "ATTACKER", &short(&a.to_uppercase(), 22));
        }
        if let Some(d) = &w.conflict.defender {
            kv(ui, "DEFENDER", &short(&d.to_uppercase(), 22));
        }
    }
    if !w.claims.is_empty() {
        ui.add_space(8.0);
        section(ui, "CLAIMS");
        for c in &w.claims {
            dim(
                ui,
                &format!(
                    "{} [{:?}] {}",
                    c.faction_id.to_uppercase(),
                    c.claim_type,
                    c.strength
                ),
            );
        }
    }
    if !w.tags.is_empty() {
        ui.add_space(8.0);
        section(ui, "TAGS");
        for t in &w.tags {
            dim(ui, &t.to_uppercase());
        }
    }
    if !w.notes.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTES");
        for n in &w.notes {
            dim(ui, n);
        }
    }
}

pub fn subsector_summary(ui: &mut Ui, sub: &Subsector, sector: &GeneratedSector) {
    title(ui, &format!("SUBSECTOR {}", sub.label.to_uppercase()));
    body(ui, &sub.name.to_uppercase());
    dim(ui, &format!("ID: {}", sub.id));
    ui.add_space(8.0);

    section(ui, "COUNTS");
    kv(ui, "SYSTEMS", &sub.summary.system_count.to_string());
    kv(ui, "WORLDS", &sub.summary.world_count.to_string());
    kv(
        ui,
        "INTERNAL ROUTES",
        &sub.summary.internal_route_count.to_string(),
    );
    kv(
        ui,
        "BORDER ROUTES",
        &sub.summary.border_route_count.to_string(),
    );
    ui.add_space(8.0);

    let sys_name = |id: &str| -> String {
        sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.to_string())
            .unwrap_or_else(|| id.to_string())
    };
    let world_name = |id: &str| -> Option<String> {
        for s in &sector.systems {
            if let Some(w) = s.worlds.iter().find(|w| w.id == id) {
                return Some(w.name.to_string());
            }
        }
        None
    };

    section(ui, "CAPITAL");
    if let Some(cap) = &sub.summary.subsector_capital_system_id {
        kv(ui, "SYSTEM", &sys_name(cap).to_uppercase());
    } else {
        dim(ui, "(none)");
    }
    if let Some(cw) = &sub.summary.subsector_capital_world_id {
        if let Some(n) = world_name(cw) {
            kv(ui, "WORLD", &n.to_uppercase());
        }
    }
    if let Some(primary) = &sub.summary.primary_system_id {
        kv(ui, "PRIMARY", &sys_name(primary).to_uppercase());
    }
    ui.add_space(8.0);

    if let Some(cf) = &sub.summary.controlling_faction_id {
        section(ui, "CONTROLLING FACTION");
        body(ui, &cf.to_uppercase());
        ui.add_space(8.0);
    }

    if !sub.summary.dominant_factions.is_empty() {
        section(ui, "DOMINANT FACTIONS");
        for sc in &sub.summary.dominant_factions {
            dim(ui, &format!("{}  ({})", sc.id.to_uppercase(), sc.score));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.faction_control.is_empty() {
        section(ui, "FACTION CONTROL");
        for r in &sub.summary.faction_control {
            dim(
                ui,
                &format!(
                    "{} [{}] sys {} wld {} ({:.1}%)",
                    r.faction_id.to_uppercase(),
                    r.control_tier.to_uppercase(),
                    r.owned_system_count,
                    r.owned_world_count,
                    r.system_share_basis_points as f32 / 100.0,
                ),
            );
        }
        ui.add_space(8.0);
    }

    if !sub.summary.world_type_counts.is_empty() {
        section(ui, "WORLD TYPES");
        let mut v: Vec<_> = sub.summary.world_type_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(10) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.population_counts.is_empty() {
        section(ui, "POPULATION");
        let mut v: Vec<_> = sub.summary.population_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.tech_level_counts.is_empty() {
        section(ui, "TECH LEVEL");
        let mut v: Vec<_> = sub.summary.tech_level_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.government_counts.is_empty() {
        section(ui, "GOVERNMENT");
        let mut v: Vec<_> = sub.summary.government_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.feature_counts.is_empty() {
        section(ui, "NOTABLE FEATURES");
        let mut v: Vec<_> = sub.summary.feature_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(10) {
            dim(ui, &format!("{}  x{}", k, n));
        }
        ui.add_space(8.0);
    }

    if !sub.connected_subsector_ids.is_empty() {
        section(ui, "CONNECTED SUBSECTORS");
        for id in &sub.connected_subsector_ids {
            dim(ui, &id.to_uppercase());
        }
        ui.add_space(8.0);
    }

    if !sub.tags.is_empty() {
        section(ui, "TAGS");
        for t in &sub.tags {
            dim(ui, &t.to_uppercase());
        }
        ui.add_space(8.0);
    }

    if !sub.notes.is_empty() {
        section(ui, "NOTES");
        for n in &sub.notes {
            dim(ui, n);
        }
    }
}

pub fn star_detail(ui: &mut Ui, sys: &GeneratedSystem) {
    if let Some(star) = &sys.star {
        title(ui, &format!("STAR OF {}", sys.id.to_uppercase()));
        ui.add_space(4.0);
        legend_row(
            ui,
            star_color(&star.colour_code),
            &format!(
                "{} - {}",
                star.colour_code.to_uppercase(),
                star.colour_name.to_uppercase()
            ),
        );
        if let Some(s) = star.spectral_type.as_ref() {
            kv(ui, "SPECTRAL", &s.to_uppercase());
        }
        kv(ui, "WORLDS", &sys.worlds.len().to_string());
        if let Some(idx) = star.source_row_index {
            dim(ui, &format!("source row: {idx}"));
        }
    } else {
        title(ui, &format!("LOCATION: {}", sys.id.to_uppercase()));
        ui.add_space(4.0);
        kv(ui, "KIND", &format!("{:?}", sys.kind).to_uppercase());
    }
}

// ── small helpers ─────────────────────────────────────────────────────────

fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn title(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT).font(mono(18.0)));
    ui.add_space(2.0);
}

fn section(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT).font(mono(13.0)).strong());
}

fn body(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT).font(mono(13.0)));
}

fn dim(ui: &mut Ui, s: &str) {
    ui.label(RichText::new(s).color(TEXT_DIM).font(mono(12.0)));
}

fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{k}:"))
                .color(TEXT_DIM)
                .font(mono(12.0)),
        );
        ui.label(RichText::new(v).color(TEXT).font(mono(12.0)));
    });
}

fn routes_block(ui: &mut Ui, sys: &GeneratedSystem, sector: &GeneratedSector) {
    let mut hits: Vec<&sectorforge::sector_model::GeneratedRoute> = sector
        .routes
        .iter()
        .filter(|r| r.from_system_id == sys.id || r.to_system_id == sys.id)
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    ui.add_space(8.0);
    section(ui, &format!("ROUTES ({})", hits.len()));
    for r in hits {
        let other = if r.from_system_id == sys.id {
            &r.to_system_id
        } else {
            &r.from_system_id
        };
        dim(
            ui,
            &format!(
                "→ {} [{:?}/{:?}] d={}",
                other.to_uppercase(),
                r.route_type,
                r.stability,
                r.distance
            ),
        );
        for c in &r.controls {
            let parts = [
                ("PTRL", c.patrol),
                ("TOLL", c.toll),
                ("INTR", c.interdiction),
                ("PIRC", c.piracy),
                ("SCRC", c.secrecy),
                ("CONF", c.confidence),
            ];
            let mut active: Vec<String> = parts
                .iter()
                .filter(|(_, v)| *v >= 30.0)
                .map(|(l, v)| format!("{l}{:.0}", v))
                .collect();
            if active.is_empty() {
                continue;
            }
            active.sort_unstable();
            let style = faction_style_by_id(&sector.factions, &c.faction_id);
            legend_row(
                ui,
                style.fill,
                &format!(
                    "{}  {}",
                    short(&c.faction_id.to_uppercase(), 14),
                    active.join(" ")
                ),
            );
        }
    }
}

fn blockade_block(ui: &mut Ui, sys: &GeneratedSystem) {
    if !sys.blockade.under_blockade {
        return;
    }
    ui.add_space(8.0);
    section(ui, "BLOCKADE");
    if let Some(b) = &sys.blockade.blockader {
        kv(ui, "BLOCKADER", &short(&b.to_uppercase(), 22));
    }
    if let Some(b) = &sys.blockade.besieged {
        kv(ui, "BESIEGED", &short(&b.to_uppercase(), 22));
    }
    kv(ui, "INTENSITY", &sys.blockade.intensity.to_string());
}

fn conflict_block(ui: &mut Ui, c: &sectorforge::conflict::ConflictState) {
    if c.intensity == 0 && c.attacker.is_none() && c.defender.is_none() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "CONFLICT");
    kv(ui, "INTENSITY", &c.intensity.to_string());
    kv(ui, "MOMENTUM", &c.momentum.to_string());
    if let Some(a) = &c.attacker {
        kv(ui, "ATTACKER", &short(&a.to_uppercase(), 22));
    }
    if let Some(d) = &c.defender {
        kv(ui, "DEFENDER", &short(&d.to_uppercase(), 22));
    }
    if let Some(v) = &c.visible_controller {
        kv(ui, "VISIBLE", &short(&v.to_uppercase(), 22));
    }
    kv(ui, "AGE", &c.age.to_string());
}

fn archetype_block(ui: &mut Ui, sys: &GeneratedSystem) {
    let a = &sys.archetype;
    if *a == sectorforge::archetypes::ArchetypeState::default() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "ARCHETYPE");
    if !a.imperial_co_sovereigns.is_empty() {
        kv(
            ui,
            "IMP STACK",
            &format!("{} factions", a.imperial_co_sovereigns.len()),
        );
    }
    if a.necron_phase != sectorforge::archetypes::NecronPhase::default() {
        kv(
            ui,
            "NECRON",
            &format!("{:?}", a.necron_phase).to_uppercase(),
        );
    }
    if a.tyranid_stage != sectorforge::archetypes::TyranidStage::default() {
        kv(
            ui,
            "TYRANID",
            &format!("{:?}", a.tyranid_stage).to_uppercase(),
        );
    }
    if a.ork_waaagh > 0 {
        kv(ui, "WAAAGH", &a.ork_waaagh.to_string());
    }
    if a.gsc_stage != sectorforge::archetypes::GscStage::default() {
        kv(ui, "GSC", &format!("{:?}", a.gsc_stage).to_uppercase());
    }
    if a.tau_sphere != sectorforge::archetypes::TauSphereBand::default() {
        kv(ui, "TAU", &format!("{:?}", a.tau_sphere).to_uppercase());
    }
    if a.aeldari_activity > 0 {
        kv(ui, "AELDARI", &a.aeldari_activity.to_string());
    }
    if a.chaos_corruption > 0 {
        kv(ui, "CHAOS", &a.chaos_corruption.to_string());
    }
    if a.daemon_manifestation > 0 {
        kv(ui, "DAEMON", &a.daemon_manifestation.to_string());
    }
}

fn orbital_assets_block(ui: &mut Ui, sys: &GeneratedSystem) {
    if sys.orbital_assets.is_empty() {
        return;
    }
    ui.add_space(8.0);
    section(
        ui,
        &format!("ORBITAL ASSETS ({})", sys.orbital_assets.len()),
    );
    for a in &sys.orbital_assets {
        dim(
            ui,
            &format!(
                "{:?} {} ({}) ",
                a.kind,
                short(&a.faction_id.to_uppercase(), 16),
                a.strength
            ),
        );
    }
}

fn stability_block(ui: &mut Ui, st: &sectorforge::stability::StabilityState) {
    if *st == sectorforge::stability::StabilityState::default() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "STABILITY");
    kv(ui, "PUBLIC ORDER", &format!("{:.0}", st.public_order));
    kv(ui, "CORRUPTION", &format!("{:.0}", st.corruption));
    kv(ui, "FEAR", &format!("{:.0}", st.fear));
    kv(ui, "REBELLION", &format!("{:.0}", st.rebellion_risk));
    kv(ui, "XENOS THREAT", &format!("{:.0}", st.xenos_threat));
    kv(ui, "WARP INSTAB.", &format!("{:.0}", st.warp_instability));
    kv(
        ui,
        "FAMINE/STRESS",
        &format!("{:.0}", st.famine_or_resource_stress),
    );
}

fn legend_row(ui: &mut Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, color);
        ui.painter()
            .rect_stroke(rect, 1.0, egui::Stroke::new(1.0, darken(color, 0.5)));
        ui.label(RichText::new(text).color(TEXT).font(mono(12.0)));
    });
}

fn legend_route_row(ui: &mut Ui, color: Color32, pattern: RoutePattern, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 12.0), egui::Sense::hover());
        let y = rect.center().y;
        let a = Pos2::new(rect.left(), y);
        let b = Pos2::new(rect.right(), y);
        draw_route_line(ui.painter(), a, b, 2.5, color, pattern);
        ui.label(RichText::new(text).color(TEXT).font(mono(12.0)));
    });
}

fn legend_control_row(ui: &mut Ui, kind: &str, text: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 12.0), egui::Sense::hover());
        let center = rect.center();
        let size = 10.0;
        let half = size / 2.0;
        let color = TEXT_DIM;
        let painter = ui.painter();

        match kind {
            "PATROL" => {
                painter.circle_filled(center, half, color);
            }
            "TOLL" => {
                painter.rect_filled(
                    egui::Rect::from_center_size(center, Vec2::splat(size)),
                    0.0,
                    color,
                );
            }
            "INTERDICTION" => {
                painter.line_segment(
                    [center - Vec2::new(0.0, half), center + Vec2::new(0.0, half)],
                    egui::Stroke::new(2.5, color),
                );
            }
            "PIRACY" => {
                painter.line_segment(
                    [
                        center - Vec2::new(half, half),
                        center + Vec2::new(half, half),
                    ],
                    egui::Stroke::new(2.5, color),
                );
                painter.line_segment(
                    [
                        center - Vec2::new(half, -half),
                        center + Vec2::new(half, -half),
                    ],
                    egui::Stroke::new(2.5, color),
                );
            }
            _ => {}
        }

        ui.label(RichText::new(text).color(TEXT).font(mono(12.0)));
    });
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn overview_cache_reuses_buckets_until_sector_key_changes() {
        let mut sector = crate::editor::state::empty_sector("cache", "Cache", "seed", 4, 4);
        let mut cache = SectorOverviewCache::default();

        let first = cache.buckets_for(&sector);
        let second = cache.buckets_for(&sector);
        assert!(Arc::ptr_eq(&first, &second));

        sector.height += 1;
        let third = cache.buckets_for(&sector);
        assert!(!Arc::ptr_eq(&second, &third));
    }

    #[test]
    fn overview_cache_invalidate_drops_buckets() {
        let sector = crate::editor::state::empty_sector("cache", "Cache", "seed", 4, 4);
        let mut cache = SectorOverviewCache::default();

        let first = cache.buckets_for(&sector);
        cache.invalidate();
        let second = cache.buckets_for(&sector);
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
