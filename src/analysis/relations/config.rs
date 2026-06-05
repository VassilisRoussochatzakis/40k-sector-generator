//! Relations config + type definitions: the `Stance` enum and the
//! `relations.toml` schema (`RelationsFile`/`RelationsConfig` + its rule/override
//! rows) plus the serialized output DTOs (`RelationsMatrix`/`FactionRelation`/
//! `DirectionalRelation`/`RelationMetrics` + the attitude/treaty status enums and
//! `RelationsReport`).

use serde::{Deserialize, Serialize};

use super::derive::canonical_pair;

// ── Stance enum ────────────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stance {
    Allied,
    Aligned,
    #[default]
    Neutral,
    Rival,
    Hostile,
    AtWar,
}

enum_slug!(Stance {
    Allied => "allied",
    Aligned => "aligned",
    Neutral => "neutral",
    Rival => "rival",
    Hostile => "hostile",
    AtWar => "at_war",
});

impl core::fmt::Display for Stance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl Stance {
    fn level(self) -> i32 {
        match self {
            Stance::Allied => -2,
            Stance::Aligned => -1,
            Stance::Neutral => 0,
            Stance::Rival => 1,
            Stance::Hostile => 2,
            Stance::AtWar => 3,
        }
    }
    fn from_level(l: i32) -> Stance {
        match l {
            i if i <= -2 => Stance::Allied,
            -1 => Stance::Aligned,
            0 => Stance::Neutral,
            1 => Stance::Rival,
            2 => Stance::Hostile,
            _ => Stance::AtWar,
        }
    }
    pub(super) fn shift(self, delta: i32) -> Stance {
        Stance::from_level((self.level() + delta).clamp(-2, 3))
    }
    /// True for Hostile / At War. Used by the tension heatmap and the
    /// "Factions at war" digest.
    #[must_use]
    pub fn is_hot(self) -> bool {
        matches!(self, Stance::Hostile | Stance::AtWar)
    }
    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Stance::Allied => "Allied",
            Stance::Aligned => "Aligned",
            Stance::Neutral => "Neutral",
            Stance::Rival => "Rival",
            Stance::Hostile => "Hostile",
            Stance::AtWar => "At War",
        }
    }
}

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationsFile {
    #[serde(default)]
    pub relations: RelationsConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationsConfig {
    /// Kind-pair base stance rules. Match is symmetric; the first match wins.
    /// Built-in defaults always apply when the file is silent on a pair.
    #[serde(default)]
    pub kind_rules: Vec<KindRule>,
    /// Disposition adjustments. Sum is applied to the kind-pair base stance.
    #[serde(default)]
    pub disposition_rules: Vec<DispositionRule>,
    /// Explicit `(faction_id, faction_id)` pin. Bypasses the kind/disposition
    /// pipeline entirely.
    #[serde(default)]
    pub pair_overrides: Vec<PairOverride>,
    /// §5 NEW2.md richer overrides. These may pin public/secret attitudes,
    /// treaty status, and selected numeric dimensions while leaving other
    /// values derived.
    #[serde(default)]
    pub overrides: Vec<RelationOverride>,
    /// Whether the derived stance should bias the conflict tick (advisory).
    #[serde(default)]
    pub feed_conflict: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KindRule {
    pub a: String,
    pub b: String,
    pub stance: Stance,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DispositionRule {
    pub a: String,
    pub b: String,
    /// Stance level delta. Positive = more hostile, negative = warmer.
    pub delta: i32,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PairOverride {
    pub a: String,
    pub b: String,
    pub stance: Stance,
    #[serde(default)]
    pub cause: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RelationOverride {
    pub a: String,
    pub b: String,
    /// Symmetric public attitude override.
    #[serde(default)]
    pub public_attitude: Option<RelationAttitude>,
    /// Symmetric secret attitude override.
    #[serde(default)]
    pub secret_attitude: Option<RelationAttitude>,
    /// Directional override for `a → b`.
    #[serde(default)]
    pub a_public_attitude: Option<RelationAttitude>,
    /// Directional override for `b → a`.
    #[serde(default)]
    pub b_public_attitude: Option<RelationAttitude>,
    /// Directional override for `a → b`.
    #[serde(default)]
    pub a_secret_attitude: Option<RelationAttitude>,
    /// Directional override for `b → a`.
    #[serde(default)]
    pub b_secret_attitude: Option<RelationAttitude>,
    #[serde(default)]
    pub treaty_status: Option<TreatyStatus>,
    #[serde(default)]
    pub trust: Option<u8>,
    #[serde(default)]
    pub fear: Option<u8>,
    #[serde(default)]
    pub rivalry: Option<u8>,
    #[serde(default)]
    pub ideological_distance: Option<u8>,
    #[serde(default)]
    pub economic_dependency: Option<u8>,
    #[serde(default)]
    pub military_pressure: Option<u8>,
    #[serde(default)]
    pub covert_activity: Option<u8>,
    #[serde(default)]
    pub reason: Option<String>,
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationsMatrix {
    pub pairs: Vec<FactionRelation>,
    /// Mirror of [`RelationsConfig::feed_conflict`] copied onto the derived
    /// matrix so [`crate::conflict::advance_sector`] knows whether to apply
    /// stance-based momentum bias on each tick.
    #[serde(default)]
    pub feed_conflict: bool,
}

impl RelationsMatrix {
    /// Lookup the stance between two faction ids (order-independent).
    #[must_use]
    pub fn stance_between(&self, a: &str, b: &str) -> Option<Stance> {
        let (lo, hi) = canonical_pair(a, b);
        self.pairs
            .iter()
            .find(|p| p.a == lo && p.b == hi)
            .map(|p| p.stance)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionRelation {
    pub a: crate::ids::FactionId,
    pub b: crate::ids::FactionId,
    /// Legacy canonical stance. In the NEW2 model this mirrors
    /// `secret_stance`, because GM-facing / mechanical consumers need the
    /// actual relationship rather than the public mask.
    pub stance: Stance,
    #[serde(default)]
    pub public_stance: Stance,
    #[serde(default)]
    pub secret_stance: Stance,
    #[serde(default)]
    pub public_attitude: RelationAttitude,
    #[serde(default)]
    pub secret_attitude: RelationAttitude,
    #[serde(default)]
    pub treaty_status: TreatyStatus,
    #[serde(default)]
    pub metrics: RelationMetrics,
    #[serde(default)]
    pub a_to_b: DirectionalRelation,
    #[serde(default)]
    pub b_to_a: DirectionalRelation,
    pub cause: String,
    /// 0..=100 derived from how often the pair co-occurs on contested worlds /
    /// active warzones. Pure read-only derivation.
    pub tension: f32,
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationAttitude {
    Allied,
    Friendly,
    #[default]
    Transactional,
    Suspicious,
    Hostile,
    ExistentialEnemy,
}

enum_slug!(RelationAttitude {
    Allied => "allied",
    Friendly => "friendly",
    Transactional => "transactional",
    Suspicious => "suspicious",
    Hostile => "hostile",
    ExistentialEnemy => "existential_enemy",
});

impl core::fmt::Display for RelationAttitude {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl RelationAttitude {
    pub(super) fn level(self) -> i32 {
        match self {
            Self::Allied => -2,
            Self::Friendly => -1,
            Self::Transactional => 0,
            Self::Suspicious => 1,
            Self::Hostile => 2,
            Self::ExistentialEnemy => 3,
        }
    }

    pub(super) fn from_stance(s: Stance) -> Self {
        match s {
            Stance::Allied => Self::Allied,
            Stance::Aligned => Self::Friendly,
            Stance::Neutral => Self::Transactional,
            Stance::Rival => Self::Suspicious,
            Stance::Hostile => Self::Hostile,
            Stance::AtWar => Self::ExistentialEnemy,
        }
    }

    pub(super) fn to_stance(self) -> Stance {
        match self {
            Self::Allied => Stance::Allied,
            Self::Friendly => Stance::Aligned,
            Self::Transactional => Stance::Neutral,
            Self::Suspicious => Stance::Rival,
            Self::Hostile => Stance::Hostile,
            Self::ExistentialEnemy => Stance::AtWar,
        }
    }

    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Allied => "Allied",
            Self::Friendly => "Friendly",
            Self::Transactional => "Transactional",
            Self::Suspicious => "Suspicious",
            Self::Hostile => "Hostile",
            Self::ExistentialEnemy => "Existential Enemy",
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TreatyStatus {
    #[default]
    None,
    Pact,
    Truce,
    Vassalage,
    Charter,
    Nonaggression,
    Vendetta,
}

enum_slug!(TreatyStatus {
    None => "none",
    Pact => "pact",
    Truce => "truce",
    Vassalage => "vassalage",
    Charter => "charter",
    Nonaggression => "nonaggression",
    Vendetta => "vendetta",
});

impl core::fmt::Display for TreatyStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl TreatyStatus {
    /// Stable human-readable label. Used by the markdown digest and the
    /// builder relations panel.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Pact => "Pact",
            Self::Truce => "Truce",
            Self::Vassalage => "Vassalage",
            Self::Charter => "Charter",
            Self::Nonaggression => "Nonaggression",
            Self::Vendetta => "Vendetta",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationMetrics {
    pub trust: u8,
    pub fear: u8,
    pub rivalry: u8,
    pub ideological_distance: u8,
    pub economic_dependency: u8,
    pub military_pressure: u8,
    pub covert_activity: u8,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectionalRelation {
    pub from: crate::ids::FactionId,
    pub to: crate::ids::FactionId,
    #[serde(default)]
    pub public_attitude: RelationAttitude,
    #[serde(default)]
    pub secret_attitude: RelationAttitude,
    #[serde(default)]
    pub public_stance: Stance,
    #[serde(default)]
    pub secret_stance: Stance,
    #[serde(default)]
    pub metrics: RelationMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationsReport {
    pub sector_id: String,
    pub seed: String,
    pub matrix: RelationsMatrix,
}
