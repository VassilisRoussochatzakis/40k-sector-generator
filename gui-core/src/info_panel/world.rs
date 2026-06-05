//! World detail render (`world_detail`). Split verbatim from `info_panel.rs`
//! (AREA_F F8, by section).


use egui::Ui;

use sectorforge::sector_model::GeneratedWorld;

use crate::palette::world_type_color;

use super::{dim, kv, legend_row, section, short, stability_block, title};

pub fn world_detail(ui: &mut Ui, w: &GeneratedWorld) {
    title(ui, &format!("WORLD: {}", w.name.to_uppercase()));
    dim(ui, &format!("ID: {}", w.id.to_uppercase()));
    dim(ui, &format!("ORBIT: {}", w.orbit));
    ui.add_space(8.0);

    section(ui, "CLASSIFICATION");
    legend_row(
        ui,
        world_type_color(&w.world.world_type),
        &w.world.world_type.to_uppercase(),
    );
    kv(
        ui,
        "STAR COLOUR",
        &format!(
            "{} - {}",
            w.world.star_colour_code.to_uppercase(),
            w.world.star_colour.to_uppercase()
        ),
    );
    ui.add_space(8.0);

    section(ui, "ENVIRONMENT");
    kv(ui, "ATMOSPHERE", &w.world.atmosphere.to_uppercase());
    kv(ui, "TEMPERATURE", &w.world.temperature.to_uppercase());
    kv(ui, "BIOSPHERE", &w.world.biosphere.to_uppercase());
    ui.add_space(8.0);

    section(ui, "SOCIETY");
    kv(ui, "POPULATION", &w.world.population.to_uppercase());
    kv(ui, "TECH LEVEL", &w.world.tech_level.to_uppercase());
    kv(ui, "GOVERNMENT", &w.world.government.to_uppercase());

    if !w.world.notable_features.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTABLE FEATURES");
        for f in &w.world.notable_features {
            dim(ui, &format!("- {f}"));
        }
    }
    if !w.factions.is_empty() {
        ui.add_space(8.0);
        section(ui, "FACTION PRESENCE");
        for fp in &w.factions {
            let sub = fp
                .subfaction_name
                .as_deref()
                .or(fp.subfaction_id.as_deref())
                .unwrap_or("");
            let force = fp
                .force_name
                .as_deref()
                .or(fp.force_id.as_deref())
                .unwrap_or("");
            let label = if sub.is_empty() {
                fp.faction_id.to_uppercase()
            } else if force.is_empty() {
                format!("{} / {}", fp.faction_id.to_uppercase(), sub.to_uppercase())
            } else {
                format!(
                    "{} / {} / {}",
                    fp.faction_id.to_uppercase(),
                    sub.to_uppercase(),
                    force.to_uppercase()
                )
            };
            dim(
                ui,
                &format!(
                    "{} [{:?}/{:?}] ctl {:.0} vis {:.0}",
                    label,
                    fp.influence,
                    fp.dominance,
                    fp.dimensions.local_control_score(),
                    fp.dimensions.visibility,
                ),
            );
        }
    }
    let wc = &w.control;
    if wc.dominant.is_some()
        || wc.sovereign.is_some()
        || wc.occupier.is_some()
        || wc.economic_hegemon.is_some()
        || wc.popular_authority.is_some()
        || wc.hidden_master.is_some()
    {
        ui.add_space(8.0);
        section(ui, "CONTROL");
        if let Some(v) = &wc.dominant {
            kv(ui, "DOMINANT", &v.to_uppercase());
        }
        if let Some(v) = &wc.sovereign {
            kv(ui, "SOVEREIGN", &v.to_uppercase());
        }
        if let Some(v) = &wc.occupier {
            kv(ui, "OCCUPIER", &v.to_uppercase());
        }
        if let Some(v) = &wc.economic_hegemon {
            kv(ui, "ECONOMIC", &v.to_uppercase());
        }
        if let Some(v) = &wc.popular_authority {
            kv(ui, "POPULAR", &v.to_uppercase());
        }
        if let Some(v) = &wc.hidden_master {
            kv(ui, "HIDDEN", &v.to_uppercase());
        }
        if wc.contested {
            dim(ui, "CONTESTED");
        }
        kv(ui, "SCORE", &format!("{:.0}", wc.control_score));
    }
    stability_block(ui, &w.stability);
    if !w.regions.is_empty() {
        ui.add_space(8.0);
        section(ui, &format!("SURFACE REGIONS ({})", w.regions.len()));
        for r in &w.regions {
            dim(
                ui,
                &format!(
                    "{:?} {} - {} ({})",
                    r.kind,
                    short(&r.name.to_uppercase(), 18),
                    r.dominant.as_deref().unwrap_or("—").to_uppercase(),
                    r.control_score
                ),
            );
        }
    }
    if w.conflict.intensity > 0 {
        ui.add_space(8.0);
        section(ui, "CONFLICT");
        kv(ui, "INTENSITY", &w.conflict.intensity.to_string());
        kv(ui, "MOMENTUM", &w.conflict.momentum.to_string());
        if let Some(a) = &w.conflict.attacker {
            kv(ui, "ATTACKER", &short(&a.to_uppercase(), 22));
        }
        if let Some(d) = &w.conflict.defender {
            kv(ui, "DEFENDER", &short(&d.to_uppercase(), 22));
        }
    }
    if !w.claims.is_empty() {
        ui.add_space(8.0);
        section(ui, "CLAIMS");
        for c in &w.claims {
            dim(
                ui,
                &format!(
                    "{} [{:?}] {}",
                    c.faction_id.to_uppercase(),
                    c.claim_type,
                    c.strength
                ),
            );
        }
    }
    if !w.tags.is_empty() {
        ui.add_space(8.0);
        section(ui, "TAGS");
        for t in &w.tags {
            dim(ui, &t.to_uppercase());
        }
    }
    if !w.notes.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTES");
        for n in &w.notes {
            dim(ui, n);
        }
    }
}

