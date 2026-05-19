//! Adventure / plot-hook generator (§7 NEW.md).
//!
//! Pure derivation over the finished sector model: scans worlds, systems,
//! and routes for combinations that imply running drama (contested claims,
//! hidden masters, blockades, perilous routes, archetype activity) and
//! emits structured one-line hooks suitable as session prompts. No new
//! random RNG draws — every hook is a deterministic consequence of the
//! input model.
//!
//! Hooks are ranked by an internal *dramatic weight* (contested-ness,
//! number of competing claims, stability extremes, conflict intensity);
//! the sector-wide digest takes the top-N by weight.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::archetypes::{GscStage, NecronPhase, TyranidStage};
use crate::errors::SectorError;
use crate::sector_model::{
    ClaimType, GeneratedRoute, GeneratedSector, GeneratedSystem, GeneratedWorld, RouteStability,
    SystemState,
};

// ── Config ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HooksConfig {
    /// Maximum hooks emitted per anchor.
    #[serde(default = "default_per_anchor")]
    pub max_per_anchor: u32,
    /// Top-N digest size for the sector-wide hook list in Markdown.
    #[serde(default = "default_top")]
    pub top_n_digest: u32,
    /// Hide hooks that depend on a Hidden-tier presence ("player-facing"
    /// mode). Defaults to false (GM mode — show everything).
    #[serde(default)]
    pub hide_hidden_hooks: bool,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            max_per_anchor: default_per_anchor(),
            top_n_digest: default_top(),
            hide_hidden_hooks: false,
        }
    }
}

fn default_per_anchor() -> u32 {
    3
}
fn default_top() -> u32 {
    10
}

// ── Output DTOs ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksReport {
    pub sector_id: String,
    pub seed: String,
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: String,
    pub kind: HookKind,
    pub anchor: HookAnchor,
    pub title: String,
    pub situation: String,
    pub stakes: String,
    pub factions: Vec<String>,
    pub complications: Vec<String>,
    pub weight: u32,
    /// True when the hook depends on a Hidden-tier presence; redacted in
    /// "player edition" output.
    pub gm_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum HookAnchor {
    System { system_id: String },
    World { system_id: String, world_id: String },
    Route { route_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookKind {
    CounterInfiltration,
    Reconquest,
    LostPassage,
    ConvoyEscort,
    BlockadeRun,
    HoldTheLine,
    SealedTombs,
    CrushUprising,
    SealedSystem,
    CultPurge,
    DiplomaticCrisis,
    SuccessionDispute,
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[must_use]
pub fn derive(sector: &GeneratedSector) -> HooksReport {
    derive_with(sector, &HooksConfig::default())
}

#[must_use]
pub fn derive_with(sector: &GeneratedSector, cfg: &HooksConfig) -> HooksReport {
    let mut out: Vec<Hook> = Vec::new();

    for sys in &sector.systems {
        emit_system_hooks(sys, cfg, &mut out);
        for w in &sys.worlds {
            emit_world_hooks(sys, w, cfg, &mut out);
        }
    }
    for r in &sector.routes {
        emit_route_hooks(r, sector, cfg, &mut out);
    }

    // Dedupe by id and apply max_per_anchor cap.
    cap_per_anchor(&mut out, cfg.max_per_anchor);
    out.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));

    if cfg.hide_hidden_hooks {
        out.retain(|h| !h.gm_only);
    }

    HooksReport {
        sector_id: sector.id.clone(),
        seed: sector.seed.clone(),
        hooks: out,
    }
}

fn cap_per_anchor(hooks: &mut Vec<Hook>, cap: u32) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    hooks.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.id.cmp(&b.id)));
    hooks.retain(|h| {
        let key = match &h.anchor {
            HookAnchor::System { system_id } => format!("s:{system_id}"),
            HookAnchor::World {
                system_id,
                world_id,
            } => format!("w:{system_id}:{world_id}"),
            HookAnchor::Route { route_id } => format!("r:{route_id}"),
        };
        let entry = counts.entry(key).or_insert(0);
        if *entry < cap {
            *entry += 1;
            true
        } else {
            false
        }
    });
}

// ── World hooks ────────────────────────────────────────────────────────────────

