//! Per-system widget: star + orbit rings + planets. Click → planet/star pick.

use egui::{Align2, FontId, Pos2, Response, Sense, Stroke, Ui, Vec2};

use sectorforge::sector_model::GeneratedSystem;

use super::palette::{
    self, contrast_text, darken, star_color, tint, world_type_color, ORBIT_RING, SELECTION,
    TEXT_DIM,
};

pub struct SystemView<'a> {
    pub system: &'a GeneratedSystem,
    pub selected: SystemSelection,
    pub side: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemSelection {
    None,
    Star,
    World(usize),
}

pub enum SystemClick {
    Star,
    World(usize),
}

impl<'a> SystemView<'a> {
    pub fn show(self, ui: &mut Ui) -> (Response, Option<SystemClick>) {
        let g = Geom::new(self.side, self.system);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(g.side, g.side), Sense::click());
        let painter = ui.painter_at(rect);
        let origin = rect.min;
        let center = Pos2::new(origin.x + g.side / 2.0, origin.y + g.side / 2.0);

        painter.rect_filled(rect, 0.0, palette::BG);

        // Orbit rings.
        let max_orbit = i32::from(
            self.system
                .worlds
                .iter()
                .map(|w| w.orbit)
                .max()
                .unwrap_or(0),
        );
        for o in 1..=max_orbit.max(1) {
            let r = g.orbit_base + (o - 1) as f32 * g.orbit_step;
            painter.circle_stroke(center, r, Stroke::new(1.0, ORBIT_RING));
        }

        // Star.
        let star = star_color(&self.system.star.colour_code);
        painter.circle_filled(center, g.star_r + 4.0, tint(star, 0.55));
        painter.circle_filled(center, g.star_r, star);
        painter.circle_stroke(center, g.star_r, Stroke::new(1.5, darken(star, 0.55)));
        if self.selected == SystemSelection::Star {
            painter.circle_stroke(center, g.star_r + 8.0, Stroke::new(2.0, SELECTION));
        }

        // Planets.
        let mut planet_positions: Vec<(usize, Pos2, f32)> = Vec::new();
        for w in &self.system.worlds {
            let orbit = i32::from(w.orbit.max(1));
            let r = g.orbit_base + (orbit - 1) as f32 * g.orbit_step;
            let a = orbit_angle(w.index, orbit).to_radians();
            let p = Pos2::new(center.x + r * a.cos(), center.y + r * a.sin());
            let color = world_type_color(&w.world.world_type);
            if self.selected == SystemSelection::World(w.index) {
                painter.circle_stroke(p, g.planet_r + 6.0, Stroke::new(2.0, SELECTION));
            }
            painter.circle_filled(p, g.planet_r, color);
            painter.circle_stroke(p, g.planet_r, Stroke::new(1.2, darken(color, 0.5)));

            // Orbit number inside planet.
            painter.text(
                p,
                Align2::CENTER_CENTER,
                w.orbit.to_string(),
                FontId::monospace(g.planet_r * 0.7),
                contrast_text(color),
            );
            // Name below.
            let name = short_upper(&w.name, 14);
            painter.text(
                Pos2::new(p.x, p.y + g.planet_r + 4.0),
                Align2::CENTER_TOP,
                name,
                FontId::monospace((g.planet_r * 0.55).max(9.0)),
                TEXT_DIM,
            );

            planet_positions.push((w.index, p, g.planet_r));
        }

        let mut click = None;
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let star_d = (center - pos).length();
                let planet_hit = planet_positions
                    .iter()
                    .map(|(idx, p, r)| (*idx, (*p - pos).length(), *r))
                    .filter(|(_, d, r)| *d <= *r * 1.25)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((idx, _, _)) = planet_hit {
                    click = Some(SystemClick::World(idx));
                } else if star_d <= g.star_r * 1.2 {
                    click = Some(SystemClick::Star);
                }
            }
        }

        (response, click)
    }
}

struct Geom {
    side: f32,
    star_r: f32,
    orbit_base: f32,
    orbit_step: f32,
    planet_r: f32,
}

impl Geom {
    fn new(side: f32, sys: &GeneratedSystem) -> Self {
        let max_orbit = f32::from(sys.worlds.iter().map(|w| w.orbit).max().unwrap_or(1).max(1));
        let usable = side * 0.45;
        let orbit_base = side * 0.13;
        let orbit_step = ((usable - orbit_base) / max_orbit).max(20.0);
        Self {
            side,
            star_r: side * 0.055,
            orbit_base,
            orbit_step,
            planet_r: (side * 0.03).max(12.0),
        }
    }
}

fn orbit_angle(index: usize, orbit: i32) -> f32 {
    let base = orbit as f32 * 137.5;
    let phase = index as f32 * 47.0;
    (base + phase + 200.0).rem_euclid(360.0)
}

fn short_upper(s: &str, max: usize) -> String {
    let upper = s.to_ascii_uppercase();
    if upper.chars().count() <= max {
        upper
    } else {
        let mut out: String = upper.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}
