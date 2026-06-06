//! Backend-neutral label predicates shared by the `bitmap` (PNG) and
//! `svg_export` (SVG) renderers.
//!
//! Only the *visibility* decision lives here (C4): it is pure logic over the
//! sector / subsector / theme model with no pixel geometry, so sharing it is
//! byte-safe. The placement geometry stays backend-specific — bitmap quantises
//! to `i32` pixels with real glyph metrics, SVG works in `f32` with heuristic
//! text widths, and unifying the two would change the golden bytes (see the
//! [`super`] module docs on the precision constraint).

use crate::map_theme::{LabelDensity, MapTheme};
use crate::sector_model::{GeneratedSector, GeneratedSystem};
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
