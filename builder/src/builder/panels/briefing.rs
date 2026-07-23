//! BRIEFING tab (§N1 / §N2) — Phase D §BR1..§BR5.
//!
//! §BR1  Preset picker: a `ComboBox` over the six built-in
//!        [`AudiencePreset`] variants (`GmFullTruth`, `ImperialNavy`,
//!        `Inquisition`, `RogueTrader`, `LocalGovernor`, `PublicAtlas`).
//!        Selecting a preset clears any cached preview so the next
//!        "Generate briefing" pass rebuilds against the new audience.
//! §BR2  Observer-faction picker: a `ComboBox` populated from
//!        `state.sector.factions`. `None` means "no observer" — every
//!        non-observer faction's intel sub-record is dropped from the
//!        pack. When set, only the observer's intel slot survives and
//!        Hidden-tier presences are filtered against the observer's
//!        visibility through `min_confidence`.
//! §BR3  `min_confidence` slider, range 0..=100, default 30. Maps onto
//!        [`BriefingProfile::minimum_intel_confidence`]. Built-in
//!        presets `clamp` this floor (e.g. Public Atlas floors at 80) so
//!        the panel shows the effective floor next to the slider.
//! §BR4  "Generate briefing" runs [`sectorforge::apply_briefing`] (the
//!        public alias for `briefing::apply`) and stores both the
//!        redacted pack and the rendered Markdown on the builder state.
//!        The redacted Markdown is shown verbatim in a scrollable side
//!        panel so the GM can read what the audience will see before
//!        committing to an export.
//! §BR5  "Export to folder…" pops a native folder picker via `rfd` and
//!        writes `briefing-<profile_id>.md` + `briefing-<profile_id>.json`
//!        through [`sectorforge::write_briefing`]. The picked folder
//!        sticks on `BuilderState::briefing_panel.export_dir` so a follow-up
//!        export to the same target needs no second pick.

use camino::Utf8PathBuf;
use egui::{Color32, RichText, Ui};

use sectorforge::briefing::{self, AudiencePreset, BriefingProfile};
use sectorforge::ids::FactionId;
use sectorforge_gui_core::palette;
use sectorforge_gui_core::ui_kit::{self, labeled};
use sectorforge_gui_core::widgets;

use crate::builder::BuilderState;
use crate::builder::DerivationKind;
use crate::builder::ModalKind;

const PRESET_VARIANTS: &[AudiencePreset] = &[
    AudiencePreset::GmFullTruth,
    AudiencePreset::ImperialNavy,
    AudiencePreset::Inquisition,
    AudiencePreset::RogueTrader,
    AudiencePreset::LocalGovernor,
    AudiencePreset::PublicAtlas,
];

pub(crate) fn show(ui: &mut Ui, state: &mut BuilderState) {
    // §39 LD4: refresh the cached preview when a prior mutation left it stale.
    // The briefing profile builder is panel-local, so this overlay self-heals
    // here rather than through `pump_derivations`. No preview yet ⇒ stay cold.
    if state.derivations.is_stale(DerivationKind::Briefing)
        && state.briefing_panel.preview_md.is_some()
    {
        regenerate_preview(state);
    }
    ui.heading("Briefing");
    ui.add_space(2.0);
    ui.colored_label(
        Color32::DARK_GRAY,
        "Build a sector briefing for a given audience: pick who is reading, who is watching, how much they can know, then preview and export the redacted pack.",
    );
    ui.separator();

    // §COLUMNS — RC-2: the profile/config controls (preset / observer /
    // confidence / generate / export) sit in the left column; the rendered
    // Markdown preview fills the right column, width-capped via
    // `reading_column` so the redacted body keeps a readable line length on a
    // wide window. Collapses to a single stacked column on a narrow window.
    egui::ScrollArea::vertical()
        .id_salt("br_root_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui_kit::columns_responsive(ui, 2, 460.0, |cols| {
                let n = cols.len();
                {
                    // Left: audience profile + generate/export controls.
                    let left = &mut cols[0];
                    show_preset_row(left, state);
                    left.separator();
                    show_observer_row(left, state);
                    left.separator();
                    show_confidence_row(left, state);
                    left.separator();
                    show_generate_row(left, state);
                    left.separator();
                    show_export_row(left, state);
                }
                {
                    // Right — or the same single column when collapsed to one:
                    // the redacted Markdown preview.
                    let right = &mut cols[if n > 1 { 1 } else { 0 }];
                    if n == 1 {
                        right.separator();
                    }
                    show_preview(right, state);
                }
            });
        });
}

