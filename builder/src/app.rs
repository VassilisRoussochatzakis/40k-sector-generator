use crate::builder::panels::{
    conflict_resolver, generate_random, nav, new_project, open_project, shortcuts, status,
};
use crate::builder::{project_io, BuilderState, BuilderWorkspace, ModalKind};

use sectorforge_gui_core::palette;
use sectorforge_gui_core::theme::{self, Theme};
use sectorforge_gui_core::widgets;

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
        // Audit finding #8: if the user has left every catalog-edit tab while a
        // coalescing session is still open, settle it now so the burst lands as
        // one undo entry rather than waiting for a `begin_catalog_session` frame
        // that will not come (the panel is no longer shown). On-tab settle is
        // handled by `begin_catalog_session` itself.
        if !state.active_tab.is_catalog_editor() {
            let _ = state.flush_catalog_session();
        }
        // §39 LD3/LD4: re-derive the active tab's overlay synchronously if a
        // prior mutation left it stale, so the panel about to paint reads a live
        // result this frame (the fast path).
        state.pump_derivations();
        // §39 LD3: push every *other* stale (off-tab) overlay onto a background
        // worker so it is fresh by the time the user visits it, then drain any
        // worker results that landed this tick. The drain's fingerprint
        // stale-guard discards results whose inputs drifted mid-flight; a fresh
        // install requests a repaint so the now-updated overlay paints.
        state.dispatch_background_derivations(ctx);
        if state.pump_derivation_jobs() {
            ctx.request_repaint();
        }
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
        let modal = self.workspace.active().feedback.modal.clone();
        // §BEAUTY §6.7: dim + inert the page behind any open modal. Called every
        // frame (even with no modal) so the scrim fades out on close.
        let _ = sectorforge_gui_core::modal::scrim(ctx, modal.is_some());
        // §41 (N5): the Ctrl-K command palette renders its own centered window
        // over the scrim (it needs the raw `Context` for keyboard handling, not
        // an outer `egui::Window`), so it is dispatched here ahead of the generic
        // window router. No-op unless a `CommandPalette` modal is open.
        crate::builder::panels::command_palette::show(ctx, self.workspace.active_mut());
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
            // §BEAUTY §6.7: the modal window MUST sit in `Order::Foreground` so it
            // renders above the `Order::Middle` scrim. egui's default Window order
            // is also Middle, and a newly-shown / clicked Area auto-bubbles to the
            // top of its order tier (`Area::begin` → `move_to_top`), so a Middle
            // window loses the z-fight to the scrim and gets buried under an
            // (invisible, during fade-in) full-screen backdrop. See `modal::scrim`.
            .order(egui::Order::Foreground)
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
                        self.workspace.active_mut().feedback.modal = None;
                    }
                }
                ModalKind::ConfirmDeleteFaction { id, name } => {
                    ui.label(format!("Delete “{name}”  ({id}) ?"));
                    // §SPRUCE D3/D7: the irreversibility warning + the Delete button
                    // read in the theme-aware danger colour; Cancel recedes as a ghost.
                    ui.colored_label(
                        palette::danger(),
                        "Removes it from the roster. You can undo this (Ctrl+Z).",
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if widgets::ghost_button(ui, "Cancel").clicked() {
                            self.workspace.active_mut().feedback.modal = None;
                        }
                        if widgets::danger_button(ui, "🗑  Delete").clicked() {
                            let state = self.workspace.active_mut();
                            crate::builder::panels::factions::delete_row(state, &id);
                            state.feedback.modal = None;
                        }
                    });
                }
                // §FRIENDLY_PANEL_PASS transform #7: generic confirm window for
                // destructive, non-undoable catalogue/config edits. The panel only
                // *opens* this modal; the irreversible mutation runs once, on Yes,
                // via `apply_confirm_action`.
                ModalKind::ConfirmDestructive { body, action, .. } => {
                    ui.label(body);
                    ui.colored_label(palette::danger(), "This can't be undone.");
                    ui.add_space(6.0);
                    // §U4: the confirm button verb adapts to the action — a snapshot
                    // revert reads "Revert", everything else reads "Delete" — so the
                    // shared confirm window never mislabels a rollback.
                    let confirm_label = action.confirm_label();
                    ui.horizontal(|ui| {
                        if widgets::ghost_button(ui, "Cancel").clicked() {
                            self.workspace.active_mut().feedback.modal = None;
                        }
                        if widgets::danger_button(ui, confirm_label).clicked() {
                            let state = self.workspace.active_mut();
                            apply_confirm_action(state, action);
                            state.feedback.modal = None;
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
        A::RevertToSnapshot(name) => panels::project::revert_snapshot(state, &name),
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
