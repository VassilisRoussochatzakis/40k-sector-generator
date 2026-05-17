//! Right-side info panel. One pure render fn per entity kind so layout is easy
//! to tweak in isolation.

use egui::{Color32, FontId, Pos2, RichText, Ui, Vec2};

use crate::sector_model::{GeneratedSector, GeneratedSystem, GeneratedWorld, RoutePattern};

use super::palette::{
    darken, draw_route_line, stability_color, star_color, world_type_color, TEXT, TEXT_DIM,
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
}

pub fn system_summary(ui: &mut Ui, sys: &GeneratedSystem) {
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
                    "{} [{:?}] {}",
                    fp.faction_id.to_uppercase(),
                    fp.influence,
                    fp.relationship_to_government
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
