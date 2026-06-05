//! Crate-internal declarative macros.

/// Generate `pub fn as_slug(&self) -> &'static str` for a fieldless enum from
/// explicit variant→slug pairs (IMPROVEMENT_REVIEW B-S3). The slugs are written
/// out verbatim — *not* derived from the variant name — so the serialized JSON /
/// markdown output stays byte-identical to the hand-written `match` blocks this
/// replaces. The generated `match` is exhaustive, so adding an enum variant
/// without a slug here is a compile error.
///
/// ```ignore
/// enum_slug!(Disposition {
///     Presence => "presence",
///     Influence => "influence",
/// });
/// ```
macro_rules! enum_slug {
    ($ty:ty { $($variant:ident => $slug:literal),+ $(,)? }) => {
        impl $ty {
            pub fn as_slug(&self) -> &'static str {
                match self {
                    $( Self::$variant => $slug, )+
                }
            }
        }
    };
    // `const` form for the few enums whose `as_slug` is a `const fn`.
    (const $ty:ty { $($variant:ident => $slug:literal),+ $(,)? }) => {
        impl $ty {
            pub const fn as_slug(&self) -> &'static str {
                match self {
                    $( Self::$variant => $slug, )+
                }
            }
        }
    };
}
