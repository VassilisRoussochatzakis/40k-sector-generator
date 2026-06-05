//! Segmentum overview widgets and on-disk bundle loading.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use sectorforge::errors::SectorError;
use sectorforge::sector_model::GeneratedSector;
use sectorforge::segmentum::{BorderOrientation, InterSectorLink, Segmentum, SegmentumChild};

use super::palette::{self, stability_color};

#[derive(Debug, Clone)]
pub struct SegmentumBundle {
    pub source_path: Utf8PathBuf,
    pub root_dir: Utf8PathBuf,
    pub segmentum: Segmentum,
    pub children: Vec<LoadedSegmentumChild>,
    by_id: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct LoadedSegmentumChild {
    pub id: String,
    pub sector_path: Utf8PathBuf,
    pub sector: GeneratedSector,
}

#[derive(Debug, Clone)]
pub(crate) enum SegmentumAction {
    OpenChild(String),
    OpenSystem {
        child_id: String,
        system_id: sectorforge::ids::SystemId,
    },
}

pub fn load_segmentum_bundle(path: &Utf8Path) -> Result<SegmentumBundle, SectorError> {
    let segmentum = sectorforge::load_segmentum_json(path)?;
    let root_dir = path
        .parent()
        .unwrap_or_else(|| Utf8Path::new("."))
        .to_path_buf();
    let mut children = Vec::with_capacity(segmentum.children.len());
    let mut by_id = BTreeMap::new();

    for meta in &segmentum.children {
        let sector_path = root_dir.join("children").join(&meta.id).join("sector.json");
        let sector = sectorforge::load_sector_json(&sector_path)?;

        by_id.insert(meta.id.clone(), children.len());
        children.push(LoadedSegmentumChild {
            id: meta.id.clone(),
            sector_path,
            sector,
        });
    }

    Ok(SegmentumBundle {
        source_path: path.to_path_buf(),
        root_dir,
        segmentum,
        children,
        by_id,
    })
}

impl SegmentumBundle {
    pub(crate) fn child(&self, id: &str) -> Option<&LoadedSegmentumChild> {
        self.by_id.get(id).and_then(|&idx| self.children.get(idx))
    }

    pub(crate) fn child_meta(&self, id: &str) -> Option<&SegmentumChild> {
        self.segmentum.children.iter().find(|c| c.id == id)
    }

    pub(crate) fn child_at(&self, column: u32, row: u32) -> Option<&SegmentumChild> {
        self.segmentum
            .children
            .iter()
            .find(|c| c.column == column && c.row == row)
    }

    pub(crate) fn system_name(&self, child_id: &str, system_id: &str) -> String {
        self.child(child_id)
            .and_then(|c| {
                c.sector
                    .systems
                    .iter()
                    .find(|s| s.id == system_id)
                    .map(|s| s.name.to_string())
            })
            .unwrap_or_else(|| system_id.to_string())
    }

    pub(crate) fn link(&self, id: &str) -> Option<&InterSectorLink> {
        self.segmentum
            .inter_sector_links
            .iter()
            .find(|l| l.id == id)
    }

    pub(crate) fn link_count_for_child(&self, id: &str) -> usize {
        self.segmentum
            .inter_sector_links
            .iter()
            .filter(|l| l.from_child_id == id || l.to_child_id == id)
            .count()
    }
}

pub(crate) fn show_overview(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<Arc<str>>,
    mode: sectorforge::sector_model::RouteViewMode,
) -> Option<SegmentumAction> {
    let mut action = None;
    header(ui, bundle);
    ui.add_space(10.0);
    action = action.or_else(|| super_map(ui, bundle, active_child_id, selected_link, mode));
    ui.add_space(12.0);
    action = action.or_else(|| super_grid(ui, bundle, active_child_id));
    ui.add_space(12.0);
    action = action.or_else(|| child_table(ui, bundle, active_child_id));
    ui.add_space(12.0);
    action = action.or_else(|| link_table(ui, bundle, selected_link, mode));
    action
}

pub(crate) fn show_side_panel(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<Arc<str>>,
    mode: sectorforge::sector_model::RouteViewMode,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new("SEGMENTUM")
            .color(palette::chrome_text())
            .strong(),
    );
    ui.label(RichText::new(&bundle.segmentum.title).color(palette::chrome_text_dim()));
    ui.add_space(8.0);

