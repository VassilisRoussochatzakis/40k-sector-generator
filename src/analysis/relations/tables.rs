//! Built-in stance/ideology data + classifiers: the `*_KINDS` membership lists,
//! the default kind-pair base stance + disposition delta tables, and the
//! kind-group helpers (`in_group`/`cross_kinds`/`is_hidden_kind`/
//! `is_merchant_kind`/`ideology_group`/`ideological_distance`) that the derive
//! pipeline consults when the user config is silent.

use super::config::Stance;
use crate::sector_model::FactionKind;

// ── Built-in stance rules ──────────────────────────────────────────────────────

pub(super) const IMPERIAL_KINDS: &[&str] = &[
    "imperial",
    "imperial_guard",
    "imperial_knight",
    "adepta_sororitas",
    "adeptus_astartes",
    "deathwatch",
    "grey_knights",
    "ecclesiarchy",
    "inquisition",
    "mechanicus",
    "talons_of_the_emperor",
    "collegia_titanica",
];

const CHAOS_KINDS: &[&str] = &[
    "chaos",
    "chaos_space_marine",
    "chaos_knight",
    "traitor_guard",
    "traitor_titan_legion",
    "dark_mechanicum",
    "daemon",
    "cult",
];

const TYRANID_KINDS: &[&str] = &["tyranid"];
const NECRON_KINDS: &[&str] = &["necron"];
const ORK_KINDS: &[&str] = &["ork"];
const AELDARI_KINDS: &[&str] = &["aeldari", "harlequin"];
const DRUKHARI_KINDS: &[&str] = &["drukhari"];
const TAU_KINDS: &[&str] = &["tau"];
const GSC_KINDS: &[&str] = &["genestealer_cult"];
const VOTANN_KINDS: &[&str] = &["leagues_of_votann"];
const MISC_XENOS_KINDS: &[&str] = &["xenos", "minor_xenos"];
const CRIMINAL_KINDS: &[&str] = &["criminal"];
pub(super) const MERCHANT_KINDS: &[&str] = &["merchant"];
const REBEL_KINDS: &[&str] = &["rebel"];

fn in_group(kind: &str, group: &[&str]) -> bool {
    group.contains(&kind)
}

/// Default base stance between two kinds. Returned only when no user kind_rule
/// matches the pair.
pub(super) fn default_kind_stance(a: &str, b: &str) -> (Stance, &'static str) {
    let both = |group: &[&str]| in_group(a, group) && in_group(b, group);
    let cross = |g1: &[&str], g2: &[&str]| {
        (in_group(a, g1) && in_group(b, g2)) || (in_group(a, g2) && in_group(b, g1))
    };

    // Same-group same-faction-family: usually warm.
    if both(IMPERIAL_KINDS) {
        return (Stance::Aligned, "Shared Imperial allegiance");
    }
    if both(CHAOS_KINDS) {
        return (Stance::Rival, "Chaos warbands eternally squabble");
    }
    if both(AELDARI_KINDS) {
        return (Stance::Aligned, "Shared Aeldari heritage");
    }
    if both(ORK_KINDS) {
        return (Stance::Rival, "Greenskin rivalry — until a Waaagh!");
    }

    // Tyranid + Necron eat / annihilate everything.
    if in_group(a, TYRANID_KINDS) || in_group(b, TYRANID_KINDS) {
        return (Stance::AtWar, "Tyranid swarm consumes all biomass");
    }
    if in_group(a, NECRON_KINDS) || in_group(b, NECRON_KINDS) {
        return (Stance::AtWar, "Necron protocols reclaim the galaxy");
    }

    // Imperial vs. Chaos / xenos / heresy.
    if cross(IMPERIAL_KINDS, CHAOS_KINDS) {
        return (Stance::AtWar, "Imperium and Chaos are eternal enemies");
    }
    if cross(IMPERIAL_KINDS, GSC_KINDS) {
        return (
            Stance::AtWar,
            "Genestealer infestation answers only to purge",
        );
    }
    if cross(IMPERIAL_KINDS, REBEL_KINDS) {
        return (Stance::AtWar, "Rebellion is heresy");
    }
    if cross(IMPERIAL_KINDS, ORK_KINDS) {
        return (Stance::AtWar, "Greenskin invasion underway");
    }
    if cross(IMPERIAL_KINDS, DRUKHARI_KINDS) {
        return (Stance::AtWar, "Drukhari slave-raids invite extermination");
    }
    if cross(IMPERIAL_KINDS, AELDARI_KINDS) {
        return (Stance::Hostile, "Aeldari motives are never trusted");
    }
    if cross(IMPERIAL_KINDS, TAU_KINDS) {
        return (
            Stance::Hostile,
            "T'au Empire expansion borders Imperial space",
        );
    }
    if cross(IMPERIAL_KINDS, MISC_XENOS_KINDS) {
        return (Stance::Hostile, "Xenos suspicion as standing policy");
    }
    if cross(IMPERIAL_KINDS, VOTANN_KINDS) {
        return (Stance::Rival, "Votann territorial claims under dispute");
    }

    // Chaos vs. the rest.
    if cross(CHAOS_KINDS, AELDARI_KINDS) {
        return (Stance::AtWar, "Aeldari oppose the Dark Gods with prophecy");
    }
    if cross(CHAOS_KINDS, ORK_KINDS) {
        return (Stance::Hostile, "Orks fight whoever is loudest");
    }
    if cross(CHAOS_KINDS, TAU_KINDS) {
        return (Stance::Hostile, "T'au reject the warp outright");
    }
    if cross(CHAOS_KINDS, MISC_XENOS_KINDS) {
        return (Stance::Hostile, "Mutual revulsion of warp and flesh");
    }
    if cross(CHAOS_KINDS, GSC_KINDS) {
        return (Stance::Rival, "Both prey on the underbelly of worlds");
    }
    if cross(CHAOS_KINDS, REBEL_KINDS) {
        return (
            Stance::Aligned,
            "Rebels make the easiest converts to the Pantheon",
        );
    }

    // Criminal / merchant pragmatic ties.
    if cross(CRIMINAL_KINDS, MERCHANT_KINDS) {
        return (Stance::Rival, "Crime and commerce share customers");
    }
    if both(CRIMINAL_KINDS) {
        return (Stance::Rival, "Rival syndicates fighting for turf");
    }
    if cross(IMPERIAL_KINDS, CRIMINAL_KINDS) {
        return (Stance::Hostile, "Imperial enforcement vs. underworld trade");
    }
    if cross(IMPERIAL_KINDS, MERCHANT_KINDS) {
        return (Stance::Aligned, "Free Traders operate under Imperial writ");
    }
    if cross(REBEL_KINDS, MERCHANT_KINDS) {
        return (
            Stance::Aligned,
            "Rebellion bankrolled by black-market trade",
        );
    }

    // Aeldari / Drukhari hate-bond.
    if cross(AELDARI_KINDS, DRUKHARI_KINDS) {
        return (Stance::Hostile, "Estranged Aeldari kin — old wounds");
    }
    // T'au + Aeldari pragmatic warmth.
    if cross(TAU_KINDS, AELDARI_KINDS) {
        return (Stance::Neutral, "Aeldari tolerate the upstart Empire");
    }
    // GSC vs. anyone else: hidden hostility but rarely openly at war.
    if in_group(a, GSC_KINDS) || in_group(b, GSC_KINDS) {
        return (Stance::Rival, "Cult infiltration plays a long game");
    }

    (Stance::Neutral, "No standing accord")
}

