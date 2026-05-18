//! Right-side info panel. One pure render fn per entity kind so layout is easy
//! to tweak in isolation.

use egui::{Color32, FontId, Pos2, RichText, Ui, Vec2};

use crate::sector_model::{GeneratedSector, GeneratedSystem, GeneratedWorld, RoutePattern};
use crate::subsectors::Subsector;

use super::palette::{
    darken, draw_route_line, faction_style_by_id, stability_color, star_color, world_type_color,
    TEXT, TEXT_DIM,
};
use crate::importance::{
    compute_display_buckets, DisplayBucket, DEFAULT_DISPLAY_CAP, DEFAULT_MINOR_FRACTION,
};

pub fn sector_overview(ui: &mut Ui, sector: &GeneratedSector) {
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
    for (rtype, name) in [
        (
            crate::sector_model::RouteType::StableWarpLane,
            "STABLE WARP LANE",
        ),
        (
            crate::sector_model::RouteType::ChartedPassage,
            "CHARTED PASSAGE",
        ),
        (
            crate::sector_model::RouteType::DangerousPassage,
            "DANGEROUS PASSAGE",
        ),
        (
            crate::sector_model::RouteType::SecretPassage,
            "SECRET PASSAGE",
        ),
    ] {
        legend_route_row(ui, TEXT, rtype.pattern(), name);
    }
    ui.add_space(8.0);

    section(ui, "ROUTE STABILITY");
    for (stab, name) in [
        (crate::sector_model::RouteStability::Stable, "STABLE"),
        (crate::sector_model::RouteStability::Unstable, "UNSTABLE"),
        (crate::sector_model::RouteStability::Hazardous, "HAZARDOUS"),
        (crate::sector_model::RouteStability::Perilous, "PERILOUS"),
    ] {
        legend_row(ui, stability_color(stab), name);
    }
    ui.add_space(8.0);

    if !sector.factions.is_empty() {
        section(ui, "FACTIONS");
        let buckets = compute_display_buckets(sector, DEFAULT_MINOR_FRACTION, DEFAULT_DISPLAY_CAP);
        for b in &buckets {
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

    section(ui, "STAR");
    legend_row(
        ui,
        star_color(&sys.star.colour_code),
        &format!(
            "{} - {}",
            sys.star.colour_code.to_uppercase(),
            sys.star.colour_name.to_uppercase()
        ),
    );
    if let Some(s) = sys.star.spectral_type.as_ref() {
        dim(ui, &format!("SPECTRAL: {}", s.to_uppercase()));
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
    routes_block(ui, sys, sector);
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
            dim(
                ui,
                &format!(
                    "{} [{:?}/{:?}] ctl {:.0} vis {:.0}",
                    fp.faction_id.to_uppercase(),
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
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let world_name = |id: &str| -> Option<String> {
        for s in &sector.systems {
            if let Some(w) = s.worlds.iter().find(|w| w.id == id) {
                return Some(w.name.clone());
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
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(10) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.population_counts.is_empty() {
        section(ui, "POPULATION");
        let mut v: Vec<_> = sub.summary.population_counts.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.tech_level_counts.is_empty() {
        section(ui, "TECH LEVEL");
        let mut v: Vec<_> = sub.summary.tech_level_counts.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.government_counts.is_empty() {
        section(ui, "GOVERNMENT");
        let mut v: Vec<_> = sub.summary.government_counts.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.feature_counts.is_empty() {
        section(ui, "NOTABLE FEATURES");
        let mut v: Vec<_> = sub.summary.feature_counts.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
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
    title(ui, &format!("STAR OF {}", sys.id.to_uppercase()));
    ui.add_space(4.0);
    legend_row(
        ui,
        star_color(&sys.star.colour_code),
        &format!(
            "{} - {}",
            sys.star.colour_code.to_uppercase(),
            sys.star.colour_name.to_uppercase()
        ),
    );
    if let Some(s) = sys.star.spectral_type.as_ref() {
        kv(ui, "SPECTRAL", &s.to_uppercase());
    }
    kv(ui, "WORLDS", &sys.worlds.len().to_string());
    if let Some(idx) = sys.star.source_row_index {
        dim(ui, &format!("source row: {idx}"));
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
    let mut hits: Vec<&crate::sector_model::GeneratedRoute> = sector
        .routes
        .iter()
        .filter(|r| r.from_system_id == sys.id || r.to_system_id == sys.id)
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_by(|a, b| a.id.cmp(&b.id));
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
            active.sort();
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

fn stability_block(ui: &mut Ui, st: &crate::stability::StabilityState) {
    if *st == crate::stability::StabilityState::default() {
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
        let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 12.0), egui::Sense::hover());
        let y = rect.center().y;
        let a = Pos2::new(rect.left(), y);
        let b = Pos2::new(rect.right(), y);
        draw_route_line(ui.painter(), a, b, 2.5, color, pattern);
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
