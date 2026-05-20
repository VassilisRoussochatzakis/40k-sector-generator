//! §11 NEW.md: self-contained interactive HTML export.
//!
//! Emits a single `sector.html` file with:
//!   * inlined sector JSON,
//!   * inlined CSS keyed to a [`HtmlTheme`],
//!   * an inlined vanilla-JS hex renderer (pan/zoom, click-to-inspect,
//!     faction filter, heatmap mode toggle).
//!
//! Determinism: output bytes depend only on the inlined `GeneratedSector`
//! plus the static template + theme. No timestamps, no external assets, no
//! runtime fetches — same input ⇒ same bytes.
//!
//! Player edition: when `HtmlConfig::player_observer` is set, the sector is
//! redacted with the same rules as `intel::redact_world_for_observer` so
//! Hidden-tier presences below the configured confidence cutoff are stripped
//! before serialisation.
//!
//! The renderer is intentionally compact and mirrors hex geometry from
//! `bitmap/mod.rs` (pointy-top, odd-r offset).

use std::fs;

use camino::Utf8Path;

use crate::config::{HtmlConfig, HtmlTheme};
use crate::errors::SectorError;
use crate::faction_style::faction_style_rgb_by_id;
use crate::sector_model::{FactionInfluence, GeneratedSector};

const RENDERER_JS: &str = include_str!("html_export/renderer.js");
const STYLE_CSS: &str = include_str!("html_export/style.css");

/// Write `sector.html` into `output_dir`. Returns the path that was written.
///
/// # Errors
///
/// [`SectorError::Io`] if the file cannot be written; [`SectorError::ExportFailed`]
/// if the sector cannot be serialised.
pub fn write_html(
    sector: &GeneratedSector,
    output_dir: &Utf8Path,
    cfg: &HtmlConfig,
) -> Result<camino::Utf8PathBuf, SectorError> {
    fs::create_dir_all(output_dir).map_err(|e| SectorError::io(output_dir.as_str(), e))?;
    let path = output_dir.join("sector.html");
    write_html_to(sector, &path, cfg)?;
    Ok(path)
}

/// Variant of [`write_html`] that writes to an explicit path.
///
/// # Errors
///
/// Same as [`write_html`].
pub fn write_html_to(
    sector: &GeneratedSector,
    path: &Utf8Path,
    cfg: &HtmlConfig,
) -> Result<(), SectorError> {
    let text =
        render_html(sector, cfg).map_err(|e| SectorError::export(path.as_str(), e.to_string()))?;
    if (text.len() as u64) > cfg.size_warn_bytes {
        eprintln!(
            "sectorforge: interactive HTML is {} bytes (warn threshold {})",
            text.len(),
            cfg.size_warn_bytes
        );
    }
    fs::write(path, text).map_err(|e| SectorError::io(path.as_str(), e))?;
    Ok(())
}

