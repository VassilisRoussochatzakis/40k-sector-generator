//! Conflict simulation (§5 NEXT.md, design §11).
//!
//! `ConflictState` is attached to each `GeneratedSystem` and to each
//! `GeneratedWorld`. The generator runs a deterministic seed-derived
//! initial-state pass; callers can then advance the simulation with
//! [`advance_sector`], which applies one tick of decay + escalation +
//! hysteresis (§11.3) to every system in place.
//!
//! Ticks are integer; one tick is "an in-universe rotation of the front."
//! The default sector save records `started_tick = 0`; consumers that
//! want to evolve the sector call `advance_sector(&mut sector)` until
//! they reach the desired age. Generation is single-shot, but the API
//! lets a tool drive ticks externally.

use serde::{Deserialize, Serialize};

use crate::relations::{RelationsMatrix, Stance};
use crate::sector_model::{
    GeneratedSector, GeneratedSystem, GeneratedWorld, SystemControlSummary, SystemState,
};

/// One tick of conflict state for a system or world.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConflictState {
    /// Tick on which the current contention began. 0 = pristine.
    pub started_tick: u32,
    /// Tick of the most recent control change (§11.3 hysteresis tracks this).
    pub last_change_tick: u32,
    /// -100..=100 — positive favours the *attacker* (the controlled
    /// faction that is losing ground), negative favours the *defender*.
    pub momentum: i8,
    /// 0..=100 — overall intensity of fighting / unrest in this entity.
    pub intensity: u8,
    /// 0..=100 — how mobilised the defender / attacker fronts are.
    pub mobilisation: u8,
    /// Active faction id pressing the conflict, if any.
    pub attacker: Option<String>,
    /// Active defender / incumbent. Usually equals
    /// `control.dominant`.
    pub defender: Option<String>,
    /// `visible_controller` (§11.3 hysteresis). Public-facing controller —
    /// only flips when control change has held for `HYSTERESIS_TICKS`.
    pub visible_controller: Option<String>,
    /// Total ticks since last reset.
    pub age: u32,
}

impl ConflictState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Hysteresis window for `visible_controller` flips (§11.3 / §16 NEXT). A
/// real change must persist this many ticks before the public-facing label
/// updates.
pub const HYSTERESIS_TICKS: u32 = 3;

/// Spec §5 NEXT: seed-derive the initial conflict state from a finalised
/// per-world control summary. Pure: zero ticks elapsed.
#[must_use]
pub fn derive_world_conflict(w: &GeneratedWorld) -> ConflictState {
    if w.factions.is_empty() {
        return ConflictState::default();
    }
    let top = w
        .factions
        .iter()
        .map(|p| (p.faction_id.as_str(), p.dimensions.local_control_score()))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let second = w
        .factions
        .iter()
        .map(|p| (p.faction_id.as_str(), p.dimensions.local_control_score()))
        .fold(None::<(&str, f32)>, |acc, x| match acc {
            None => Some(x),
            Some(b) if b.1 < x.1 && top.map(|t| t.0) != Some(x.0) => Some(x),
            other => other,
        });

    let (defender, top_score) = top
        .map(|(id, s)| (Some(id.to_string()), s))
        .unwrap_or((None, 0.0));
    let (attacker, gap) = match second {
        Some((id, s)) => (Some(id.to_string()), top_score - s),
        None => (None, top_score),
    };

    let contested = w.control.contested;
    let intensity = if contested {
        ((100.0 - gap.clamp(0.0, 100.0)) as u8).min(100)
    } else {
        // Even pacified worlds carry a low background unrest level.
        (w.stability.rebellion_risk * 0.3) as u8
    };
    let momentum: i8 = if contested {
        // Attacker has half the defender's score: momentum biased toward attacker.
        let m = -((gap.clamp(0.0, 50.0) / 50.0 * 100.0) as i32) + 0;
        m.clamp(-100, 100) as i8
    } else {
        0
    };

    ConflictState {
        started_tick: 0,
        last_change_tick: 0,
        momentum,
        intensity,
        mobilisation: (intensity / 2).min(100),
        attacker,
        defender: defender.clone(),
        visible_controller: defender,
        age: 0,
    }
}

/// Roll up world-level conflict to a system-level summary. Picks the
/// strongest contested world and copies its `attacker`/`defender`/momentum
/// up to the system.
#[must_use]
pub fn derive_system_conflict(sys: &GeneratedSystem) -> ConflictState {
    if sys.worlds.is_empty() {
        return ConflictState::default();
    }
    let mut worst: Option<&ConflictState> = None;
    for w in &sys.worlds {
        if worst
            .map(|x| w.conflict.intensity > x.intensity)
            .unwrap_or(true)
        {
            worst = Some(&w.conflict);
        }
    }
    let mut out = worst.cloned().unwrap_or_default();
    // System state-driven baseline if no world has been seeded yet.
    if out.intensity == 0 {
        out.intensity = baseline_for_state(sys.control.state);
    }
    out.visible_controller = sys.control.dominant.clone();
    out
}

fn baseline_for_state(state: Option<SystemState>) -> u8 {
    match state {
        Some(SystemState::Warzone) => 70,
        Some(SystemState::Blockaded) => 40,
        Some(SystemState::Infiltrated) => 30,
        Some(SystemState::Fragmented) => 25,
        Some(SystemState::Quarantined) => 50,
        _ => 0,
    }
}

