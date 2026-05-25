//! Right-hand legend: title, route-type/stability/control, factions, heatmap.

use image::Rgba;

use crate::bitmap::RenderOptions;
use crate::heatmap::HeatmapMode;
use crate::map_theme::LegendStyle;
use crate::sector_model::{GeneratedSector, RouteStability, RouteType};

use super::colors::{darken, rgba_from_tuple, route_thickness, short, stability_color};
use super::primitives::{circle, line, rect, text};
use super::routes::{draw_route_pattern, ControlKind};

pub(super) const LEGEND_PAD: f32 = 16.0;
const LINE_H: f32 = 18.0;
const TITLE_FONT: f32 = 18.0;
const BODY_FONT: f32 = 11.0;

pub(super) fn legend_height(sector: &GeneratedSector, opts: &RenderOptions) -> f32 {
    if matches!(opts.theme.legend, LegendStyle::Hidden) {
        return 0.0;
    }
    let heatmap_lines: i32 = if matches!(opts.heatmap, HeatmapMode::Off) {
        0
    } else {
        2
    };
    if matches!(opts.theme.legend, LegendStyle::Compact) {
        let lines = 4 + 1 + 5 + 1 + factions_visible(sector) as i32 + heatmap_lines;
        return LEGEND_PAD.mul_add(2.0, lines as f32 * LINE_H);
    }
    let route_type_rows = match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => RouteType::ALL.len() as i32,
        crate::sector_model::RouteViewMode::TopLevel => {
            crate::sector_model::RouteKind::ALL.len() as i32
        }
    };
    let route_control_lines: i32 = if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        5
    } else {
        0
    };
    let lines = 4
        + 1
        + route_type_rows
        + 1
        + 1
        + 4
        + 1
        + route_control_lines
        + 1
        + factions_visible(sector) as i32
        + heatmap_lines;
    LEGEND_PAD.mul_add(2.0, lines as f32 * LINE_H)
}

fn factions_visible(sector: &GeneratedSector) -> usize {
    if sector.factions.is_empty() {
        return 0;
    }
    let buckets = crate::importance::compute_display_buckets(
        sector,
        crate::importance::DEFAULT_MINOR_FRACTION,
        crate::importance::DEFAULT_DISPLAY_CAP,
    );
    1 + buckets.len()
}

pub(super) fn draw_legend(
    s: &mut String,
    sector: &GeneratedSector,
    map_w: f32,
    opts: &RenderOptions,
) {
    let x0 = map_w + LEGEND_PAD;
    let mut y = LEGEND_PAD + TITLE_FONT;
    let theme = &opts.theme;

    text(
        s,
        x0,
        y,
        &format!("SECTOR: {}", sector.id.to_uppercase()),
        theme.text,
        TITLE_FONT,
        "start",
    );
    y += LINE_H + 4.0;
    text(
        s,
        x0,
        y,
        &format!("SEED: {}", short(&sector.seed, 20)),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H - 4.0;
    text(
        s,
        x0,
        y,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.all_worlds().count(),
        ),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H + 4.0;
    text(
        s,
        x0,
        y,
        &format!("THEME: {}", short(&theme.name.to_uppercase(), 20)),
        theme.text_dim,
        BODY_FONT,
        "start",
    );
    y += LINE_H;

    if matches!(theme.legend, LegendStyle::Compact) {
        y += 4.0;
        draw_compact_legend_body(s, sector, x0, y, opts);
        return;
    }

    // ROUTE TYPE
    text(s, x0, y, "ROUTE TYPE", theme.text, BODY_FONT, "start");
    y += LINE_H;
    match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => {
            for rt in RouteType::ALL {
                draw_route_pattern(
                    s,
                    x0,
                    BODY_FONT.mul_add(-0.3, y),
                    x0 + 30.0,
                    BODY_FONT.mul_add(-0.3, y),
                    theme.route_type,
                    3.0,
                    rt.pattern(opts.route_view_mode),
                );
                text(s, x0 + 38.0, y, rt.label(), theme.text, BODY_FONT, "start");
                y += LINE_H;
            }
        }
        crate::sector_model::RouteViewMode::TopLevel => {
            for kind in crate::sector_model::RouteKind::ALL {
                draw_route_pattern(
                    s,
                    x0,
                    BODY_FONT.mul_add(-0.3, y),
                    x0 + 30.0,
                    BODY_FONT.mul_add(-0.3, y),
                    theme.route_type,
                    3.0,
                    kind.patterns()[0],
                );
                text(
                    s,
                    x0 + 38.0,
                    y,
                    kind.label(),
                    theme.text,
                    BODY_FONT,
                    "start",
                );
                y += LINE_H;
            }
        }
    }
    y += 4.0;

    // ROUTE STABILITY
    text(s, x0, y, "ROUTE STABILITY", theme.text, BODY_FONT, "start");
    y += LINE_H;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Perilous, "PERILOUS"),
    ] {
        let color = stability_color(theme, stab);
        line(
            s,
            x0,
            BODY_FONT.mul_add(-0.3, y),
            x0 + 22.0,
            BODY_FONT.mul_add(-0.3, y),
            color,
            3.0,
            None,
        );
        text(s, x0 + 30.0, y, name, theme.text, BODY_FONT, "start");
        y += LINE_H;
    }
    y += 4.0;

    // ROUTE CONTROL
    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        text(s, x0, y, "ROUTE CONTROL", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let glyph_cx = x0 + 8.0;
        let glyph_size = 10.0;
        let half = glyph_size * 0.5;
        let neutral = theme.route_control_neutral;
        for (name, kind) in [
            ("PATROL", ControlKind::Patrol),
            ("TOLL", ControlKind::Toll),
            ("INTERDICTION", ControlKind::Interdiction),
            ("PIRACY", ControlKind::Piracy),
        ] {
            let cy_y = BODY_FONT.mul_add(-0.3, y);
            match kind {
                ControlKind::Patrol => {
                    circle(
                        s,
                        glyph_cx,
                        cy_y,
                        half,
                        neutral,
                        Some(darken(neutral, 0.5)),
                        1.0,
                    );
                }
                ControlKind::Toll => {
                    rect(
                        s,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        neutral,
                        Some(darken(neutral, 0.5)),
                    );
                }
                ControlKind::Interdiction => {
                    line(
                        s,
                        glyph_cx,
                        cy_y - half,
                        glyph_cx,
                        cy_y + half,
                        neutral,
                        2.0,
                        None,
                    );
                }
                ControlKind::Piracy => {
                    line(
                        s,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_cx + half,
                        cy_y + half,
                        neutral,
                        2.0,
                        None,
                    );
                    line(
                        s,
                        glyph_cx - half,
                        cy_y + half,
                        glyph_cx + half,
                        cy_y - half,
                        neutral,
                        2.0,
                        None,
                    );
                }
            }
            text(s, x0 + 22.0, y, name, theme.text, BODY_FONT, "start");
            y += LINE_H;
        }
        y += 4.0;
    }

    // FACTIONS
    if !sector.factions.is_empty() {
        text(s, x0, y, "FACTIONS", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            crate::importance::DEFAULT_MINOR_FRACTION,
            crate::importance::DEFAULT_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, world_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    world_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, *world_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    world_count,
                    ..
                } => (
                    label.to_uppercase(),
                    *system_count,
                    *world_count,
                    (140, 140, 150),
                ),
            };
            let swatch_color = rgba_from_tuple(swatch_rgb);
            rect(
                s,
                x0,
                y - swatch + 2.0,
                swatch,
                swatch,
                swatch_color,
                Some(darken(swatch_color, 0.5)),
            );
            text(
                s,
                x0 + swatch + 8.0,
                y,
                &format!("{} ({} SYS, {} W)", short(&label, 16), sys_n, world_n),
                theme.text_dim,
                BODY_FONT,
                "start",
            );
            y += LINE_H;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4.0;
        text(s, x0, y, "HEATMAP", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        rect(
            s,
            x0,
            y - swatch + 2.0,
            swatch,
            swatch,
            chip,
            Some(darken(chip, 0.5)),
        );
        text(
            s,
            x0 + swatch + 8.0,
            y,
            opts.heatmap.label(),
            theme.text,
            BODY_FONT,
            "start",
        );
    }
}

