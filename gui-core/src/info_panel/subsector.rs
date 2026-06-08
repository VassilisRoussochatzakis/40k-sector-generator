//! Subsector summary render (`subsector_summary`). Split verbatim from
//! `info_panel.rs` (AREA_F F8, by section).

use egui::Ui;

use sectorforge::sector_model::GeneratedSector;
use sectorforge::subsectors::Subsector;

use super::{body, dim, kv, section, title};

pub fn subsector_summary(ui: &mut Ui, sub: &Subsector, sector: &GeneratedSector) {
    title(ui, &format!("SUBSECTOR {}", sub.label.to_uppercase()));
    body(ui, &sub.name.to_uppercase());
    dim(ui, &format!("ID: {}", sub.id));
    ui.add_space(8.0);

    section(ui, "COUNTS");
    kv(ui, "SYSTEMS", &sub.summary.system_count.to_string());
    kv(ui, "WORLDS", &sub.summary.world_count.to_string());
    kv(
        ui,
        "INTERNAL ROUTES",
        &sub.summary.internal_route_count.to_string(),
    );
    kv(
        ui,
        "BORDER ROUTES",
        &sub.summary.border_route_count.to_string(),
    );
    ui.add_space(8.0);

    let sys_name = |id: &str| -> String {
        sector
            .systems
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.to_string())
            .unwrap_or_else(|| id.to_string())
    };
    let world_name = |id: &str| -> Option<String> {
        for s in &sector.systems {
            if let Some(w) = s.worlds.iter().find(|w| w.id == id) {
                return Some(w.name.to_string());
            }
        }
        None
    };

    section(ui, "CAPITAL");
    if let Some(cap) = &sub.summary.subsector_capital_system_id {
        kv(ui, "SYSTEM", &sys_name(cap).to_uppercase());
    } else {
        dim(ui, "(none)");
    }
    if let Some(cw) = &sub.summary.subsector_capital_world_id {
        if let Some(n) = world_name(cw) {
            kv(ui, "WORLD", &n.to_uppercase());
        }
    }
    if let Some(primary) = &sub.summary.primary_system_id {
        kv(ui, "PRIMARY", &sys_name(primary).to_uppercase());
    }
    ui.add_space(8.0);

    if let Some(cf) = &sub.summary.controlling_faction_id {
        section(ui, "CONTROLLING FACTION");
        body(ui, &cf.to_uppercase());
        ui.add_space(8.0);
    }

    if !sub.summary.dominant_factions.is_empty() {
        section(ui, "DOMINANT FACTIONS");
        for sc in &sub.summary.dominant_factions {
            dim(ui, &format!("{}  ({})", sc.id.to_uppercase(), sc.score));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.faction_control.is_empty() {
        section(ui, "FACTION CONTROL");
        for r in &sub.summary.faction_control {
            dim(
                ui,
                &format!(
                    "{} [{}] sys {} wld {} ({:.1}%)",
                    r.faction_id.to_uppercase(),
                    r.control_tier.to_uppercase(),
                    r.owned_system_count,
                    r.owned_world_count,
                    r.system_share_basis_points as f32 / 100.0,
                ),
            );
        }
        ui.add_space(8.0);
    }

    if !sub.summary.world_type_counts.is_empty() {
        section(ui, "WORLD TYPES");
        let mut v: Vec<_> = sub.summary.world_type_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(10) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.population_counts.is_empty() {
        section(ui, "POPULATION");
        let mut v: Vec<_> = sub.summary.population_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.tech_level_counts.is_empty() {
        section(ui, "TECH LEVEL");
        let mut v: Vec<_> = sub.summary.tech_level_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.government_counts.is_empty() {
        section(ui, "GOVERNMENT");
        let mut v: Vec<_> = sub.summary.government_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(8) {
            dim(ui, &format!("{}  x{}", k.to_uppercase(), n));
        }
        ui.add_space(8.0);
    }

    if !sub.summary.feature_counts.is_empty() {
        section(ui, "NOTABLE FEATURES");
        let mut v: Vec<_> = sub.summary.feature_counts.iter().collect();
        v.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (k, n) in v.iter().take(10) {
            dim(ui, &format!("{}  x{}", k, n));
        }
        ui.add_space(8.0);
    }

    if !sub.connected_subsector_ids.is_empty() {
        section(ui, "CONNECTED SUBSECTORS");
        for id in &sub.connected_subsector_ids {
            dim(ui, &id.to_uppercase());
        }
        ui.add_space(8.0);
    }

    if !sub.tags.is_empty() {
        section(ui, "TAGS");
        for t in &sub.tags {
            dim(ui, &t.to_uppercase());
        }
        ui.add_space(8.0);
    }

    if !sub.notes.is_empty() {
        section(ui, "NOTES");
        for n in &sub.notes {
            dim(ui, n);
        }
    }
}
