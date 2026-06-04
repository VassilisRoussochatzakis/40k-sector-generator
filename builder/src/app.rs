use crate::builder::panels::{
    conflict_resolver, generate_random, nav, new_project, open_project, shortcuts, status,
};
use crate::builder::{project_io, BuilderState, BuilderWorkspace, ModalKind};

use sectorforge_gui_core::palette;
use sectorforge_gui_core::theme::{self, Theme};

pub struct BuilderApp {
    pub workspace: BuilderWorkspace,
    theme: Theme,
    applied_theme: Option<Theme>,
}

impl BuilderApp {
    pub fn new() -> Self {
        // Sectors must be square (CLAUDE.md): blank scratch state is 8×8, the
        // "Small" preset, so a fresh workspace never starts non-square.
        Self::with_initial_state(BuilderState::new_blank(
            "new-sector",
            "New Sector",
            "seed-1",
            8,
            8,
        ))
    }

    pub fn with_initial_state(state: BuilderState) -> Self {
        Self {
            workspace: BuilderWorkspace::new(state),
            theme: Theme::default(),
            applied_theme: None,
        }
    }

    /// Override the active chrome theme before the first frame — equivalent to
    /// picking it from the in-app `Theme:` menu. Used by the builder's
    /// `--theme` capture flag to verify a component across presets.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }
}

impl Default for BuilderApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for BuilderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.applied_theme != Some(self.theme) {
            self.theme.apply(ctx);
            self.applied_theme = Some(self.theme);
        }
        self.pump_active_state(ctx);

        // §UO6 P2: tier-1 app chrome. The tab strip and status bar sit on the
        // themed panel fill with explicit padding; the central workspace gets
        // the darker window fill so tier-2 `ui_kit::section` boxes (faint_bg)
        // float against a contrasting backdrop.
        egui::TopBottomPanel::top("builder_workspace_tabs")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_panel())
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::menu(ui, &mut self.theme);
                    ui.separator();
                    self.show_workspace_tabs(ui);
                });
                ui.separator();
                nav::show_top_bar(ui, self.workspace.active_mut());
            });

        egui::TopBottomPanel::bottom("builder_status")
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_panel())
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
            )
            .show(ctx, |ui| {
                status::show(ui, self.workspace.active_mut());
            });

        // §COLUMNS §6.1: the left cluster nav rail replaces the wrapping 26-tab
        // top strip. SidePanels must be declared before the CentralPanel so they
        // claim their edge first; this one sits between the top/bottom bars and
        // above the active panel's own roster rail. Hidden when collapsed — the
        // `☰` toggle in `nav::show_top_bar` restores it.
        if !self.workspace.active().nav_rail_collapsed {
            egui::SidePanel::left("builder_nav_rail")
                .resizable(false)
                .exact_width(nav::rail_width(ctx))
                .frame(
                    egui::Frame::none()
                        .fill(palette::chrome_panel())
                        .inner_margin(egui::Margin::same(8.0)),
                )
                .show(ctx, |ui| {
                    nav::show_nav_rail(ui, self.workspace.active_mut());
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(palette::chrome_bg())
                    .inner_margin(egui::Margin::same(10.0)),
            )
            .show(ctx, |ui| {
                nav::show_active_panel(ui, self.workspace.active_mut());
            });

        self.show_modal(ctx);
    }
}

impl BuilderApp {
    fn pump_active_state(&mut self, ctx: &egui::Context) {
        let state = self.workspace.active_mut();
        shortcuts::handle(ctx, state);
        project_io::drain_watcher_events(state);
        // §39 LD3/LD4: re-derive the active tab's overlay if a prior mutation
        // left it stale, so the panel about to paint reads a live result.
        state.pump_derivations();
        if state.pump_validation() {
            ctx.request_repaint();
        }
    }

    fn show_workspace_tabs(&mut self, ui: &mut egui::Ui) {
        let mut switch_to = None;
        let mut close_active = false;
        ui.horizontal_wrapped(|ui| {
            for (idx, state) in self.workspace.iter() {
                let title = if state.sector.title.is_empty() {
                    state.sector.id.as_ref()
                } else {
                    state.sector.title.as_ref()
                };
                let dirty = if state.dirty { "*" } else { "" };
                let label = format!("{dirty}{title}");
                if ui
                    .selectable_label(idx == self.workspace.active_index(), label)
                    .clicked()
                {
                    switch_to = Some(idx);
                }
            }
            ui.separator();
            if ui
                .button("+")
                .on_hover_text("New blank workspace")
                .clicked()
            {
                let n = self.workspace.len() + 1;
                self.workspace.push(BuilderState::new_blank(
                    &format!("new-sector-{n}"),
                    &format!("New Sector {n}"),
                    "seed-1",
                    8,
                    8,
                ));
            }
            if self.workspace.len() > 1
                && ui
                    .button("x")
                    .on_hover_text("Close active workspace")
                    .clicked()
            {
                close_active = true;
            }
        });
        if let Some(idx) = switch_to {
            self.workspace.switch_to(idx);
        }
        if close_active {
            let _ = self.workspace.close_active();
        }
    }