    if let Some(link_id) = selected_link.as_deref() {
        if let Some(link) = bundle.link(link_id) {
            ui.separator();
            action = action.or_else(|| link_detail(ui, bundle, link, mode));
            ui.add_space(8.0);
            if ui.button(RichText::new("CLEAR LINK")).clicked() {
                *selected_link = None;
            }
            return action;
        }
        *selected_link = None;
    }

    if let Some(id) = active_child_id {
        if let Some(meta) = bundle.child_meta(id) {
            ui.separator();
            child_detail(ui, bundle, meta);
            ui.add_space(8.0);
            if ui.button(RichText::new("OPEN ACTIVE MAP")).clicked() {
                action = Some(SegmentumAction::OpenChild(id.to_string()));
            }
        }
    }

    ui.separator();
    ui.label(
        RichText::new("FILES")
            .color(palette::chrome_text())
            .strong(),
    );
    kv(ui, "segmentum", bundle.source_path.as_str());
    kv(ui, "root", bundle.root_dir.as_str());
    action
}

fn header(ui: &mut Ui, bundle: &SegmentumBundle) {
    let seg = &bundle.segmentum;
    ui.label(
        RichText::new(format!("{} — {}", seg.id, seg.title))
            .color(palette::chrome_text())
            .strong(),
    );
    ui.label(
        RichText::new(format!(
            "{}x{} grid  ·  {} children  ·  {} inter-sector links",
            seg.columns,
            seg.rows,
            seg.children.len(),
            seg.inter_sector_links.len()
        ))
        .color(palette::chrome_text_dim()),
    );
    ui.add_space(8.0);
    egui::Grid::new("segmentum_stats")
        .num_columns(4)
        .spacing([18.0, 4.0])
        .show(ui, |ui| {
            stat(ui, "systems", bundle.segmentum.manifest.system_count);
            stat(ui, "worlds", bundle.segmentum.manifest.world_count);
            stat(ui, "routes", bundle.segmentum.manifest.route_count);
            stat(
                ui,
                "stitches",
                bundle.segmentum.manifest.inter_sector_link_count,
            );
            ui.end_row();
        });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        chip(
            ui,
            &format!("seed {}", seg.stitch_seed),
            palette::chrome_panel(),
        );
        chip(
            ui,
            &format!("factions {}", seg.faction_mode),
            palette::chrome_panel(),
        );
        chip(
            ui,
            &format!("{} {}", seg.generator_name, seg.generator_version),
            palette::chrome_panel(),
        );
    });
}

