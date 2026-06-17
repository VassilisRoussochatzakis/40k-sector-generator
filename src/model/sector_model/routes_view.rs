//! Route render vocabulary — the visual line patterns and view modes used to
//! draw routes, plus the deterministic pattern-selection logic. Split out of
//! `mod.rs` (review finding A1) so the serialized DTO types stop carrying
//! render concerns. The DTO/identity types (`RouteType`, `RouteKind`,
//! `RouteStability`, `GeneratedRoute`) stay in `mod.rs`; the inherent render
//! methods live here in additional `impl` blocks of the same crate.

use serde::{Deserialize, Serialize};

use super::{GeneratedRoute, RouteKind, RouteStability, RouteType};

impl GeneratedRoute {
    /// Deterministic visual rhythm for this route. `salt` should be a sector
    /// seed/id so same local route ids in different sectors do not all draw
    /// with the same pattern.
    #[must_use]
    pub fn pattern_with_salt(&self, salt: &str, mode: RouteViewMode) -> RoutePattern {
        let key = format!(
            "{}\0{}\0{}\0{}\0{}\0{}",
            salt,
            self.id,
            self.from_system_id,
            self.to_system_id,
            self.distance,
            self.stability.pattern_key()
        );
        self.route_type.pattern_for_key(&key, mode)
    }
}

impl RouteKind {
    #[must_use]
    pub fn patterns(self) -> &'static [RoutePattern] {
        match self {
            RouteKind::Warp => &[RoutePattern::Solid],
            RouteKind::Webway => &[RoutePattern::Burst],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RouteViewMode {
    #[default]
    Detailed,
    TopLevel,
}

enum_slug!(RouteViewMode {
    Detailed => "detailed",
    TopLevel => "top_level",
});

impl core::fmt::Display for RouteViewMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_slug())
    }
}

impl RouteType {
    /// Canonical legend/default pattern for this route type.
    #[must_use]
    pub fn pattern(self, mode: RouteViewMode) -> RoutePattern {
        match mode {
            RouteViewMode::Detailed => self.patterns()[0],
            RouteViewMode::TopLevel => self.kind().patterns()[0],
        }
    }

    /// Full pattern family for this route type. Families are disjoint and cover
    /// every `RoutePattern`, so generated routes spread across the new styles
    /// without making two route types share the same default glyph.
    #[must_use]
    pub fn patterns(self) -> &'static [RoutePattern] {
        match self {
            RouteType::StableWarpLane => &[
                RoutePattern::Solid,
                RoutePattern::Railroad,
                RoutePattern::March,
            ],
            RouteType::ChartedPassage => &[
                RoutePattern::Dashed,
                RoutePattern::Bridge,
                RoutePattern::Twin,
                RoutePattern::DotDash,
                RoutePattern::Cracked,
                RoutePattern::Staccato,
            ],
            RouteType::SecretPassage => &[
                RoutePattern::Dotted,
                RoutePattern::Tick,
                RoutePattern::Whisper,
            ],
            RouteType::Webway => &[
                RoutePattern::Burst,
                RoutePattern::Tripod,
                RoutePattern::Patter,
            ],
            RouteType::BlackShip => &[RoutePattern::Quartet, RoutePattern::DoubleTap],
            RouteType::SmugglingLane => &[
                RoutePattern::Gravel,
                RoutePattern::Pebble,
                RoutePattern::Ghost,
            ],
        }
    }

    #[must_use]
    pub fn pattern_for_key(self, key: &str, mode: RouteViewMode) -> RoutePattern {
        match mode {
            RouteViewMode::Detailed => {
                let pool = self.patterns();
                pool[(stable_pattern_hash(self, key) as usize) % pool.len()]
            }
            RouteViewMode::TopLevel => self.kind().patterns()[0],
        }
    }
}

