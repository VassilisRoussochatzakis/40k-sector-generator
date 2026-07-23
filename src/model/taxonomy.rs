//! Maps variant-name strings (used in user configs and tags) to worlds.rs enums.
//!
//! worlds.rs `FromStr` already accepts the *display* names (e.g. "Hive World").
//! Configs in this app use *variant* names (e.g. "`HiveWorld`"). Helpers here
//! bridge both directions and provide stable `snake_case` tag forms.

use crate::worlds::{Government, NotableFeature, StarColour, WorldType};

/// Convert a CamelCase variant name like "`HiveWorld`" to "`hive_world`".
pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_upper = true;
    for ch in name.chars() {
        if ch.is_uppercase() && !prev_upper {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
        prev_upper = ch.is_uppercase();
    }
    out
}

pub fn star_colour_variant_name(sc: StarColour) -> &'static str {
    match sc {
        StarColour::BlueHypergiant => "BlueHypergiant",
        StarColour::BlueWhite => "BlueWhite",
        StarColour::White => "White",
        StarColour::YellowWhite => "YellowWhite",
        StarColour::Yellow => "Yellow",
        StarColour::OrangeDwarf => "OrangeDwarf",
        StarColour::RedDwarf => "RedDwarf",
    }
}

pub fn parse_star_colour_variant(s: &str) -> Option<StarColour> {
    StarColour::VARIANTS
        .iter()
        .copied()
        .find(|v| star_colour_variant_name(*v) == s)
}

pub fn parse_world_type_variant(s: &str) -> Option<WorldType> {
    WorldType::VARIANTS
        .iter()
        .find(|v| format!("{v:?}") == s)
        .cloned()
}

pub fn parse_government_variant(s: &str) -> Option<Government> {
    Government::VARIANTS
        .iter()
        .find(|v| format!("{v:?}") == s)
        .cloned()
}

pub fn parse_notable_feature_variant(s: &str) -> Option<NotableFeature> {
    NotableFeature::VARIANTS
        .iter()
        .find(|v| AsRef::<str>::as_ref(*v) == s)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_handles_camel() {
        assert_eq!(to_snake_case("HiveWorld"), "hive_world");
        assert_eq!(to_snake_case("ExtremelyDense"), "extremely_dense");
        assert_eq!(to_snake_case("TradeHub"), "trade_hub");
        assert_eq!(to_snake_case("None"), "none");
    }

    #[test]
    fn variant_name_round_trip_for_world_type() {
        let v = WorldType::HiveWorld;
        let parsed = parse_world_type_variant(&v.to_string()).unwrap();
        assert_eq!(parsed, v);
    }

    /// Exhaustive A4 guard: every variant of each enum must round-trip through
    /// its variant-name parse table. A variant added to `worlds.rs` with no
    /// matching arm here would otherwise silently parse to `None` with no
    /// compile error. `WorldType`/`Government`/`NotableFeature` `Display` is the
    /// `{:?}` variant name (the parse keys); `StarColour::Display` is the short
    /// code, so its round-trip pairs `star_colour_variant_name` with the parser.
    #[test]
    fn parse_tables_cover_all_variants() {
        for v in StarColour::VARIANTS {
            assert_eq!(
                parse_star_colour_variant(star_colour_variant_name(*v)),
                Some(*v),
                "StarColour::{v:?} missing from parse_star_colour_variant"
            );
        }
        for v in WorldType::VARIANTS {
            assert_eq!(
                parse_world_type_variant(&v.to_string()).as_ref(),
                Some(v),
                "WorldType::{v:?} missing from parse_world_type_variant"
            );
        }
        for v in Government::VARIANTS {
            assert_eq!(
                parse_government_variant(&v.to_string()).as_ref(),
                Some(v),
                "Government::{v:?} missing from parse_government_variant"
            );
        }
        for v in NotableFeature::VARIANTS {
            // `NotableFeature: AsRef<str>` yields the variant name directly (same
            // string its `Display`/`{:?}` produces, and the producer's tag key).
            assert_eq!(
                parse_notable_feature_variant(v.as_ref()).as_ref(),
                Some(v),
                "NotableFeature::{v:?} missing from parse_notable_feature_variant"
            );
        }
    }
}