fn super_map(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<Arc<str>>,
    mode: sectorforge::sector_model::RouteViewMode,
) -> Option<SegmentumAction> {
    ui.label(
        RichText::new("SUPER-MAP")
            .color(palette::chrome_text())
            .strong(),
    );
    ui.add_space(4.0);

    let cell_w = 270.0;
    let cell_h = 220.0;
    let gap = 46.0;
    let cols = bundle.segmentum.columns.max(1) as f32;
    let rows = bundle.segmentum.rows.max(1) as f32;
    let total = Vec2::new(
        cols * cell_w + (cols - 1.0) * gap,
        rows * cell_h + (rows - 1.0) * gap,
    );
    let (rect, response) = ui.allocate_exact_size(total, Sense::click());
    palette::paint_rect_filled(ui, rect, rect, 0.0, palette::BG);

    let mut child_rects: BTreeMap<String, Rect> = BTreeMap::new();
    let mut centers: HashMap<(String, sectorforge::ids::SystemId), Pos2> = HashMap::new();

    for meta in &bundle.segmentum.children {
        let min = Pos2::new(
            rect.left() + meta.column as f32 * (cell_w + gap),
            rect.top() + meta.row as f32 * (cell_h + gap),
        );
        let child_rect = Rect::from_min_size(min, Vec2::new(cell_w, cell_h));
        child_rects.insert(meta.id.clone(), child_rect);

        let active = active_child_id == Some(meta.id.as_str());
        palette::paint_rect_filled(
            ui,
            rect,
            child_rect,
            2.0,
            if active {
                Color32::from_rgb(40, 36, 52)
            } else {
                palette::PANEL_BG
            },
        );
        palette::paint_rect_stroke(
            ui,
            rect,
            child_rect,
            2.0,
            Stroke::new(
                if active { 2.0 } else { 1.0 },
                if active {
                    palette::SELECTION
                } else {
                    palette::HEX_OUTLINE
                },
            ),
        );
        palette::paint_text(
            ui,
            rect,
            child_rect.left_top() + Vec2::new(10.0, 8.0),
            Align2::LEFT_TOP,
            meta.id.to_uppercase(),
            FontId::monospace(13.0),
            palette::TEXT,
        );
        palette::paint_text(
            ui,
            rect,
            child_rect.left_top() + Vec2::new(10.0, 24.0),
            Align2::LEFT_TOP,
            format!("{} sys / {} routes", meta.system_count, meta.route_count),
            FontId::monospace(10.0),
            palette::TEXT_DIM,
        );

        let plot = child_rect.shrink2(Vec2::new(14.0, 12.0));
        let plot = Rect::from_min_max(plot.min + Vec2::new(0.0, 28.0), plot.max);
        if let Some(loaded) = bundle.child(&meta.id) {
            for sys in &loaded.sector.systems {
                let p = scaled_system_pos(meta, sys.coord.q, sys.coord.r, plot);
                centers.insert((meta.id.clone(), sys.id.clone()), p);
            }
            for route in &loaded.sector.routes {
                let a = centers.get(&(meta.id.clone(), route.from_system_id.clone()));
                let b = centers.get(&(meta.id.clone(), route.to_system_id.clone()));
                if let (Some(&a), Some(&b)) = (a, b) {
                    palette::draw_route_line_clipped(
                        ui,
                        rect,
                        a,
                        b,
                        1.0,
                        stability_color(route.stability).linear_multiply(0.55),
                        route.route_type.pattern(mode),
                    );
                }
            }
            for sys in &loaded.sector.systems {
                if let Some(&p) = centers.get(&(meta.id.clone(), sys.id.clone())) {
                    let fill = if let Some(star) = &sys.star {
                        palette::star_color(&star.colour_code)
                    } else {
                        Color32::from_rgb(140, 140, 150)
                    };
                    palette::paint_circle_filled(ui, rect, p, 3.2, fill);
                    palette::paint_circle_stroke(
                        ui,
                        rect,
                        p,
                        3.2,
                        Stroke::new(0.8, Color32::BLACK),
                    );
                }
            }
        }
    }

    for link in &bundle.segmentum.inter_sector_links {
        let a = centers.get(&(link.from_child_id.clone(), link.from_system_id.clone()));
        let b = centers.get(&(link.to_child_id.clone(), link.to_system_id.clone()));
        let (Some(&a), Some(&b)) = (a, b) else {
            continue;
        };
        let selected = selected_link.as_deref() == Some(link.id.as_str());
        let color = if selected {
            palette::PATH_HIGHLIGHT
        } else {
            stability_color(link.stability)
        };
        palette::draw_route_line_clipped(
            ui,
            rect,
            a,
            b,
            if selected { 3.2 } else { 1.8 },
            color,
            link.route_type.pattern_for_key(
                &format!(
                    "{}:{}:{}",
                    bundle.segmentum.stitch_seed, link.id, link.distance_units
                ),
                mode,
            ),
        );
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        palette::paint_text(
            ui,
            rect,
            mid,
            Align2::CENTER_CENTER,
            &link.id,
            FontId::monospace(9.5),
            if selected {
                palette::TEXT
            } else {
                palette::TEXT_DIM
            },
        );
    }

    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((child_id, system_id)) = centers.iter().find_map(|((child, sys), p)| {
                (p.distance(pos) <= 7.0).then(|| (child.clone(), sys.clone()))
            }) {
                return Some(SegmentumAction::OpenSystem {
                    child_id,
                    system_id,
                });
            }
            for (child_id, child_rect) in child_rects {
                if child_rect.contains(pos) {
                    return Some(SegmentumAction::OpenChild(child_id));
                }
            }
        }
    }

    None
}

