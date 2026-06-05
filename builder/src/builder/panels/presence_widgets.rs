//! Shared claim / presence chip widgets used by the CONTROL and WORLD panels.
//!
//! §E14 seeds this module with `claim_chip_colours`, which was byte-identical in
//! both `control.rs` and `world/claims.rs`. (§E5 is slated to fold the
//! duplicated `show_add_presence_row` here too.)

use egui::Color32;
use sectorforge::sector_model::ClaimType;

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
