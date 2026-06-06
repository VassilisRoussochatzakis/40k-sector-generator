//! §41 (N5): Ctrl-K command palette.
//!
//! A centered modal that fuzzy-searches every builder action *by name*. The
//! catalog is built deterministically (see [`catalog`]): the §N1 tabs in their
//! canonical [`BuilderTab::ALL`] order first, then every other action sorted by
//! label. Three action shapes back the entries (see [`PaletteAction`]):
//!
//! * [`PaletteAction::SwitchTab`] — every [`BuilderTab`]. Selecting it routes to
//!   that tab. This is also how the per-`BuilderCommand` *names* become
//!   searchable: a command that needs arguments (coords, ids) cannot run blind,
//!   so its catalog entry switches to the tab where the user completes it. The
//!   command name is the searchable text; selecting performs the routing.
//! * [`PaletteAction::RunCommand`] — the parameterless, sector-wide
//!   [`BuilderCommand`]s that *can* execute directly from the palette
//!   (auto-assign archetypes §AR2, derive baseline intel §I3). These go through
//!   the command bus so they stay undoable (§R4).
//! * [`PaletteAction::AppAction`] — app-level actions that are methods, not
//!   commands: Undo / Redo / Save / Take snapshot.
//!
//! Keyboard: typing filters; Up/Down move the highlight (clamped); Enter runs the
//! highlighted entry; Esc (or clicking the scrim) closes. The Ctrl-K *open* toggle
//! lives in [`super::shortcuts`]; this module renders and dispatches.
//!
//! All palette state (query text, highlight index, open/closed) is transient UI
//! state held on [`ModalKind::CommandPalette`] — it never lands in `sector.json`
//! and has nothing to undo, so it is written directly per the §R4 carve-out.

use crate::builder::command::BuilderCommand;
use crate::builder::state::BuilderTab;
use crate::builder::{BuilderState, ModalKind};

/// What a selected palette entry does. Data-only so the catalog can be rebuilt
/// cheaply every frame the palette is open without borrowing `BuilderState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// Route to a tab. Used for the §N1 tab entries *and* for every
    /// argument-taking `BuilderCommand` name (selecting routes to the owning
    /// tab where the command is actually performed).
    SwitchTab(BuilderTab),
    /// Run a parameterless, sector-wide command through the bus (undoable).
    RunCommand(PaletteCommand),
    /// Invoke an app-level method (not a `BuilderCommand`).
    AppAction(AppAction),
}

/// The parameterless `BuilderCommand`s the palette can dispatch directly. Each
/// maps to one fully-defaulted [`BuilderCommand`] in [`PaletteCommand::build`];
/// kept as a small data enum so [`PaletteAction`] stays `Clone + PartialEq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    /// §AR2: sector-wide `BuilderCommand::AutoAssignArchetypes` with every axis
    /// enabled.
    AutoAssignArchetypes,
    /// §I3: `BuilderCommand::DeriveBaselineIntel` over the whole sector with no
    /// extra observers.
    DeriveBaselineIntel,
}

impl PaletteCommand {
    /// Construct the concrete, ready-to-dispatch command. `before`/result slots
    /// are empty — the bus fills them on `apply`.
    fn build(self) -> BuilderCommand {
        match self {
            Self::AutoAssignArchetypes => BuilderCommand::AutoAssignArchetypes {
                flags: crate::builder::command::ArchetypeApplyFlags::default(),
                before: Vec::new(),
            },
            Self::DeriveBaselineIntel => BuilderCommand::DeriveBaselineIntel {
                observers: Vec::new(),
                before_systems: Vec::new(),
                before_worlds: Vec::new(),
            },
        }
    }
}

/// App-level actions that are `BuilderState` methods / IO calls rather than
/// `BuilderCommand`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Undo,
    Redo,
    Save,
    TakeSnapshot,
}

/// One row in the command palette: a searchable label and the action it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub label: String,
    pub action: PaletteAction,
}