// ── §BR1 preset picker ────────────────────────────────────────────────────

fn show_preset_row(ui: &mut Ui, state: &mut BuilderState) {
    labeled(
        ui,
        "Audience",
        "Who the briefing is written for (schema: preset). Sets how aggressively secrets are redacted.",
        |ui| {
            let label = preset_label(state.briefing_panel.preset);
            let prev = state.briefing_panel.preset;
            ui_kit::combo("br1_preset", label).show_ui(ui, |ui| {
                for p in PRESET_VARIANTS {
                    ui.selectable_value(&mut state.briefing_panel.preset, *p, preset_label(*p))
                        .on_hover_text(preset_blurb(*p));
                }
            });
            if state.briefing_panel.preset != prev {
                invalidate_preview(state);
            }
        },
    );
    ui.label(RichText::new(preset_blurb(state.briefing_panel.preset)).color(Color32::DARK_GRAY));
}

// ── §BR2 observer-faction picker ──────────────────────────────────────────

fn show_observer_row(ui: &mut Ui, state: &mut BuilderState) {
    labeled(
        ui,
        "Seen through",
        "The faction whose knowledge limits the briefing (schema: observer_faction). Leave as (none) for an audience with no faction of its own.",
        |ui| {
            let selected_text = state
                .briefing_panel.observer
                .as_ref()
                .map(|id| observer_label(state, id))
                .unwrap_or_else(|| "(none)".to_string());
            let prev = state.briefing_panel.observer.clone();
            ui_kit::combo("br2_observer", selected_text).show_ui(ui, |ui| {
                ui.selectable_value(&mut state.briefing_panel.observer, None, "(none)");
                for f in &state.sector.factions {
                    let label = format!("{} ({})", f.name, f.id);
                    ui.selectable_value(&mut state.briefing_panel.observer, Some(f.id.clone()), label);
                }
            });
            if state.briefing_panel.observer != prev {
                invalidate_preview(state);
            }
        },
    );
    ui.label(
        RichText::new("With no watcher, intel that belongs to another faction is left out.")
            .color(Color32::DARK_GRAY),
    );
}

fn observer_label(state: &BuilderState, id: &FactionId) -> String {
    match state.sector.factions.iter().find(|f| &f.id == id) {
        Some(f) => format!("{} ({})", f.name, f.id),
        None => id.to_string(),
    }
}

// ── §BR3 min-confidence slider ────────────────────────────────────────────

fn show_confidence_row(ui: &mut Ui, state: &mut BuilderState) {
    labeled(
        ui,
        "Confidence floor",
        "Lowest certainty an item needs to appear (schema: minimum_intel_confidence). 0 shows everything; 100 keeps only what is directly observed.",
        |ui| {
            let prev = state.briefing_panel.min_confidence;
            ui.add(
                egui::Slider::new(&mut state.briefing_panel.min_confidence, 0..=100)
                    .text("0 = show all, 100 = directly observable only"),
            );
            if state.briefing_panel.min_confidence != prev {
                invalidate_preview(state);
            }
        },
    );
    let effective = effective_min_confidence(state);
    if effective != state.briefing_panel.min_confidence {
        ui.colored_label(
            palette::warning(),
            format!(
                "The {} audience raises the floor to {effective}.",
                preset_label(state.briefing_panel.preset)
            ),
        );
    }
}

fn effective_min_confidence(state: &BuilderState) -> u8 {
    let mut profile = build_profile(state);
    if let Some(preset) = profile.preset {
        preset_apply(preset, &mut profile);
    }
    profile.minimum_intel_confidence
}