/// Advance every system in a sector by one tick. Mutates in place.
///
/// Per-tick rules (§11):
/// * Intensity decays by 2 toward 0 when not actively contested
///   (`attacker == defender` or no attacker).
/// * Intensity grows by `+(momentum.abs() / 10)` when an attacker exists.
/// * Momentum drifts toward 0 by 3 each tick (cease-fire bias).
/// * If `intensity >= 80` and `momentum < -40`, the attacker is recorded
///   as the new defender — this is the actual control change.
/// * The public `visible_controller` only updates after the swap has held
///   for `HYSTERESIS_TICKS` ticks (§11.3).
pub fn advance_sector(sector: &mut GeneratedSector) {
    // §4 NEW.md: if the project enabled `relations.feed_conflict`, the derived
    // matrix mirrors the flag and we apply a stance-driven momentum/intensity
    // bias before the standard tick logic. Read-only on the matrix.
    let bias = sector.relations.feed_conflict;
    let relations = sector.relations.clone();
    for sys in sector.systems.iter_mut() {
        for w in sys.worlds.iter_mut() {
            if bias {
                apply_relations_bias(&mut w.conflict, &relations);
            }
            advance_one(&mut w.conflict);
        }
        // Roll updated world conflict back into the system summary.
        sys.conflict = derive_system_conflict(sys);
        propagate_to_summary(&mut sys.control, &sys.conflict);
    }
}

fn apply_relations_bias(c: &mut ConflictState, relations: &RelationsMatrix) {
    let (Some(a), Some(d)) = (c.attacker.as_deref(), c.defender.as_deref()) else {
        return;
    };
    if a == d {
        return;
    }
    let Some(stance) = relations.stance_between(a, d) else {
        return;
    };
    match stance {
        Stance::AtWar => {
            c.momentum = (c.momentum as i32 - 4).clamp(-100, 100) as i8;
            c.intensity = (c.intensity as u16 + 3).min(100) as u8;
        }
        Stance::Hostile => {
            c.momentum = (c.momentum as i32 - 2).clamp(-100, 100) as i8;
            c.intensity = (c.intensity as u16 + 1).min(100) as u8;
        }
        Stance::Allied | Stance::Aligned => {
            c.momentum = (c.momentum as i32 + 2).clamp(-100, 100) as i8;
            c.intensity = c.intensity.saturating_sub(2);
        }
        Stance::Rival | Stance::Neutral => {}
    }
}

fn advance_one(c: &mut ConflictState) {
    c.age = c.age.saturating_add(1);

    // Momentum drifts toward 0.
    if c.momentum > 0 {
        c.momentum = (c.momentum - 3).max(0);
    } else if c.momentum < 0 {
        c.momentum = (c.momentum + 3).min(0);
    }

    let active = c
        .attacker
        .as_ref()
        .zip(c.defender.as_ref())
        .map(|(a, d)| a != d)
        .unwrap_or(false);
    if active {
        let bump = (c.momentum.unsigned_abs() / 10).min(20);
        c.intensity = (c.intensity as u16 + bump as u16).min(100) as u8;
    } else {
        c.intensity = c.intensity.saturating_sub(2);
    }
    c.mobilisation = c.mobilisation.saturating_sub(1);

    // Control change candidate: attacker decisively winning.
    if c.intensity >= 80 && c.momentum <= -40 {
        if let Some(attacker) = c.attacker.clone() {
            if c.defender.as_deref() != Some(attacker.as_str()) {
                c.defender = Some(attacker);
                c.last_change_tick = c.age;
                c.momentum = 0;
                c.intensity = c.intensity.saturating_sub(20);
            }
        }
    }

    // Hysteresis: `visible_controller` lags `defender` by HYSTERESIS_TICKS.
    if let Some(def) = c.defender.clone() {
        let already_visible = c.visible_controller.as_deref() == Some(def.as_str());
        if !already_visible && c.age.saturating_sub(c.last_change_tick) >= HYSTERESIS_TICKS {
            c.visible_controller = Some(def);
        }
    }
}

fn propagate_to_summary(summary: &mut SystemControlSummary, c: &ConflictState) {
    if let Some(vc) = &c.visible_controller {
        summary.dominant = Some(vc.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_reduces_intensity_when_not_active() {
        let mut c = ConflictState {
            intensity: 50,
            ..Default::default()
        };
        advance_one(&mut c);
        assert_eq!(c.intensity, 48);
        for _ in 0..30 {
            advance_one(&mut c);
        }
        assert_eq!(c.intensity, 0);
    }

    #[test]
    fn attacker_can_flip_control_after_threshold_then_hysteresis() {
        let mut c = ConflictState {
            attacker: Some("a".into()),
            defender: Some("d".into()),
            visible_controller: Some("d".into()),
            momentum: -80,
            intensity: 85,
            ..Default::default()
        };
        // First tick: momentum at -80 (abs/10 = 8) bumps intensity to 93.
        // Intensity 93 >= 80, momentum -80 <= -40 → swap defender to attacker.
        advance_one(&mut c);
        // Tip-over conditions met → defender flipped, intensity slashed.
        assert_eq!(c.defender.as_deref(), Some("a"));
        // visible_controller not yet flipped (hysteresis 3 ticks).
        assert_eq!(c.visible_controller.as_deref(), Some("d"));
        // Advance 3 more ticks → visible flips.
        for _ in 0..3 {
            advance_one(&mut c);
        }
        assert_eq!(c.visible_controller.as_deref(), Some("a"));
    }
}
