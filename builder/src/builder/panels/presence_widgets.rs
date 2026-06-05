//! Shared claim / presence chip widgets used by the CONTROL and WORLD panels.
//!
//! §E14 seeds this module with `claim_chip_colours`, which was byte-identical in
//! both `control.rs` and `world/claims.rs`.
//!
//! §E5 — the two `show_add_presence_row` editors (CONTROL `control.rs` and WORLD
//! `world/factions.rs`) were **not** byte-identical: they diverge in picker
//! widgets (the WORLD row has an extra dominance combo + `Display` tier labels;
//! the CONTROL row has influence-help tooltips + a faction-`id` hover), id salts,
//! placeholder strings and modal text. That divergence is **intentional** (per-tab
//! UX) and is left in place. The one genuinely shared, UX-neutral piece — the
//! candidate computation (which sector factions are *not* yet present on the
//! world) — is folded here as [`presence_candidates`]; each caller still renders
//! its own placeholders + pickers.

use std::collections::BTreeSet;

use egui::Color32;
use sectorforge::ids::FactionId;
use sectorforge::sector_model::{ClaimType, GeneratedWorld};

/// Faction-picker candidates for an "add presence" row: the sector factions that
/// are not already present on `world`. Shared by the CONTROL and WORLD presence
/// editors (§E5); the caller maps each variant to its own placeholder text.
pub(crate) enum PresenceCandidates<'a> {
    /// No factions exist to choose from at all.
    NoFactions,
    /// Every available faction already has a presence row on this world.
    AllPresent,
    /// Factions still available to add (borrowing the caller's faction list).
    Available(Vec<&'a (FactionId, String)>),
}

/// Compute the add-presence candidates: filter `factions` down to those absent
/// from `world.factions`. Pure — no UI, no mutation.
pub(crate) fn presence_candidates<'a>(
    world: &GeneratedWorld,
    factions: &'a [(FactionId, String)],
) -> PresenceCandidates<'a> {
    if factions.is_empty() {
        return PresenceCandidates::NoFactions;
    }
    let already: BTreeSet<FactionId> = world
        .factions
        .iter()
        .map(|p| p.faction_id.clone())
        .collect();
    let candidates: Vec<&(FactionId, String)> = factions
        .iter()
        .filter(|(fid, _)| !already.contains(fid))
        .collect();
    if candidates.is_empty() {
        PresenceCandidates::AllPresent
    } else {
        PresenceCandidates::Available(candidates)
    }
}

/// Background + foreground colours for a claim-type chip. This is a **data-viz**
/// palette (lore-coded claim tiers), intentionally hardcoded rather than routed
/// through the theme-status `palette::*` helpers.
pub(crate) fn claim_chip_colours(kind: ClaimType) -> (Color32, Color32) {
    match kind {
        ClaimType::LegalSovereignty => (Color32::from_rgb(40, 60, 100), Color32::LIGHT_BLUE),
        ClaimType::ImperialMandate => (Color32::from_rgb(80, 70, 30), Color32::YELLOW),
        ClaimType::TreatyRight => (Color32::from_rgb(40, 80, 80), Color32::LIGHT_GREEN),
        ClaimType::ReligiousMandate => (Color32::from_rgb(80, 60, 30), Color32::LIGHT_YELLOW),
        ClaimType::DynasticRight => (Color32::from_rgb(80, 30, 70), Color32::LIGHT_RED),
        ClaimType::CommercialCharter => (Color32::from_rgb(40, 90, 50), Color32::GREEN),
        ClaimType::MilitaryOccupation => (Color32::from_rgb(100, 30, 30), Color32::LIGHT_RED),
        ClaimType::AncientDomain => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
        ClaimType::HuntingGround => (Color32::from_rgb(60, 50, 30), Color32::LIGHT_YELLOW),
        ClaimType::CovertWrit => (Color32::from_rgb(30, 30, 60), Color32::LIGHT_BLUE),
        ClaimType::Rebellion => (Color32::from_rgb(120, 30, 30), Color32::LIGHT_RED),
        _ => (Color32::from_rgb(50, 50, 60), Color32::LIGHT_GRAY),
    }
}
