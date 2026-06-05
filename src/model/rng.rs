//! Deterministic, stage-based RNG helpers.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::errors::SectorError;

/// Derive a 32-byte stage seed from `(root_seed, stage, discriminator)`.
pub fn derive_stage_seed(root_seed: &str, stage: &str, discriminator: &str) -> [u8; 32] {
    let input = format!("sectorforge:{root_seed}:{stage}:{discriminator}");
    *blake3::hash(input.as_bytes()).as_bytes()
}

/// Build a `ChaCha8` RNG from a stage seed.
pub fn stage_rng(root_seed: &str, stage: &str, discriminator: &str) -> ChaCha8Rng {
    ChaCha8Rng::from_seed(derive_stage_seed(root_seed, stage, discriminator))
}

/// Hash the root seed once. Useful to expose in the manifest.
pub fn hash_root_seed(root_seed: &str) -> [u8; 32] {
    *blake3::hash(root_seed.as_bytes()).as_bytes()
}

/// Hash arbitrary bytes (canonical JSON, file contents, etc.) and return the
/// lowercase hex digest. Used by the GUI builder's derivation cache (LD1) to
/// key cached overlays by input slice.
pub fn digest_bytes(bytes: &[u8]) -> String {
    hex(blake3::hash(bytes).as_bytes())
}

/// Weighted choice over `(item, weight)` slice. Returns an index or an error.
/// Skips entries with non-finite or non-positive weights.
pub fn weighted_index<T>(
    pool: &[(T, f64)],
    rng: &mut impl rand::Rng,
    context: &str,
) -> Result<usize, SectorError> {
    let total: f64 = pool
        .iter()
        .map(|(_, w)| if w.is_finite() && *w > 0.0 { *w } else { 0.0 })
        .sum();

    if total <= 0.0 || !total.is_finite() {
        return Err(SectorError::WeightedSelectionFailed {
            context: context.to_string(),
        });
    }

    let mut roll = rng.gen::<f64>() * total;
    for (i, (_, w)) in pool.iter().enumerate() {
        if !w.is_finite() || *w <= 0.0 {
            continue;
        }
        roll -= *w;
        if roll <= 0.0 {
            return Ok(i);
        }
    }
    // Floating-point edge: fall back to last non-zero entry.
    for (i, (_, w)) in pool.iter().enumerate().rev() {
        if w.is_finite() && *w > 0.0 {
            return Ok(i);
        }
    }
    Err(SectorError::WeightedSelectionFailed {
        context: context.to_string(),
    })
}

/// Format a 32-byte hash as a lowercase hex string.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `write!` into a `String` is infallible; byte-identical lowercase hex
        // to the old `push_str(&format!(...))` — feeds golden seed_hash/digest.
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_seed_is_stable() {
        let a = derive_stage_seed("seed-1", "placement", "sector");
        let b = derive_stage_seed("seed-1", "placement", "sector");
        assert_eq!(a, b);
        let c = derive_stage_seed("seed-2", "placement", "sector");
        assert_ne!(a, c);
        let d = derive_stage_seed("seed-1", "system", "sector");
        assert_ne!(a, d);
    }

    #[test]
    fn weighted_index_picks_only_item() {
        let mut rng = stage_rng("s", "t", "d");
        let pool = vec![("only", 1.0)];
        assert_eq!(weighted_index(&pool, &mut rng, "test").unwrap(), 0);
    }

    #[test]
    fn weighted_index_skips_invalid() {
        let mut rng = stage_rng("s", "t", "d");
        let pool = vec![("bad", 0.0), ("good", 1.0), ("nan", f64::NAN)];
        for _ in 0..50 {
            assert_eq!(weighted_index(&pool, &mut rng, "test").unwrap(), 1);
        }
    }
}
