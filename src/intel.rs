//! Intel / fog-of-war layer (§7 NEXT.md, design §12).
//!
//! Tracks, for an *observer* faction, what is known about a system's
//! presences. The default `SystemIntel::default()` represents "omniscient
//! view" — empty `by_observer`, so the renderer falls back to the raw
//! state. When `by_observer` carries an entry for an observer id, that
//! entry shadows the raw state for that observer: hidden factions become
//! `IntelSource::Suspected` (with confidence) or vanish entirely.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::sector_model::{GeneratedSystem, GeneratedWorld};

/// Per-system fog-of-war record. Empty by default — renders as full
/// fidelity.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SystemIntel {
    /// Observer faction id → what they see in this system.
    pub by_observer: BTreeMap<String, ObserverView>,
}

impl SystemIntel {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_observer.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ObserverView {
    /// Tick on which the observer last verified the state.
    pub last_verified_tick: u32,
    /// Confidence 0..=100 in the current snapshot.
    pub confidence: u8,
    /// Suspected presences — faction ids the observer believes are
    /// present, even when intel is partial.
    pub suspected_presences: Vec<SuspectedPresence>,
    /// Propaganda state — how the observer paints public-facing
    /// narratives.
    pub propaganda_state: PropagandaState,
    /// Classified state — what is known internally but cannot be acted
    /// on without triggering escalation.
    pub classified_state: ClassifiedState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspectedPresence {
    pub faction_id: String,
    pub source: IntelSource,
    pub confidence: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntelSource {
    #[default]
    DirectObservation,
    AstropathicReport,
    InquisitorialAnalysis,
    Rumor,
    ImaginedDeduction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropagandaState {
    #[default]
    None,
    OfficialPacified,
    OfficialContested,
    OfficialLost,
    Counterfactual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClassifiedState {
    #[default]
    Public,
    CodexRedactus,
    PurgatusSigillum,
    ExterminatusFlag,
}

/// Build the default-observer intel record for a system. Records what
/// faction-id-equals-"observer" would see — i.e. each faction-as-observer
/// gets their own per-presence visibility scaled by their visibility into
/// other factions.
#[must_use]
pub fn derive_system_intel(sys: &GeneratedSystem, observer_ids: &[&str]) -> SystemIntel {
    let mut out = SystemIntel::default();
    for obs in observer_ids {
        let view = derive_observer_view(sys, obs);
        out.by_observer.insert((*obs).to_string(), view);
    }
    out
}

fn derive_observer_view(sys: &GeneratedSystem, observer: &str) -> ObserverView {
    let mut suspected: Vec<SuspectedPresence> = Vec::new();
    for w in &sys.worlds {
        suspected.extend(derive_world_suspected(w, observer));
    }
    suspected.sort_by(|a, b| a.faction_id.cmp(&b.faction_id));
    suspected.dedup_by(|a, b| a.faction_id == b.faction_id);

    let observer_present = sys
        .worlds
        .iter()
        .flat_map(|w| w.factions.iter())
        .any(|p| p.faction_id == observer);
    let confidence = if observer_present { 90 } else { 30 };

    let propaganda = if sys.control.dominant.as_deref() == Some(observer) {
        PropagandaState::OfficialPacified
    } else if sys.worlds.iter().any(|w| w.control.contested) {
        PropagandaState::OfficialContested
    } else {
        PropagandaState::None
    };
    let classified = if sys
        .worlds
        .iter()
        .any(|w| w.tags.iter().any(|t| t.ends_with(":quarantined")))
    {
        ClassifiedState::CodexRedactus
    } else {
        ClassifiedState::Public
    };

    ObserverView {
        last_verified_tick: 0,
        confidence,
        suspected_presences: suspected,
        propaganda_state: propaganda,
        classified_state: classified,
    }
}

fn derive_world_suspected(w: &GeneratedWorld, observer: &str) -> Vec<SuspectedPresence> {
    let observer_visibility = w
        .factions
        .iter()
        .find(|p| p.faction_id == observer)
        .map(|p| p.dimensions.visibility)
        .unwrap_or(20.0);

    let mut out: Vec<SuspectedPresence> = Vec::new();
    for p in &w.factions {
        if p.faction_id == observer {
            continue;
        }
        // Observer "sees" another faction as a function of that faction's
        // own visibility * observer's local visibility.
        let raw_conf = (p.dimensions.visibility * observer_visibility) / 100.0;
        if raw_conf < 5.0 {
            continue;
        }
        let source = match raw_conf as u32 {
            0..=20 => IntelSource::Rumor,
            21..=50 => IntelSource::AstropathicReport,
            51..=80 => IntelSource::InquisitorialAnalysis,
            _ => IntelSource::DirectObservation,
        };
        out.push(SuspectedPresence {
            faction_id: p.faction_id.clone(),
            source,
            confidence: raw_conf.clamp(0.0, 100.0) as u8,
        });
    }
    out
}

/// Convenience for the renderer: redact a world's `factions` list to what
/// `observer` would see. Hidden factions whose confidence < `min_conf`
/// disappear from the returned list.
#[must_use]
pub fn redact_world_for_observer<'a>(
    w: &'a GeneratedWorld,
    observer: &str,
    min_conf: u8,
) -> Vec<&'a crate::sector_model::WorldFactionPresence> {
    let observer_visibility = w
        .factions
        .iter()
        .find(|p| p.faction_id == observer)
        .map(|p| p.dimensions.visibility)
        .unwrap_or(20.0);
    w.factions
        .iter()
        .filter(|p| {
            if p.faction_id == observer {
                return true;
            }
            let conf = (p.dimensions.visibility * observer_visibility) / 100.0;
            (conf as u8) >= min_conf
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedStar, GeneratedSystem, GeneratedWorld, HexCoord,
        PresenceDimensions, SystemControlSummary, WorldControlSummary, WorldDto,
        WorldFactionPresence,
    };

    fn mk_world(presences: Vec<(&str, f32, f32)>) -> GeneratedWorld {
        GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
                notable_features: vec![],
            },
            factions: presences
                .into_iter()
                .map(|(fid, vis, covert)| WorldFactionPresence {
                    faction_id: fid.into(),
                    influence: FactionInfluence::Significant,
                    relationship_to_government: "lawful".into(),
                    dimensions: PresenceDimensions {
                        visibility: vis,
                        covert,
                        ..Default::default()
                    },
                    dominance: DominanceState::default(),
                    intel_confidence: vis as u8,
                })
                .collect(),
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
        }
    }

    fn mk_sys(worlds: Vec<GeneratedWorld>) -> GeneratedSystem {
        GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "T".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "A".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds,
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    #[test]
    fn imperial_observer_with_high_visibility_sees_hidden_aeldari_only_faintly() {
        let w = mk_world(vec![("imp", 90.0, 5.0), ("aeld", 10.0, 80.0)]);
        let sys = mk_sys(vec![w]);
        let intel = derive_system_intel(&sys, &["imp"]);
        let view = intel.by_observer.get("imp").unwrap();
        let aeld = view
            .suspected_presences
            .iter()
            .find(|p| p.faction_id == "aeld");
        assert!(aeld.is_some());
        assert!(aeld.unwrap().confidence < 20);
    }

    #[test]
    fn redaction_drops_low_visibility_factions() {
        let w = mk_world(vec![("imp", 90.0, 5.0), ("aeld", 2.0, 80.0)]);
        let kept = redact_world_for_observer(&w, "imp", 5);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].faction_id, "imp");
    }
}
