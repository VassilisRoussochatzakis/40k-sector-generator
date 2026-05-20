//! Segmentum overview widgets and on-disk bundle loading.

use std::collections::{BTreeMap, HashMap};

use camino::{Utf8Path, Utf8PathBuf};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::{
    errors::SectorError,
    sector_model::GeneratedSector,
    segmentum::{BorderOrientation, InterSectorLink, Segmentum, SegmentumChild},
};

use super::palette::{self, stability_color, TEXT, TEXT_DIM};

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
pub enum SegmentumAction {
    OpenChild(String),
    OpenSystem {
        child_id: String,
        system_id: crate::ids::SystemId,
    },
}

pub fn load_segmentum_bundle(path: &Utf8Path) -> Result<SegmentumBundle, SectorError> {
    let segmentum = crate::load_segmentum_json(path)?;
    let root_dir = path
        .parent()
        .unwrap_or_else(|| Utf8Path::new("."))
        .to_path_buf();
    let mut children = Vec::with_capacity(segmentum.children.len());
    let mut by_id = BTreeMap::new();

    for meta in &segmentum.children {
        let sector_path = root_dir.join("children").join(&meta.id).join("sector.json");
        let sector = crate::load_sector_json(&sector_path)?;
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
    pub fn child(&self, id: &str) -> Option<&LoadedSegmentumChild> {
        self.by_id.get(id).and_then(|&idx| self.children.get(idx))
    }

    pub fn child_meta(&self, id: &str) -> Option<&SegmentumChild> {
        self.segmentum.children.iter().find(|c| c.id == id)
    }

    pub fn child_at(&self, column: u32, row: u32) -> Option<&SegmentumChild> {
        self.segmentum
            .children
            .iter()
            .find(|c| c.column == column && c.row == row)
    }

    pub fn system_name(&self, child_id: &str, system_id: &str) -> String {
        self.child(child_id)
            .and_then(|c| {
                c.sector
                    .systems
                    .iter()
                    .find(|s| s.id == system_id)
                    .map(|s| s.name.clone())
            })
            .unwrap_or_else(|| system_id.to_string())
    }

    pub fn link(&self, id: &str) -> Option<&InterSectorLink> {
        self.segmentum
            .inter_sector_links
            .iter()
            .find(|l| l.id == id)
    }

    pub fn link_count_for_child(&self, id: &str) -> usize {
        self.segmentum
            .inter_sector_links
            .iter()
            .filter(|l| l.from_child_id == id || l.to_child_id == id)
            .count()
    }
}

pub fn show_overview(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<String>,
) -> Option<SegmentumAction> {
    let mut action = None;
    header(ui, bundle);
    ui.add_space(10.0);
    action = action.or_else(|| super_map(ui, bundle, active_child_id, selected_link));
    ui.add_space(12.0);
    action = action.or_else(|| super_grid(ui, bundle, active_child_id));
    ui.add_space(12.0);
    action = action.or_else(|| child_table(ui, bundle, active_child_id));
    ui.add_space(12.0);
    action = action.or_else(|| link_table(ui, bundle, selected_link));
    action
}

pub fn show_side_panel(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<String>,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(RichText::new("SEGMENTUM").color(TEXT).monospace().strong());
    ui.label(
        RichText::new(&bundle.segmentum.title)
            .color(TEXT_DIM)
            .monospace(),
    );
    ui.add_space(8.0);

    if let Some(link_id) = selected_link.as_deref() {
        if let Some(link) = bundle.link(link_id) {
            ui.separator();
            action = action.or_else(|| link_detail(ui, bundle, link));
            ui.add_space(8.0);
            if ui.button(RichText::new("CLEAR LINK").monospace()).clicked() {
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
            if ui
                .button(RichText::new("OPEN ACTIVE MAP").monospace())
                .clicked()
            {
                action = Some(SegmentumAction::OpenChild(id.to_string()));
            }
        }
    }

    ui.separator();
    ui.label(RichText::new("FILES").color(TEXT).monospace().strong());
    kv(ui, "segmentum", bundle.source_path.as_str());
    kv(ui, "root", bundle.root_dir.as_str());
    action
}

fn header(ui: &mut Ui, bundle: &SegmentumBundle) {
    let seg = &bundle.segmentum;
    ui.label(
        RichText::new(format!("{} — {}", seg.id, seg.title))
            .color(TEXT)
            .monospace()
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
        .color(TEXT_DIM)
        .monospace(),
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
        chip(ui, &format!("seed {}", seg.stitch_seed), palette::PANEL_BG);
        chip(
            ui,
            &format!("factions {:?}", seg.faction_mode),
            palette::PANEL_BG,
        );
        chip(
            ui,
            &format!("{} {}", seg.generator_name, seg.generator_version),
            palette::PANEL_BG,
        );
    });
}

fn super_map(
    ui: &mut Ui,
    bundle: &SegmentumBundle,
    active_child_id: Option<&str>,
    selected_link: &mut Option<String>,
) -> Option<SegmentumAction> {
    ui.label(RichText::new("SUPER-MAP").color(TEXT).monospace().strong());
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
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, palette::BG);

    let mut child_rects: BTreeMap<String, Rect> = BTreeMap::new();
    let mut centers: HashMap<(String, crate::ids::SystemId), Pos2> = HashMap::new();

    for meta in &bundle.segmentum.children {
        let min = Pos2::new(
            rect.left() + meta.column as f32 * (cell_w + gap),
            rect.top() + meta.row as f32 * (cell_h + gap),
        );
        let child_rect = Rect::from_min_size(min, Vec2::new(cell_w, cell_h));
        child_rects.insert(meta.id.clone(), child_rect);

        let active = active_child_id == Some(meta.id.as_str());
        painter.rect_filled(
            child_rect,
            2.0,
            if active {
                Color32::from_rgb(40, 36, 52)
            } else {
                palette::PANEL_BG
            },
        );
        painter.rect_stroke(
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
        painter.text(
            child_rect.left_top() + Vec2::new(10.0, 8.0),
            Align2::LEFT_TOP,
            meta.id.to_uppercase(),
            FontId::monospace(13.0),
            TEXT,
        );
        painter.text(
            child_rect.left_top() + Vec2::new(10.0, 24.0),
            Align2::LEFT_TOP,
            format!("{} sys / {} routes", meta.system_count, meta.route_count),
            FontId::monospace(10.0),
            TEXT_DIM,
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
                    painter.line_segment(
                        [a, b],
                        Stroke::new(1.0, stability_color(route.stability).linear_multiply(0.55)),
                    );
                }
            }
            for sys in &loaded.sector.systems {
                if let Some(&p) = centers.get(&(meta.id.clone(), sys.id.clone())) {
                    let fill = palette::star_color(&sys.star.colour_code);
                    painter.circle_filled(p, 3.2, fill);
                    painter.circle_stroke(p, 3.2, Stroke::new(0.8, Color32::BLACK));
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
        palette::draw_route_line(
            &painter,
            a,
            b,
            if selected { 3.2 } else { 1.8 },
            color,
            link.route_type.pattern(),
        );
        let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        painter.text(
            mid,
            Align2::CENTER_CENTER,
            &link.id,
            FontId::monospace(9.5),
            if selected { TEXT } else { TEXT_DIM },
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
    ui.label(RichText::new("SUPER-GRID").color(TEXT).monospace().strong());
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
                                palette::PANEL_BG
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
                                    .color(TEXT)
                                    .monospace()
                                    .strong(),
                            );
                            ui.label(RichText::new(&child.title).color(TEXT_DIM).monospace());
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!(
                                    "slot ({}, {})  {}x{}",
                                    child.column, child.row, child.width, child.height
                                ))
                                .color(TEXT_DIM)
                                .monospace(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} sys  {} worlds  {} routes",
                                    child.system_count, child.world_count, child.route_count
                                ))
                                .color(TEXT_DIM)
                                .monospace(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} stitch links",
                                    bundle.link_count_for_child(&child.id)
                                ))
                                .color(TEXT_DIM)
                                .monospace(),
                            );
                            ui.add_space(4.0);
                            if ui.button(RichText::new("OPEN MAP").monospace()).clicked() {
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
                                        .color(TEXT_DIM)
                                        .monospace(),
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
            .color(TEXT)
            .monospace()
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
                ui.label(RichText::new(h).color(TEXT_DIM).monospace().strong());
            }
            ui.end_row();
            for c in &bundle.segmentum.children {
                let active = active_child_id == Some(c.id.as_str());
                let color = if active { palette::SELECTION } else { TEXT };
                ui.label(RichText::new(&c.id).color(color).monospace());
                ui.label(RichText::new(format!("({}, {})", c.column, c.row)).monospace());
                ui.label(RichText::new(&c.title).monospace());
                ui.label(RichText::new(&c.seed).color(TEXT_DIM).monospace());
                ui.label(RichText::new(c.system_count.to_string()).monospace());
                ui.label(RichText::new(c.world_count.to_string()).monospace());
                ui.label(RichText::new(c.route_count.to_string()).monospace());
                ui.label(RichText::new(bundle.link_count_for_child(&c.id).to_string()).monospace());
                if ui.button(RichText::new("OPEN").monospace()).clicked() {
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
    selected_link: &mut Option<String>,
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new("INTER-SECTOR LINKS")
            .color(TEXT)
            .monospace()
            .strong(),
    );
    ui.add_space(4.0);
    if bundle.segmentum.inter_sector_links.is_empty() {
        ui.label(RichText::new("none").color(TEXT_DIM).monospace());
        return None;
    }
    egui::Grid::new("segmentum_links")
        .num_columns(8)
        .striped(true)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for h in ["ID", "FROM", "TO", "EDGE", "UNITS", "TYPE", "STABILITY", ""] {
                ui.label(RichText::new(h).color(TEXT_DIM).monospace().strong());
            }
            ui.end_row();
            for l in &bundle.segmentum.inter_sector_links {
                let selected = selected_link.as_deref() == Some(l.id.as_str());
                let color = if selected { palette::SELECTION } else { TEXT };
                ui.label(RichText::new(&l.id).color(color).monospace());
                endpoint_label(ui, bundle, &l.from_child_id, &l.from_system_id);
                endpoint_label(ui, bundle, &l.to_child_id, &l.to_system_id);
                ui.label(RichText::new(orientation_label(l.orientation)).monospace());
                ui.label(RichText::new(l.distance_units.to_string()).monospace());
                ui.label(RichText::new(format!("{:?}", l.route_type)).monospace());
                ui.label(
                    RichText::new(format!("{:?}", l.stability))
                        .color(stability_color(l.stability))
                        .monospace(),
                );
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("INFO").monospace()).clicked() {
                        *selected_link = Some(l.id.clone());
                    }
                    if ui.button(RichText::new("FROM").monospace()).clicked() {
                        action = Some(SegmentumAction::OpenSystem {
                            child_id: l.from_child_id.clone(),
                            system_id: l.from_system_id.clone(),
                        });
                    }
                    if ui.button(RichText::new("TO").monospace()).clicked() {
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
) -> Option<SegmentumAction> {
    let mut action = None;
    ui.label(
        RichText::new(format!("LINK {}", link.id.to_uppercase()))
            .color(TEXT)
            .monospace()
            .strong(),
    );
    kv(ui, "edge", orientation_label(link.orientation));
    kv(ui, "units", &link.distance_units.to_string());
    kv(ui, "type", &format!("{:?}", link.route_type));
    kv(ui, "stability", &format!("{:?}", link.stability));
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
    if ui
        .button(RichText::new("OPEN FROM SYSTEM").monospace())
        .clicked()
    {
        action = Some(SegmentumAction::OpenSystem {
            child_id: link.from_child_id.clone(),
            system_id: link.from_system_id.clone(),
        });
    }
    ui.add_space(8.0);
    endpoint_detail(ui, "TO", &link.to_child_id, &link.to_system_id, &to_name);
    if ui
        .button(RichText::new("OPEN TO SYSTEM").monospace())
        .clicked()
    {
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
            .color(TEXT)
            .monospace()
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
                .color(TEXT)
                .monospace()
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
                .color(TEXT_DIM)
                .monospace(),
            );
        }
    }
}