fn scaled_system_pos(meta: &SegmentumChild, q: i32, r: i32, plot: Rect) -> Pos2 {
    let w = (meta.width.saturating_sub(1)).max(1) as f32;
    let h = (meta.height.saturating_sub(1)).max(1) as f32;
    let x = plot.left() + (q.max(0) as f32 / w).clamp(0.0, 1.0) * plot.width();
    let y = plot.top() + (r.max(0) as f32 / h).clamp(0.0, 1.0) * plot.height();
    Pos2::new(x, y)
}

fn super_grid(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new("SUPER-GRID")
            .color(palette::chrome_text())
            .strong(),
    );
    ui.add_space(4.0);
    egui::Grid::new("segmentum_super_grid")
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            for row in 0..bundle.segmentum.rows {
                for col in 0..bundle.segmentum.columns {
                    if let Some(child) = bundle.child_at(col, row) {
                        let active = active_child_id == Some(child.id.as_str());
                        let frame = egui::Frame::none()
                            .fill(if active {
                                Color32::from_rgb(42, 38, 52)
                            } else {
                                palette::chrome_panel()
                            })
                            .stroke(Stroke::new(
                                if active { 2.0 } else { 1.0 },
                                if active {
                                    palette::SELECTION
                                } else {
                                    palette::HEX_OUTLINE
                                },
                            ))
                            .inner_margin(8.0);
                        frame.show(ui, |ui| {
                            ui.set_min_size(egui::vec2(220.0, 128.0));
                            ui.label(
                                RichText::new(child.id.to_uppercase())
                                    .color(palette::chrome_text())
                                    .strong(),
                            );
                            ui.label(RichText::new(&child.title).color(palette::chrome_text_dim()));
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "slot ({}, {})  {}x{}",
                                    child.column, child.row, child.width, child.height
                                ))
                                .color(palette::chrome_text_dim()),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} sys  {} worlds  {} routes",
                                    child.system_count, child.world_count, child.route_count
                                ))
                                .color(palette::chrome_text_dim()),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} stitch links",
                                    bundle.link_count_for_child(&child.id)
                                ))
                                .color(palette::chrome_text_dim()),
                            );
                            ui.add_space(4.0);
                            if ui.button(RichText::new("OPEN MAP")).clicked() {
                                action = Some(SegmentumAction::OpenChild(child.id.clone()));
                            }
                        });
                    } else {
                        egui::Frame::none()
                            .fill(Color32::from_rgb(18, 16, 24))
                            .stroke(Stroke::new(1.0, palette::HEX_OUTLINE))
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.set_min_size(egui::vec2(220.0, 128.0));
                                ui.label(
                                    RichText::new(format!("EMPTY ({}, {})", col, row))
                                        .color(palette::chrome_text_dim()),
                                );
                            });
                    }
                }
                ui.end_row();
            }
        });
    action
}

