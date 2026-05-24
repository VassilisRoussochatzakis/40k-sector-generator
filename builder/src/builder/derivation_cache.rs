//! BLAKE3-keyed derivation cache (LD1, R5). Each overlay (analytics,
//! relations, history, ...) is cached by digest over the slice of sector
//! state it consumes. Mutations invalidate entries via [`Self::invalidate`].
//!
//! Per R5 derivations must be pure functions of their input slice. Callers
//! compute the digest with [`digest_input`] over the canonical JSON of that
//! slice and look up cached results with [`Self::get`]. On a miss, compute
//! the derivation and store it with [`Self::put`].

use std::collections::BTreeMap;

use serde::Serialize;

use sectorforge::rng::digest_bytes;

#[derive(Debug, Clone, Default)]
pub struct DerivationCache {
    pub entries: BTreeMap<String, CacheEntry>,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Hex-encoded BLAKE3 digest of the input slice that produced the value.
    pub digest: String,
    /// Serialised value blob (JSON). Concrete types decode on demand.
    pub value: String,
}

impl DerivationCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn invalidate(&mut self, key: &str) {
        self.entries.remove(key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn get(&self, key: &str, digest: &str) -> Option<&str> {
        let e = self.entries.get(key)?;
        if e.digest == digest {
            Some(&e.value)
        } else {
            None
        }
    }

    pub fn put(&mut self, key: String, digest: String, value: String) {
        self.entries.insert(key, CacheEntry { digest, value });
    }
}

/// Compute the BLAKE3 hex digest of a serializable input slice. Used as the
/// cache key for an overlay. Determinism relies on the slice serializing in a
/// stable order — prefer types backed by `BTreeMap`/`Vec`, not `HashMap`.
pub fn digest_input<T: Serialize>(input: &T) -> String {
    let bytes = serde_json::to_vec(input).unwrap_or_default();
    digest_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_round_trip() {
        let mut c = DerivationCache::new();
        let key = "analytics".to_string();
        let digest = digest_input(&vec![1u32, 2, 3]);
        c.put(key.clone(), digest.clone(), "{\"score\":42}".into());
        assert_eq!(c.get(&key, &digest), Some("{\"score\":42}"));
        let stale = digest_input(&vec![1u32, 2, 4]);
        assert_eq!(c.get(&key, &stale), None);
    }

    #[test]
    fn digest_input_is_stable() {
        let a = digest_input(&("alpha", 1u32));
        let b = digest_input(&("alpha", 1u32));
        assert_eq!(a, b);
        let c = digest_input(&("alpha", 2u32));
        assert_ne!(a, c);
    }
}