    fn show_modal(&mut self, ctx: &egui::Context) {
        let modal = self.workspace.active().modal.clone();
        let Some(modal) = modal else {
            return;
        };
        // SaveAs / PlaceSystem / ConfirmRevertSnapshot / NewFromPreset are
        // panel-managed transient state — they render inside their owning
        // panel (map.rs, generation.rs, etc.) and do not need an outer window.
        let title: String = match &modal {
            ModalKind::NewProject { .. } => "New project".into(),
            ModalKind::OpenProject { .. } => "Open project".into(),
            ModalKind::GenerateRandom { .. } => "Random sector".into(),
            ModalKind::Message(_) => "Message".into(),
            ModalKind::ConflictResolver { .. } => "External change".into(),
            ModalKind::ConfirmDeleteFaction { .. } => "Delete faction?".into(),
            // §FRIENDLY_PANEL_PASS transform #7: generic confirm for non-undoable
            // catalogue/config deletes — the window title is carried on the modal.
            ModalKind::ConfirmDestructive { title, .. } => title.clone(),
            _ => return,
        };
        egui::Window::new(title.as_str())
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| match modal {
                ModalKind::NewProject { .. } => {
                    let state = self.workspace.active_mut();
                    let _ = new_project::show(ui, state);
                }
                ModalKind::OpenProject { .. } => {
                    let state = self.workspace.active_mut();
                    let _ = open_project::show(ui, state);
                }
                ModalKind::GenerateRandom { .. } => {
                    let state = self.workspace.active_mut();
                    let _ = generate_random::show(ui, state);
                }
                ModalKind::ConflictResolver { .. } => {
                    let state = self.workspace.active_mut();
                    let _ = conflict_resolver::show(ui, state);
                }
                ModalKind::Message(message) => {
                    ui.label(message);
                    if ui.button("OK").clicked() {
                        self.workspace.active_mut().modal = None;
                    }
                }
                ModalKind::ConfirmDeleteFaction { id, name } => {
                    ui.label(format!("Delete “{name}”  ({id}) ?"));
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 140, 120),
                        "Removes it from the roster. This can't be undone.",
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.workspace.active_mut().modal = None;
                        }
                        if ui.button("🗑  Delete").clicked() {
                            let state = self.workspace.active_mut();
                            crate::builder::panels::factions::delete_row(state, &id);
                            state.modal = None;
                        }
                    });
                }
                // §FRIENDLY_PANEL_PASS transform #7: generic confirm window for
                // destructive, non-undoable catalogue/config edits. The panel only
                // *opens* this modal; the irreversible mutation runs once, on Yes,
                // via `apply_confirm_action`.
                ModalKind::ConfirmDestructive { body, action, .. } => {
                    ui.label(body);
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 140, 120),
                        "This can't be undone.",
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.workspace.active_mut().modal = None;
                        }
                        if ui.button("🗑  Delete").clicked() {
                            let state = self.workspace.active_mut();
                            apply_confirm_action(state, action);
                            state.modal = None;
                        }
                    });
                }
                _ => {}
            });
    }
}

/// Dispatch a confirmed [`ModalKind::ConfirmDestructive`] (§FRIENDLY_PANEL_PASS
/// transform #7) to the owning panel's `pub(crate)` delete/clear fn. Centralising
/// dispatch here keeps the irreversible mutation off the panels' hot render path:
/// a panel only *opens* the confirm modal, and the edit runs once, on Yes.
fn apply_confirm_action(state: &mut BuilderState, action: crate::builder::state::ConfirmAction) {
    use crate::builder::panels;
    use crate::builder::state::ConfirmAction as A;
    match action {
        A::DeleteSnapshot(name) => panels::project::delete_snapshot(state, &name),
        A::ClearSubsectorOverrides => panels::subsectors::clear_all_overrides(state),
        A::DeleteSegmentumChild(id) => panels::segmentum::delete_child(state, &id),
        A::DeleteSegmentumLink(i) => panels::segmentum::delete_link(state, i),
        A::DeleteWorldGenRow(i) => panels::worlds_editor::delete_gen_row(state, i),
        A::DeleteManualHook(i) => panels::hooks::delete_manual(state, i),
        A::DeleteManualMission(i) => panels::missions::delete_manual(state, i),
        A::DeleteManualPersona(i) => panels::personae::delete_manual(state, i),
        A::DeleteManualSite(i) => panels::sites::delete_manual(state, i),
        A::DeleteRelationPair(i) => panels::relations::delete_pair_override(state, i),
        A::DeleteRelationKindRule(i) => panels::relations::delete_kind_rule(state, i),
        A::DeleteRelationDispositionRule(i) => panels::relations::delete_disposition_rule(state, i),
    }
}