fn emit_world_hooks(
    sys: &GeneratedSystem,
    w: &GeneratedWorld,
    _cfg: &HooksConfig,
    out: &mut Vec<Hook>,
) {
    let anchor = HookAnchor::World {
        system_id: sys.id.clone(),
        world_id: w.id.clone(),
    };

    // Counter-infiltration: hidden master is GSC-like (genestealer kind),
    // approximated by presence of hidden_master while archetype gsc_stage is set.
    if let Some(hidden) = &w.control.hidden_master {
        if sys.archetype.gsc_stage != GscStage::None
            && sys.archetype.gsc_stage != GscStage::default()
        {
            out.push(Hook {
                id: format!("hook-{}-{}-counter_infiltration", sys.id, w.id),
                kind: HookKind::CounterInfiltration,
                anchor: anchor.clone(),
                title: format!("Counter-infiltration on {}", w.name),
                situation: format!(
                    "Cult activity is suspected on {}; covert master {} pulls the strings.",
                    w.name, hidden
                ),
                stakes: "Identify the Patriarch before the cult triggers a rising.".into(),
                factions: vec![hidden.clone()],
                complications: vec![
                    "Local enforcers are themselves compromised.".into(),
                    "Witnesses keep disappearing.".into(),
                ],
                weight: 80 + dramatic_bonus(w),
                gm_only: true,
            });
        }
    }

    // Reconquest / liberation: a claim mix with MilitaryOccupation alongside
    // ImperialMandate / LegalSovereignty.
    let occupier_claim = w
        .claims
        .iter()
        .find(|c| c.claim_type == ClaimType::MilitaryOccupation);
    let legitimate_claim = w.claims.iter().find(|c| {
        matches!(
            c.claim_type,
            ClaimType::ImperialMandate | ClaimType::LegalSovereignty | ClaimType::DynasticRight
        )
    });
    if let (Some(occ), Some(legit)) = (occupier_claim, legitimate_claim) {
        if occ.faction_id != legit.faction_id {
            out.push(Hook {
                id: format!("hook-{}-{}-reconquest", sys.id, w.id),
                kind: HookKind::Reconquest,
                anchor: anchor.clone(),
                title: format!("Liberation of {}", w.name),
                situation: format!(
                    "{} holds {} by force while {} retains the legitimate claim.",
                    occ.faction_id, w.name, legit.faction_id
                ),
                stakes: "Restore the legitimate authority — or entrench the occupier.".into(),
                factions: vec![occ.faction_id.clone(), legit.faction_id.clone()],
                complications: vec![
                    "Civilian resistance is itself fractured.".into(),
                    "Off-world reinforcements may arrive on either side.".into(),
                ],
                weight: 75 + dramatic_bonus(w),
                gm_only: false,
            });
        }
    }

    // Crush uprising — Rebellion claim.
    if let Some(rebel) = w
        .claims
        .iter()
        .find(|c| c.claim_type == ClaimType::Rebellion)
    {
        out.push(Hook {
            id: format!("hook-{}-{}-crush_uprising", sys.id, w.id),
            kind: HookKind::CrushUprising,
            anchor: anchor.clone(),
            title: format!("Uprising on {}", w.name),
            situation: format!(
                "{} has declared open rebellion against the planetary government.",
                rebel.faction_id
            ),
            stakes: "Suppress the revolt before it spreads to neighbouring worlds.".into(),
            factions: vec![rebel.faction_id.clone()],
            complications: vec![
                "Loyalist regiments suspect their own officers.".into(),
                "Off-world sympathisers run guns to the rebels.".into(),
            ],
            weight: 65 + dramatic_bonus(w),
            gm_only: false,
        });
    }

    // Cult purge — chaos corruption on the system.
    if sys.archetype.chaos_corruption >= 30 && !w.factions.is_empty() {
        out.push(Hook {
            id: format!("hook-{}-{}-cult_purge", sys.id, w.id),
            kind: HookKind::CultPurge,
            anchor: anchor.clone(),
            title: format!("Cult Purge on {}", w.name),
            situation: format!(
                "Whispers of the Dark Gods walk openly in {}'s under-strata; corruption rating {}.",
                w.name, sys.archetype.chaos_corruption
            ),
            stakes: "Excise the cult — quietly, or with fire.".into(),
            factions: w.factions.iter().map(|p| p.faction_id.clone()).collect(),
            complications: vec![
                "Local magistrates are themselves tainted.".into(),
                "Daemonic manifestation risk rises with every public action.".into(),
            ],
            weight: 70 + sys.archetype.chaos_corruption as u32 / 2,
            gm_only: false,
        });
    }

    // Hold the line — Tyranid contact.
    if sys.archetype.tyranid_stage != TyranidStage::None {
        out.push(Hook {
            id: format!("hook-{}-{}-hold_the_line", sys.id, w.id),
            kind: HookKind::HoldTheLine,
            anchor: anchor.clone(),
            title: format!("Hold the Line at {}", w.name),
            situation: format!(
                "Tyranid {:?} pressure has reached {}.",
                sys.archetype.tyranid_stage, w.name
            ),
            stakes: "Buy time for evacuation, or die in place.".into(),
            factions: vec![],
            complications: vec![
                "The bioform shifts adaptations across the campaign.".into(),
                "Hive-fleet psychic shadow blinds astropaths.".into(),
            ],
            weight: 90,
            gm_only: false,
        });
    }
}

