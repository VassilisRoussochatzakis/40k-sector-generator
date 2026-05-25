//! Per-world emission: foundation, claim-derived (mandate/charter/dynasty),
//! contested-control reconquest, hidden-cult exposure, full-warfare purge.

use crate::sector_model::{ClaimType, GeneratedSystem, GeneratedWorld};

use super::build::build_event;
use super::context::EmitContext;
use super::labels::article_phrase;
use super::model::{EventKind, HistoryAnchor, HistoryEvent};

pub(super) fn emit_world_events(
    ctx: &EmitContext,
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    out: &mut Vec<HistoryEvent>,
) {
    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();

    // Foundation — every world gets one.
    let foundation_text = format!(
        "Records place the founding of {} in this era; the world was settled as {} and registered {}.",
        w.name,
        article_phrase(&w.world.world_type),
        article_phrase(&w.world.government),
    );
    buf.push((
        EventKind::Foundation,
        foundation_text,
        Vec::new(),
        EventKind::Foundation.base_weight(),
    ));

    // Claim-derived events.
    for c in &w.claims {
        let fname = ctx
            .faction_names
            .get(c.faction_id.as_str())
            .copied()
            .unwrap_or(c.faction_id.as_str());
        let (kind, text) = match c.claim_type {
            ClaimType::LegalSovereignty => (
                EventKind::DynasticClaim,
                format!("{fname} secured legal sovereignty over {}.", w.name),
            ),
            ClaimType::ImperialMandate => (
                EventKind::ImperialMandateGranted,
                format!(
                    "The Adeptus Terra entered {} into the Imperial registry; {fname} took custody under Imperial Mandate.",
                    w.name
                ),
            ),
            ClaimType::TreatyRight => (
                EventKind::CommercialCharter,
                format!("{fname} concluded a treaty asserting standing rights on {}.", w.name),
            ),
            ClaimType::ReligiousMandate => (
                EventKind::Consecration,
                format!("{fname} consecrated {} as a charge of the faith.", w.name),
            ),
            ClaimType::DynasticRight => (
                EventKind::DynasticClaim,
                format!("{fname} pressed an ancestral dynastic right over {}.", w.name),
            ),
            ClaimType::CommercialCharter => (
                EventKind::CommercialCharter,
                format!("{fname} was granted a commercial charter to operate on {}.", w.name),
            ),
            ClaimType::MilitaryOccupation => (
                EventKind::Annexation,
                format!("{fname} seized {} by force of arms.", w.name),
            ),
            ClaimType::AncientDomain => (
                EventKind::Discovery,
                format!("Surveys revealed the ancient domain of {fname} beneath {}.", w.name),
            ),
            ClaimType::HuntingGround => (
                EventKind::AeldariActivity,
                format!("{fname} marked {} as a recurring hunting ground.", w.name),
            ),
            ClaimType::CovertWrit => (
                EventKind::CultExposed,
                format!("{fname} obtained a covert writ binding agents on {}.", w.name),
            ),
            ClaimType::Rebellion => (
                EventKind::Uprising,
                format!(
                    "Loyalist authority on {} collapsed; {fname} declared open rebellion.",
                    w.name
                ),
            ),
        };
        let weight = (kind.base_weight() as u32 + (c.strength as u32 / 4)).min(100) as u8;
        buf.push((kind, text, vec![c.faction_id.clone()], weight));
    }

    // Contested status — a recent reconquest event.
    if w.control.contested {
        if let (Some(dom), Some(sov)) = (&w.control.dominant, &w.control.sovereign) {
            if dom != sov {
                let dom_n = ctx.faction_names.get(dom.as_str()).copied().unwrap_or(dom);
                let sov_n = ctx.faction_names.get(sov.as_str()).copied().unwrap_or(sov);
                buf.push((
                    EventKind::Reconquest,
                    format!(
                        "Authority on {} fractured: {dom_n} seized de facto control while {sov_n} retained the sovereign claim.",
                        w.name
                    ),
                    vec![dom.clone(), sov.clone()],
                    EventKind::Reconquest.base_weight(),
                ));
            }
        }
        if let Some(hidden) = &w.control.hidden_master {
            let n = ctx
                .faction_names
                .get(hidden.as_str())
                .copied()
                .unwrap_or(hidden);
            buf.push((
                EventKind::CultExposed,
                format!(
                    "Inquisitorial probes catalogued covert influence by {n} on {}.",
                    w.name
                ),
                vec![hidden.clone()],
                EventKind::CultExposed.base_weight(),
            ));
        }
    }

    // Conflict — a Purge event if heavy intensity.
    if w.conflict.intensity >= 60 {
        let attacker = w.conflict.attacker.clone().unwrap_or_default();
        let defender = w.conflict.defender.clone().unwrap_or_default();
        let an = ctx
            .faction_names
            .get(attacker.as_str())
            .copied()
            .unwrap_or(attacker.as_str());
        let dn = ctx
            .faction_names
            .get(defender.as_str())
            .copied()
            .unwrap_or(defender.as_str());
        buf.push((
            EventKind::Purge,
            format!(
                "Open warfare engulfed {}; {an} pressed an offensive against {dn}.",
                w.name
            ),
            [attacker, defender]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect(),
            EventKind::Purge.base_weight(),
        ));
    }

    // Truncate per-world, retain strongest.
    if buf.len() as u32 > ctx.cfg.max_events_per_world {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(ctx.cfg.max_events_per_world as usize);
    }

    // Resort by topological rank so the chronicle reads forward.
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::World {
            system_id: sys.id.clone(),
            world_id: w.id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}