// AudiencePreset::apply is `pub(crate)`; emulate the public-only behaviour
// by re-issuing the preset through `briefing::preset` and copying back the
// effective floor.
fn preset_apply(preset: AudiencePreset, profile: &mut BriefingProfile) {
    let baseline = briefing::preset(preset);
    profile.minimum_intel_confidence = match preset {
        AudiencePreset::GmFullTruth | AudiencePreset::Inquisition => profile
            .minimum_intel_confidence
            .min(baseline.minimum_intel_confidence),
        AudiencePreset::ImperialNavy
        | AudiencePreset::RogueTrader
        | AudiencePreset::LocalGovernor
        | AudiencePreset::PublicAtlas => profile
            .minimum_intel_confidence
            .max(baseline.minimum_intel_confidence),
        _ => profile
            .minimum_intel_confidence
            .max(baseline.minimum_intel_confidence),
    };
}

// ── §BR4 generate ─────────────────────────────────────────────────────────

fn show_generate_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if widgets::primary_button(ui, "🔄  Generate briefing")
            .on_hover_text("Build the redacted preview from the settings above")
            .clicked()
        {
            regenerate_preview(state);
        }
        if state.briefing_panel.preview_md.is_some() {
            ui.colored_label(Color32::DARK_GRAY, "preview ready");
        }
    });
    if state.briefing_panel.preview_md.is_none() {
        ui_kit::placeholder(ui, "No preview yet — click Generate briefing to build one.");
    }
}

fn regenerate_preview(state: &mut BuilderState) {
    let profile = build_profile(state);
    let pack = sectorforge::apply_briefing(&state.sector, &profile);
    let md = briefing::render_markdown(&pack, &profile);
    state.briefing_panel.preview_md = Some(md);
    state.briefing_panel.preview_pack = Some(pack);
    state.mark_derivation_fresh(DerivationKind::Briefing);
}

fn invalidate_preview(state: &mut BuilderState) {
    state.briefing_panel.preview_md = None;
    state.briefing_panel.preview_pack = None;
}

fn build_profile(state: &BuilderState) -> BriefingProfile {
    let mut profile = briefing::preset(state.briefing_panel.preset);
    profile.minimum_intel_confidence = state.briefing_panel.min_confidence;
    profile.observer_faction = state.briefing_panel.observer.clone();
    profile
}

// ── §BR5 export ───────────────────────────────────────────────────────────

fn show_export_row(ui: &mut Ui, state: &mut BuilderState) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("📂  Choose folder…")
            .on_hover_text("Pick the folder the .md and .json files are written to")
            .clicked()
        {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Choose briefing export folder")
                .pick_folder()
            {
                if let Ok(path) = Utf8PathBuf::from_path_buf(folder) {
                    state.briefing_panel.export_dir = Some(path);
                }
            }
        }
        let has_pack = state.briefing_panel.preview_pack.is_some();
        let has_dir = state.briefing_panel.export_dir.is_some();
        if ui
            .add_enabled(
                has_pack && has_dir,
                egui::Button::new("💾  Export .md + .json"),
            )
            .on_hover_text("Write the previewed briefing to the chosen folder")
            .clicked()
        {
            export_pack(state);
        }
        let dir_label = state
            .briefing_panel
            .export_dir
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(no folder picked)".to_string());
        ui.colored_label(Color32::DARK_GRAY, dir_label);
    });
    if state.briefing_panel.preview_pack.is_none() {
        ui_kit::placeholder(
            ui,
            "Generate the briefing first — export writes the previewed pack.",
        );
    }
}

fn export_pack(state: &mut BuilderState) {
    let Some(dir) = state.briefing_panel.export_dir.clone() else {
        return;
    };
    let Some(pack) = state.briefing_panel.preview_pack.clone() else {
        return;
    };
    let profile = build_profile(state);
    match sectorforge::write_briefing(dir.as_path(), &pack, &profile) {
        Ok(()) => {
            state.feedback.modal = Some(ModalKind::Message(format!(
                "Wrote briefing-{}.md and briefing-{}.json to {}",
                profile.id, profile.id, dir
            )));
        }
        Err(e) => {
            state.feedback.modal = Some(ModalKind::Message(format!("Briefing export failed: {e}")));
        }
    }
}

// ── §BR4 preview pane ─────────────────────────────────────────────────────

