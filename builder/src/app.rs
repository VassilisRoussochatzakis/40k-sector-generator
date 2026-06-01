use crate::builder::panels::{
    conflict_resolver, generate_random, nav, new_project, open_project, shortcuts, status,
};
use crate::builder::{project_io, BuilderState, BuilderWorkspace, ModalKind};

pub struct BuilderApp {
    pub workspace: BuilderWorkspace,
}

impl BuilderApp {
    pub fn new() -> Self {
        Self::with_initial_state(BuilderState::new_blank(
            "new-sector",
            "New Sector",
            "seed-1",
            8,
            10,
        ))
    }

    pub fn with_initial_state(state: BuilderState) -> Self {
        Self {
            workspace: BuilderWorkspace::new(state),
        }
    }
}

impl Default for BuilderApp {
    fn default() -> Self {
        Self::new()
    }
}

impl eframe::App for BuilderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme(ctx);
        self.pump_active_state(ctx);

        egui::TopBottomPanel::top("builder_workspace_tabs").show(ctx, |ui| {
            self.show_workspace_tabs(ui);
            ui.separator();
            nav::show_top_bar(ui, self.workspace.active_mut());
        });

        egui::TopBottomPanel::bottom("builder_status").show(ctx, |ui| {
            status::show(ui, self.workspace.active_mut());
        });

        egui::CentralPanel::default().show(ctx, |ui| {
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
                    10,
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
        let title = match &modal {
            ModalKind::NewProject { .. } => "New project",
            ModalKind::OpenProject { .. } => "Open project",
            ModalKind::GenerateRandom { .. } => "Random sector",
            ModalKind::Message(_) => "Message",
            ModalKind::ConflictResolver { .. } => "External change",
            _ => return,
        };
        egui::Window::new(title)
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
                _ => {}
            });
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 20, 25);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(30, 30, 40);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 60);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(60, 60, 80);
    visuals.selection.bg_fill = egui::Color32::from_rgb(60, 120, 180);
    ctx.set_visuals(visuals);
}
