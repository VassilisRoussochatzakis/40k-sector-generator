//! Strongly-typed identifier newtypes + deterministic string constructors.
//!
//! IDs were previously plain `String` values across DTOs, BTreeMap keys, and
//! function arguments. That allowed accidental cross-domain mix-ups (passing a
//! faction id where a system id was expected) and made API signatures
//! self-documenting only via field names. The wrapper types defined below are
//! `#[serde(transparent)]` so the on-disk JSON representation is unchanged, but
//! the Rust API surface enforces the distinction at compile time.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug,
            Clone,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            Default,
        )]
        #[serde(transparent)]
        pub struct $name(pub Arc<str>);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<Arc<str>>) -> Self {
                Self(value.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            #[must_use]
            pub fn into_string(self) -> String {
                self.0.to_string()
            }

            #[must_use]
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(Arc::from(value))
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(Arc::from(value))
            }
        }

        impl From<&String> for $name {
            fn from(value: &String) -> Self {
                Self(Arc::from(value.as_str()))
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> String {
                value.0.to_string()
            }
        }

        impl From<&$name> for String {
            fn from(value: &$name) -> String {
                value.0.to_string()
            }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool {
                &*self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool {
                &*self.0 == *other
            }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool {
                &*self.0 == other
            }
        }

        impl PartialEq<$name> for str {
            fn eq(&self, other: &$name) -> bool {
                self == &*other.0
            }
        }

        impl PartialEq<$name> for &str {
            fn eq(&self, other: &$name) -> bool {
                *self == &*other.0
            }
        }

        impl PartialEq<$name> for String {
            fn eq(&self, other: &$name) -> bool {
                self == &*other.0
            }
        }
    };
}

define_id!(
    /// Identifier for a `GeneratedSystem` ("sys-NNNN").
    SystemId
);
define_id!(
    /// Identifier for a `GeneratedWorld` ("sys-NNNN-wMM").
    WorldId
);
define_id!(
    /// Identifier for a `GeneratedFaction`.
    FactionId
);
define_id!(
    /// Identifier for a `GeneratedRoute` ("route-<a>-<b>").
    RouteId
);

#[must_use]
pub fn system_id(index: usize) -> SystemId {
    SystemId(Arc::from(format!("sys-{index:04}")))
}

#[must_use]
pub fn world_id(system_index: usize, world_index: usize) -> WorldId {
    WorldId(Arc::from(format!("sys-{system_index:04}-w{world_index:02}")))
}

#[must_use]
pub fn route_id(a: &SystemId, b: &SystemId) -> RouteId {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    RouteId(Arc::from(format!("route-{lo}-{hi}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_ids_are_stable() {
        assert_eq!(system_id(1).as_str(), "sys-0001");
        assert_eq!(system_id(42).as_str(), "sys-0042");
        assert_eq!(system_id(9999).as_str(), "sys-9999");
    }

    #[test]
    fn world_ids_are_stable() {
        assert_eq!(world_id(1, 1).as_str(), "sys-0001-w01");
        assert_eq!(world_id(7, 12).as_str(), "sys-0007-w12");
    }

    #[test]
    fn route_id_orders_system_ids() {
        let a = system_id(2);
        let b = system_id(7);
        assert_eq!(route_id(&a, &b).as_str(), "route-sys-0002-sys-0007");
        assert_eq!(route_id(&b, &a).as_str(), "route-sys-0002-sys-0007");
    }

    #[test]
    fn id_serializes_as_bare_string() {
        let s = SystemId::new("sys-0001");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"sys-0001\"");
        let back: SystemId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn id_compares_with_str() {
        let s = SystemId::new("sys-0001");
        assert_eq!(s, "sys-0001");
        assert_eq!("sys-0001", s);
    }
}
