//! Name pools for systems and worlds.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NameTables {
    #[serde(default)]
    pub system_names: SystemNamesTable,
    #[serde(default)]
    pub location_names: LocationNamesTable,
    #[serde(default)]
    pub world_names: WorldNamesTable,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SystemNamesTable {
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub suffixes: Vec<String>,
    #[serde(default)]
    pub single_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocationNamesTable {
    #[serde(default = "default_true")]
    pub reuse_workbook_location_names: bool,
    #[serde(default = "default_fallback_pattern")]
    pub fallback_pattern: String,
}

impl Default for LocationNamesTable {
    fn default() -> Self {
        Self {
            reuse_workbook_location_names: true,
            fallback_pattern: default_fallback_pattern(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_fallback_pattern() -> String {
    "{system_name} {roman}".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorldNamesTable {
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub suffixes: Vec<String>,
}

pub fn roman_numeral(mut n: usize) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let pairs = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (val, sym) in pairs {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_basics() {
        assert_eq!(roman_numeral(1), "I");
        assert_eq!(roman_numeral(4), "IV");
        assert_eq!(roman_numeral(9), "IX");
        assert_eq!(roman_numeral(42), "XLII");
        assert_eq!(roman_numeral(1999), "MCMXCIX");
    }
}
