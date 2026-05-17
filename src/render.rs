//! Sector → Markdown rendering. Pure; never mutates the sector.

use std::collections::HashMap;

use crate::sector_model::{GeneratedSector, GeneratedSystem};

/// Spec §12: deterministic Markdown overview for a generated sector.
pub fn render_sector_markdown(sector: &GeneratedSector) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} — {}\n\n", sector.id, sector.title));
    s.push_str(&format!("Seed: `{}`\n\n", sector.seed));
    s.push_str(&format!(
        "Generator: {} v{}\n\n",
        sector.generator_name, sector.generator_version
    ));
    s.push_str(&format!(
        "- **Sector size:** {}×{}\n",
        sector.width, sector.height
    ));
    s.push_str(&format!("- **Systems:** {}\n", sector.systems.len()));
    s.push_str(&format!(
        "- **Worlds:** {}\n",
        sector.systems.iter().map(|s| s.worlds.len()).sum::<usize>()
    ));
    s.push_str(&format!("- **Routes:** {}\n", sector.routes.len()));
    s.push_str(&format!("- **Factions:** {}\n\n", sector.factions.len()));

    s.push_str("## Sector map\n\n");
    s.push_str(&format_sector_map(sector));
    s.push('\n');

    s.push_str("## System index\n\n");
    s.push_str("| ID | Name | Coord | Star | Worlds |\n");
    s.push_str("|---|---|---|---|---:|\n");
    for sys in &sector.systems {
        s.push_str(&format!(
            "| {} | {} | (q={}, r={}) | {} / {} | {} |\n",
            sys.id,
            sys.name,
            sys.coord.q,
            sys.coord.r,
            sys.star.colour_code,
            sys.star.colour_name,
            sys.worlds.len()
        ));
    }
    s.push('\n');

    for sys in &sector.systems {
        s.push_str(&format_system_section(sys));
    }

    s.push_str("## Routes\n\n");
    if sector.routes.is_empty() {
        s.push_str("_No routes._\n\n");
    } else {
        s.push_str("| ID | From | To | Distance | Type | Stability |\n");
        s.push_str("|---|---|---|---:|---|---|\n");
        for r in &sector.routes {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {:?} | {:?} |\n",
                r.id, r.from_system_id, r.to_system_id, r.distance, r.route_type, r.stability
            ));
        }
        s.push('\n');
    }

    s.push_str("## Factions\n\n");
    if sector.factions.is_empty() {
        s.push_str("_No factions._\n\n");
    } else {
        s.push_str("| ID | Name | Kind | Systems | Worlds |\n");
        s.push_str("|---|---|---|---:|---:|\n");
        for f in &sector.factions {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                f.id,
                f.name,
                f.kind,
                f.system_presence.len(),
                f.world_presence.len()
            ));
        }
        s.push('\n');
    }

    s
}

pub fn render_system_markdown(sys: &GeneratedSystem) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} — {}\n\n", sys.id.to_uppercase(), sys.name));
    s.push_str(&format!(
        "- **Coordinates:** q={}, r={}\n",
        sys.coord.q, sys.coord.r
    ));
    s.push_str(&format!(
        "- **Star:** {} / {} / {}\n",
        sys.star.colour_code,
        sys.star.colour_name,
        sys.star.spectral_type.as_deref().unwrap_or("?")
    ));
    if !sys.primary_factions.is_empty() {
        s.push_str(&format!(
            "- **Primary factions:** {}\n",
            sys.primary_factions.join(", ")
        ));
    }
    s.push('\n');
    s.push_str(&format_world_table(sys));
    s
}

fn format_sector_map(sector: &GeneratedSector) -> String {
    let mut at: HashMap<(i32, i32), &str> = HashMap::new();
    for s in &sector.systems {
        at.insert((s.coord.q, s.coord.r), s.star.colour_code.as_str());
    }
    let mut out = String::new();
    out.push_str("```\n");
    for r in 0..(sector.height as i32) {
        if r % 2 == 1 {
            out.push(' ');
        }
        for q in 0..(sector.width as i32) {
            match at.get(&(q, r)) {
                Some(code) => {
                    out.push_str(code);
                    out.push(' ');
                }
                None => out.push_str(". "),
            }
        }
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

fn format_system_section(sys: &GeneratedSystem) -> String {
    let mut s = String::new();
    s.push_str(&format!("## {} — {}\n\n", sys.id.to_uppercase(), sys.name));
    s.push_str(&format!(
        "- **Coordinates:** q={}, r={}\n",
        sys.coord.q, sys.coord.r
    ));
    s.push_str(&format!(
        "- **Star:** {} / {} / {}\n",
        sys.star.colour_code,
        sys.star.colour_name,
        sys.star.spectral_type.as_deref().unwrap_or("?")
    ));
    if !sys.primary_factions.is_empty() {
        s.push_str(&format!(
            "- **Primary factions:** {}\n",
            sys.primary_factions.join(", ")
        ));
    }
    s.push('\n');
    s.push_str(&format_world_table(sys));
    s
}

fn format_world_table(sys: &GeneratedSystem) -> String {
    let mut s = String::new();
    if sys.worlds.is_empty() {
        s.push_str("_No worlds._\n\n");
        return s;
    }
    s.push_str(
        "| Orbit | World | Type | Atmosphere | Population | Tech | Government | Features |\n",
    );
    s.push_str("|---:|---|---|---|---|---|---|---|\n");
    for w in &sys.worlds {
        let features = w.world.notable_features.join("; ");
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            w.orbit,
            w.name,
            w.world.world_type,
            w.world.atmosphere,
            w.world.population,
            w.world.tech_level,
            w.world.government,
            features
        ));
    }
    s.push('\n');
    s
}
