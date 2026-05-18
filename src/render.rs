//! Sector → Markdown rendering. Pure; never mutates the sector.

use std::collections::HashMap;

use crate::importance::{
    compute_display_buckets, DisplayBucket, DEFAULT_DISPLAY_CAP, DEFAULT_MINOR_FRACTION,
};
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
        s.push_str(&format_route_controls(sector));
    }

    s.push_str(&format_faction_display_buckets(sector));

    s.push_str("## Factions\n\n");
    if sector.factions.is_empty() {
        s.push_str("_No factions._\n\n");
    } else {
        s.push_str("| ID | Name | Kind | Disposition | Systems | Worlds | Projection | Mil | Naval | Econ | Covert |\n");
        s.push_str("|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|\n");
        for f in &sector.factions {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
                f.id,
                f.name,
                f.kind,
                f.disposition,
                f.system_presence.len(),
                f.world_presence.len(),
                f.power.total_projection(),
                f.power.military,
                f.power.naval,
                f.power.economic,
                f.power.covert,
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
    s.push_str(&format_system_control(sys));
    s.push('\n');
    s.push_str(&format_world_table(sys));
    s.push_str(&format_world_control_blocks(sys));
    s
}

fn format_route_controls(sector: &GeneratedSector) -> String {
    if sector.routes.iter().all(|r| r.controls.is_empty()) {
        return String::new();
    }
    let mut s = String::new();
    s.push_str("### Route control\n\n");
    s.push_str("Per-faction projection along each route (§3). Patrol / Toll / Interdiction / Piracy / Secrecy / Confidence, all 0..=100.\n\n");
    s.push_str("| Route | Faction | Patrol | Toll | Interdict | Piracy | Secrecy | Conf |\n");
    s.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
    for r in &sector.routes {
        for c in &r.controls {
            s.push_str(&format!(
                "| {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
                r.id,
                c.faction_id,
                c.patrol,
                c.toll,
                c.interdiction,
                c.piracy,
                c.secrecy,
                c.confidence,
            ));
        }
    }
    s.push('\n');
    s
}

fn format_faction_display_buckets(sector: &GeneratedSector) -> String {
    if sector.factions.is_empty() {
        return String::new();
    }
    let buckets = compute_display_buckets(sector, DEFAULT_MINOR_FRACTION, DEFAULT_DISPLAY_CAP);
    if buckets.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str("## Faction display buckets\n\n");
    s.push_str("Ranked by [`display_importance`](src/importance.rs) (total projected power × √presence breadth). Low-importance factions are rolled up by kind group.\n\n");
    s.push_str("| Rank | Bucket | Kind / Group | Systems | Worlds | Importance | Members |\n");
    s.push_str("|---:|---|---|---:|---:|---:|---|\n");
    for (i, b) in buckets.iter().enumerate() {
        let (label, kind_label, sys_n, world_n, importance, members) = match b {
            DisplayBucket::Faction {
                id,
                name,
                kind,
                importance,
                system_count,
                world_count,
            } => (
                format!("{name} (`{id}`)"),
                kind.clone(),
                *system_count,
                *world_count,
                *importance,
                String::new(),
            ),
            DisplayBucket::Aggregated {
                label,
                group,
                importance,
                system_count,
                world_count,
                faction_ids,
            } => (
                format!("_{label}_"),
                format!("{group:?}"),
                *system_count,
                *world_count,
                *importance,
                faction_ids.join(", "),
            ),
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.0} | {} |\n",
            i + 1,
            label,
            kind_label,
            sys_n,
            world_n,
            importance,
            members,
        ));
    }
    s.push('\n');
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
    s.push_str(&format_system_control(sys));
    s.push('\n');
    s.push_str(&format_world_table(sys));
    s.push_str(&format_world_control_blocks(sys));
    s
}