// ── System hooks ───────────────────────────────────────────────────────────────

fn emit_system_hooks(sys: &GeneratedSystem, _cfg: &HooksConfig, out: &mut Vec<Hook>) {
    let anchor = HookAnchor::System {
        system_id: sys.id.clone(),
    };
    if let Some(state) = sys.control.state {
        match state {
            SystemState::Quarantined => out.push(Hook {
                id: format!("hook-{}-sealed_system", sys.id),
                kind: HookKind::SealedSystem,
                anchor: anchor.clone(),
                title: format!("Breach the Quarantine of {}", sys.name),
                situation: format!(
                    "{} is under interdict; warp routes inward sealed.",
                    sys.name
                ),
                stakes: "Recover what is inside — or ensure it stays sealed.".into(),
                factions: vec![],
                complications: vec![
                    "Whatever the Inquisition sealed away wants out.".into(),
                    "Sentinel patrols shoot first.".into(),
                ],
                weight: 70,
                gm_only: false,
            }),
            SystemState::Blockaded => {
                if sys.blockade.under_blockade {
                    out.push(Hook {
                        id: format!("hook-{}-blockade_run", sys.id),
                        kind: HookKind::BlockadeRun,
                        anchor: anchor.clone(),
                        title: format!("Run the Blockade of {}", sys.name),
                        situation: format!(
                            "{} is closed by {}; supplies are running thin within.",
                            sys.name,
                            sys.blockade
                                .blockader
                                .as_deref()
                                .unwrap_or("unknown forces"),
                        ),
                        stakes: "Get the cargo through — or starve the system.".into(),
                        factions: sys.blockade.blockader.iter().cloned().collect(),
                        complications: vec![
                            "Inside contacts may have already turned.".into(),
                            "The blockade fleet is hunting for a pattern of break-runs.".into(),
                        ],
                        weight: 75,
                        gm_only: false,
                    });
                }
            }
            SystemState::Warzone => out.push(Hook {
                id: format!("hook-{}-warzone", sys.id),
                kind: HookKind::Reconquest,
                anchor: anchor.clone(),
                title: format!("Front lines of {}", sys.name),
                situation: format!("{} is an active warzone; fronts shift weekly.", sys.name),
                stakes: "Take the next objective before it is taken from you.".into(),
                factions: vec![],
                complications: vec![
                    "Atrocity allegations on both sides.".into(),
                    "A third faction is taking advantage.".into(),
                ],
                weight: 78,
                gm_only: false,
            }),
            _ => {}
        }
    }
    if sys.archetype.necron_phase != NecronPhase::None
        && sys.archetype.necron_phase != NecronPhase::default()
    {
        out.push(Hook {
            id: format!("hook-{}-sealed_tombs", sys.id),
            kind: HookKind::SealedTombs,
            anchor: anchor.clone(),
            title: format!("Sealed Tombs beneath {}", sys.name),
            situation: format!(
                "Necron phase {:?} signatures pulse beneath {}'s surface.",
                sys.archetype.necron_phase, sys.name
            ),
            stakes: "Seal the tomb again before it stirs — or claim what lies within.".into(),
            factions: vec![],
            complications: vec![
                "Multiple expeditions race to the same vault.".into(),
                "Cryptek defenders adapt mid-mission.".into(),
            ],
            weight: 80,
            gm_only: false,
        });
    }
    if sys.control.top_factions.len() >= 3 {
        out.push(Hook {
            id: format!("hook-{}-succession", sys.id),
            kind: HookKind::SuccessionDispute,
            anchor,
            title: format!("Three-way contest in {}", sys.name),
            situation: format!("{} has three competing power blocs.", sys.name),
            stakes: "Tip the balance — or sell to all three.".into(),
            factions: sys
                .control
                .top_factions
                .iter()
                .take(3)
                .map(|f| f.faction_id.clone())
                .collect(),
            complications: vec![
                "Promises made to one bloc are leaked to another.".into(),
                "A fourth player is waiting for them to bleed each other out.".into(),
            ],
            weight: 68,
            gm_only: false,
        });
    }
}

