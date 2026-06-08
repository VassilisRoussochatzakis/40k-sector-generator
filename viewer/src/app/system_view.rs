use egui::{RichText, ScrollArea, SidePanel, TopBottomPanel};

use sectorforge::ids::SystemId;

use super::{info_panel, palette, App, PendingExport, View};
use crate::editor::state::SectorEditTool;
use crate::system_view::{SystemClick, SystemLayout, SystemSelection, SystemView};

impl App {
    pub(super) fn draw_system_layout(
        &mut self,
        ctx: &egui::Context,
        system_id: &SystemId,
        selection: SystemSelection,
    ) {
        SidePanel::right("info")
            .resizable(true)
            .default_width(320.0)
            .min_width(260.0)
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_panel())
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    let sys_opt = self.system_by_id(system_id.as_str()).cloned();
                    if let Some(sys) = sys_opt.as_ref() {
                        let sector = self.sector.as_ref().expect("sector loaded");
                        match selection {
                            SystemSelection::World(idx) => {
                                if let Some(w) = sys.worlds.iter().find(|w| w.index == idx) {
                                    info_panel::world_detail(ui, w);
                                    info_panel::world_history(ui, sector, w.id.as_str());
                                    ui.add_space(10.0);
                                }
                            }
                            SystemSelection::Star => {
                                info_panel::star_detail(ui, sys);
                                ui.add_space(10.0);
                            }
                            SystemSelection::None => {}
                            _ => {}
                        }
                        ui.separator();
                        info_panel::system_summary(ui, sys, sector, self.sector_map_cache.as_ref());
                        ui.add_space(10.0);
                        if ui.button(RichText::new("← BACK TO SECTOR")).clicked() {
                            self.view = View::Sector;
                        }
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette::chrome_bg()))
            .show(ctx, |ui| self.show_system(ui, system_id, selection));
    }

    pub(super) fn show_system(
        &mut self,
        ui: &mut egui::Ui,
        system_id: &SystemId,
        selection: SystemSelection,
    ) {
        let sys_id_owned = system_id.clone();
        TopBottomPanel::bottom("system_controls")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(6.0),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("SIZE").color(palette::chrome_text_dim()));
                    ui.add(
                        egui::Slider::new(&mut self.system_side, 400.0..=1200.0).show_value(false),
                    );
                    ui.separator();
                    ui.label(RichText::new("LAYOUT").color(palette::chrome_text_dim()));
                    let mut horiz = matches!(self.system_layout, SystemLayout::Horizontal);
                    if ui
                        .selectable_label(horiz, RichText::new("HORIZ"))
                        .on_hover_text("star left, planets arrayed right in orbit order")
                        .clicked()
                    {
                        horiz = true;
                    }
                    if ui
                        .selectable_label(!horiz, RichText::new("ORBITAL"))
                        .on_hover_text("concentric orbit rings")
                        .clicked()
                    {
                        horiz = false;
                    }
                    self.system_layout = if horiz {
                        SystemLayout::Horizontal
                    } else {
                        SystemLayout::Orbital
                    };
                    if ui
                        .button(RichText::new("EXPORT MAP PNG"))
                        .on_hover_text("export this system's map to a PNG")
                        .clicked()
                    {
                        self.pending_export = Some(PendingExport::SystemPng(sys_id_owned.clone()));
                    }
                    ui.separator();
                    if ui
                        .selectable_label(self.map_edit_mode, RichText::new("EDIT MAP"))
                        .clicked()
                    {
                        self.map_edit_mode = !self.map_edit_mode;
                        self.editor.tool = SectorEditTool::Select;
                        self.pending_route_start = None;
                    }
                    if self.map_edit_mode {
                        if ui.button(RichText::new("ADD PLANET")).clicked() {
                            if let Some(world_index) = self.add_planet_to_system(&sys_id_owned) {
                                self.view = View::System {
                                    system_id: sys_id_owned.clone(),
                                    selection: SystemSelection::World(world_index),
                                };
                            }
                        }
                        let selected_world = match selection {
                            SystemSelection::World(idx) => Some(idx),
                            SystemSelection::None | SystemSelection::Star => None,
                            _ => None,
                        };
                        if ui
                            .add_enabled(
                                selected_world.is_some(),
                                egui::Button::new(RichText::new("REMOVE PLANET")),
                            )
                            .clicked()
                        {
                            if let Some(idx) = selected_world {
                                self.remove_planet_from_system(&sys_id_owned, idx);
                            }
                        }
                    }
                    if self.editor.dirty {
                        ui.label(RichText::new("UNSAVED").color(palette::warning()));
                    }
                });
            });
        ScrollArea::both().show(ui, |ui| {
            let sys_clone = self.system_by_id(system_id.as_str()).cloned();
            let Some(sys) = sys_clone else {
                ui.label(RichText::new("system not found").color(palette::danger()));
                return;
            };
            let (_resp, click) = SystemView {
                system: &sys,
                selected: selection,
                side: self.system_side,
                height: self.system_side,
                layout: self.system_layout,
            }
            .show(ui);
            if let Some(c) = click {
                let new_sel = match c {
                    SystemClick::Star => SystemSelection::Star,
                    SystemClick::World(i) => SystemSelection::World(i),
                    _ => SystemSelection::None,
                };
                self.view = View::System {
                    system_id: system_id.clone(),
                    selection: new_sel,
                };
            }
        });
    }

    pub(super) fn add_planet_to_system(
        &mut self,
        system_id: &sectorforge::ids::SystemId,
    ) -> Option<usize> {
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return None;
        };
        let Some(sys) = sector.systems.iter_mut().find(|s| &s.id == system_id) else {
            self.export_status = "system not found".into();
            return None;
        };
        let next = sys.worlds.iter().map(|w| w.index).max().unwrap_or(0) + 1;
        let mut world =
            sectorforge::sector_model::empty_world(sys.index, next, format!("Planet {next}"));
        if let Some(star) = &sys.star {
            world.world.star_colour = star
                .colour_code
                .parse()
                .unwrap_or(sectorforge::worlds::StarColour::White);
        } else {
            world.world.star_colour = sectorforge::worlds::StarColour::White;
        }
        sys.worlds.push(world);
        self.mark_live_sector_dirty(format!("added planet {}:{}", system_id, next));
        Some(next)
    }

    pub(super) fn remove_planet_from_system(
        &mut self,
        system_id: &sectorforge::ids::SystemId,
        world_index: usize,
    ) {
        let Some(sector) = self.editor.sector.as_mut() else {
            self.export_status = "no sector loaded".into();
            return;
        };
        let Some(sys) = sector.systems.iter_mut().find(|s| &s.id == system_id) else {
            self.export_status = "system not found".into();
            return;
        };
        let removed_world_id = sys
            .worlds
            .iter()
            .find(|w| w.index == world_index)
            .map(|w| w.id.clone());
        let before = sys.worlds.len();
        sys.worlds.retain(|w| w.index != world_index);
        if sys.worlds.len() == before {
            self.export_status = "selected planet not found".into();
            return;
        }
        if let Some(world_id) = removed_world_id {
            for faction in &mut sector.factions {
                faction.world_presence.retain(|x| x != &world_id);
            }
        }
        self.view = View::System {
            system_id: system_id.clone(),
            selection: SystemSelection::None,
        };
        self.mark_live_sector_dirty(format!("removed planet {}:{}", system_id, world_index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::ids::{system_id, world_id, FactionId};
    use sectorforge::sector_model::{
        empty_faction, empty_sector, empty_system, empty_world, GeneratedStar, HexCoord, SystemKind,
    };
    use sectorforge::worlds::StarColour;

    /// Two `Star` systems (`sys-0001` at (0,0) with one world `sys-0001-w01`,
    /// `sys-0002` at (3,0)) plus a faction present on `sys-0001` and its world.
    /// Tests mutate `editor.sector` (source of truth) and assert against it.
    fn app_with_two_systems() -> App {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        let sector = app.editor.sector.as_mut().unwrap();
        let mut s1 = empty_system(
            system_id(1),
            1,
            "S1".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            None,
        );
        s1.worlds.push(empty_world(1, 1, "W1".into()));
        let s2 = empty_system(
            system_id(2),
            2,
            "S2".into(),
            HexCoord { q: 3, r: 0 },
            SystemKind::Star,
            None,
        );
        sector.systems.push(s1);
        sector.systems.push(s2);
        let mut f = empty_faction(&FactionId::new("imperium"));
        f.system_presence.push(system_id(1));
        f.world_presence.push(world_id(1, 1));
        sector.factions.push(f);
        app
    }

    /// A single starless `Star`-kind system at index 1 with no worlds.
    fn app_with_one_starless_system() -> App {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        let sys = empty_system(
            system_id(1),
            1,
            "S".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            None,
        );
        app.editor.sector.as_mut().unwrap().systems.push(sys);
        app
    }

    // ── Gap 220: remove_planet_from_system ──────────────────────────────────

    /// Removing a planet scrubs it from the system's worlds and from every
    /// faction's `world_presence`, resets the system view selection to `None`, and
    /// marks the editor dirty.
    #[test]
    fn remove_planet_from_system_scrubs_presence_and_marks_dirty() {
        let mut app = app_with_two_systems();

        app.remove_planet_from_system(&system_id(1), 1);

        let sector = app.editor.sector.as_ref().unwrap();
        // sys-0001 now has zero worlds
        assert!(sector
            .systems
            .iter()
            .find(|s| s.id == system_id(1))
            .unwrap()
            .worlds
            .is_empty());
        // faction world_presence scrubbed of sys-0001-w01
        assert!(sector.factions[0].world_presence.is_empty());
        // selection reset to the system view with no sub-selection
        match &app.view {
            View::System {
                system_id: open_id,
                selection,
            } => {
                assert_eq!(*open_id, system_id(1));
                assert!(matches!(selection, SystemSelection::None));
            }
            other => panic!("expected View::System, got {other:?}"),
        }
        assert!(app.editor.dirty);
    }

    /// Removing a non-existent planet index is a no-op: it sets a status and does
    /// NOT mark the editor dirty, leaving the system's worlds intact.
    #[test]
    fn remove_planet_from_system_not_found_is_noop_and_not_dirty() {
        let mut app = app_with_two_systems();
        app.editor.dirty = false;

        app.remove_planet_from_system(&system_id(1), 999);

        assert_eq!(app.export_status, "selected planet not found");
        assert!(!app.editor.dirty);
        assert_eq!(
            app.editor
                .sector
                .as_ref()
                .unwrap()
                .systems
                .iter()
                .find(|s| s.id == system_id(1))
                .unwrap()
                .worlds
                .len(),
            1
        );
    }

    // ── Gap 225: add_planet_to_system ───────────────────────────────────────

    /// On a starless system with no worlds, adding a planet returns index 1, names
    /// it `"Planet 1"`, defaults the star colour to `White`, and marks dirty.
    #[test]
    fn add_planet_to_starless_system_defaults_white() {
        let mut app = app_with_one_starless_system();

        let idx = app.add_planet_to_system(&system_id(1)).unwrap();

        assert_eq!(idx, 1);
        let sector = app.editor.sector.as_ref().unwrap();
        let sys = sector
            .systems
            .iter()
            .find(|s| s.id == system_id(1))
            .unwrap();
        let world = sys.worlds.iter().find(|w| w.index == 1).unwrap();
        assert_eq!(&*world.name, "Planet 1");
        assert_eq!(world.index, 1);
        assert_eq!(world.world.star_colour, StarColour::White);
        assert!(app.editor.dirty);
    }

    /// The new planet's index is `max(existing world index) + 1`: with a world at
    /// index 5 already present, the next planet is index 6 / `"Planet 6"`.
    #[test]
    fn add_planet_index_is_max_plus_one() {
        let mut app = app_with_one_starless_system();
        app.editor.sector.as_mut().unwrap().systems[0]
            .worlds
            .push(empty_world(1, 5, "old".into()));

        let idx = app.add_planet_to_system(&system_id(1)).unwrap();

        assert_eq!(idx, 6);
        let sector = app.editor.sector.as_ref().unwrap();
        let world = sector.systems[0]
            .worlds
            .iter()
            .find(|w| w.index == 6)
            .unwrap();
        assert_eq!(&*world.name, "Planet 6");
    }

    /// A new planet inherits the system star's colour when its `colour_code` parses
    /// (`"K"` → `OrangeDwarf`).
    #[test]
    fn add_planet_inherits_parseable_star_colour() {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        let sys = empty_system(
            system_id(1),
            1,
            "S".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            Some(GeneratedStar {
                colour_code: "K".into(),
                colour_name: "orange".into(),
                spectral_type: None,
                source_row_index: None,
            }),
        );
        app.editor.sector.as_mut().unwrap().systems.push(sys);

        app.add_planet_to_system(&system_id(1)).unwrap();

        let sector = app.editor.sector.as_ref().unwrap();
        let world = sector.systems[0]
            .worlds
            .iter()
            .find(|w| w.index == 1)
            .unwrap();
        assert_eq!(world.world.star_colour, StarColour::OrangeDwarf);
    }

    /// An unparseable star `colour_code` (e.g. `"W"`) falls back to `White` — the
    /// planet is still created (the `None`/status branch is system-not-found only).
    #[test]
    fn add_planet_unparseable_star_colour_falls_back_to_white() {
        let mut app = App::new(empty_sector("t", "T", "s", 8, 8));
        let sys = empty_system(
            system_id(1),
            1,
            "S".into(),
            HexCoord { q: 0, r: 0 },
            SystemKind::Star,
            Some(GeneratedStar {
                colour_code: "W".into(),
                colour_name: "weird".into(),
                spectral_type: None,
                source_row_index: None,
            }),
        );
        app.editor.sector.as_mut().unwrap().systems.push(sys);

        let idx = app.add_planet_to_system(&system_id(1)).unwrap();

        assert_eq!(idx, 1);
        let sector = app.editor.sector.as_ref().unwrap();
        let world = sector.systems[0]
            .worlds
            .iter()
            .find(|w| w.index == 1)
            .unwrap();
        assert_eq!(world.world.star_colour, StarColour::White);
    }

    /// Adding a planet to an unknown system id returns `None`, sets the
    /// `"system not found"` status, and adds no world.
    #[test]
    fn add_planet_to_unknown_system_returns_none() {
        let mut app = app_with_one_starless_system();
        app.editor.dirty = false;

        let result = app.add_planet_to_system(&SystemId::new("sys-9999"));

        assert!(result.is_none());
        assert_eq!(app.export_status, "system not found");
        assert!(!app.editor.dirty);
        // the existing system gained no world
        assert!(app.editor.sector.as_ref().unwrap().systems[0]
            .worlds
            .is_empty());
    }
}