fn child_table(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new("CHILD SECTORS")
            .color(palette::chrome_text())
            .strong(),
    );
    ui.add_space(4.0);
    egui::Grid::new("segmentum_children")
        .num_columns(9)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in [
                "ID", "SLOT", "TITLE", "SEED", "SYS", "WORLDS", "ROUTES", "LINKS", "",
            ] {
                ui.label(RichText::new(h).color(palette::chrome_text_dim()).strong());
            }
            ui.end_row();
            for c in &bundle.segmentum.children {
                let active = active_child_id == Some(c.id.as_str());
                let color = if active {
                    palette::SELECTION
                } else {
                    palette::chrome_text()
                };
                ui.label(RichText::new(&c.id).color(color));
                ui.label(RichText::new(format!("({}, {})", c.column, c.row)));
                ui.label(RichText::new(&c.title));
                ui.label(RichText::new(&c.seed).color(palette::chrome_text_dim()));
                ui.label(RichText::new(c.system_count.to_string()));
                ui.label(RichText::new(c.world_count.to_string()));
                ui.label(RichText::new(c.route_count.to_string()));
                ui.label(RichText::new(
                    bundle.link_count_for_child(&c.id).to_string(),
                ));
                if ui.button(RichText::new("OPEN")).clicked() {
                    action = Some(SegmentumAction::OpenChild(c.id.clone()));
                }
                ui.end_row();
            }
        });
    action
}

fn link_table(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    selected_link: &mut Option<Arc<str>>,
    mode: sectorforge::sector_model::RouteViewMode,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new("INTER-SECTOR LINKS")
            .color(palette::chrome_text())
            .strong(),
    );
    ui.add_space(4.0);
    if bundle.segmentum.inter_sector_links.is_empty() {
        ui.label(RichText::new("none").color(palette::chrome_text_dim()));
        return None;
    }
    egui::Grid::new("segmentum_links")
        .num_columns(8)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in ["ID", "FROM", "TO", "EDGE", "UNITS", "TYPE", "STABILITY", ""] {
                ui.label(RichText::new(h).color(palette::chrome_text_dim()).strong());
            }
            ui.end_row();
            for l in &bundle.segmentum.inter_sector_links {
                let selected = selected_link.as_deref() == Some(l.id.as_str());
                let color = if selected {
                    palette::SELECTION
                } else {
                    palette::chrome_text()
                };
                ui.label(RichText::new(&l.id).color(color));
                endpoint_label(ui, bundle, &l.from_child_id, &l.from_system_id);
                endpoint_label(ui, bundle, &l.to_child_id, &l.to_system_id);
                ui.label(RichText::new(orientation_label(l.orientation)));
                ui.label(RichText::new(l.distance_units.to_string()));
                let type_label = match mode {
                    sectorforge::sector_model::RouteViewMode::Detailed => l.route_type.label(),
                    sectorforge::sector_model::RouteViewMode::TopLevel => {
                        l.route_type.kind().label()
                    }
                    _ => l.route_type.label(),
                };
                ui.label(RichText::new(type_label));
                ui.label(
                    RichText::new(format!("{}", l.stability)).color(stability_color(l.stability)),
                );
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("INFO")).clicked() {
                        *selected_link = Some(Arc::from(l.id.as_str()));
                    }
                    if ui.button(RichText::new("FROM")).clicked() {
                        action = Some(SegmentumAction::OpenSystem {
                            child_id: l.from_child_id.clone(),
                            system_id: l.from_system_id.clone(),
                        });
                    }
                    if ui.button(RichText::new("TO")).clicked() {
                        action = Some(SegmentumAction::OpenSystem {
                            child_id: l.to_child_id.clone(),
                            system_id: l.to_system_id.clone(),
                        });
                    }
                });
                ui.end_row();
            }
        });
    action
}

