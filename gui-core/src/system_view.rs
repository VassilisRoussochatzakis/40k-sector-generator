//! Per-system widget: star + orbit rings + planets. Click → planet/star pick.

use egui::{Align2, Color32, FontId, Pos2, Response, Sense, Stroke, Ui, Vec2};

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

/// §CTX1 Phase 6 — what a pointer position inside the [`SystemView`] widget
/// resolves to. Wider than [`SystemClick`] (which only fires for star/planet
/// hits in `show()`) so the SYSTEM-tab right-click menu can distinguish
/// EMPTY-ORBIT and BACKGROUND clicks for the `§6.8`/`§6.9` schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemPick {
    /// Click landed on the central star disk (or the special-location marker
    /// when the system has no star).
    Star,
    /// Click landed on a planet disk. Carries the world's `index` field so the
    /// caller can locate the matching [`sectorforge::sector_model::GeneratedWorld`].
    World(usize),
    /// Click landed on an orbit ring at an angle that no current planet
    /// occupies. `orbit` is `1..=max_orbit`.
    EmptyOrbit(u8),
    /// Click landed in empty space inside the widget (no star, no ring, no
    /// planet). Phase 6 §6.9 background schema.
    Background,
}

/// §CTX1 Phase 6 — hit-test `local_pos` (relative to the widget's `rect.min`)
/// against the planet/star/orbit-ring layout for `system`. Threshold tuned to
/// the same `planet_r * 1.25` /`star_r * 1.2` halos used by [`SystemView::show`]
/// for primary clicks. Pure read of `system` so unit tests can call it without
/// an egui context.
#[must_use]
pub fn pick_world(side: f32, system: &GeneratedSystem, local_pos: Pos2) -> SystemPick {
    let g = SystemGeom::new(side, system);
    let center = Pos2::new(side / 2.0, side / 2.0);

    // 1. Planet hit (nearest within 1.25 * planet_r).
    let mut best: Option<(usize, f32)> = None;
    for w in &system.worlds {
        let orbit = i32::from(w.orbit.max(1));
        let r = g.orbit_base + (orbit - 1) as f32 * g.orbit_step;
        let a = orbit_angle(w.index, orbit).to_radians();
        let p = Pos2::new(center.x + r * a.cos(), center.y + r * a.sin());
        let d = (p - local_pos).length();
        if d <= g.planet_r * 1.25 && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((w.index, d));
        }
    }
    if let Some((idx, _)) = best {
        return SystemPick::World(idx);
    }

    // 2. Star hit.
    let star_d = (center - local_pos).length();
    if star_d <= g.star_r * 1.2 {
        return SystemPick::Star;
    }

    // 3. Empty-orbit ring hit. Probe every potential orbit (1..=max+1) so the
    //    user can right-click an empty ring beyond the current max to add a
    //    planet there. Tolerance is `planet_r * 0.9` either side of the ring
    //    so the band is comfortable but does not overlap with the star halo.
    let max_orbit = system
        .worlds
        .iter()
        .map(|w| w.orbit)
        .max()
        .unwrap_or(0)
        .max(1);
    let probe = max_orbit.saturating_add(1);
    let dist_to_center = (local_pos - center).length();
    for o in 1..=probe {
        let ring_r = g.orbit_base + (o as f32 - 1.0) * g.orbit_step;
        if (dist_to_center - ring_r).abs() <= g.planet_r * 0.9 {
            return SystemPick::EmptyOrbit(o);
        }
    }

    SystemPick::Background
}

