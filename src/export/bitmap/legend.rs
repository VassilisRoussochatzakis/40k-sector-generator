//! Right-hand legend (title, route key, faction list, heatmap chip).

use image::{Rgba, RgbaImage};

use crate::heatmap::HeatmapMode;
use crate::importance::{
    DEFAULT_DISPLAY_CAP as FACTION_DISPLAY_CAP, DEFAULT_MINOR_FRACTION as FACTION_MINOR_FRACTION,
};
use crate::map_theme::LegendStyle;
use crate::sector_model::{GeneratedSector, RouteStability, RouteType};

use super::colors::{darken, rgba, route_thickness, short, stability_color};
use super::geom::Geom;
use super::primitives::{draw_line_thick, draw_rect_outline, draw_text, fill_circle, fill_rect};
use super::routes::{draw_route_pattern_legend, ControlKind};
use super::RenderOptions;

pub(super) fn legend_height(sector: &GeneratedSector, g: &Geom, opts: &RenderOptions) -> i32 {
    if matches!(opts.theme.legend, LegendStyle::Hidden) {
        return 0;
    }
    if matches!(opts.theme.legend, LegendStyle::Compact) {
        let heatmap_lines = if matches!(opts.heatmap, HeatmapMode::Off) {
            0
        } else {
            2
        };
        let lines = 4 + 1 + 5 + 1 + factions_visible(sector) + heatmap_lines;
        return g.legend_pad * 2 + lines as i32 * g.line_h;
    }
    // title block (4) + spacer
    // + ROUTE TYPE header + N type rows + spacer
    // + ROUTE STABILITY header + 4 stab rows + spacer
    // + optional ROUTE CONTROL header + 4 rows + spacer
    // + factions block + optional heatmap row + footer pad.
    let heatmap_lines = if matches!(opts.heatmap, HeatmapMode::Off) {
        0
    } else {
        2
    };
    let route_control_lines = if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        5
    } else {
        0
    };
    let route_type_rows = match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => RouteType::ALL.len() as i32,
        crate::sector_model::RouteViewMode::TopLevel => {
            crate::sector_model::RouteKind::ALL.len() as i32
        }
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
    g.legend_pad * 2 + lines * g.line_h
}

fn factions_visible(sector: &GeneratedSector) -> usize {
    if sector.factions.is_empty() {
        return 0;
    }
    let buckets = crate::importance::compute_display_buckets(
        sector,
        FACTION_MINOR_FRACTION,
        FACTION_DISPLAY_CAP,
    );
    1 + buckets.len()
}