fn format_system_control(sys: &GeneratedSystem) -> String {
    let c = &sys.control;
    if c.state.is_none() && c.dominant.is_none() && c.top_factions.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    if let Some(state) = c.state {
        s.push_str(&format!("- **System state:** {state:?}\n"));
    }
    let row = |label: &str, v: &Option<String>| -> String {
        v.as_deref()
            .map(|x| format!("- **{label}:** {x}\n"))
            .unwrap_or_default()
    };
    s.push_str(&row("Dominant controller", &c.dominant));
    s.push_str(&row("Sovereign", &c.sovereign));
    s.push_str(&row("Orbital controller", &c.orbital_controller));
    s.push_str(&row("Economic hegemon", &c.economic_hegemon));
    s.push_str(&row("Hidden master", &c.hidden_master));
    if !c.top_factions.is_empty() {
        let parts: Vec<String> = c
            .top_factions
            .iter()
            .map(|f| format!("{} ({:.0})", f.faction_id, f.score))
            .collect();
        s.push_str(&format!("- **Top factions:** {}\n", parts.join(", ")));
    }
    s.push_str(&format_stability_line(&sys.stability));
    if sys.blockade.under_blockade {
        s.push_str(&format!(
            "- **Blockade:** {} blockading {} (intensity {})\n",
            sys.blockade.blockader.as_deref().unwrap_or("?"),
            sys.blockade.besieged.as_deref().unwrap_or("?"),
            sys.blockade.intensity,
        ));
    }
    if sys.conflict.intensity > 0 {
        s.push_str(&format!(
            "- **Conflict:** intensity {} momentum {} (age {})\n",
            sys.conflict.intensity, sys.conflict.momentum, sys.conflict.age
        ));
    }
    s.push_str(&format_archetype_lines(sys));
    s.push_str(&format_orbital_assets_block(sys));
    s
}

fn format_archetype_lines(sys: &GeneratedSystem) -> String {
    let a = &sys.archetype;
    if *a == crate::archetypes::ArchetypeState::default() {
        return String::new();
    }
    let mut s = String::new();
    if !a.imperial_co_sovereigns.is_empty() {
        s.push_str(&format!(
            "- **Imperial co-sovereigns:** {}\n",
            a.imperial_co_sovereigns.join(", ")
        ));
    }
    if a.necron_phase != crate::archetypes::NecronPhase::default() {
        s.push_str(&format!("- **Necron phase:** {:?}\n", a.necron_phase));
    }
    if a.tyranid_stage != crate::archetypes::TyranidStage::default() {
        s.push_str(&format!("- **Tyranid stage:** {:?}\n", a.tyranid_stage));
    }
    if a.ork_waaagh > 0 {
        s.push_str(&format!("- **Ork Waaagh!:** {}\n", a.ork_waaagh));
    }
    if a.gsc_stage != crate::archetypes::GscStage::default() {
        s.push_str(&format!("- **Genestealer stage:** {:?}\n", a.gsc_stage));
    }
    if a.tau_sphere != crate::archetypes::TauSphereBand::default() {
        s.push_str(&format!("- **Tau sphere:** {:?}\n", a.tau_sphere));
    }
    if a.aeldari_activity > 0 {
        s.push_str(&format!("- **Aeldari activity:** {}\n", a.aeldari_activity));
    }
    if a.chaos_corruption > 0 || a.daemon_manifestation > 0 {
        s.push_str(&format!(
            "- **Chaos:** corruption {} / daemonic {}\n",
            a.chaos_corruption, a.daemon_manifestation
        ));
    }
    s
}

fn format_orbital_assets_block(sys: &GeneratedSystem) -> String {
    if sys.orbital_assets.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    s.push_str("\n**Orbital assets:**\n\n");
    s.push_str("| Kind | Faction | Strength |\n");
    s.push_str("|---|---|---:|\n");
    for a in &sys.orbital_assets {
        s.push_str(&format!(
            "| {:?} | {} | {} |\n",
            a.kind, a.faction_id, a.strength
        ));
    }
    s.push('\n');
    s
}

fn format_stability_line(st: &crate::stability::StabilityState) -> String {
    if *st == crate::stability::StabilityState::default() {
        return String::new();
    }
    format!(
        "- **Stability:** order={:.0} corr={:.0} fear={:.0} rebel={:.0} xenos={:.0} warp={:.0} stress={:.0}\n",
        st.public_order,
        st.corruption,
        st.fear,
        st.rebellion_risk,
        st.xenos_threat,
        st.warp_instability,
        st.famine_or_resource_stress,
    )
}

