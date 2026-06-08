use egui::{RichText, ScrollArea, TopBottomPanel};

use super::{dashboard, palette, App, View};
use crate::system_view::SystemSelection;

impl App {
    pub(super) fn draw_dashboard_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::chrome_bg()))
                .show(ctx, |ui| {
                    ui.label(RichText::new("no sector loaded").color(palette::chrome_text_dim()));
                });
            return;
        };
        TopBottomPanel::top("dashboard_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_panel())
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(RichText::new("RECOMPUTE"))
                        .on_hover_text("re-run analytics on the current sector")
                        .clicked()
                    {
                        self.dashboard.invalidate();
                    }
                    ui.label(
                        RichText::new("§8 NEW.md — analytics dashboard")
                            .color(palette::chrome_text_dim()),
                    );
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    dashboard::show(
                        ui,
                        &sector,
                        self.sector_map_cache.as_ref(),
                        &mut self.dashboard,
                    );
                });
            });
    }

    pub(super) fn draw_history_layout(&mut self, ctx: &egui::Context) {
        let Some(sector) = self.sector.clone() else {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(palette::chrome_bg()))
                .show(ctx, |ui| {
                    ui.label(RichText::new("no sector loaded").color(palette::chrome_text_dim()));
                });
            return;
        };

        TopBottomPanel::top("history_toolbar")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_panel())
                    .inner_margin(6.0),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("SECTOR HISTORY")
                            .color(palette::chrome_text())
                            .strong(),
                    );
                    ui.label(
                        RichText::new("§1 NEW2.md — typed timeline records")
                            .color(palette::chrome_text_dim()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} EVENTS", sector.chronicle.events.len()))
                            .color(palette::chrome_text_dim()),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(14.0),
            )
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    if sector.chronicle.events.is_empty() {
                        ui.label(
                            RichText::new("no chronicle events embedded in this sector")
                                .color(palette::chrome_text_dim()),
                        );
                        return;
                    }

                    ui.columns(2, |columns| {
                        columns[0].vertical(|ui| {
                            ui.label(
                                RichText::new("TIMELINE")
                                    .color(palette::chrome_text())
                                    .strong(),
                            );
                            ui.add_space(6.0);
                            let selected = self.history_selected_event.clone();
                            for e in &sector.chronicle.events {
                                let is_selected = selected.as_deref() == Some(e.id.as_str());
                                let label = format!("{}  {}  {}", e.date, e.kind, e.summary);
                                if ui
                                    .selectable_label(is_selected, RichText::new(short(&label, 96)))
                                    .clicked()
                                {
                                    self.history_selected_event = Some(e.id.as_str().into());
                                }
                            }
                        });

                        columns[1].vertical(|ui| {
                            let active = self
                                .history_selected_event
                                .as_ref()
                                .and_then(|id| {
                                    sector
                                        .chronicle
                                        .events
                                        .iter()
                                        .find(|e| e.id.as_str() == id.as_ref())
                                })
                                .or_else(|| sector.chronicle.events.first());
                            if let Some(e) = active {
                                ui.label(
                                    RichText::new("EVENT DETAIL")
                                        .color(palette::chrome_text())
                                        .strong(),
                                );
                                ui.label(RichText::new(&e.id).color(palette::chrome_text_dim()));
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} · {:?}",
                                        e.date, e.era_label, e.kind
                                    ))
                                    .color(palette::chrome_text()),
                                );
                                ui.label(RichText::new(&e.narrative).color(palette::chrome_text()));
                                if !e.entities.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new("REFS")
                                            .color(palette::chrome_text_dim())
                                            .strong(),
                                    );
                                    for ent in &e.entities {
                                        let role = ent.role.as_deref().unwrap_or("");
                                        ui.label(
                                            RichText::new(format!(
                                                "{:?}  {}  {}",
                                                ent.kind, ent.id, role
                                            ))
                                            .color(palette::chrome_text_dim()),
                                        );
                                    }
                                }
                                if !e.consequences.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new("CONSEQUENCES")
                                            .color(palette::chrome_text_dim())
                                            .strong(),
                                    );
                                    for c in &e.consequences {
                                        ui.label(
                                            RichText::new(format!(
                                                "{:?}  sev{}  {}",
                                                c.kind, c.severity, c.description
                                            ))
                                            .color(palette::chrome_text_dim()),
                                        );
                                    }
                                }
                                if let Some(sys_id) = first_system_from_event(e) {
                                    ui.add_space(8.0);
                                    if ui.button(RichText::new("OPEN FIRST SYSTEM")).clicked() {
                                        self.view = View::System {
                                            system_id: sys_id,
                                            selection: SystemSelection::None,
                                        };
                                    }
                                }
                            }
                        });
                    });

                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("SNAPSHOTS / VERSIONING")
                            .color(palette::chrome_text())
                            .strong(),
                    );
                    if ui.button(RichText::new("+ CREATE SNAPSHOT")).clicked() {
                        let name = format!("Snapshot {}", self.history_snapshots.len() + 1);
                        self.history_snapshots.push((name, sector.as_ref().clone()));
                    }
                    ui.add_space(8.0);
                    let mut revert_idx: Option<usize> = None;
                    // VAPP-5: snapshot names are user-editable (was a read-only auto-generated label).
                    // Nothing keys off the name — revert uses the index — so in-place rename is safe.
                    for (i, (name, _)) in self.history_snapshots.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(name);
                            if ui.button("REVERT").clicked() {
                                revert_idx = Some(i);
                            }
                        });
                    }
                    if let Some(i) = revert_idx {
                        let (_, sec) = self.history_snapshots[i].clone();
                        let source = self
                            .sector_source_path
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_string());
                        self.set_loaded_sector(sec, source);
                    }
                });
            });
    }
}

fn first_system_from_event(
    event: &sectorforge::history::HistoryEvent,
) -> Option<sectorforge::ids::SystemId> {
    let mut systems = Vec::new();
    match &event.anchor {
        sectorforge::history::HistoryAnchor::System { system_id } => {
            systems.push(system_id.clone());
        }
        sectorforge::history::HistoryAnchor::World { system_id, .. } => {
            systems.push(system_id.clone());
        }
        sectorforge::history::HistoryAnchor::Route {
            from_system_id,
            to_system_id,
            ..
        } => {
            systems.push(from_system_id.clone());
            systems.push(to_system_id.clone());
        }
        _ => {}
    }
    for ent in &event.entities {
        if ent.kind == sectorforge::history::HistoryEntityKind::System {
            systems.push(sectorforge::ids::SystemId::new(ent.id.clone()));
        }
    }
    systems.into_iter().min()
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
