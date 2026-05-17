//! Per-system map renderer: a star at the center, planets on concentric
//! orbit rings color-coded by world type, with a side legend.
//!
//! Re-uses the embedded font and drawing primitives from `bitmap.rs`. All
//! sizes scale by an integer factor so the same renderer can produce a
//! thumbnail or a ~4K-class poster.

use std::fs;

use camino::Utf8Path;
use image::{Rgba, RgbaImage};

use crate::bitmap::{
    darken, draw_circle, draw_rect_outline, draw_ring, draw_text, fill_circle, fill_rect, short,
    star_color, text_size, tint, BG, PANEL_BG, TEXT, TEXT_DIM,
};
use crate::errors::SectorError;
use crate::sector_model::{GeneratedSector, GeneratedSystem};

/// Common resolution presets. Map area is 720 × scale pixels tall; legend adds 360 × scale wide.
/// Scale 1 = ~720p (1080px total width). Scale 2 = 1440p. Scale 3 = 4K (2160px).
pub const RESOLUTION_720P: u32 = 1;
pub const RESOLUTION_1440P: u32 = 2;
pub const RESOLUTION_4K: u32 = 3;

/// Maximum supported scale. Cap prevents memory issues at extreme scales.
pub const MAX_SCALE: u32 = 5;

pub fn write_system_maps(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    scale: u32,
) -> Result<(), SectorError> {
    let systems_dir = output_dir.join("systems");
    fs::create_dir_all(&systems_dir).map_err(|e| SectorError::io(systems_dir.as_str(), e))?;
    for sys in &sector.systems {
        let path = systems_dir.join(format!("{}.png", sys.id));
        let img = render_system(sys, scale);
        img.save(path.as_std_path())
            .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    }
    Ok(())
}

/// Render a single system to a specific PNG path.
pub fn write_one_system_png(
    sys: &GeneratedSystem,
    path: &Utf8Path,
    scale: u32,
) -> Result<(), SectorError> {
    let img = render_system(sys, scale);
    img.save(path.as_std_path())
        .map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    Ok(())
}

struct SysGeom {
    scale: i32,
    side: i32,
    star_r: i32,
    orbit_base: i32,
    orbit_step: i32,
    planet_r: i32,
    legend_w: i32,
    legend_pad: i32,
    line_h: i32,
    text_scale: i32,
    title_scale: i32,
}

impl SysGeom {
    fn new(scale: u32) -> Self {
        let s = (scale.clamp(1, MAX_SCALE)) as i32;
        Self {
            scale: s,
            side: 720 * s,
            star_r: 40 * s,
            orbit_base: 90 * s,
            orbit_step: 50 * s,
            planet_r: 22 * s,
            legend_w: 360 * s,
            legend_pad: 18 * s,
            line_h: 18 * s,
            text_scale: s,
            title_scale: 2 * s,
        }
    }
}

fn render_system(sys: &GeneratedSystem, scale: u32) -> RgbaImage {
    let g = SysGeom::new(scale);
    let total_w = g.side + g.legend_w;
    let total_h = g.side;
    let mut img = RgbaImage::from_pixel(total_w as u32, total_h as u32, BG);
    let cx = g.side / 2;
    let cy = g.side / 2;

    // Orbits — one dim ring per occupied orbit slot.
    let max_orbit = i32::from(sys.worlds.iter().map(|w| w.orbit).max().unwrap_or(0));
    for o in 1..=max_orbit.max(1) {
        let r = g.orbit_base + (o - 1) * g.orbit_step;
        draw_ring(&mut img, cx, cy, r, g.scale, Rgba([55, 50, 72, 255]));
    }

    // Star: faint corona + filled disk + outline.
    let star = star_color(&sys.star.colour_code);
    fill_circle(&mut img, cx, cy, g.star_r + 4 * g.scale, tint(star, 0.55));
    fill_circle(&mut img, cx, cy, g.star_r, star);
    draw_circle(&mut img, cx, cy, g.star_r, darken(star, 0.55));

    // Planets.
    for w in &sys.worlds {
        let orbit = i32::from(w.orbit.max(1));
        let r = g.orbit_base + (orbit - 1) * g.orbit_step;
        let a = orbit_angle(w.index, orbit).to_radians();
        let px = cx + (r as f32 * a.cos()).round() as i32;
        let py = cy + (r as f32 * a.sin()).round() as i32;
        let color = world_type_color(&w.world.world_type);
        fill_circle(&mut img, px, py, g.planet_r, color);
        draw_circle(&mut img, px, py, g.planet_r, darken(color, 0.5));

        // Orbit number centered on the planet.
        let num = format!("{}", w.orbit);
        let (nw, nh) = text_size(&num, g.text_scale);
        draw_text(
            &mut img,
            px - nw / 2,
            py - nh / 2,
            &num,
            contrast_text(color),
            g.text_scale,
        );

        // World name under the planet disk.
        let name = short(&w.name.to_uppercase(), 14);
        let (tw, _) = text_size(&name, g.text_scale);
        draw_text(
            &mut img,
            px - tw / 2,
            py + g.planet_r + 4 * g.scale,
            &name,
            TEXT_DIM,
            g.text_scale,
        );
    }

    // Legend panel.
    fill_rect(&mut img, g.side, 0, g.legend_w, total_h, PANEL_BG);
    draw_legend(&mut img, sys, g.side, &g);

    img
}