/// Build the full, deterministically-ordered palette catalog.
///
/// Ordering (stable, never frame-dependent, never `FxHashMap`-iterated):
///   1. Every [`BuilderTab`] in [`BuilderTab::ALL`] order, as `"Go to <LABEL>"`.
///   2. Every other action (run-commands + app-actions) sorted by label.
///
/// This satisfies §41's "fuzzy-search every `BuilderCommand` by name": the
/// argument-taking commands surface as `"<NAME>… (go to <TAB>)"` entries whose
/// label is the command name and whose action routes to the owning tab.
#[must_use]
pub fn catalog() -> Vec<PaletteEntry> {
    let mut entries: Vec<PaletteEntry> = Vec::new();

    // 1. Tabs, in canonical order.
    for &tab in BuilderTab::ALL {
        entries.push(PaletteEntry {
            label: format!("Go to {}", tab.label()),
            action: PaletteAction::SwitchTab(tab),
        });
    }

    // 2. App actions + parameterless commands + the argument-taking command
    //    names (each routed to its owning tab). Collected into a side vec and
    //    sorted by label so the tail of the catalog is deterministic regardless
    //    of source order. The fixed app/command rows seed the vec; the
    //    argument-taking command names extend it from `COMMAND_NAME_ROUTES`.
    let mut tail: Vec<PaletteEntry> = vec![
        // App-level actions.
        PaletteEntry {
            label: "Undo".into(),
            action: PaletteAction::AppAction(AppAction::Undo),
        },
        PaletteEntry {
            label: "Redo".into(),
            action: PaletteAction::AppAction(AppAction::Redo),
        },
        PaletteEntry {
            label: "Save project".into(),
            action: PaletteAction::AppAction(AppAction::Save),
        },
        PaletteEntry {
            label: "Take snapshot".into(),
            action: PaletteAction::AppAction(AppAction::TakeSnapshot),
        },
        // Parameterless, sector-wide commands that run directly through the bus.
        PaletteEntry {
            label: "Auto-assign archetypes".into(),
            action: PaletteAction::RunCommand(PaletteCommand::AutoAssignArchetypes),
        },
        PaletteEntry {
            label: "Derive baseline intel".into(),
            action: PaletteAction::RunCommand(PaletteCommand::DeriveBaselineIntel),
        },
    ];

    // Argument-taking BuilderCommand *names* — searchable, routed to the tab
    // where they are performed. (`label = the command name`; `… (go to TAB)`
    // tells the user selecting routes there.) Listed in source order; the final
    // sort makes the emitted order deterministic.
    tail.extend(COMMAND_NAME_ROUTES.iter().map(|(name, tab)| PaletteEntry {
        label: format!("{name}… (go to {})", tab.label()),
        action: PaletteAction::SwitchTab(*tab),
    }));

    tail.sort_by(|a, b| a.label.cmp(&b.label));
    entries.extend(tail);
    entries
}

/// Searchable names for the argument-taking [`BuilderCommand`] variants, each
/// paired with the tab where the user completes the action. The *name* is what
/// §41 requires to be fuzzy-searchable; selecting routes to the owning tab.
/// Parameterless commands (auto-assign archetypes, derive baseline intel) are
/// not listed here — they appear as direct [`PaletteCommand`] run-entries above.
const COMMAND_NAME_ROUTES: &[(&str, BuilderTab)] = &[
    ("Add system", BuilderTab::Map),
    ("Remove system", BuilderTab::Map),
    ("Move system", BuilderTab::Map),
    ("Rename system", BuilderTab::Map),
    ("Swap systems", BuilderTab::Map),
    ("Replace system", BuilderTab::Map),
    ("Add world", BuilderTab::System),
    ("Remove world", BuilderTab::System),
    ("Rename world", BuilderTab::System),
    ("Set world orbit", BuilderTab::System),
    ("Edit world", BuilderTab::World),
    ("Edit system", BuilderTab::System),
    ("Add route", BuilderTab::Routes),
    ("Remove route", BuilderTab::Routes),
    ("Replace routes", BuilderTab::Routes),
    ("Set route type", BuilderTab::Routes),
    ("Set route stability", BuilderTab::Routes),
    ("Add faction", BuilderTab::Factions),
    ("Remove faction", BuilderTab::Factions),
    ("Apply faction power", BuilderTab::Control),
    ("Set archetype", BuilderTab::Control),
    ("Set orbital assets", BuilderTab::Control),
    ("Set blockade report", BuilderTab::Control),
    ("Set surface regions", BuilderTab::World),
    ("Set world conflict", BuilderTab::Control),
    ("Set system conflict", BuilderTab::Control),
    ("Set world stability", BuilderTab::Control),
    ("Advance conflict ticks", BuilderTab::Control),
    ("Add region", BuilderTab::Regions),
    ("Remove region", BuilderTab::Regions),
    ("Edit region", BuilderTab::Regions),
    ("Set region kind", BuilderTab::Regions),
    ("Rename region", BuilderTab::Regions),
    ("Set star", BuilderTab::System),
    ("Set star spectral class", BuilderTab::System),
    ("Edit chronicle", BuilderTab::History),
    ("Bulk edit worlds", BuilderTab::Control),
];

