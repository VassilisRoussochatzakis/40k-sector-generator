//! Faction definitions loaded from factions.toml.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FactionsFile {
    #[serde(default)]
    pub factions: Vec<FactionDef>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
    /// §F2 builder override: explicit fill colour as `#RRGGBB`. When `None` the
    /// renderer falls back to [`crate::faction_style::faction_style_rgb`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_fill: Option<String>,
    /// §F2 builder override: accent colour as `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_accent: Option<String>,
    /// §F2 builder override: single-character legend glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_glyph: Option<String>,
    /// §F2 builder override: border style. One of `"clean"`, `"jagged"`,
    /// `"dotted"`, `"thin"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_border: Option<String>,
    /// §F7 builder override: when `Some(false)` the legend renderer should hide
    /// this faction (or aggregate it into the kind-group bucket). `None` means
    /// "use default importance logic".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend_visible: Option<bool>,
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
            .unwrap_or_else(|| legacy_top_faction_name(&self.kind).into_owned())
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
            .unwrap_or_else(|| display_name_from_id(&self.kind).into_owned())
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
pub fn legacy_top_faction_name(kind: &str) -> Cow<'static, str> {
    match legacy_top_faction_id(kind).as_str() {
        "imperial" => Cow::Borrowed("Imperium"),
        "chaos" => Cow::Borrowed("Chaos"),
        "ork" => Cow::Borrowed("Orks"),
        "tau" => Cow::Borrowed("T'au Empire"),
        "necron" => Cow::Borrowed("Necrons"),
        "tyranid" => Cow::Borrowed("Tyranids"),
        "aeldari" => Cow::Borrowed("Aeldari"),
        "drukhari" => Cow::Borrowed("Drukhari"),
        "leagues_of_votann" => Cow::Borrowed("Leagues of Votann"),
        "xenos" => Cow::Borrowed("Xenos"),
        "merchant" => Cow::Borrowed("Merchant Powers"),
        "criminal" => Cow::Borrowed("Criminal Powers"),
        "rebel" => Cow::Borrowed("Rebel Powers"),
        _ => display_name_from_id(kind),
    }
}

use std::borrow::Cow;

#[must_use]
pub fn display_name_from_id(id: &str) -> Cow<'static, str> {
    match id {
        "imperial" => Cow::Borrowed("Imperial Institutions"),
        "tau" => Cow::Borrowed("T'au"),
        "ork" => Cow::Borrowed("Orks"),
        "tyranid" => Cow::Borrowed("Tyranids"),
        "necron" => Cow::Borrowed("Necrons"),
        "aeldari" => Cow::Borrowed("Aeldari"),
        "drukhari" => Cow::Borrowed("Drukhari"),
        "harlequin" => Cow::Borrowed("Harlequins"),
        "xenos" => Cow::Borrowed("Xenos"),
        "minor_xenos" => Cow::Borrowed("Minor Xenos"),
        "leagues_of_votann" => Cow::Borrowed("Leagues of Votann"),
        _ => Cow::Owned(
            id.split('_')
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
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_style_fields_skip_serialize_when_none() {
        let def = FactionDef {
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            id: crate::ids::FactionId::new("f1"),
            name: "F".into(),
            kind: "imperial".into(),
            weight: 1.0,
            default_disposition: "lawful".into(),
            preferred_world_types: Vec::new(),
            preferred_governments: Vec::new(),
            preferred_notable_features: Vec::new(),
            style_fill: None,
            style_accent: None,
            style_glyph: None,
            style_border: None,
            legend_visible: None,
        };
        let s = toml::to_string(&def).unwrap();
        assert!(!s.contains("style_fill"));
        assert!(!s.contains("legend_visible"));
    }

    #[test]
    fn subfaction_serde_aliases_deserialize() {
        // Both legacy spellings of the sub-faction id (`sub_faction`,
        // `subfaction_id`) must land in `.subfaction`; both name spellings
        // (`sub_faction_name`, `subfaction_name`) must land in
        // `.subfaction_name`.
        let toml_a = r#"
id = "force-1"
name = "First"
kind = "imperial_guard"
weight = 1.0
sub_faction = "sf-aaa"
sub_faction_name = "Sub Aaa"
"#;
        let f: FactionDef = toml::from_str(toml_a).unwrap();
        assert_eq!(f.subfaction.as_deref(), Some("sf-aaa"));
        assert_eq!(f.subfaction_name.as_deref(), Some("Sub Aaa"));

        let toml_b = r#"
id = "force-2"
name = "Second"
kind = "imperial"
weight = 2.0
subfaction_id = "sf-bbb"
subfaction_name = "Sub Bbb"
"#;
        let g: FactionDef = toml::from_str(toml_b).unwrap();
        assert_eq!(g.subfaction.as_deref(), Some("sf-bbb"));
        assert_eq!(g.subfaction_name.as_deref(), Some("Sub Bbb"));
    }

    #[test]
    fn optional_style_fields_round_trip() {
        let def = FactionDef {
            faction: None,
            faction_name: None,
            subfaction: None,
            subfaction_name: None,
            id: crate::ids::FactionId::new("f1"),
            name: "F".into(),
            kind: "imperial".into(),
            weight: 1.0,
            default_disposition: "lawful".into(),
            preferred_world_types: Vec::new(),
            preferred_governments: Vec::new(),
            preferred_notable_features: Vec::new(),
            style_fill: Some("#112233".into()),
            style_accent: Some("#445566".into()),
            style_glyph: Some("X".into()),
            style_border: Some("jagged".into()),
            legend_visible: Some(false),
        };
        let s = toml::to_string(&def).unwrap();
        let parsed: FactionDef = toml::from_str(&s).unwrap();
        assert_eq!(parsed, def);
    }
}