// ── Route hooks ────────────────────────────────────────────────────────────────

fn emit_route_hooks(
    r: &GeneratedRoute,
    _: &GeneratedSector,
    _cfg: &HooksConfig,
    out: &mut Vec<Hook>,
) {
    if r.stability == RouteStability::Perilous {
        out.push(Hook {
            id: format!("hook-{}-lost_passage", r.id),
            kind: HookKind::LostPassage,
            anchor: HookAnchor::Route {
                route_id: r.id.clone(),
            },
            title: format!("Find the Lost Passage ({})", r.id),
            situation: format!(
                "The route between {} and {} is rated Perilous; merchant traffic has all but ceased.",
                r.from_system_id, r.to_system_id
            ),
            stakes: "Open a safer transit — or close it for good.".into(),
            factions: vec![],
            complications: vec![
                "Previous expeditions failed to return.".into(),
                "The hazard may itself be a deception.".into(),
            ],
            weight: 65,
            gm_only: false,
        });
    }
    // Convoy escort: patrol + tolls + piracy on any controller.
    let convoy_target = r.controls.iter().find(|c| c.patrol > 30.0 && c.toll > 30.0);
    let pirated = r.controls.iter().any(|c| c.piracy > 30.0);
    if let Some(c) = convoy_target {
        if pirated {
            out.push(Hook {
                id: format!("hook-{}-convoy", r.id),
                kind: HookKind::ConvoyEscort,
                anchor: HookAnchor::Route {
                    route_id: r.id.clone(),
                },
                title: format!("Convoy Escort ({} → {})", r.from_system_id, r.to_system_id),
                situation: format!(
                    "{} patrols and tolls this lane but cannot stop the piracy along it.",
                    c.faction_id
                ),
                stakes: "Deliver the cargo intact through pirate-infested space.".into(),
                factions: vec![c.faction_id.clone()],
                complications: vec![
                    "The toll authority and the pirates share a quartermaster.".into(),
                    "One of your passengers is the actual mark.".into(),
                ],
                weight: 70,
                gm_only: false,
            });
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn dramatic_bonus(w: &GeneratedWorld) -> u32 {
    let mut b = 0u32;
    if w.control.contested {
        b += 10;
    }
    b += (w.claims.len() as u32).min(4) * 3;
    if w.conflict.intensity >= 40 {
        b += 5;
    }
    b
}

// ── Markdown ───────────────────────────────────────────────────────────────────

#[must_use]
pub fn render_markdown(report: &HooksReport, cfg: &HooksConfig) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Plot Hooks — {}", report.sector_id);
    let _ = writeln!(s, "\nSeed: `{}`", report.seed);
    let _ = writeln!(s, "\nTotal hooks: **{}**", report.hooks.len());

    let n = (cfg.top_n_digest as usize).min(report.hooks.len());
    let _ = writeln!(s, "\n## Top hooks");
    for h in report.hooks.iter().take(n) {
        render_hook(&mut s, h);
    }

    // Group remainder by anchor type.
    if report.hooks.len() > n {
        let _ = writeln!(s, "\n## Remaining hooks");
        for h in report.hooks.iter().skip(n) {
            render_hook(&mut s, h);
        }
    }
    s
}

fn render_hook(s: &mut String, h: &Hook) {
    let anchor = match &h.anchor {
        HookAnchor::System { system_id } => format!("`{system_id}`"),
        HookAnchor::World {
            system_id,
            world_id,
        } => format!("`{system_id}/{world_id}`"),
        HookAnchor::Route { route_id } => format!("`route {route_id}`"),
    };
    let gm = if h.gm_only { " (GM-only)" } else { "" };
    let _ = writeln!(
        s,
        "\n### {} — {} {anchor}{gm}",
        h.title,
        format!("{:?}", h.kind)
    );
    let _ = writeln!(s, "\n- **Weight**: {}", h.weight);
    let _ = writeln!(s, "- **Situation**: {}", h.situation);
    let _ = writeln!(s, "- **Stakes**: {}", h.stakes);
    if !h.factions.is_empty() {
        let _ = writeln!(s, "- **Factions**: {}", h.factions.join(", "));
    }
    if !h.complications.is_empty() {
        let _ = writeln!(s, "- **Complications**:");
        for c in &h.complications {
            let _ = writeln!(s, "  - {c}");
        }
    }
}

/// Write `hooks.md` + `hooks.json` into `output_dir`.
///
/// # Errors
///
/// Returns [`SectorError::Io`] on write failure and
/// [`SectorError::ExportFailed`] on serialisation failure.
pub fn write_report(
    output_dir: &Utf8Path,
    report: &HooksReport,
    cfg: &HooksConfig,
) -> Result<(), SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let md = render_markdown(report, cfg);
    let md_path = output_dir.join("hooks.md");
    fs::write(&md_path, md).map_err(|e| SectorError::io(md_path.as_str(), e))?;
    let json_path = output_dir.join("hooks.json");
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| SectorError::export(json_path.as_str(), e.to_string()))?;
    fs::write(&json_path, json).map_err(|e| SectorError::io(json_path.as_str(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        FactionClaim, GeneratedStar, GeneratedSystem, GeneratedWorld, GenerationManifest, HexCoord,
        SystemControlSummary, WorldControlSummary, WorldDto,
    };
    use std::collections::BTreeMap as Map;

    fn empty_sector() -> GeneratedSector {
        GeneratedSector {
            id: "test".into(),
            title: "Test".into(),
            seed: "hooks-seed".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0".into(),
            width: 4,
            height: 4,
            systems: vec![],
            routes: vec![],
            factions: vec![],
            manifest: GenerationManifest {
                project_id: "t".into(),
                generated_at_policy: "n".into(),
                generator_name: "sf".into(),
                generator_version: "0".into(),
                seed: "s".into(),
                seed_hash: "h".into(),
                profile: None,
                input_digests: Map::new(),
                settings_digest: "d".into(),
                system_count: 0,
                world_count: 0,
                route_count: 0,
            },
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    fn world(id: &str, name: &str) -> GeneratedWorld {
        GeneratedWorld {
            id: id.into(),
            index: 1,
            name: name.into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "Y".into(),
                star_colour_code: "Y".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Standard".into(),
                population: "Massive".into(),
                tech_level: "Imperial".into(),
                government: "ImperialCommander".into(),
                notable_features: vec![],
            },
            factions: vec![],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: WorldControlSummary::default(),
            stability: Default::default(),
            regions: vec![],
            conflict: Default::default(),
        }
    }

    fn system(id: &str) -> GeneratedSystem {
        GeneratedSystem {
            id: id.into(),
            index: 1,
            name: id.into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "G".into(),
                colour_name: "Yellow".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![],
            primary_factions: vec![],
            tags: vec![],
            notes: vec![],
            control: SystemControlSummary::default(),
            stability: Default::default(),
            orbital_assets: vec![],
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        }
    }

    #[test]
    fn reconquest_hook_fires() {
        let mut sec = empty_sector();
        let mut sys = system("sys-0001");
        let mut w = world("wrld-0001-1", "Hadrumetum III");
        w.claims = vec![
            FactionClaim {
                faction_id: "chaos".into(),
                claim_type: ClaimType::MilitaryOccupation,
                strength: 80,
            },
            FactionClaim {
                faction_id: "imp".into(),
                claim_type: ClaimType::ImperialMandate,
                strength: 70,
            },
        ];
        sys.worlds.push(w);
        sec.systems.push(sys);
        let r = derive(&sec);
        assert!(r.hooks.iter().any(|h| h.kind == HookKind::Reconquest));
    }

    #[test]
    fn deterministic() {
        let sec = empty_sector();
        let a = derive(&sec);
        let b = derive(&sec);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