/// A fuzzy subsequence match of `query` against `haystack`, both folded to
/// lowercase. Returns `None` when `query` is not a subsequence of `haystack`;
/// otherwise a score where lower is a tighter match. The score rewards:
/// contiguous runs (each matched char adjacent to the previous costs 0) and an
/// early first match; gaps add to the score. An empty query matches everything
/// with score 0 (catalog order preserved). Deterministic and crate-free.
#[must_use]
pub fn fuzzy_score(query: &str, haystack: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().flat_map(char::to_lowercase).collect();
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();

    let mut score: u32 = 0;
    let mut hay_idx = 0usize;
    let mut last_match: Option<usize> = None;
    for &nc in &needle {
        let mut found = None;
        while hay_idx < hay.len() {
            if hay[hay_idx] == nc {
                found = Some(hay_idx);
                hay_idx += 1;
                break;
            }
            hay_idx += 1;
        }
        let pos = found?;
        match last_match {
            // First matched char: penalise a late start so a prefix hit ranks
            // above a mid-string hit.
            None => score = score.saturating_add(pos as u32),
            // Subsequent chars: each skipped haystack char between matches is a
            // gap; contiguous matches add nothing.
            Some(prev) => {
                let gap = pos.saturating_sub(prev + 1) as u32;
                score = score.saturating_add(gap);
            }
        }
        last_match = Some(pos);
    }
    Some(score)
}

/// Filter + rank `entries` against `query`. Returns indices into `entries`
/// (so callers can map back to the owning entry) ordered by `(score, original
/// index)` — a stable, deterministic ordering: better fuzzy matches first, ties
/// broken by catalog position.
#[must_use]
pub fn filter(entries: &[PaletteEntry], query: &str) -> Vec<usize> {
    let mut scored: Vec<(u32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| fuzzy_score(query, &e.label).map(|s| (s, i)))
        .collect();
    // Stable sort on score; the original index is the explicit tiebreak so the
    // order never depends on sort stability alone.
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// Render the command palette over the modal scrim (painted by
/// [`crate::app::BuilderApp::show_modal`]). No-op unless a
/// [`ModalKind::CommandPalette`] is open. Handles its own keyboard: Esc closes,
/// Up/Down move the highlight, Enter runs the highlighted entry.
pub fn show(ctx: &egui::Context, state: &mut BuilderState) {
    let Some(ModalKind::CommandPalette { query, selected }) = state.feedback.modal.clone() else {
        return;
    };
    let mut query = query;
    let mut selected = selected;
    let mut close = false;
    let mut chosen: Option<PaletteAction> = None;

    let entries = catalog();
    let filtered = filter(&entries, &query);
    // Clamp the highlight to the filtered range (it can fall out of range when
    // the query shrinks the list).
    if filtered.is_empty() {
        selected = 0;
    } else if selected >= filtered.len() {
        selected = filtered.len() - 1;
    }

    // Keyboard navigation. Consume the keys so the row list / TextEdit don't
    // also act on them. Esc closes; arrows move; Enter runs.
    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            close = true;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) && !filtered.is_empty() {
            selected = (selected + 1).min(filtered.len() - 1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            selected = selected.saturating_sub(1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
            if let Some(&idx) = filtered.get(selected) {
                chosen = Some(entries[idx].action.clone());
            }
        }
    });

    egui::Window::new("command_palette")
        .title_bar(false)
        // Must sit above the Order::Middle scrim — see `gui-core::modal::scrim`.
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 96.0))
        .fixed_size(egui::vec2(480.0, 0.0))
        .show(ctx, |ui| {
            ui.heading("Command palette");
            ui.add_space(4.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text("Type to search actions…")
                    .desired_width(f32::INFINITY),
            );
            // Keep keyboard focus in the search box every frame so typing always
            // lands here (and arrows/Enter, consumed above, never get eaten by
            // the field's own caret movement first since we consumed them).
            edit.request_focus();
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .max_height(360.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.weak("No matching actions.");
                    }
                    for (row, &idx) in filtered.iter().enumerate() {
                        let entry = &entries[idx];
                        let is_sel = row == selected;
                        let resp = ui.selectable_label(is_sel, &entry.label);
                        if is_sel {
                            // Keep the highlighted row in view as arrows move it.
                            resp.scroll_to_me(None);
                        }
                        if resp.clicked() {
                            chosen = Some(entry.action.clone());
                        }
                    }
                });
        });

    if let Some(action) = chosen {
        invoke(state, action);
        close = true;
    }

    if close {
        state.feedback.modal = None;
    } else {
        // Persist the live query + highlight back onto the modal.
        state.feedback.modal = Some(ModalKind::CommandPalette { query, selected });
    }
}