pub(super) fn draw_legend(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    map_w: i32,
    g: &Geom,
    opts: &RenderOptions,
) {
    let x0 = map_w + g.legend_pad;
    let mut y = g.legend_pad;
    let line_h = g.line_h;
    let body = g.text_scale;
    let title = g.title_scale;
    let swatch = 12 * g.scale;

    let title_text = format!("SECTOR: {}", sector.id.to_uppercase());
    draw_text(img, x0, y, &title_text, opts.theme.text, title);
    y += line_h + 4 * g.scale;
    draw_text(
        img,
        x0,
        y,
        &format!("SEED: {}", short(&sector.seed, 20)),
        opts.theme.text_dim,
        body,
    );
    y += line_h - 4 * g.scale;
    draw_text(
        img,
        x0,
        y,
        &format!(
            "{}x{} - {} SYS, {} WORLDS",
            sector.width,
            sector.height,
            sector.systems.len(),
            sector.all_worlds().count(),
        ),
        opts.theme.text_dim,
        body,
    );
    y += line_h + 4 * g.scale;

    draw_text(
        img,
        x0,
        y,
        &format!("THEME: {}", short(&opts.theme.name.to_uppercase(), 20)),
        opts.theme.text_dim,
        body,
    );
    y += line_h;

    if matches!(opts.theme.legend, LegendStyle::Compact) {
        y += 4 * g.scale;
        draw_compact_legend_body(img, sector, x0, y, g, opts);
        return;
    }

    draw_text(img, x0, y, "ROUTE TYPE", opts.theme.text, body);
    y += line_h;
    match opts.route_view_mode {
        crate::sector_model::RouteViewMode::Detailed => {
            for rtype in RouteType::ALL {
                draw_route_pattern_legend(
                    img,
                    x0,
                    y + 8 * g.scale,
                    x0 + 30 * g.scale,
                    y + 8 * g.scale,
                    opts.theme.route_type,
                    3 * g.scale,
                    rtype.pattern(opts.route_view_mode),
                );
                draw_text(
                    img,
                    x0 + 38 * g.scale,
                    y,
                    rtype.label(),
                    opts.theme.text,
                    body,
                );
                y += line_h;
            }
        }
        crate::sector_model::RouteViewMode::TopLevel => {
            for kind in crate::sector_model::RouteKind::ALL {
                draw_route_pattern_legend(
                    img,
                    x0,
                    y + 8 * g.scale,
                    x0 + 30 * g.scale,
                    y + 8 * g.scale,
                    opts.theme.route_type,
                    3 * g.scale,
                    kind.patterns()[0],
                );
                draw_text(
                    img,
                    x0 + 38 * g.scale,
                    y,
                    kind.label(),
                    opts.theme.text,
                    body,
                );
                y += line_h;
            }
        }
    }
    y += 4 * g.scale;

    draw_text(img, x0, y, "ROUTE STABILITY", opts.theme.text, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARDOUS"),
        (RouteStability::Perilous, "PERILOUS"),
    ] {
        let color = stability_color(&opts.theme, stab);
        draw_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 22 * g.scale,
            y + 8 * g.scale,
            color,
            3 * g.scale,
        );
        draw_text(img, x0 + 30 * g.scale, y, name, opts.theme.text, body);
        y += line_h;
    }
    y += 4 * g.scale;

    if sector.routes.iter().any(|r| !r.controls.is_empty()) {
        draw_text(img, x0, y, "ROUTE CONTROL", opts.theme.text, body);
        y += line_h;
        let glyph_cx = x0 + 8 * g.scale;
        let glyph_size = 10 * g.scale;
        let half = glyph_size / 2;
        let neutral = opts.theme.route_control_neutral;
        for (name, kind) in [
            ("PATROL", ControlKind::Patrol),
            ("TOLL", ControlKind::Toll),
            ("INTERDICTION", ControlKind::Interdiction),
            ("PIRACY", ControlKind::Piracy),
        ] {
            let cy_y = y + 8 * g.scale;
            match kind {
                ControlKind::Patrol => {
                    fill_circle(img, glyph_cx, cy_y, half, neutral);
                    super::primitives::draw_circle(img, glyph_cx, cy_y, half, darken(neutral, 0.5));
                }
                ControlKind::Toll => {
                    fill_rect(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        neutral,
                    );
                    draw_rect_outline(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_size,
                        glyph_size,
                        darken(neutral, 0.5),
                    );
                }
                ControlKind::Interdiction => {
                    draw_line_thick(
                        img,
                        glyph_cx,
                        cy_y - half,
                        glyph_cx,
                        cy_y + half,
                        neutral,
                        2 * g.scale,
                    );
                }
                ControlKind::Piracy => {
                    draw_line_thick(
                        img,
                        glyph_cx - half,
                        cy_y - half,
                        glyph_cx + half,
                        cy_y + half,
                        neutral,
                        2 * g.scale,
                    );
                    draw_line_thick(
                        img,
                        glyph_cx - half,
                        cy_y + half,
                        glyph_cx + half,
                        cy_y - half,
                        neutral,
                        2 * g.scale,
                    );
                }
            }
            draw_text(img, x0 + 22 * g.scale, y, name, opts.theme.text, body);
            y += line_h;
        }
        y += 4 * g.scale;
    }

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", opts.theme.text, body);
        y += line_h;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            FACTION_MINOR_FRACTION,
            FACTION_DISPLAY_CAP,
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
            let swatch_color = rgba(swatch_rgb);
            fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, swatch_color);
            draw_rect_outline(
                img,
                x0,
                y + 2 * g.scale,
                swatch,
                swatch,
                darken(swatch_color, 0.5),
            );
            draw_text(
                img,
                x0 + swatch + 8 * g.scale,
                y,
                &format!("{} ({} SYS, {} W)", short(&label, 16), sys_n, world_n),
                opts.theme.text_dim,
                body,
            );
            y += line_h;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4 * g.scale;
        draw_text(img, x0, y, "HEATMAP", opts.theme.text, body);
        y += line_h;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, chip);
        draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(chip, 0.5));
        draw_text(
            img,
            x0 + swatch + 8 * g.scale,
            y,
            opts.heatmap.label(),
            opts.theme.text,
            body,
        );
    }
}

fn draw_compact_legend_body(
    img: &mut RgbaImage,
    sector: &GeneratedSector,
    x0: i32,
    mut y: i32,
    g: &Geom,
    opts: &RenderOptions,
) {
    let body = g.text_scale;
    let line_h = g.line_h;
    let swatch = 12 * g.scale;

    draw_text(img, x0, y, "ROUTES", opts.theme.text, body);
    y += line_h;
    for (stab, name) in [
        (RouteStability::Stable, "STABLE"),
        (RouteStability::Unstable, "UNSTABLE"),
        (RouteStability::Hazardous, "HAZARD"),
        (RouteStability::Perilous, "PERIL"),
    ] {
        let color = stability_color(&opts.theme, stab);
        draw_line_thick(
            img,
            x0,
            y + 8 * g.scale,
            x0 + 22 * g.scale,
            y + 8 * g.scale,
            color,
            route_thickness(&opts.theme, stab, g),
        );
        draw_text(img, x0 + 30 * g.scale, y, name, opts.theme.text, body);
        y += line_h;
    }
    y += 4 * g.scale;

    if !sector.factions.is_empty() {
        draw_text(img, x0, y, "FACTIONS", opts.theme.text, body);
        y += line_h;
        let buckets = crate::importance::compute_display_buckets(
            sector,
            FACTION_MINOR_FRACTION,
            FACTION_DISPLAY_CAP,
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
            let swatch_color = rgba(swatch_rgb);
            fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, swatch_color);
            draw_rect_outline(
                img,
                x0,
                y + 2 * g.scale,
                swatch,
                swatch,
                darken(swatch_color, 0.5),
            );
            draw_text(
                img,
                x0 + swatch + 8 * g.scale,
                y,
                &format!("{} ({} SYS)", short(&label, 14), sys_n),
                opts.theme.text_dim,
                body,
            );
            y += line_h;
        }
    }

    if !matches!(opts.heatmap, HeatmapMode::Off) {
        y += 4 * g.scale;
        draw_text(img, x0, y, "HEATMAP", opts.theme.text, body);
        y += line_h;
        let (r, gc, b) = opts.heatmap.base_color_rgb();
        let chip = Rgba([r, gc, b, 255]);
        fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, chip);
        draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(chip, 0.5));
        draw_text(
            img,
            x0 + swatch + 8 * g.scale,
            y,
            opts.heatmap.label(),
            opts.theme.text,
            body,
        );
    }
}