fn draw_compact_legend_body(
    s: &mut String,
    sector: &GeneratedSector,
    x0: f32,
    mut y: f32,
    opts: &RenderOptions,
) {
    let theme = &opts.theme;
    text(s, x0, y, "ROUTES", theme.text, BODY_FONT, "start");
    y += LINE_H;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARD"),
        (RouteStability::Perilous, "PERIL"),
    ] {
        let color = stability_color(theme, stab);
        line(
            s,
            x0,
            BODY_FONT.mul_add(-0.3, y),
            x0 + 22.0,
            BODY_FONT.mul_add(-0.3, y),
            color,
            route_thickness(theme, stab),
            None,
        );
        text(s, x0 + 30.0, y, name, theme.text, BODY_FONT, "start");
        y += LINE_H;
    }
    y += 4.0;
    if !sector.factions.is_empty() {
        text(s, x0, y, "FACTIONS", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            crate::importance::DEFAULT_MINOR_FRACTION,
            crate::importance::DEFAULT_DISPLAY_CAP,
        );
        for b in &buckets {
            let (label, sys_n, swatch_rgb) = match b {
                crate::importance::DisplayBucket::Faction {
                    name,
                    kind,
                    id,
                    system_count,
                    ..
                } => {
                    let style = crate::faction_style::faction_style_rgb(kind, id, "lawful");
                    (name.to_uppercase(), *system_count, style.fill)
                }
                crate::importance::DisplayBucket::Aggregated {
                    label,
                    system_count,
                    ..
                } => (label.to_uppercase(), *system_count, (140, 140, 150)),
            };
            let swatch_color = rgba_from_tuple(swatch_rgb);
            rect(
                s,
                x0,
                y - swatch + 2.0,
                swatch,
                swatch,
                swatch_color,
                Some(darken(swatch_color, 0.5)),
            );
            text(
                s,
                x0 + swatch + 8.0,
                y,
                &format!("{} ({} SYS)", short(&label, 14), sys_n),
                theme.text_dim,
                BODY_FONT,
                "start",
            );
            y += LINE_H;
        }
    }
    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4.0;
        text(s, x0, y, "HEATMAP", theme.text, BODY_FONT, "start");
        y += LINE_H;
        let swatch = 12.0;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        rect(
            s,
            x0,
            y - swatch + 2.0,
            swatch,
            swatch,
            chip,
            Some(darken(chip, 0.5)),
        );
        text(
            s,
            x0 + swatch + 8.0,
            y,
            opts.heatmap.label(),
            theme.text,
            BODY_FONT,
            "start",
        );
    }
}