/// Execute a chosen [`PaletteAction`]. Tab switches go through
/// [`BuilderState::set_active_tab`] (transient); commands through the bus
/// ([`BuilderState::run`]); app actions call the matching method. Errors are
/// surfaced via [`crate::builder::ModalKind::Message`] so a failure is visible
/// rather than silently dropped.
pub fn invoke(state: &mut BuilderState, action: PaletteAction) {
    match action {
        PaletteAction::SwitchTab(tab) => state.set_active_tab(tab),
        PaletteAction::RunCommand(cmd) => {
            if let Err(e) = state.run(cmd.build()) {
                state.feedback.modal = Some(ModalKind::Message(format!("command failed: {e}")));
                return;
            }
        }
        PaletteAction::AppAction(app) => match app {
            AppAction::Undo => {
                if let Err(e) = state.undo() {
                    state.feedback.modal = Some(ModalKind::Message(format!("undo failed: {e}")));
                    return;
                }
            }
            AppAction::Redo => {
                if let Err(e) = state.redo() {
                    state.feedback.modal = Some(ModalKind::Message(format!("redo failed: {e}")));
                    return;
                }
            }
            AppAction::Save => {
                if let Err(e) = crate::builder::project_io::save_project(state) {
                    state.feedback.modal = Some(ModalKind::Message(format!("save failed: {e}")));
                    return;
                }
            }
            AppAction::TakeSnapshot => state.snapshot("palette: snapshot"),
        },
    }
    // Defensive: clear any open palette modal once the action ran cleanly (the
    // `show` caller also clears it, but `invoke` is `pub` for tests).
    if matches!(state.feedback.modal, Some(ModalKind::CommandPalette { .. })) {
        state.feedback.modal = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::BuilderState;

    fn blank() -> BuilderState {
        BuilderState::new_blank("t", "T", "seed", 8, 8)
    }

    /// Opening the palette sets the modal to a fresh `CommandPalette`.
    #[test]
    fn open_sets_modal() {
        let mut state = blank();
        state.feedback.modal = Some(ModalKind::CommandPalette {
            query: String::new(),
            selected: 0,
        });
        assert!(matches!(
            state.feedback.modal,
            Some(ModalKind::CommandPalette { selected: 0, .. })
        ));
    }

    /// The catalog leads with every tab in `BuilderTab::ALL` order, then sorts
    /// the tail by label. Deterministic across calls.
    #[test]
    fn catalog_is_deterministic_and_tabs_lead() {
        let a = catalog();
        let b = catalog();
        assert_eq!(a, b, "catalog must be stable across calls");

        // First N entries are the tabs, in canonical order.
        for (i, &tab) in BuilderTab::ALL.iter().enumerate() {
            assert_eq!(a[i].action, PaletteAction::SwitchTab(tab));
            assert_eq!(a[i].label, format!("Go to {}", tab.label()));
        }
        // The tail (after the tabs) is sorted by label.
        let tail = &a[BuilderTab::ALL.len()..];
        let mut sorted: Vec<&str> = tail.iter().map(|e| e.label.as_str()).collect();
        let original = sorted.clone();
        sorted.sort_unstable();
        assert_eq!(original, sorted, "catalog tail must be label-sorted");
    }

    /// The two parameterless run-commands and the four app actions are present.
    #[test]
    fn catalog_has_runnable_actions() {
        let c = catalog();
        let has = |a: &PaletteAction| c.iter().any(|e| &e.action == a);
        assert!(has(&PaletteAction::AppAction(AppAction::Undo)));
        assert!(has(&PaletteAction::AppAction(AppAction::Redo)));
        assert!(has(&PaletteAction::AppAction(AppAction::Save)));
        assert!(has(&PaletteAction::AppAction(AppAction::TakeSnapshot)));
        assert!(has(&PaletteAction::RunCommand(
            PaletteCommand::AutoAssignArchetypes
        )));
        assert!(has(&PaletteAction::RunCommand(
            PaletteCommand::DeriveBaselineIntel
        )));
    }

    /// Empty query matches everything (catalog order preserved).
    #[test]
    fn empty_query_matches_all_in_order() {
        let c = catalog();
        let idxs = filter(&c, "");
        assert_eq!(idxs.len(), c.len());
        assert!(idxs.iter().enumerate().all(|(i, &v)| i == v));
    }

    /// A subsequence query filters to the expected matches in deterministic
    /// (score, index) order. "map" must surface the MAP tab entry, and a
    /// contiguous-prefix match must outrank a scattered one.
    #[test]
    fn subsequence_filter_matches_expected() {
        let c = catalog();
        let idxs = filter(&c, "gotomap");
        assert!(!idxs.is_empty());
        // The top hit for "gotomap" is exactly "Go to MAP".
        assert_eq!(c[idxs[0]].label, "Go to MAP");

        // A query that is not a subsequence of any label yields nothing.
        assert!(filter(&c, "zzqxzzqx").is_empty());
    }

    /// `fuzzy_score` is a subsequence test: present in order → Some, absent →
    /// None, and contiguous beats scattered (lower score).
    #[test]
    fn fuzzy_score_ranks_contiguous_lower() {
        assert!(fuzzy_score("abc", "xxabcxx").is_some());
        assert!(fuzzy_score("abc", "axbxc").is_some());
        assert!(fuzzy_score("abc", "acb").is_none());
        let contiguous = fuzzy_score("abc", "abcxx").unwrap();
        let scattered = fuzzy_score("abc", "axbxcx").unwrap();
        assert!(
            contiguous < scattered,
            "contiguous {contiguous} should beat scattered {scattered}"
        );
    }

    /// Selecting a tab entry switches `active_tab` (and does not touch the
    /// sector / command log).
    #[test]
    fn invoke_switch_tab_sets_active_tab() {
        let mut state = blank();
        let before_log = state.command_log.len();
        invoke(&mut state, PaletteAction::SwitchTab(BuilderTab::Routes));
        assert_eq!(state.active_tab, BuilderTab::Routes);
        assert_eq!(
            state.command_log.len(),
            before_log,
            "tab switch must not push a command"
        );
    }

    /// Selecting a parameterless command dispatches it through the bus, pushing
    /// onto the undo log.
    #[test]
    fn invoke_run_command_dispatches_through_bus() {
        let mut state = blank();
        let before = state.command_log.len();
        invoke(
            &mut state,
            PaletteAction::RunCommand(PaletteCommand::DeriveBaselineIntel),
        );
        assert_eq!(
            state.command_log.len(),
            before + 1,
            "run-command must dispatch one command onto the log"
        );
        assert!(matches!(
            state.command_log.last(),
            Some(BuilderCommand::DeriveBaselineIntel { .. })
        ));
    }

    /// Selecting the Take-snapshot app action records a snapshot without a
    /// command-log entry.
    #[test]
    fn invoke_take_snapshot_records_snapshot() {
        let mut state = blank();
        let snaps = state.snapshots.len();
        let log = state.command_log.len();
        invoke(
            &mut state,
            PaletteAction::AppAction(AppAction::TakeSnapshot),
        );
        assert_eq!(state.snapshots.len(), snaps + 1);
        assert_eq!(state.command_log.len(), log);
    }
}
