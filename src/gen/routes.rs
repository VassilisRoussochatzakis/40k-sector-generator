//! Route rules loaded from `route_rules.toml`.

use crate::sector_model::RouteType;
use crate::worlds::{Government, NotableFeature, WorldType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRulesFile {
    pub routes: RouteRules,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteRules {
    #[serde(default = "default_weight")]
    pub default_weight: f64,
    #[serde(default = "default_max_distance")]
    pub max_distance: u32,
    #[serde(default)]
    pub prefer_populated_worlds: bool,
    #[serde(default)]
    pub prefer_trade_hubs: bool,
    #[serde(default)]
    pub avoid_warp_phenomena: bool,
    #[serde(default)]
    pub modifiers: Vec<RouteModifier>,
}

impl Default for RouteRules {
    fn default() -> Self {
        Self {
            default_weight: default_weight(),
            max_distance: default_max_distance(),
            prefer_populated_worlds: true,
            prefer_trade_hubs: true,
            avoid_warp_phenomena: true,
            modifiers: Vec::new(),
        }
    }
}

fn default_weight() -> f64 {
    1.0
}

fn default_max_distance() -> u32 {
    4
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteModifier {
    pub when: RouteCondition,
    pub multiplier: f64,
}

/// A route-modifier predicate. Each field is an optional *typed* enum: a value
/// present in `route_rules.toml` is deserialized into the corresponding domain
/// enum, so a misspelled condition (e.g. `notable_feature = "TradeHubb"`) is a
/// hard load-time error rather than a silently-never-matching string (P10).
///
/// `notable_feature`/`world_type`/`government` parse from their PascalCase
/// variant names (no `rename_all`); `route_type` parses from its snake_case key
/// (with the `dangerous_passage`/`DangerousPassage` aliases on `ChartedPassage`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RouteCondition {
    #[serde(default)]
    pub notable_feature: Option<NotableFeature>,
    #[serde(default)]
    pub world_type: Option<WorldType>,
    #[serde(default)]
    pub government: Option<Government>,
    #[serde(default)]
    pub route_type: Option<RouteType>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_condition_accepts_route_type_key() {
        let text = r#"
            [routes]
            default_weight = 1.0
            max_distance = 4

            [[routes.modifiers]]
            when = { route_type = "charted_passage", government = "MilitaryGovernor" }
            multiplier = 0.5
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        let modifier = &file.routes.modifiers[0];
        assert_eq!(modifier.when.route_type, Some(RouteType::ChartedPassage));
        assert_eq!(modifier.when.government, Some(Government::MilitaryGovernor));
    }

    #[test]
    fn route_condition_route_type_dangerous_passage_alias() {
        // The legacy `dangerous_passage` key still deserializes (alias on
        // ChartedPassage), so older configs keep loading.
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { route_type = "dangerous_passage" }
            multiplier = 0.5
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        assert_eq!(
            file.routes.modifiers[0].when.route_type,
            Some(RouteType::ChartedPassage)
        );
    }

    #[test]
    fn route_condition_pascalcase_world_fields_parse() {
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { notable_feature = "TradeHub", world_type = "ForgeWorld" }
            multiplier = 2.0
        "#;
        let file: RouteRulesFile = toml::from_str(text).unwrap();
        let cond = &file.routes.modifiers[0].when;
        assert_eq!(cond.notable_feature, Some(NotableFeature::TradeHub));
        assert_eq!(cond.world_type, Some(WorldType::ForgeWorld));
    }

    #[test]
    fn route_condition_misspelled_feature_is_a_load_error() {
        // P10: a typo in a condition value is now a hard deserialize error
        // instead of a silently-never-matching string.
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { notable_feature = "NotARealFeature" }
            multiplier = 2.0
        "#;
        let err = toml::from_str::<RouteRulesFile>(text)
            .expect_err("misspelled notable_feature must fail to deserialize");
        let msg = err.to_string();
        assert!(
            msg.contains("NotARealFeature") || msg.contains("unknown variant"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn route_condition_misspelled_route_type_is_a_load_error() {
        let text = r#"
            [routes]
            [[routes.modifiers]]
            when = { route_type = "charted_passagee" }
            multiplier = 2.0
        "#;
        toml::from_str::<RouteRulesFile>(text)
            .expect_err("misspelled route_type must fail to deserialize");
    }

    #[test]
    fn route_condition_round_trips_through_toml() {
        // A builder-written rules file (PascalCase world fields, snake_case
        // route_type) must re-parse identically.
        let original = RouteRulesFile {
            routes: RouteRules {
                modifiers: vec![RouteModifier {
                    when: RouteCondition {
                        notable_feature: Some(NotableFeature::TradeHub),
                        world_type: Some(WorldType::ForgeWorld),
                        government: Some(Government::MilitaryGovernor),
                        route_type: Some(RouteType::ChartedPassage),
                    },
                    multiplier: 0.5,
                }],
                ..Default::default()
            },
        };
        let text = toml::to_string(&original).unwrap();
        assert!(text.contains("notable_feature = \"TradeHub\""));
        assert!(text.contains("route_type = \"charted_passage\""));
        let reparsed: RouteRulesFile = toml::from_str(&text).unwrap();
        let cond = &reparsed.routes.modifiers[0].when;
        assert_eq!(cond.notable_feature, Some(NotableFeature::TradeHub));
        assert_eq!(cond.world_type, Some(WorldType::ForgeWorld));
        assert_eq!(cond.government, Some(Government::MilitaryGovernor));
        assert_eq!(cond.route_type, Some(RouteType::ChartedPassage));
    }
}
