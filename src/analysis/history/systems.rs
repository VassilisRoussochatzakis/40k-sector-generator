//! Per-system emission: system-state calamities (quarantine/blockade/warzone)
//! and archetype activations (necron awakening, tyranid contact, ork waaagh,
//! GSC exposure, tau contact, aeldari raiding, chaos incursion).

use crate::archetypes::{GscStage, NecronPhase, TauSphereBand, TyranidStage};
use crate::sector_model::{GeneratedSystem, SystemState};

use super::build::build_event;
use super::context::EmitContext;
use super::labels::{gsc_stage_label, tau_band_label};
use super::model::{EventKind, HistoryAnchor, HistoryEvent};

pub(super) fn emit_system_events(
    ctx: &EmitContext,
    sys: &GeneratedSystem,
    out: &mut Vec<HistoryEvent>,
) {
    let mut buf: Vec<(EventKind, String, Vec<crate::ids::FactionId>, u8)> = Vec::new();

    if let Some(state) = sys.control.state {
        match state {
            SystemState::Quarantined => buf.push((
                EventKind::QuarantineDeclared,
                format!(
                    "{} was placed under interdict; warp routes inward sealed.",
                    sys.name
                ),
                Vec::new(),
                EventKind::QuarantineDeclared.base_weight(),
            )),
            SystemState::Blockaded => {
                if sys.blockade.under_blockade {
                    let b = sys.blockade.blockader.clone().unwrap_or_default();
                    let bn = ctx
                        .faction_names
                        .get(b.as_str())
                        .copied()
                        .unwrap_or(b.as_str());
                    buf.push((
                        EventKind::Blockade,
                        format!("{bn} threw a void blockade around {}.", sys.name),
                        if b.is_empty() { vec![] } else { vec![b] },
                        EventKind::Blockade.base_weight(),
                    ));
                }
            }
            SystemState::Warzone => buf.push((
                EventKind::Reconquest,
                format!("{} burned as an open warzone.", sys.name),
                Vec::new(),
                EventKind::Reconquest.base_weight(),
            )),
            SystemState::Infiltrated => buf.push((
                EventKind::CultExposed,
                format!(
                    "Quiet purges revealed entrenched infiltration in {}.",
                    sys.name
                ),
                Vec::new(),
                EventKind::CultExposed.base_weight(),
            )),
            SystemState::Fragmented => buf.push((
                EventKind::Secession,
                format!(
                    "Central authority over {} collapsed into competing seats.",
                    sys.name
                ),
                Vec::new(),
                EventKind::Secession.base_weight(),
            )),
            SystemState::Uncharted => buf.push((
                EventKind::Discovery,
                format!(
                    "{} reappears in the rolls only as an uncharted designate.",
                    sys.name
                ),
                Vec::new(),
                EventKind::Discovery.base_weight(),
            )),
            SystemState::Pacified => {}
        }
    }

    // Archetype-driven events.
    let a = &sys.archetype;
    if a.necron_phase != NecronPhase::None && a.necron_phase != NecronPhase::default() {
        let phrasing = match a.necron_phase {
            NecronPhase::Awakening => "Tomb-stirrings broke the silence in",
            NecronPhase::Awake => "Necron legions ascended openly from the tombs of",
            NecronPhase::Dormant => "Dormant tomb signals were charted beneath",
            NecronPhase::None => "",
        };
        buf.push((
            EventKind::NecronAwakening,
            format!("{phrasing} {}.", sys.name),
            Vec::new(),
            EventKind::NecronAwakening.base_weight(),
        ));
    }
    if a.tyranid_stage != TyranidStage::None {
        let phrase = match a.tyranid_stage {
            TyranidStage::Inhabited => "Splinter bioforms entered",
            TyranidStage::Besieged => "Hive-fleet vanguard pressed",
            TyranidStage::Consumed => "Hive-fleet tendrils stripped",
            TyranidStage::None => "",
        };
        buf.push((
            EventKind::TyranidContact,
            format!("{phrase} {}.", sys.name),
            Vec::new(),
            EventKind::TyranidContact.base_weight(),
        ));
    }
    if a.ork_waaagh >= 40 {
        buf.push((
            EventKind::OrkWaaagh,
            format!("A Waaagh! gathered momentum across {}.", sys.name),
            Vec::new(),
            (EventKind::OrkWaaagh.base_weight() as u32 + a.ork_waaagh as u32 / 5).min(100) as u8,
        ));
    }
    if a.gsc_stage != GscStage::None && a.gsc_stage != GscStage::default() {
        buf.push((
            EventKind::CultExposed,
            format!(
                "Cultist activity surfaced in {} ({}).",
                sys.name,
                gsc_stage_label(a.gsc_stage)
            ),
            Vec::new(),
            EventKind::CultExposed.base_weight(),
        ));
    }
    if a.tau_sphere != TauSphereBand::None && a.tau_sphere != TauSphereBand::default() {
        buf.push((
            EventKind::TauContact,
            format!(
                "T'au expansionists made contact with {} as a {} band.",
                sys.name,
                tau_band_label(a.tau_sphere)
            ),
            Vec::new(),
            EventKind::TauContact.base_weight(),
        ));
    }
    if a.aeldari_activity >= 30 {
        buf.push((
            EventKind::AeldariActivity,
            format!("Aeldari raiding parties returned to {}.", sys.name),
            Vec::new(),
            EventKind::AeldariActivity.base_weight(),
        ));
    }
    if a.chaos_corruption >= 40 || a.daemon_manifestation >= 40 {
        buf.push((
            EventKind::ChaosIncursion,
            format!("Chaotic taint flared visibly through {}.", sys.name),
            Vec::new(),
            (EventKind::ChaosIncursion.base_weight() as u32
                + (a.chaos_corruption as u32 + a.daemon_manifestation as u32) / 10)
                .min(100) as u8,
        ));
    }

    if sys.conflict.intensity >= 50 && !buf.iter().any(|(k, ..)| *k == EventKind::Reconquest) {
        buf.push((
            EventKind::Reconquest,
            format!(
                "Conflict ground on across {}; fronts re-formed annually.",
                sys.name
            ),
            sys.conflict
                .attacker
                .iter()
                .chain(sys.conflict.defender.iter())
                .cloned()
                .collect(),
            EventKind::Reconquest.base_weight(),
        ));
    }

    if buf.len() as u32 > ctx.cfg.max_events_per_system {
        buf.sort_by(|a, b| {
            b.3.cmp(&a.3)
                .then_with(|| a.0.topo_rank().cmp(&b.0.topo_rank()))
        });
        buf.truncate(ctx.cfg.max_events_per_system as usize);
    }
    buf.sort_by_key(|a| a.0.topo_rank());

    for (i, (kind, text, factions, weight)) in buf.into_iter().enumerate() {
        let anchor = HistoryAnchor::System {
            system_id: sys.id.clone(),
        };
        out.push(build_event(ctx, anchor, kind, text, factions, weight, i));
    }
}