fn link_detail(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    link: &InterSectorLink,
    mode: sectorforge::sector_model::RouteViewMode,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new(format!("LINK {}", link.id.to_uppercase()))
            .color(palette::chrome_text())
            .strong(),
    );
    kv(ui, "edge", orientation_label(link.orientation));
    kv(ui, "units", &link.distance_units.to_string());
    match mode {
        sectorforge::sector_model::RouteViewMode::Detailed => {
            kv(ui, "type", link.route_type.label());
        }
        sectorforge::sector_model::RouteViewMode::TopLevel => {
            kv(ui, "type", link.route_type.kind().label());
        }
        _ => {
            kv(ui, "type", link.route_type.label());
        }
    }
    kv(
        ui,
        "stability",
        &format!("{}", link.stability).to_uppercase(),
    );
    ui.add_space(6.0);

    let from_name = bundle.system_name(&link.from_child_id, &link.from_system_id);
    let to_name = bundle.system_name(&link.to_child_id, &link.to_system_id);
    endpoint_detail(
        ui,
        "FROM",
        &link.from_child_id,
        &link.from_system_id,
        &from_name,
    );
    if ui.button(RichText::new("OPEN FROM SYSTEM")).clicked() {
        action = Some(SegmentumAction::OpenSystem {
            child_id: link.from_child_id.clone(),
            system_id: link.from_system_id.clone(),
        });
    }
    ui.add_space(8.0);
    endpoint_detail(ui, "TO", &link.to_child_id, &link.to_system_id, &to_name);
    if ui.button(RichText::new("OPEN TO SYSTEM")).clicked() {
        action = Some(SegmentumAction::OpenSystem {
            child_id: link.to_child_id.clone(),
            system_id: link.to_system_id.clone(),
        });
    }
    action
}

fn child_detail(ui: &mut Ui, bundle: &SegmentumBundle, child: &SegmentumChild) {
    ui.label(
        RichText::new(format!("ACTIVE CHILD {}", child.id.to_uppercase()))
            .color(palette::chrome_text())
            .strong(),
    );
    kv(ui, "title", &child.title);
    kv(ui, "slot", &format!("({}, {})", child.column, child.row));
    kv(ui, "sector", &child.sector_id);
    kv(ui, "seed", &child.seed);
    kv(ui, "systems", &child.system_count.to_string());
    kv(ui, "worlds", &child.world_count.to_string());
    kv(ui, "routes", &child.route_count.to_string());
    kv(
        ui,
        "stitches",
        &bundle.link_count_for_child(&child.id).to_string(),
    );
    if let Some(loaded) = bundle.child(&child.id) {
        kv(ui, "file", loaded.sector_path.as_str());
        ui.add_space(6.0);
        ui.label(
            RichText::new("LOCAL BORDER LINKS")
                .color(palette::chrome_text())
                .strong(),
        );
        for l in bundle
            .segmentum
            .inter_sector_links
            .iter()
            .filter(|l| l.from_child_id == child.id || l.to_child_id == child.id)
        {
            ui.label(
                RichText::new(format!(
                    "{}  {}:{} ↔ {}:{}",
                    l.id, l.from_child_id, l.from_system_id, l.to_child_id, l.to_system_id
                ))
                .color(palette::chrome_text_dim()),
            );
        }
    }
}

fn endpoint_detail(ui: &mut Ui, label: &str, child_id: &str, system_id: &str, name: &str) {
    ui.label(
        RichText::new(label)
            .color(palette::chrome_text_dim())
            .strong(),
    );
    kv(ui, "child", child_id);
    kv(ui, "system", system_id);
    kv(ui, "name", name);
}

fn endpoint_label(ui: &mut Ui, bundle: &SegmentumBundle, child_id: &str, system_id: &str) {
    ui.label(RichText::new(format!(
        "{}/{} ({})",
        child_id,
        system_id,
        bundle.system_name(child_id, system_id)
    )));
}

fn orientation_label(o: BorderOrientation) -> &'static str {
    match o {
        BorderOrientation::EastWest => "E-W",
        BorderOrientation::NorthSouth => "N-S",
        _ => "UNKNOWN",
    }
}

fn stat(ui: &mut Ui, label: &str, value: usize) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(value.to_string())
                .color(palette::chrome_text())
                .strong(),
        );
        ui.label(RichText::new(label.to_ascii_uppercase()).color(palette::chrome_text_dim()));
    });
}

fn chip(ui: &mut Ui, text: &str, fill: Color32) {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0, palette::HEX_OUTLINE))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(palette::chrome_text_dim()));
        });
}

fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [86.0, 0.0],
            egui::Label::new(
                RichText::new(k.to_ascii_uppercase()).color(palette::chrome_text_dim()),
            ),
        );
        ui.label(RichText::new(v).color(palette::chrome_text()));
    });
}
