//! BLAKE3-keyed derivation cache (LD1). Each overlay (analytics, relations,
//! history, ...) is cached by digest over the slice of sector state it
//! consumes. Mutations invalidate entries via [`Self::invalidate`].

use std::collections::BTreeMap;

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
