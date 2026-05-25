//! Tiny string helpers shared across emit modules.

use crate::archetypes::{GscStage, TauSphereBand};

use super::model::EventKind;

pub(super) fn kind_slug(k: EventKind) -> &'static str {
    use EventKind::*;
    match k {
        Foundation => "foundation",
        Discovery => "discovery",
        Annexation => "annexation",
        ImperialMandateGranted => "mandate",
        Consecration => "consecration",
        CommercialCharter => "charter",
        DynasticClaim => "dynasty",
        Secession => "secession",
        Uprising => "uprising",
        Reconquest => "reconquest",
        Purge => "purge",
        CultExposed => "cult",
        NecronAwakening => "necron",
        TyranidContact => "tyranid",
        OrkWaaagh => "waaagh",
        QuarantineDeclared => "quarantine",
        Blockade => "blockade",
        WarpStormSurge => "warpstorm",
        TauContact => "tau",
        AeldariActivity => "aeldari",
        ChaosIncursion => "chaos",
    }
}

pub(super) fn article_phrase(s: &str) -> String {
    if s.is_empty() {
        return "an outpost".into();
    }
    let lower = s.to_ascii_lowercase();
    let first = lower.chars().next().unwrap_or('x');
    let article = if matches!(first, 'a' | 'e' | 'i' | 'o' | 'u') {
        "an"
    } else {
        "a"
    };
    format!("{article} {s}")
}

pub(super) fn gsc_stage_label(s: GscStage) -> &'static str {
    match s {
        GscStage::None => "no contact",
        GscStage::Rumor => "rumour stage",
        GscStage::HiddenCell => "hidden cell",
        GscStage::DistrictControl => "district control",
        GscStage::ParallelGovernment => "parallel government",
        GscStage::Uprising => "open uprising",
        GscStage::PlanetarySeizure => "planetary seizure",
    }
}

pub(super) fn tau_band_label(b: TauSphereBand) -> &'static str {
    use TauSphereBand::*;
    match b {
        None => "no contact",
        Core => "core",
        Client => "client",
        Fringe => "fringe",
        Contact => "first-contact",
    }
}
