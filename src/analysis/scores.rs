//! TF-NT-2 score newtypes. Thin wrappers over `f32` so callers comparing scores
//! across `control.rs`, `importance.rs`, and `power_projection.rs` cannot
//! accidentally mix incompatible numbers. `#[serde(transparent)]` keeps the
//! JSON representation byte-identical to the bare `f32` they replace, so
//! field-by-field adoption is non-breaking.

use serde::{Deserialize, Serialize};

macro_rules! score_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub f32);

        impl $name {
            #[inline]
            #[must_use]
            pub fn get(self) -> f32 {
                self.0
            }
        }

        impl From<f32> for $name {
            #[inline]
            fn from(v: f32) -> Self {
                Self(v)
            }
        }

        impl From<$name> for f32 {
            #[inline]
            fn from(v: $name) -> f32 {
                v.0
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

score_newtype!(
    ControlScore,
    "Per-world / per-system control score (0..=100). Output of `analysis::control`."
);

score_newtype!(
    DisplayImportance,
    "Importance rank used for UI prioritisation. Output of `analysis::importance`."
);

score_newtype!(
    ProjectedPower,
    "Faction projection budget aggregate. Output of `analysis::power_projection`."
);