/// Pure transform: build the HTML string. Exposed for tests and for callers
/// that want to embed the rendered document elsewhere.
///
/// # Errors
///
/// Returns the underlying `serde_json` error if the sector cannot be
/// serialised. Should not happen for any sector produced by this crate.
pub fn render_html(
    sector: &GeneratedSector,
    cfg: &HtmlConfig,
) -> Result<String, serde_json::Error> {
    let view = match cfg.player_observer.as_deref() {
        Some(observer) => redact_for_observer(sector, observer, cfg.player_min_confidence),
        None => sector.clone(),
    };
    let sector_json = if cfg.compact_json {
        serde_json::to_string(&view)?
    } else {
        serde_json::to_string_pretty(&view)?
    };
    let faction_palette = build_faction_palette_json(&view)?;
    let theme = theme_payload(cfg.theme);
    let edition = cfg
        .player_observer
        .as_deref()
        .map(|o| format!("PLAYER EDITION ({})", o.to_uppercase()))
        .unwrap_or_else(|| "GM EDITION".to_string());
    let title_html = html_escape(&view.title);
    let id_html = html_escape(&view.id);
    Ok(format!(
        "<!doctype html>\n\
<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
<meta name=\"generator\" content=\"sectorforge\">\n\
<meta name=\"sector-id\" content=\"{id}\">\n\
<title>sectorforge: {title}</title>\n\
<style>\n{base_css}\n{theme_css}\n</style>\n\
</head><body>\n\
<div id=\"app\">\n\
  <header id=\"hdr\"><span class=\"title\">{title}</span> <span class=\"edition\">{edition}</span></header>\n\
  <aside id=\"side\"><div id=\"panel\">Click a system.</div></aside>\n\
  <main><canvas id=\"map\"></canvas></main>\n\
  <footer id=\"ctl\">\n\
    <label>Heatmap <select id=\"heat\"></select></label>\n\
    <label><input type=\"checkbox\" id=\"factionFill\" checked>Faction fill</label>\n\
    <label><input type=\"checkbox\" id=\"showRoutes\" checked>Routes</label>\n\
    <label><input type=\"checkbox\" id=\"showLabels\" checked>Labels</label>\n\
    <span id=\"filter\"></span>\n\
  </footer>\n\
</div>\n\
<script>\n\
const SECTOR = {sector_json};\n\
const FACTION_PALETTE = {faction_palette};\n\
const THEME = {theme};\n\
{renderer}\n\
</script>\n\
</body></html>\n",
        id = id_html,
        title = title_html,
        edition = html_escape(&edition),
        base_css = STYLE_CSS,
        theme_css = theme_css(cfg.theme),
        sector_json = sector_json,
        faction_palette = faction_palette,
        theme = theme,
        renderer = RENDERER_JS,
    ))
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn build_faction_palette_json(sector: &GeneratedSector) -> Result<String, serde_json::Error> {
    // BTreeMap to keep key order stable across builds.
    let mut entries: std::collections::BTreeMap<&str, (String, &str)> = Default::default();
    for f in &sector.factions {
        let style = faction_style_rgb_by_id(&sector.factions, &f.id);
        let hex = format!(
            "#{:02x}{:02x}{:02x}",
            style.fill.0, style.fill.1, style.fill.2
        );
        entries.insert(f.id.as_str(), (hex, f.name.as_str()));
    }
    let mut out = String::from("{");
    let mut first = true;
    for (id, (hex, name)) in entries {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&serde_json::to_string(id)?);
        out.push(':');
        out.push_str(&format!(
            "{{\"fill\":{},\"name\":{}}}",
            serde_json::to_string(&hex)?,
            serde_json::to_string(name)?
        ));
    }
    out.push('}');
    Ok(out)
}

fn theme_css(theme: HtmlTheme) -> &'static str {
    match theme {
        HtmlTheme::Dark => include_str!("html_export/theme_dark.css"),
        HtmlTheme::Parchment => include_str!("html_export/theme_parchment.css"),
        HtmlTheme::Hololithic => include_str!("html_export/theme_hololithic.css"),
    }
}

fn theme_payload(theme: HtmlTheme) -> &'static str {
    match theme {
        HtmlTheme::Dark => {
            r##"{"name":"dark","bg":"#0e0c14","panel":"#161220","hex":"#1c1a26","outline":"#3c374e","text":"#e8e4f0","dim":"#969aa5","accent":"#ffdc64"}"##
        }
        HtmlTheme::Parchment => {
            r##"{"name":"parchment","bg":"#f0e6cf","panel":"#e6d8b3","hex":"#e4d4a8","outline":"#9a8454","text":"#2c2316","dim":"#6e5a30","accent":"#7c3a16"}"##
        }
        HtmlTheme::Hololithic => {
            r##"{"name":"hololithic","bg":"#0a1422","panel":"#10243a","hex":"#13334d","outline":"#3a8fcc","text":"#cfe6ff","dim":"#7090b0","accent":"#7fe0ff"}"##
        }
    }
}

// ── Redaction ───────────────────────────────────────────────────────────────

