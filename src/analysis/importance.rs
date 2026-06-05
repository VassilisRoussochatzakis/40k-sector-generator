//! Display-importance scoring + minor-faction aggregation (§10.3, §15).
//!
//! At sector zoom, a long list of faction pills (one per faction id) is
//! illegible. Each faction gets a `display_importance` score that combines
//! total projected power with how widespread its presence is. Below a
//! configurable cutoff, factions are bucketed by `kind` group ("Other
//! Imperial", "Minor Xenos", "Criminal Networks", etc.).

use crate::sector_model::{GeneratedFaction, GeneratedSector};

/// Default minor-fraction cutoff for [`compute_display_buckets`]. Shared by the
/// PNG legend, GUI sector overview, and Markdown renderer so the three surfaces
/// stay in sync.
pub const DEFAULT_MINOR_FRACTION: f32 = 0.12;

/// Default standalone-bucket cap for [`compute_display_buckets`]. See
/// [`DEFAULT_MINOR_FRACTION`].
pub const DEFAULT_DISPLAY_CAP: usize = 6;

/// Aggregation buckets surfaced in legends. `Faction` carries a single
/// faction; `Aggregated` rolls up several low-importance factions belonging
/// to the same kind-group.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum DisplayBucket {
    Faction {
        id: crate::ids::FactionId,
        name: String,
        kind: String,
        importance: f32,
        system_count: usize,
        world_count: usize,
    },
    Aggregated {
        label: String,
        group: KindGroup,
        importance: f32,
        system_count: usize,
        world_count: usize,
        faction_ids: Vec<crate::ids::FactionId>,
    },
}

impl DisplayBucket {
    #[must_use]
    pub fn importance(&self) -> f32 {
        match self {
            DisplayBucket::Faction { importance, .. }
            | DisplayBucket::Aggregated { importance, .. } => *importance,
        }
    }
}

/// Coarse kind grouping used to pick an aggregation label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum KindGroup {
    Imperial,
    Mechanicus,
    Chaos,
    Aeldari,
    Xenos,
    Criminal,
    Other,
}

enum_slug!(KindGroup {
    Imperial => "imperial",
    Mechanicus => "mechanicus",
    Chaos => "chaos",
    Aeldari => "aeldari",
    Xenos => "xenos",
    Criminal => "criminal",
    Other => "other",
});

impl KindGroup {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            KindGroup::Imperial => "Other Imperial",
            KindGroup::Mechanicus => "Other Mechanicus",
            KindGroup::Chaos => "Other Chaos",
            KindGroup::Aeldari => "Minor Aeldari",
            KindGroup::Xenos => "Minor Xenos",
            KindGroup::Criminal => "Criminal Networks",
            KindGroup::Other => "Other Factions",
        }
    }

    #[must_use]
    pub fn from_kind(kind: &str) -> Self {
        match kind {
            "imperial"
            | "imperial_guard"
            | "imperial_knight"
            | "adepta_sororitas"
            | "adeptus_astartes"
            | "deathwatch"
            | "grey_knights"
            | "talons_of_the_emperor"
            | "inquisition"
            | "collegia_titanica" => KindGroup::Imperial,
            "mechanicus" | "dark_mechanicum" => KindGroup::Mechanicus,
            "chaos_space_marine"
            | "chaos_knight"
            | "traitor_guard"
            | "traitor_titan_legion"
            | "daemon"
            | "cult" => KindGroup::Chaos,
            "aeldari" | "drukhari" | "harlequin" => KindGroup::Aeldari,
            "tau" | "ork" | "tyranid" | "necron" | "leagues_of_votann" | "minor_xenos"
            | "xenos" | "genestealer_cult" => KindGroup::Xenos,
            "criminal" | "rebel" | "merchant" => KindGroup::Criminal,
            _ => KindGroup::Other,
        }
    }
}

impl core::fmt::Display for KindGroup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

/// §10.3: combined display weight = total projected power × √(presence breadth).
/// The square root keeps a single very powerful faction with thin presence
/// roughly comparable to a moderate faction with broad reach.
#[must_use]
pub fn display_importance(f: &GeneratedFaction) -> f32 {
    let breadth = (f.system_presence.len() + f.world_presence.len()) as f32;
    f.power.total_projection() * (1.0 + breadth).sqrt()
}