fn draw_legend(img: &mut RgbaImage, sys: &GeneratedSystem, x_origin: i32, g: &SysGeom) {
    let x0 = x_origin + g.legend_pad;
    let mut y = g.legend_pad;
    let body = g.text_scale;
    let title = g.title_scale;
    let swatch = 12 * g.scale;
    let swatch_gap = 8 * g.scale;

    draw_text(
        img,
        x0,
        y,
        &format!("SYSTEM: {}", sys.id.to_uppercase()),
        TEXT,
        title,
    );
    y += g.line_h + 4 * g.scale;
    draw_text(img, x0, y, &short(&sys.name.to_uppercase(), 28), TEXT, body);
    y += g.line_h;
    draw_text(
        img,
        x0,
        y,
        &format!("COORD: Q={} R={}", sys.coord.q, sys.coord.r),
        TEXT_DIM,
        body,
    );
    y += g.line_h + 4 * g.scale;

    // Star swatch + label.
    draw_text(img, x0, y, "STAR", TEXT, body);
    y += g.line_h;
    let star = star_color(&sys.star.colour_code);
    fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, star);
    draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(star, 0.5));
    let star_label = format!(
        "{} - {}",
        sys.star.colour_code.to_uppercase(),
        sys.star.colour_name.to_uppercase()
    );
    draw_text(
        img,
        x0 + swatch + swatch_gap,
        y,
        &short(&star_label, 26),
        TEXT,
        body,
    );
    y += g.line_h + 4 * g.scale;

    // Worlds: orbit + name + type, with a per-type color swatch.
    draw_text(img, x0, y, "WORLDS", TEXT, body);
    y += g.line_h;
    for w in &sys.worlds {
        let color = world_type_color(&w.world.world_type);
        fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, color);
        draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(color, 0.5));
        let label = format!(
            "{}  {}  {}",
            w.orbit,
            short(&w.name.to_uppercase(), 12),
            short(&w.world.world_type.to_uppercase(), 12),
        );
        draw_text(img, x0 + swatch + swatch_gap, y, &label, TEXT, body);
        y += g.line_h;
    }
    y += 4 * g.scale;

    // Distinct world-type legend (so the color coding is unambiguous).
    let mut seen: Vec<&str> = Vec::new();
    for w in &sys.worlds {
        let t = w.world.world_type.as_str();
        if !seen.contains(&t) {
            seen.push(t);
        }
    }
    if !seen.is_empty() {
        draw_text(img, x0, y, "WORLD TYPES", TEXT, body);
        y += g.line_h;
        for t in seen {
            let color = world_type_color(t);
            fill_rect(img, x0, y + 2 * g.scale, swatch, swatch, color);
            draw_rect_outline(img, x0, y + 2 * g.scale, swatch, swatch, darken(color, 0.5));
            draw_text(
                img,
                x0 + swatch + swatch_gap,
                y,
                &short(&t.to_uppercase(), 26),
                TEXT,
                body,
            );
            y += g.line_h;
        }
    }
}