fn redact_for_observer(sector: &GeneratedSector, observer: &str, min_conf: u8) -> GeneratedSector {
    let mut out = sector.clone();
    for sys in &mut out.systems {
        // Drop other observers' intel records — keep only the requested one.
        sys.intel.by_observer.retain(|k, _| k == observer);
        // Stripping per-world Hidden / low-confidence presences.
        for w in &mut sys.worlds {
            let observer_vis = w
                .factions
                .iter()
                .find(|p| p.faction_id == observer)
                .map(|p| p.dimensions.visibility)
                .unwrap_or(20.0);
            w.factions.retain(|p| {
                if p.faction_id == observer {
                    return true;
                }
                if matches!(p.influence, FactionInfluence::Hidden) {
                    let conf = (p.dimensions.visibility * observer_vis) / 100.0;
                    return (conf as u8) >= min_conf;
                }
                let conf = (p.dimensions.visibility * observer_vis) / 100.0;
                (conf as u8) >= min_conf
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector_model::{
        DominanceState, FactionInfluence, GeneratedFaction, GeneratedStar, GeneratedSystem,
        GeneratedWorld, GenerationManifest, HexCoord, PresenceDimensions, WorldDto,
        WorldFactionPresence,
    };
    use std::collections::BTreeMap;

    fn manifest() -> GenerationManifest {
        GenerationManifest {
            project_id: "p".into(),
            generated_at_policy: "x".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            seed: "abc".into(),
            seed_hash: "h".into(),
            profile: None,
            input_digests: BTreeMap::new(),
            settings_digest: "s".into(),
            system_count: 1,
            world_count: 1,
            route_count: 0,
        }
    }

    fn sample(observer_vis: f32, hidden_vis: f32) -> GeneratedSector {
        let world = GeneratedWorld {
            id: "sys-0001-w1".into(),
            index: 1,
            name: "W".into(),
            orbit: 1,
            source_row_index: 0,
            world: WorldDto {
                star_colour: "amber".into(),
                star_colour_code: "A".into(),
                world_type: "HiveWorld".into(),
                atmosphere: "Breathable".into(),
                temperature: "Temperate".into(),
                biosphere: "Thriving".into(),
                population: "DenselyPopulated".into(),
                tech_level: "High".into(),
                government: "MagistrateCouncil".into(),
                notable_features: vec![],
            },
            factions: vec![
                WorldFactionPresence {
                    faction_id: "imp".into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: FactionInfluence::Dominant,
                    relationship_to_government: "lawful".into(),
                    dimensions: PresenceDimensions {
                        visibility: observer_vis,
                        ..Default::default()
                    },
                    dominance: DominanceState::default(),
                    intel_confidence: 100,
                },
                WorldFactionPresence {
                    faction_id: "cult".into(),
                    subfaction_id: None,
                    subfaction_name: None,
                    force_id: None,
                    force_name: None,
                    influence: FactionInfluence::Hidden,
                    relationship_to_government: "outlaw".into(),
                    dimensions: PresenceDimensions {
                        visibility: hidden_vis,
                        covert: 90.0,
                        ..Default::default()
                    },
                    dominance: DominanceState::default(),
                    intel_confidence: 5,
                },
            ],
            tags: vec![],
            notes: vec![],
            claims: vec![],
            control: Default::default(),
            stability: Default::default(),
            regions: Vec::new(),
            conflict: Default::default(),
        };
        let sys = GeneratedSystem {
            id: "sys-0001".into(),
            index: 1,
            name: "Test".into(),
            coord: HexCoord { q: 0, r: 0 },
            star: GeneratedStar {
                colour_code: "A".into(),
                colour_name: "amber".into(),
                spectral_type: None,
                source_row_index: None,
            },
            worlds: vec![world],
            primary_factions: vec!["imp".into()],
            tags: vec![],
            notes: vec![],
            control: Default::default(),
            stability: Default::default(),
            orbital_assets: Vec::new(),
            blockade: Default::default(),
            conflict: Default::default(),
            intel: Default::default(),
            archetype: Default::default(),
        };
        GeneratedSector {
            id: "demo".into(),
            title: "Demo".into(),
            seed: "abc".into(),
            generator_name: "sectorforge".into(),
            generator_version: "0.1.0".into(),
            width: 4,
            height: 4,
            systems: vec![sys],
            routes: vec![],
            factions: vec![
                GeneratedFaction {
                    id: "imp".into(),
                    name: "Imperium".into(),
                    kind: "imperium".into(),
                    disposition: "lawful".into(),
                    subfactions: Vec::new(),
                    system_presence: vec!["sys-0001".into()],
                    world_presence: vec!["sys-0001-w1".into()],
                    power: Default::default(),
                },
                GeneratedFaction {
                    id: "cult".into(),
                    name: "Cult".into(),
                    kind: "chaos".into(),
                    disposition: "outlaw".into(),
                    subfactions: Vec::new(),
                    system_presence: vec![],
                    world_presence: vec!["sys-0001-w1".into()],
                    power: Default::default(),
                },
            ],
            manifest: manifest(),
            influence_field: Default::default(),
            power_projection: Default::default(),
            relations: Default::default(),
            regions: Vec::new(),
            economy: Default::default(),
        }
    }

    #[test]
    fn html_is_byte_stable() {
        let s = sample(90.0, 10.0);
        let cfg = HtmlConfig::default();
        let a = render_html(&s, &cfg).unwrap();
        let b = render_html(&s, &cfg).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn player_edition_drops_hidden_low_confidence_factions() {
        let s = sample(90.0, 5.0); // 90 * 5 / 100 = 4.5 -> u8 4, < 30 = drop
        let cfg = HtmlConfig {
            player_observer: Some("imp".into()),
            player_min_confidence: 30,
            ..Default::default()
        };
        let html = render_html(&s, &cfg).unwrap();
        assert!(
            !html.contains("\"faction_id\":\"cult\""),
            "cult presence should be redacted"
        );
        assert!(html.contains("\"faction_id\":\"imp\""));
    }

    #[test]
    fn gm_edition_keeps_hidden_factions() {
        let s = sample(90.0, 5.0);
        let cfg = HtmlConfig::default();
        let html = render_html(&s, &cfg).unwrap();
        assert!(html.contains("\"faction_id\":\"cult\""));
    }

    #[test]
    fn theme_changes_css() {
        let s = sample(90.0, 10.0);
        let dark = render_html(
            &s,
            &HtmlConfig {
                theme: HtmlTheme::Dark,
                ..Default::default()
            },
        )
        .unwrap();
        let parchment = render_html(
            &s,
            &HtmlConfig {
                theme: HtmlTheme::Parchment,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(dark, parchment);
        assert!(parchment.contains("parchment"));
    }

    #[test]
    fn output_has_required_anchors() {
        let s = sample(90.0, 10.0);
        let html = render_html(&s, &HtmlConfig::default()).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        for marker in [
            "id=\"map\"",
            "id=\"panel\"",
            "id=\"heat\"",
            "const SECTOR =",
            "const FACTION_PALETTE =",
            "const THEME =",
        ] {
            assert!(html.contains(marker), "missing marker {marker}");
        }
    }

    #[test]
    fn html_escape_handles_specials() {
        assert_eq!(html_escape("a<b&c>'\""), "a&lt;b&amp;c&gt;&#39;&quot;");
    }

    #[test]
    fn faction_palette_json_escapes_specials() {
        let mut s = sample(90.0, 10.0);
        s.factions[0].id = "imp\"x".into();
        s.factions[0].name = "Imperium <&> \"Prime\"".into();

        let palette: serde_json::Value =
            serde_json::from_str(&build_faction_palette_json(&s).unwrap()).unwrap();

        assert_eq!(
            palette["imp\"x"]["name"].as_str(),
            Some("Imperium <&> \"Prime\"")
        );
        assert!(palette["imp\"x"]["fill"]
            .as_str()
            .is_some_and(|hex| hex.starts_with('#') && hex.len() == 7));
    }
}