/// Group factions into display buckets. Factions whose importance is below
/// `minor_fraction * max_importance` are rolled into a kind-group aggregate.
/// `max_visible` caps the number of standalone faction entries — anything
/// past the cap also rolls up.
#[must_use]
pub fn compute_display_buckets(
    sector: &GeneratedSector,
    minor_fraction: f32,
    max_visible: usize,
) -> Vec<DisplayBucket> {
    let mut ranked: Vec<(usize, f32)> = sector
        .factions
        .iter()
        .enumerate()
        .map(|(i, f)| (i, display_importance(f)))
        .collect();
    ranked.sort_by(|a, b| crate::analysis::cmp_f32_desc(a.1, b.1));

    let max_imp = ranked.first().map(|(_, v)| *v).unwrap_or(0.0);
    let cutoff = (max_imp * minor_fraction).max(0.0);

    let mut visible: Vec<DisplayBucket> = Vec::new();
    let mut groups: std::collections::BTreeMap<KindGroup, AggregateAcc> =
        std::collections::BTreeMap::new();

    for (rank, (idx, importance)) in ranked.iter().enumerate() {
        let f = &sector.factions[*idx];
        let force_aggregate = rank >= max_visible || *importance < cutoff;
        if !force_aggregate {
            visible.push(DisplayBucket::Faction {
                id: f.id.clone(),
                name: f.name.to_string(),
                kind: f.kind.to_string(),
                importance: *importance,
                system_count: f.system_presence.len(),
                world_count: f.world_presence.len(),
            });
        } else {
            let g = KindGroup::from_kind(&f.kind);
            let acc = groups.entry(g).or_default();
            acc.importance += importance;
            acc.system_count += f.system_presence.len();
            acc.world_count += f.world_presence.len();
            acc.faction_ids.push(f.id.clone());
        }
    }

    let mut aggregates: Vec<DisplayBucket> = groups
        .into_iter()
        .map(|(g, acc)| DisplayBucket::Aggregated {
            label: g.label().to_string(),
            group: g,
            importance: acc.importance,
            system_count: acc.system_count,
            world_count: acc.world_count,
            faction_ids: acc.faction_ids,
        })
        .collect();
    aggregates.sort_by(|a, b| {
        crate::analysis::cmp_f32_desc(a.importance(), b.importance())
            .then_with(|| aggregate_label_cmp(a, b))
    });

    visible.extend(aggregates);
    visible
}

fn aggregate_label_cmp(a: &DisplayBucket, b: &DisplayBucket) -> std::cmp::Ordering {
    let label = |b: &DisplayBucket| -> String {
        match b {
            DisplayBucket::Aggregated { label, .. } => label.clone(),
            DisplayBucket::Faction { name, .. } => name.clone(),
        }
    };
    label(a).cmp(&label(b))
}

#[derive(Default)]
struct AggregateAcc {
    importance: f32,
    system_count: usize,
    world_count: usize,
    faction_ids: Vec<crate::ids::FactionId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        GeneratedFaction, GeneratedSector, GenerationManifest, PowerProfile,
    };
    use std::collections::BTreeMap;

    fn manifest() -> GenerationManifest {
        GenerationManifest {
            project_id: "p".into(),
            generated_at_policy: "x".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: "abc".into(),
            seed_hash: "h".into(),
            base_seed: None,
            candidate_index: None,
            constraints_digest: None,
            profile: None,
            input_digests: BTreeMap::new(),
            settings_digest: "s".into(),
            system_count: 0,
            world_count: 0,
            route_count: 0,
        }
    }

    fn fac(id: &str, kind: &str, total: f32, presence: usize) -> GeneratedFaction {
        GeneratedFaction {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            disposition: "lawful".into(),
            subfactions: Vec::new(),
            system_presence: (0..presence)
                .map(|i| crate::ids::SystemId::new(format!("s{i}")))
                .collect(),
            world_presence: (0..presence)
                .map(|i| crate::ids::WorldId::new(format!("w{i}")))
                .collect(),
            power: PowerProfile {
                military: total,
                ..PowerProfile::default()
            },
        }
    }

    fn sector_with(factions: Vec<GeneratedFaction>) -> GeneratedSector {
        GeneratedSector {
            id: "demo".into(),
            title: "Demo".into(),
            seed: "abc".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            width: 4,
            height: 4,
            systems: vec![],
            routes: vec![],
            factions,
            manifest: manifest(),
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new().into(),
            economy: Default::default(),
            chronicle: Default::default(),
            ..Default::default()
        }
    }

    #[test]
    fn aggregates_minor_imperial_factions() {
        let s = sector_with(vec![
            fac("big_imperial", "imperial", 100.0, 8),
            fac("minor_imp1", "imperial", 5.0, 1),
            fac("minor_imp2", "imperial_guard", 4.0, 1),
        ]);
        let buckets = compute_display_buckets(&s, 0.15, 8);
        // 1 visible faction + 1 aggregated bucket
        assert_eq!(buckets.len(), 2);
        match &buckets[1] {
            DisplayBucket::Aggregated { group, .. } => assert_eq!(*group, KindGroup::Imperial),
            _ => panic!("expected aggregate"),
        }
    }

    #[test]
    fn caps_max_visible() {
        let s = sector_with(vec![
            fac("a", "imperial", 10.0, 1),
            fac("b", "imperial", 10.0, 1),
            fac("c", "imperial", 10.0, 1),
            fac("d", "imperial", 10.0, 1),
        ]);
        let buckets = compute_display_buckets(&s, 0.0, 2);
        // 2 visible + 1 aggregated bucket holding the rest
        assert_eq!(buckets.len(), 3);
    }
}