impl<'a> SystemView<'a> {
    pub fn show(self, ui: &mut Ui) -> (Response, Option<SystemClick>) {
        let g = SystemGeom::new(self.side, self.system);
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

        // Star / Center.
        if let Some(star_data) = &self.system.star {
            let star = star_color(&star_data.colour_code);
            painter.circle_filled(center, g.star_r + 4.0, tint(star, 0.55));
            painter.circle_filled(center, g.star_r, star);
            painter.circle_stroke(center, g.star_r, Stroke::new(1.5, darken(star, 0.55)));
            if self.selected == SystemSelection::Star {
                painter.circle_stroke(center, g.star_r + 8.0, Stroke::new(2.0, SELECTION));
            }
        } else {
            // Special location marker.
            let r = g.star_r * 1.5;
            let pts = vec![
                Pos2::new(center.x, center.y - r),
                Pos2::new(center.x + r, center.y),
                Pos2::new(center.x, center.y + r),
                Pos2::new(center.x - r, center.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                pts.clone(),
                Color32::TRANSPARENT,
                Stroke::new(2.0, TEXT_DIM),
            ));
            if self.selected == SystemSelection::Star {
                painter.add(egui::Shape::convex_polygon(
                    pts.iter().map(|p| *p + (*p - center) * 0.5).collect(),
                    Color32::TRANSPARENT,
                    Stroke::new(2.0, SELECTION),
                ));
            }
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

/// §CTX1 Phase 6 — public so [`pick_world`] (and any future external hit-test)
/// can reproduce the layout used by [`SystemView::show`].
pub struct SystemGeom {
    pub side: f32,
    pub star_r: f32,
    pub orbit_base: f32,
    pub orbit_step: f32,
    pub planet_r: f32,
}

impl SystemGeom {
    pub fn new(side: f32, sys: &GeneratedSystem) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::ids::{system_id, world_id};
    use sectorforge::sector_model::{GeneratedStar, GeneratedSystem, GeneratedWorld, HexCoord};
    use std::sync::Arc;

    fn sys_with_world(orbit: u8) -> GeneratedSystem {
        let id = system_id(1);
        let mut sys = GeneratedSystem::new_at(id.clone(), 1, HexCoord { q: 0, r: 0 }, "Alpha");
        sys.star = Some(GeneratedStar {
            colour_code: Arc::from("G"),
            colour_name: Arc::from("Yellow"),
            spectral_type: None,
            source_row_index: None,
        });
        let wid = world_id(1, 1);
        let mut w = GeneratedWorld::new(wid, 1, "W");
        w.orbit = orbit;
        sys.worlds.push(w);
        sys
    }

    #[test]
    fn pick_world_returns_star_at_center() {
        let sys = sys_with_world(2);
        let side = 480.0;
        let pick = pick_world(side, &sys, Pos2::new(side / 2.0, side / 2.0));
        assert_eq!(pick, SystemPick::Star);
    }

    #[test]
    fn pick_world_returns_background_at_corner() {
        let sys = sys_with_world(2);
        let pick = pick_world(480.0, &sys, Pos2::new(2.0, 2.0));
        assert_eq!(pick, SystemPick::Background);
    }

    #[test]
    fn pick_world_returns_world_when_on_planet() {
        let sys = sys_with_world(3);
        let g = SystemGeom::new(480.0, &sys);
        let center = Pos2::new(240.0, 240.0);
        let orbit = 3_i32;
        let r = g.orbit_base + (orbit - 1) as f32 * g.orbit_step;
        let a = orbit_angle(1, orbit).to_radians();
        let p = Pos2::new(center.x + r * a.cos(), center.y + r * a.sin());
        let pick = pick_world(480.0, &sys, p);
        assert_eq!(pick, SystemPick::World(1));
    }

    #[test]
    fn pick_world_returns_empty_orbit_on_ring_without_planet() {
        // Place world on orbit 1 — probe orbit 2's ring on the *opposite* angle
        // so there is no planet there.
        let sys = sys_with_world(1);
        let g = SystemGeom::new(480.0, &sys);
        let center = Pos2::new(240.0, 240.0);
        let r = g.orbit_base + g.orbit_step;
        let p = Pos2::new(center.x + r, center.y);
        let pick = pick_world(480.0, &sys, p);
        assert_eq!(pick, SystemPick::EmptyOrbit(2));
    }
}