/// Deterministic angle (degrees) for a planet on a given orbit. Uses a
/// golden-angle offset so adjacent orbits don't line up radially.
fn orbit_angle(index: usize, orbit: i32) -> f32 {
    let base = orbit as f32 * 137.5;
    let phase = index as f32 * 47.0;
    (base + phase + 200.0).rem_euclid(360.0)
}

fn world_type_color(t: &str) -> Rgba<u8> {
    match t {
        "AgriWorld" => Rgba([120, 200, 110, 255]),
        "Asteroid" => Rgba([150, 145, 130, 255]),
        "BastionWorld" => Rgba([170, 175, 185, 255]),
        "DeathWorld" => Rgba([200, 60, 70, 255]),
        "DeadWorld" => Rgba([90, 85, 95, 255]),
        "ExtractiveColony" => Rgba([200, 130, 70, 255]),
        "FeralWorld" => Rgba([100, 150, 90, 255]),
        "FeudalWorld" => Rgba([180, 130, 70, 255]),
        "ForgeWorld" => Rgba([220, 110, 50, 255]),
        "FrontierWorld" => Rgba([210, 190, 130, 255]),
        "HiveWorld" => Rgba([200, 80, 200, 255]),
        "ImperialWorld" => Rgba([230, 220, 90, 255]),
        "IndustrialWorld" => Rgba([180, 110, 60, 255]),
        "Orbital" => Rgba([110, 200, 230, 255]),
        "PenalWorld" => Rgba([130, 60, 60, 255]),
        "PlanetaryDump" => Rgba([130, 130, 80, 255]),
        "PlanetaryMonument" => Rgba([230, 200, 90, 255]),
        "PleasureWorld" => Rgba([240, 150, 200, 255]),
        "ResearchStation" => Rgba([130, 170, 230, 255]),
        "ShrineWorld" => Rgba([230, 220, 200, 255]),
        "TombWorld" => Rgba([200, 200, 210, 255]),
        "WarpLostWorld" => Rgba([140, 90, 200, 255]),
        "Worldship" => Rgba([80, 100, 180, 255]),
        "XenosWorld" => Rgba([120, 220, 120, 255]),
        _ => Rgba([180, 180, 180, 255]),
    }
}

fn contrast_text(c: Rgba<u8>) -> Rgba<u8> {
    let luma = 0.2126 * f32::from(c.0[0]) + 0.7152 * f32::from(c.0[1]) + 0.0722 * f32::from(c.0[2]);
    if luma > 140.0 {
        Rgba([20, 20, 30, 255])
    } else {
        Rgba([240, 240, 245, 255])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::*;

    fn world(orbit: u8, index: usize, name: &str, ty: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: format!("sys-x-w{index:02}"),
            index,
            name: name.into(),
            orbit,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: ty.into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![],
            notes: vec![],
        }
    }

    fn sample_system() -> GeneratedSystem {
        GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Vaxanide".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "amber".into(),
                spectral_type: Some("A".into()),
                source_row_index: None,
            },
            worlds: vec![
                world(1, 1, "Olbia Prime", "AgriWorld"),
                world(2, 2, "Yperion", "DeathWorld"),
                world(3, 3, "Mourn Yperion", "DeathWorld"),
                world(4, 4, "Tarsi", "AgriWorld"),
                world(5, 5, "Last Meridian Inner", "AgriWorld"),
            ],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn renders_without_panicking() {
        let sys = sample_system();
        let img = render_system(&sys, 1);
        assert_eq!(img.width(), (720 + 360) as u32);
        assert_eq!(img.height(), 720);
    }

    #[test]
    fn scaled_render_is_larger() {
        let sys = sample_system();
        let img = render_system(&sys, 4);
        assert_eq!(img.width(), (720 + 360) as u32 * 4);
    }

    #[test]
    fn clamps_above_max_scale() {
        let sys = sample_system();
        let img = render_system(&sys, MAX_SCALE + 10);
        let expected = (720 + 360) * MAX_SCALE;
        assert_eq!(img.width(), expected);
    }

    #[test]
    fn resolution_presets_produce_expected_heights() {
        let sys = sample_system();
        assert_eq!(render_system(&sys, RESOLUTION_720P).height(), 720);
        assert_eq!(render_system(&sys, RESOLUTION_1440P).height(), 1440);
        assert_eq!(render_system(&sys, RESOLUTION_4K).height(), 2160);
    }
}