fn stable_pattern_hash(route_type: RouteType, key: &str) -> u32 {
    fn feed(hash: &mut u32, bytes: &[u8]) {
        for b in bytes {
            *hash ^= u32::from(*b);
            *hash = hash.wrapping_mul(16_777_619);
        }
    }

    let mut hash = 2_166_136_261_u32;
    feed(&mut hash, b"sectorforge:route-pattern:v1");
    feed(&mut hash, &[0]);
    feed(&mut hash, route_type.as_slug().as_bytes());
    feed(&mut hash, &[0]);
    feed(&mut hash, key.as_bytes());
    hash
}

/// Visual line pattern used to encode route type plus per-route variety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoutePattern {
    Solid,
    Dashed,
    DotDash,
    Dotted,
    /// Jagged broken line, reads as unstable or damaged.
    Cracked,
    /// Sparse low-contrast long dashes, barely-there / ghostly.
    Ghost,
    /// Repeating cross-burst marks.
    Burst,
    /// Sawtooth / zigzag route.
    Staccato,
    /// Dense irregular dot trail.
    Gravel,
    /// Parallel twin lane.
    Twin,
    /// Repeating triangular markers.
    Tripod,
    /// Sparse perpendicular tick marks over a faint spine.
    Tick,
    /// Ladder / bridge marks over a faint spine.
    Bridge,
    /// Hollow pip trail.
    Patter,
    /// Four-dot clusters.
    Quartet,
    /// Parallel rails with cross-ties.
    Railroad,
    /// Paired perpendicular ticks.
    DoubleTap,
    /// Alternating pebble dots.
    Pebble,
    /// Very sparse small dots.
    Whisper,
    /// Repeating chevrons.
    March,
}

impl RoutePattern {
    /// Fallback alternating on/off run-lengths in multiples of the stroke unit.
    /// Geometric renderers use this directly for the dash/dot family and use
    /// custom motifs for rails, ticks, chevrons, bursts, and marker trails.
    pub fn strides(self) -> &'static [f32] {
        match self {
            RoutePattern::Solid => &[],
            // Long bars: easy to read at a glance, period ~3x the dotted period.
            RoutePattern::Dashed => &[10.0, 5.0],
            // Dash + two dots: compound shape so it can't be confused with
            // a plain dash or a plain dot trail.
            RoutePattern::DotDash => &[5.0, 2.0, 1.0, 2.0, 1.0, 4.0],
            // Tight fine stippling.
            RoutePattern::Dotted => &[1.0, 2.0],
            // --- 16 new strides ---
            RoutePattern::Cracked => &[3.0, 2.0],
            RoutePattern::Ghost => &[12.0, 15.0],
            RoutePattern::Burst => &[1.5, 2.0, 1.5, 2.0, 1.5, 8.0],
            RoutePattern::Staccato => &[6.0, 3.0, 2.0, 3.0],
            RoutePattern::Gravel => &[2.0, 1.5],
            RoutePattern::Twin => &[4.0, 2.0, 4.0, 5.0],
            RoutePattern::Tripod => &[6.0, 1.0, 1.0, 1.0, 1.0, 1.0, 6.0],
            RoutePattern::Tick => &[2.0, 8.0],
            RoutePattern::Bridge => &[4.0, 2.0, 4.0, 2.0],
            RoutePattern::Patter => &[0.8, 1.2],
            RoutePattern::Quartet => &[5.0, 3.0, 3.0, 7.0],
            RoutePattern::Railroad => &[14.0, 6.0],
            RoutePattern::DoubleTap => &[2.5, 2.0, 2.5, 6.0],
            RoutePattern::Pebble => &[1.0, 1.0],
            RoutePattern::Whisper => &[1.0, 14.0],
            RoutePattern::March => &[3.0, 3.0, 3.0, 3.0, 3.0, 3.0],
        }
    }
}

impl RouteStability {
    fn pattern_key(self) -> &'static str {
        match self {
            RouteStability::Stable => "stable",
            RouteStability::Unstable => "unstable",
            RouteStability::Hazardous => "hazardous",
            RouteStability::Perilous => "perilous",
        }
    }
}
