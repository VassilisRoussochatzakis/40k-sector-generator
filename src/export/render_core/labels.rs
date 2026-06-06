//! Backend-neutral label predicates shared by the `bitmap` (PNG) and
//! `svg_export` (SVG) renderers.
//!
//! Only the *visibility* decision lives here (C4): it is pure logic over the
//! sector / subsector / theme model with no pixel geometry, so sharing it is
//! byte-safe. The placement geometry stays backend-specific — bitmap quantises
//! to `i32` pixels with real glyph metrics, SVG works in `f32` with heuristic
//! text widths, and unifying the two would change the golden bytes (see the
//! [`super`] module docs on the precision constraint).

use std::collections::HashSet;

use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::{offset_r_neighbors, GeneratedSector, GeneratedSystem};
use crate::subsectors::Subsector;

/// Whether `sys`'s name label should be drawn under the active label density.
///
/// `ImportantOnly` keeps a system labelled when it is populous (≥4 worlds),
/// faction-aligned, or a subsector capital.
pub(crate) fn system_label_visible(
    sys: &GeneratedSystem,
    subsectors: &[Subsector],
    theme: &MapTheme,
    sector: &GeneratedSector,
) -> bool {
    match theme.label_density {
        LabelDensity::All => true,
        LabelDensity::None => false,
        LabelDensity::ImportantOnly => {
            sector.get_worlds_for_system(sys).len() >= 4
                || !sys.primary_factions.is_empty()
                || subsectors.iter().any(|s| {
                    s.summary.subsector_capital_system_id.as_deref() == Some(sys.id.as_str())
                })
        }
    }
}

/// Whether the same-subsector hexes that must back a two-line label block at
/// anchor `(q, r)` are present in `cells` (C-S3).
///
/// A block placed *above* the anchor needs the NW + NE neighbours; *below*
/// needs the SW + SE neighbours. Indices follow [`offset_r_neighbors`]:
/// `0:E 1:SE 2:SW 3:W 4:NW 5:NE`. Pure offset-coordinate logic, identical for
/// both backends — it is the only part of subsector placement that touches no
/// pixel geometry, so it is byte-safe to share. The surrounding centroid /
/// candidate-sort / collision / fallback math stays backend-specific: bitmap
/// works in scaled `i32` pixels with real glyph metrics, SVG in raw `f32` with
/// heuristic text widths, and merging those would move the golden bytes.
pub(crate) fn subsector_label_backed(
    q: i32,
    r: i32,
    above: bool,
    cells: &HashSet<(i32, i32)>,
) -> bool {
    let nbrs = offset_r_neighbors(r);
    if above {
        let nw = (q + nbrs[4].0, r + nbrs[4].1);
        let ne = (q + nbrs[5].0, r + nbrs[5].1);
        cells.contains(&nw) && cells.contains(&ne)
    } else {
        let se = (q + nbrs[1].0, r + nbrs[1].1);
        let sw = (q + nbrs[2].0, r + nbrs[2].1);
        cells.contains(&se) && cells.contains(&sw)
    }
}