fn show_preview(ui: &mut Ui, state: &mut BuilderState) {
    ui.label(RichText::new("Redacted preview").strong());
    let Some(md) = state.briefing_panel.preview_md.as_ref() else {
        ui_kit::placeholder(
            ui,
            "No preview yet — set the audience, watcher, and confidence above, then click Generate briefing.",
        );
        return;
    };
    // §COLUMNS — width-cap the rendered Markdown so the monospace body keeps a
    // readable line length on a wide window instead of running edge-to-edge.
    egui::ScrollArea::both()
        .id_salt("br_preview_scroll")
        .max_height(360.0)
        .show(ui, |ui| {
            ui_kit::reading_column(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut md.as_str())
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .font(egui::TextStyle::Monospace),
                );
            });
        });
}

// ── label helpers ─────────────────────────────────────────────────────────

fn preset_label(p: AudiencePreset) -> &'static str {
    match p {
        AudiencePreset::GmFullTruth => "GM — full truth",
        AudiencePreset::ImperialNavy => "Imperial Navy",
        AudiencePreset::Inquisition => "Inquisition",
        AudiencePreset::RogueTrader => "Rogue Trader",
        AudiencePreset::LocalGovernor => "Local Governor",
        AudiencePreset::PublicAtlas => "Public Atlas",
        _ => "UNKNOWN",
    }
}

fn preset_blurb(p: AudiencePreset) -> &'static str {
    match p {
        AudiencePreset::GmFullTruth => "no redaction",
        AudiencePreset::ImperialNavy => "hidden routes hidden, secret relations stripped",
        AudiencePreset::Inquisition => "hidden routes kept, secret relations visible",
        AudiencePreset::RogueTrader => "trade-oriented, hidden routes hidden",
        AudiencePreset::LocalGovernor => "own faction only, no orbital detail",
        AudiencePreset::PublicAtlas => "unknown factions + claims + archetype state stripped",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sectorforge::sector_model::{GeneratedFaction, GeneratedSector, PowerProfile};

    fn seeded_state() -> BuilderState {
        let mut state = BuilderState::new_blank("t", "T", "s", 4, 4);
        state.sector = empty_sector_with_factions().into();
        state
    }

    fn empty_sector_with_factions() -> GeneratedSector {
        let mut sec = GeneratedSector::empty("t", "T", "s", 4, 4);
        sec.factions = vec![
            GeneratedFaction {
                id: FactionId::new("imp"),
                name: "Imperium".into(),
                kind: "imperial".into(),
                disposition: "lawful".into(),
                subfactions: Vec::new(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: PowerProfile::default(),
            },
            GeneratedFaction {
                id: FactionId::new("cult"),
                name: "Cult".into(),
                kind: "chaos".into(),
                disposition: "outlaw".into(),
                subfactions: Vec::new(),
                system_presence: Vec::new(),
                world_presence: Vec::new(),
                power: PowerProfile::default(),
            },
        ];
        sec
    }

    #[test]
    fn defaults_seed_gm_full_truth_with_min_30() {
        let state = seeded_state();
        assert_eq!(state.briefing_panel.preset, AudiencePreset::GmFullTruth);
        assert_eq!(state.briefing_panel.min_confidence, 30);
        assert!(state.briefing_panel.observer.is_none());
        assert!(state.briefing_panel.preview_md.is_none());
        assert!(state.briefing_panel.preview_pack.is_none());
    }

    #[test]
    fn generate_populates_preview_and_pack() {
        let mut state = seeded_state();
        regenerate_preview(&mut state);
        assert!(state.briefing_panel.preview_md.is_some());
        assert!(state.briefing_panel.preview_pack.is_some());
        let pack = state.briefing_panel.preview_pack.as_ref().unwrap();
        assert_eq!(pack.profile_id, "gm_full_truth");
    }

    #[test]
    fn invalidate_clears_preview() {
        let mut state = seeded_state();
        regenerate_preview(&mut state);
        invalidate_preview(&mut state);
        assert!(state.briefing_panel.preview_md.is_none());
        assert!(state.briefing_panel.preview_pack.is_none());
    }

    #[test]
    fn build_profile_layers_observer_and_confidence() {
        let mut state = seeded_state();
        state.briefing_panel.preset = AudiencePreset::Inquisition;
        state.briefing_panel.observer = Some(FactionId::new("imp"));
        state.briefing_panel.min_confidence = 5;
        let p = build_profile(&state);
        assert_eq!(p.preset, Some(AudiencePreset::Inquisition));
        assert_eq!(p.observer_faction.as_deref(), Some("imp"));
        assert_eq!(p.minimum_intel_confidence, 5);
    }
}