fn endpoint_detail(ui: &mut Ui, label: &str, child_id: &str, system_id: &str, name: &str) {
    ui.label(RichText::new(label).color(TEXT_DIM).monospace().strong());
    kv(ui, "child", child_id);
    kv(ui, "system", system_id);
    kv(ui, "name", name);
}

fn endpoint_label(ui: &mut Ui, bundle: &SegmentumBundle, child_id: &str, system_id: &str) {
    ui.label(
        RichText::new(format!(
            "{}/{} ({})",
            child_id,
            system_id,
            bundle.system_name(child_id, system_id)
        ))
        .monospace(),
    );
}

fn orientation_label(o: BorderOrientation) -> &'static str {
    match o {
        BorderOrientation::EastWest => "E-W",
        BorderOrientation::NorthSouth => "N-S",
    }
}

fn stat(ui: &mut Ui, label: &str, value: usize) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(value.to_string())
                .color(TEXT)
                .monospace()
                .strong(),
        );
        ui.label(
            RichText::new(label.to_ascii_uppercase())
                .color(TEXT_DIM)
                .monospace(),
        );
    });
}

fn chip(ui: &mut Ui, text: &str, fill: Color32) {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0, palette::HEX_OUTLINE))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(TEXT_DIM).monospace());
        });
}

fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [86.0, 0.0],
            egui::Label::new(
                RichText::new(k.to_ascii_uppercase())
                    .color(TEXT_DIM)
                    .monospace(),
            ),
        );
        ui.label(RichText::new(v).color(TEXT).monospace());
    });
}
