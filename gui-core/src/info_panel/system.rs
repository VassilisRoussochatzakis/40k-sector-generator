//! System detail renders (`system_summary`, `star_detail`) and the per-system
//! info blocks. Split verbatim from `info_panel.rs` (AREA_F F8, by section).


use egui::Ui;

use sectorforge::sector_model::{
    GeneratedSector, GeneratedSystem,
};

use crate::palette::{
    faction_style_by_id, star_color,
    world_type_color,
};
use crate::sector_view::SectorMapCache;

use super::history::system_history;
use super::{body, dim, kv, legend_row, section, short, stability_block, title};

pub fn system_summary(
    ui: &mut Ui,
    sys: &GeneratedSystem,
    sector: &GeneratedSector,
    cache: Option<&SectorMapCache>,
) {
    title(ui, &format!("SYSTEM: {}", sys.id.to_uppercase()));
    body(ui, &short(&sys.name.to_uppercase(), 28));
    dim(ui, &format!("COORD: Q{:+} R{:+}", sys.coord.q, sys.coord.r));
    ui.add_space(8.0);

    if let Some(star) = &sys.star {
        section(ui, "STAR");
        legend_row(
            ui,
            star_color(&star.colour_code),
            &format!(
                "{} - {}",
                star.colour_code.to_uppercase(),
                star.colour_name.to_uppercase()
            ),
        );
        if let Some(s) = star.spectral_type.as_ref() {
            dim(ui, &format!("SPECTRAL: {}", s.to_uppercase()));
        }
    } else {
        section(ui, "KIND");
        body(ui, &format!("{}", sys.kind).to_uppercase());
    }
    ui.add_space(8.0);

    section(ui, &format!("WORLDS ({})", sys.worlds.len()));
    for w in &sys.worlds {
        legend_row(
            ui,
            world_type_color(&w.world.world_type),
            &format!(
                "{}  {}  {}",
                w.orbit,
                short(&w.name.to_uppercase(), 16),
                short(&w.world.world_type.to_uppercase(), 14),
            ),
        );
    }

    if !sys.primary_factions.is_empty() {
        ui.add_space(8.0);
        section(ui, "PRIMARY FACTIONS");
        for f in &sys.primary_factions {
            body(ui, &short(&f.to_uppercase(), 28));
        }
    }
    let c = &sys.control;
    if c.state.is_some()
        || c.dominant.is_some()
        || c.sovereign.is_some()
        || c.orbital_controller.is_some()
        || c.economic_hegemon.is_some()
        || c.hidden_master.is_some()
    {
        ui.add_space(8.0);
        section(ui, "CONTROL");
        if let Some(state) = c.state {
            kv(ui, "STATE", &format!("{state}").to_uppercase());
        }
        if let Some(v) = &c.dominant {
            kv(ui, "DOMINANT", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.sovereign {
            kv(ui, "SOVEREIGN", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.orbital_controller {
            kv(ui, "ORBITAL", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.economic_hegemon {
            kv(ui, "ECONOMIC", &short(&v.to_uppercase(), 22));
        }
        if let Some(v) = &c.hidden_master {
            kv(ui, "HIDDEN", &short(&v.to_uppercase(), 22));
        }
    }
    stability_block(ui, &sys.stability);
    blockade_block(ui, sys);
    conflict_block(ui, &sys.conflict);
    archetype_block(ui, sys);
    orbital_assets_block(ui, sys);
    routes_block(ui, sys, sector, cache);
    system_history(ui, sector, sys.id.as_str());
    if !sys.tags.is_empty() {
        ui.add_space(8.0);
        section(ui, "TAGS");
        for t in &sys.tags {
            dim(ui, &t.to_uppercase());
        }
    }
    if !sys.notes.is_empty() {
        ui.add_space(8.0);
        section(ui, "NOTES");
        for n in &sys.notes {
            dim(ui, n);
        }
    }
}

pub fn star_detail(ui: &mut Ui, sys: &GeneratedSystem) {
    if let Some(star) = &sys.star {
        title(ui, &format!("STAR OF {}", sys.id.to_uppercase()));
        ui.add_space(4.0);
        legend_row(
            ui,
            star_color(&star.colour_code),
            &format!(
                "{} - {}",
                star.colour_code.to_uppercase(),
                star.colour_name.to_uppercase()
            ),
        );
        if let Some(s) = star.spectral_type.as_ref() {
            kv(ui, "SPECTRAL", &s.to_uppercase());
        }
        kv(ui, "WORLDS", &sys.worlds.len().to_string());
        if let Some(idx) = star.source_row_index {
            dim(ui, &format!("source row: {idx}"));
        }
    } else {
        title(ui, &format!("LOCATION: {}", sys.id.to_uppercase()));
        ui.add_space(4.0);
        kv(ui, "KIND", &format!("{}", sys.kind).to_uppercase());
    }
}

// ── small helpers ─────────────────────────────────────────────────────────

fn routes_block(
    ui: &mut Ui,
    sys: &GeneratedSystem,
    sector: &GeneratedSector,
    cache: Option<&SectorMapCache>,
) {
    let mut hits: Vec<&sectorforge::sector_model::GeneratedRoute> = sector
        .routes
        .iter()
        .filter(|r| r.from_system_id == sys.id || r.to_system_id == sys.id)
        .collect();
    if hits.is_empty() {
        return;
    }
    hits.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    ui.add_space(8.0);
    section(ui, &format!("ROUTES ({})", hits.len()));
    for r in hits {
        let other = if r.from_system_id == sys.id {
            &r.to_system_id
        } else {
            &r.from_system_id
        };
        dim(
            ui,
            &format!(
                "→ {} [{:?}/{:?}] d={}",
                other.to_uppercase(),
                r.route_type,
                r.stability,
                r.distance
            ),
        );
        for c in &r.controls {
            let parts = [
                ("PTRL", c.patrol),
                ("TOLL", c.toll),
                ("INTR", c.interdiction),
                ("PIRC", c.piracy),
                ("SCRC", c.secrecy),
                ("CONF", c.confidence),
            ];
            let mut active: Vec<String> = parts
                .iter()
                .filter(|(_, v)| *v >= 30.0)
                .map(|(l, v)| format!("{l}{:.0}", v))
                .collect();
            if active.is_empty() {
                continue;
            }
            active.sort_unstable();
            let style = cache
                .and_then(|mc| mc.faction_style(&c.faction_id).copied())
                .unwrap_or_else(|| faction_style_by_id(&sector.factions, &c.faction_id));
            legend_row(
                ui,
                style.fill,
                &format!(
                    "{}  {}",
                    short(&c.faction_id.to_uppercase(), 14),
                    active.join(" ")
                ),
            );
        }
    }
}

fn blockade_block(ui: &mut Ui, sys: &GeneratedSystem) {
    if !sys.blockade.under_blockade {
        return;
    }
    ui.add_space(8.0);
    section(ui, "BLOCKADE");
    if let Some(b) = &sys.blockade.blockader {
        kv(ui, "BLOCKADER", &short(&b.to_uppercase(), 22));
    }
    if let Some(b) = &sys.blockade.besieged {
        kv(ui, "BESIEGED", &short(&b.to_uppercase(), 22));
    }
    kv(ui, "INTENSITY", &sys.blockade.intensity.to_string());
}

fn conflict_block(ui: &mut Ui, c: &sectorforge::conflict::ConflictState) {
    if c.intensity == 0 && c.attacker.is_none() && c.defender.is_none() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "CONFLICT");
    kv(ui, "INTENSITY", &c.intensity.to_string());
    kv(ui, "MOMENTUM", &c.momentum.to_string());
    if let Some(a) = &c.attacker {
        kv(ui, "ATTACKER", &short(&a.to_uppercase(), 22));
    }
    if let Some(d) = &c.defender {
        kv(ui, "DEFENDER", &short(&d.to_uppercase(), 22));
    }
    if let Some(v) = &c.visible_controller {
        kv(ui, "VISIBLE", &short(&v.to_uppercase(), 22));
    }
    kv(ui, "AGE", &c.age.to_string());
}

fn archetype_block(ui: &mut Ui, sys: &GeneratedSystem) {
    let a = &sys.archetype;
    if *a == sectorforge::archetypes::ArchetypeState::default() {
        return;
    }
    ui.add_space(8.0);
    section(ui, "ARCHETYPE");
    if !a.imperial_co_sovereigns.is_empty() {
        kv(
            ui,
            "IMP STACK",
            &format!("{} factions", a.imperial_co_sovereigns.len()),
        );
    }
    if a.necron_phase != sectorforge::archetypes::NecronPhase::default() {
        kv(ui, "NECRON", &format!("{}", a.necron_phase).to_uppercase());
    }
    if a.tyranid_stage != sectorforge::archetypes::TyranidStage::default() {
        kv(
            ui,
            "TYRANID",
            &format!("{}", a.tyranid_stage).to_uppercase(),
        );
    }
    if a.ork_waaagh > 0 {
        kv(ui, "WAAAGH", &a.ork_waaagh.to_string());
    }
    if a.gsc_stage != sectorforge::archetypes::GscStage::default() {
        kv(ui, "GSC", &format!("{}", a.gsc_stage).to_uppercase());
    }
    if a.tau_sphere != sectorforge::archetypes::TauSphereBand::default() {
        kv(ui, "TAU", &format!("{}", a.tau_sphere).to_uppercase());
    }
    if a.aeldari_activity > 0 {
        kv(ui, "AELDARI", &a.aeldari_activity.to_string());
    }
    if a.chaos_corruption > 0 {
        kv(ui, "CHAOS", &a.chaos_corruption.to_string());
    }
    if a.daemon_manifestation > 0 {
        kv(ui, "DAEMON", &a.daemon_manifestation.to_string());
    }
}

fn orbital_assets_block(ui: &mut Ui, sys: &GeneratedSystem) {
    if sys.orbital_assets.is_empty() {
        return;
    }
    ui.add_space(8.0);
    section(
        ui,
        &format!("ORBITAL ASSETS ({})", sys.orbital_assets.len()),
    );
    for a in &sys.orbital_assets {
        dim(
            ui,
            &format!(
                "{:?} {} ({}) ",
                a.kind,
                short(&a.faction_id.to_uppercase(), 16),
                a.strength
            ),
        );
    }
}

