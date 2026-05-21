//! Faction definitions loaded from factions.toml.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionsFile {
    #[serde(default)]
    pub factions: Vec<FactionDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionDef {
    /// Highest-level faction id. When omitted, legacy catalogs derive it from
    /// `kind` (for example, `imperial_guard` -> `imperial`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<crate::ids::FactionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction_name: Option<String>,
    /// Middle-level sub-faction id. When omitted, legacy `kind` is used.
    #[serde(
        default,
        alias = "sub_faction",
        alias = "subfaction_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub subfaction: Option<crate::ids::FactionId>,
    #[serde(
        default,
        alias = "sub_faction_name",
        alias = "subfaction_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub subfaction_name: Option<String>,
    /// Force-level id: specific regiment, warband, dynasty, sept, chapter, etc.
    pub id: crate::ids::FactionId,
    pub name: String,
    /// Legacy force kind. Also used as the default sub-faction id and for
    /// presence heuristics.
    pub kind: String,
    pub weight: f64,
    #[serde(default)]
    pub default_disposition: String,
    #[serde(default)]
    pub preferred_world_types: Vec<String>,
    #[serde(default)]
    pub preferred_governments: Vec<String>,
    #[serde(default)]
    pub preferred_notable_features: Vec<String>,
}

impl FactionDef {
    #[must_use]
    pub fn top_faction_id(&self) -> crate::ids::FactionId {
        self.faction
            .clone()
            .unwrap_or_else(|| crate::ids::FactionId::new(legacy_top_faction_id(&self.kind)))
    }

    #[must_use]
    pub fn top_faction_name(&self) -> String {
        self.faction_name
            .clone()
            .unwrap_or_else(|| legacy_top_faction_name(&self.kind))
    }

    #[must_use]
    pub fn subfaction_id(&self) -> crate::ids::FactionId {
        self.subfaction
            .clone()
            .unwrap_or_else(|| crate::ids::FactionId::new(self.kind.as_str()))
    }

    #[must_use]
    pub fn subfaction_name(&self) -> String {
        self.subfaction_name
            .clone()
            .unwrap_or_else(|| display_name_from_id(&self.kind))
    }
}

#[must_use]
pub fn legacy_top_faction_id(kind: &str) -> String {
    match kind {
        "imperial"
        | "inquisition"
        | "talons_of_the_emperor"
        | "adeptus_astartes"
        | "grey_knights"
        | "deathwatch"
        | "adepta_sororitas"
        | "imperial_guard"
        | "imperial_knight"
        | "mechanicus"
        | "collegia_titanica" => "imperial".to_string(),
        "chaos_space_marine"
        | "traitor_guard"
        | "dark_mechanicum"
        | "daemon"
        | "chaos_knight"
        | "traitor_titan_legion"
        | "cult" => "chaos".to_string(),
        "genestealer_cult" => "tyranid".to_string(),
        "harlequin" => "aeldari".to_string(),
        "minor_xenos" => "xenos".to_string(),
        "ork" => "ork".to_string(),
        "tau" => "tau".to_string(),
        "necron" => "necron".to_string(),
        "tyranid" => "tyranid".to_string(),
        "aeldari" => "aeldari".to_string(),
        "drukhari" => "drukhari".to_string(),
        "leagues_of_votann" => "leagues_of_votann".to_string(),
        "xenos" => "xenos".to_string(),
        "merchant" => "merchant".to_string(),
        "criminal" => "criminal".to_string(),
        "rebel" => "rebel".to_string(),
        _ => kind.to_string(),
    }
}

#[must_use]
pub fn legacy_top_faction_name(kind: &str) -> String {
    match legacy_top_faction_id(kind).as_str() {
        "imperial" => "Imperium",
        "chaos" => "Chaos",
        "ork" => "Orks",
        "tau" => "T'au Empire",
        "necron" => "Necrons",
        "tyranid" => "Tyranids",
        "aeldari" => "Aeldari",
        "drukhari" => "Drukhari",
        "leagues_of_votann" => "Leagues of Votann",
        "xenos" => "Xenos",
        "merchant" => "Merchant Powers",
        "criminal" => "Criminal Powers",
        "rebel" => "Rebel Powers",
        _ => return display_name_from_id(kind),
    }
    .to_string()
}

#[must_use]
pub fn display_name_from_id(id: &str) -> String {
    match id {
        "imperial" => "Imperial Institutions".to_string(),
        "tau" => "T'au".to_string(),
        "ork" => "Orks".to_string(),
        "tyranid" => "Tyranids".to_string(),
        "necron" => "Necrons".to_string(),
        "aeldari" => "Aeldari".to_string(),
        "drukhari" => "Drukhari".to_string(),
        "harlequin" => "Harlequins".to_string(),
        "xenos" => "Xenos".to_string(),
        "minor_xenos" => "Minor Xenos".to_string(),
        "leagues_of_votann" => "Leagues of Votann".to_string(),
        _ => id
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(first) => {
                        let mut out = first.to_uppercase().collect::<String>();
                        out.push_str(chars.as_str());
                        out
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