fn format_world_control_blocks(sys: &GeneratedSystem) -> String {
    let mut s = String::new();
    for w in &sys.worlds {
        if w.factions.is_empty() && w.claims.is_empty() {
            continue;
        }
        s.push_str(&format!("### {} — {}\n\n", w.id.to_uppercase(), w.name));
        if !w.factions.is_empty() {
            s.push_str(
                "| Faction | Influence | Dominance | Control | Admin | Mil | Orb | Econ | Ind | Ideo | Covert | Visib |\n",
            );
            s.push_str("|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
            for p in &w.factions {
                let d = &p.dimensions;
                s.push_str(&format!(
                    "| {} | {:?} | {:?} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
                    p.faction_id,
                    p.influence,
                    p.dominance,
                    d.local_control_score(),
                    d.admin,
                    d.military,
                    d.orbital,
                    d.economic,
                    d.industrial,
                    d.ideological,
                    d.covert,
                    d.visibility,
                ));
            }
            s.push('\n');
        }
        let c = &w.control;
        if c.dominant.is_some()
            || c.sovereign.is_some()
            || c.occupier.is_some()
            || c.economic_hegemon.is_some()
            || c.popular_authority.is_some()
            || c.hidden_master.is_some()
        {
            s.push_str("**Control:**\n");
            let row = |label: &str, v: &Option<String>| -> String {
                v.as_deref()
                    .map(|x| format!("- {label}: {x}\n"))
                    .unwrap_or_default()
            };
            s.push_str(&row("Dominant", &c.dominant));
            s.push_str(&row("Sovereign", &c.sovereign));
            s.push_str(&row("Occupier", &c.occupier));
            s.push_str(&row("Economic hegemon", &c.economic_hegemon));
            s.push_str(&row("Popular authority", &c.popular_authority));
            s.push_str(&row("Hidden master", &c.hidden_master));
            if c.contested {
                s.push_str("- Contested: yes\n");
            }
            s.push_str(&format!("- Control score: {:.0}\n\n", c.control_score));
        }
        if w.stability != crate::stability::StabilityState::default() {
            s.push_str("**Stability:**\n");
            s.push_str(&format!(
                "- Public order: {:.0}\n",
                w.stability.public_order
            ));
            s.push_str(&format!("- Corruption: {:.0}\n", w.stability.corruption));
            s.push_str(&format!("- Fear: {:.0}\n", w.stability.fear));
            s.push_str(&format!(
                "- Rebellion risk: {:.0}\n",
                w.stability.rebellion_risk
            ));
            s.push_str(&format!(
                "- Xenos threat: {:.0}\n",
                w.stability.xenos_threat
            ));
            s.push_str(&format!(
                "- Warp instability: {:.0}\n",
                w.stability.warp_instability
            ));
            s.push_str(&format!(
                "- Famine / resource stress: {:.0}\n\n",
                w.stability.famine_or_resource_stress
            ));
        }
        if !w.claims.is_empty() {
            s.push_str("**Claims:**\n\n");
            s.push_str("| Faction | Type | Strength |\n");
            s.push_str("|---|---|---:|\n");
            for c in &w.claims {
                s.push_str(&format!(
                    "| {} | {:?} | {} |\n",
                    c.faction_id, c.claim_type, c.strength
                ));
            }
            s.push('\n');
        }
        if !w.regions.is_empty() {
            s.push_str("**Surface regions:**\n\n");
            s.push_str("| Region | Kind | Dominant | Score | Pop% | Vis |\n");
            s.push_str("|---|---|---|---:|---:|---:|\n");
            for r in &w.regions {
                s.push_str(&format!(
                    "| {} | {:?} | {} | {} | {} | {} |\n",
                    r.name,
                    r.kind,
                    r.dominant.as_deref().unwrap_or("—"),
                    r.control_score,
                    r.population_weight,
                    r.visibility,
                ));
            }
            s.push('\n');
        }
        if w.conflict.intensity > 0 {
            s.push_str(&format!(
                "**Conflict:** intensity {} momentum {} attacker={} defender={}\n\n",
                w.conflict.intensity,
                w.conflict.momentum,
                w.conflict.attacker.as_deref().unwrap_or("—"),
                w.conflict.defender.as_deref().unwrap_or("—"),
            ));
        }
    }
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
