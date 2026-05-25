//! Declarative `[[event_rules]]` enforcement: ensures minimum events for
//! matching current-state facts (e.g., `Warzone` → at least one War event).

use std::collections::BTreeMap;

use crate::sector_model::GeneratedSystem;

use super::build::build_event;
use super::config::HistoryEventRule;
use super::context::EmitContext;
use super::labels::kind_slug;
use super::model::{EventKind, HistoryAnchor, HistoryEvent};

pub(super) fn apply_event_rules(ctx: &EmitContext, out: &mut Vec<HistoryEvent>) {
    for (rule_idx, rule) in ctx.cfg.event_rules.iter().enumerate() {
        let Some(kind) = rule.prefer_event.as_deref().and_then(event_kind_from_str) else {
            continue;
        };
        for sys in &ctx.sector.systems {
            if !rule_matches_system(rule, sys) {
                continue;
            }
            let existing = out
                .iter()
                .filter(|e| e.kind == kind)
                .filter(|e| match &e.anchor {
                    HistoryAnchor::System { system_id } => system_id == &sys.id,
                    _ => false,
                })
                .count() as u32;
            for extra in existing..rule.minimum_events {
                let text = rule_event_text(kind, sys, ctx.faction_names);
                let anchor = HistoryAnchor::System {
                    system_id: sys.id.clone(),
                };
                let mut ev = build_event(
                    ctx,
                    anchor,
                    kind,
                    text,
                    sys.conflict
                        .attacker
                        .iter()
                        .chain(sys.conflict.defender.iter())
                        .cloned()
                        .collect(),
                    kind.base_weight(),
                    rule_idx * 1000 + extra as usize,
                );
                let rule_id = rule
                    .id
                    .as_deref()
                    .map(normalize_key)
                    .unwrap_or_else(|| format!("rule-{rule_idx}"));
                ev.id = format!("evt-{rule_id}-{}-{}-{extra}", sys.id, kind_slug(kind));
                out.push(ev);
            }
        }
    }
}

pub(super) fn event_kind_from_str(s: &str) -> Option<EventKind> {
    let key: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    use EventKind::*;
    match key.as_str() {
        "foundation" | "founding" => Some(Foundation),
        "discovery" | "disappearance" => Some(Discovery),
        "annexation" => Some(Annexation),
        "compliance" | "imperialmandate" | "imperialmandategranted" => Some(ImperialMandateGranted),
        "consecration" => Some(Consecration),
        "treaty" | "commercialcharter" => Some(CommercialCharter),
        "dynasticclaim" | "dynasty" => Some(DynasticClaim),
        "schism" | "secession" => Some(Secession),
        "rebellion" | "uprising" => Some(Uprising),
        "war" | "reconquest" | "crusade" => Some(Reconquest),
        "purge" => Some(Purge),
        "cultexposed" | "cult" => Some(CultExposed),
        "awakening" | "necronawakening" | "necron" => Some(NecronAwakening),
        "tyranidcontact" | "tyranid" => Some(TyranidContact),
        "orkwaaagh" | "waaagh" | "ork" => Some(OrkWaaagh),
        "quarantinedeclared" | "quarantine" => Some(QuarantineDeclared),
        "blockade" => Some(Blockade),
        "plague" | "warpstormsurge" | "warpstorm" => Some(WarpStormSurge),
        "taucontact" | "tau" => Some(TauContact),
        "aeldariactivity" | "aeldari" => Some(AeldariActivity),
        "chaosincursion" | "chaos" => Some(ChaosIncursion),
        _ => None,
    }
}

fn rule_matches_system(rule: &HistoryEventRule, sys: &GeneratedSystem) -> bool {
    let Some(want) = rule.when_system_state.as_deref() else {
        return true;
    };
    let Some(state) = sys.control.state else {
        return false;
    };
    normalize_key(want) == normalize_key(&format!("{state:?}"))
}

fn normalize_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn rule_event_text(
    kind: EventKind,
    sys: &GeneratedSystem,
    faction_names: &BTreeMap<&str, &str>,
) -> String {
    match kind {
        EventKind::Reconquest | EventKind::Purge => {
            let actors: Vec<&str> = sys
                .conflict
                .attacker
                .iter()
                .chain(sys.conflict.defender.iter())
                .map(|f| faction_names.get(f.as_str()).copied().unwrap_or(f.as_str()))
                .collect();
            if actors.is_empty() {
                format!("{} entered the chronicle as a sustained warzone.", sys.name)
            } else {
                format!(
                    "{} entered the chronicle as a sustained warzone involving {}.",
                    sys.name,
                    actors.join(" and ")
                )
            }
        }
        EventKind::Blockade => format!(
            "Route ledgers preserve the blockade crisis at {}.",
            sys.name
        ),
        EventKind::QuarantineDeclared => {
            format!(
                "{} was sealed by rule-triggered quarantine memory.",
                sys.name
            )
        }
        EventKind::Uprising => format!("{} retains records of a precursor rebellion.", sys.name),
        _ => format!(
            "{} gained a rule-mandated {:?} entry in the sector chronicle.",
            sys.name, kind
        ),
    }
}