const DISPOSITION_DELTAS: &[(&str, &str, i32)] = &[
    ("hostile", "hostile", 2),
    ("hostile", "lawful", 1),
    ("hostile", "zealous", 2),
    ("zealous", "zealous", 1),
    ("zealous", "secretive", 1),
    ("opportunistic", "opportunistic", -1),
    ("opportunistic", "lawful", 0),
    ("lawful", "lawful", -1),
    ("insular", "insular", -1),
    ("insular", "zealous", 1),
    ("secretive", "secretive", 0),
];

pub(super) fn default_disposition_delta(a: &str, b: &str) -> i32 {
    for (x, y, d) in DISPOSITION_DELTAS {
        if (*x == a && *y == b) || (*x == b && *y == a) {
            return *d;
        }
    }
    0
}

pub(super) fn cross_kinds(a: &FactionKind, b: &FactionKind, g1: &[&str], g2: &[&str]) -> bool {
    let (a, b) = (a.as_slug(), b.as_slug());
    (in_group(a, g1) && in_group(b, g2)) || (in_group(a, g2) && in_group(b, g1))
}

pub(super) fn is_hidden_kind(kind: &FactionKind) -> bool {
    kind.is_hidden()
}

pub(super) fn is_merchant_kind(kind: &FactionKind) -> bool {
    kind.is_merchant()
}

fn ideology_group(kind: &str) -> &'static str {
    if in_group(kind, IMPERIAL_KINDS) {
        "imperial"
    } else if in_group(kind, CHAOS_KINDS) {
        "chaos"
    } else if in_group(kind, AELDARI_KINDS) || in_group(kind, DRUKHARI_KINDS) {
        "aeldari"
    } else if in_group(kind, TAU_KINDS) {
        "tau"
    } else if in_group(kind, ORK_KINDS) {
        "ork"
    } else if in_group(kind, NECRON_KINDS) {
        "necron"
    } else if in_group(kind, TYRANID_KINDS) || in_group(kind, GSC_KINDS) {
        "hive"
    } else if in_group(kind, VOTANN_KINDS) {
        "votann"
    } else if in_group(kind, MERCHANT_KINDS) {
        "merchant"
    } else if in_group(kind, CRIMINAL_KINDS) {
        "criminal"
    } else if in_group(kind, REBEL_KINDS) {
        "rebel"
    } else {
        "misc"
    }
}

pub(super) fn ideological_distance(a: &str, b: &str, stance: Stance) -> u8 {
    if a == b {
        return 5;
    }
    let ga = ideology_group(a);
    let gb = ideology_group(b);
    if ga == gb {
        return match ga {
            "chaos" | "ork" | "criminal" => 45,
            "imperial" | "aeldari" | "merchant" => 22,
            _ => 30,
        };
    }
    if matches!(
        (ga, gb),
        ("imperial", "chaos")
            | ("chaos", "imperial")
            | ("imperial", "hive")
            | ("hive", "imperial")
            | ("chaos", "aeldari")
            | ("aeldari", "chaos")
    ) {
        return 100;
    }
    if ga == "necron" || gb == "necron" || ga == "hive" || gb == "hive" {
        return 90;
    }
    match stance {
        Stance::Allied => 20,
        Stance::Aligned => 30,
        Stance::Neutral => 45,
        Stance::Rival => 65,
        Stance::Hostile => 82,
        Stance::AtWar => 96,
    }
}
