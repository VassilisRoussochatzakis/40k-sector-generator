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
    // Needed for `.gen::<u64>()` / `.gen::<f64>()` draws in the stage-stream tests.
    use rand::Rng;

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

    // GAP 1 (H) — discriminator + delimiter injectivity of `derive_stage_seed`.
    // Format string is `sectorforge:{root_seed}:{stage}:{discriminator}`; the `:`
    // delimiters must prevent field-boundary collisions.
    #[test]
    fn derive_stage_seed_discriminator_and_delimiter_are_injective() {
        // Discriminator distinguishes.
        assert_ne!(
            derive_stage_seed("r", "s", "sys-1"),
            derive_stage_seed("r", "s", "sys-2"),
        );
        // Empty vs non-empty discriminator distinguishes.
        assert_ne!(
            derive_stage_seed("r", "s", "sys-1"),
            derive_stage_seed("r", "s", ""),
        );
        // Delimiter is not collapsible: shifting characters across the field
        // boundaries must yield distinct seeds (the `:` separators prevent the
        // "abc" collision).
        let abc = derive_stage_seed("a", "b", "c");
        let a_bc = derive_stage_seed("a", "bc", "");
        let ab_c = derive_stage_seed("ab", "c", "");
        assert_ne!(abc, a_bc);
        assert_ne!(abc, ab_c);
        assert_ne!(a_bc, ab_c);
    }

    // GAP 2 (H) — `weighted_index` error paths surface the verbatim caller context.
    // Every malformed pool routes through the `total <= 0.0` guard (non-finite
    // weights are zeroed before the sum, so the sum can only be `<= 0.0`).
    #[test]
    fn weighted_index_error_paths_carry_verbatim_context() {
        fn assert_err_context<T>(pool: &[(T, f64)], ctx: &str) {
            let mut rng = stage_rng("s", "t", "d");
            let err = weighted_index(pool, &mut rng, ctx)
                .expect_err("malformed pool must error");
            match err {
                SectorError::WeightedSelectionFailed { context } => {
                    assert_eq!(context, ctx);
                }
                other => panic!("expected WeightedSelectionFailed, got {other:?}"),
            }
        }

        // Empty pool: total is 0.0.
        let empty: Vec<(&str, f64)> = vec![];
        assert_err_context(&empty, "empty-ctx");

        // All-zero + negative: every weight coerces to 0.0.
        assert_err_context(&[("z", 0.0), ("n", -1.0)], "zero-neg-ctx");

        // Single +inf: coerced to 0.0 in the sum, so total == 0.0 (not via a
        // non-finite total).
        assert_err_context(&[("i", f64::INFINITY)], "inf-ctx");

        // Single NaN: coerced to 0.0 in the sum, total == 0.0.
        assert_err_context(&[("x", f64::NAN)], "nan-ctx");
    }

    // GAP 3 (M) — `weighted_index` skips neg/inf entries and respects weight
    // proportionality across many draws on a single advancing RNG.
    #[test]
    fn weighted_index_skips_and_is_proportional() {
        // Part A — skip: only "good" (index 1) is finite-positive; "neg" and
        // "inf" are skipped on every draw.
        let pool = vec![("neg", -5.0), ("good", 2.0), ("inf", f64::INFINITY)];
        let mut rng = stage_rng("g3", "skip", "");
        for _ in 0..200 {
            assert_eq!(weighted_index(&pool, &mut rng, "p").unwrap(), 1);
        }

        // Part B — proportionality: pool is 1:9, so index 1 ("b") should win
        // ~90% of 5000 draws. The fixed seed `stage_rng("prop","test","")`,
        // reused across all 5000 sequential draws, deterministically yields 4518.
        // Reuse ONE rng for the whole loop — re-creating it inside would replay
        // identical draws and collapse the count.
        let pool = vec![("a", 1.0_f64), ("b", 9.0_f64)];
        let mut rng = stage_rng("prop", "test", "");
        let mut count_b = 0usize;
        for _ in 0..5000 {
            if weighted_index(&pool, &mut rng, "prop").unwrap() == 1 {
                count_b += 1;
            }
        }
        // Spec window (mean ~4500); the deterministic seed pins exactly 4518.
        assert!((4300..4700).contains(&count_b), "count_b={count_b}");
        assert_eq!(count_b, 4518, "proportionality canary drifted");
    }

    // GAP 4 (M) — `stage_rng` produces identical streams for identical keys and
    // decorrelated streams when stage or discriminator differ.
    #[test]
    fn stage_rng_stream_identity_and_decorrelation() {
        fn draw16(rs: &str, st: &str, di: &str) -> Vec<u64> {
            let mut r = stage_rng(rs, st, di);
            (0..16).map(|_| r.gen::<u64>()).collect()
        }

        // Identity: same key -> same stream.
        assert_eq!(
            draw16("k", "stage", "disc"),
            draw16("k", "stage", "disc"),
        );
        // Distinct discriminator decorrelates.
        assert_ne!(
            draw16("k", "stage", "disc"),
            draw16("k", "stage", "disc2"),
        );
        // Distinct stage decorrelates.
        assert_ne!(
            draw16("k", "stage", "disc"),
            draw16("k", "stage2", "disc"),
        );
    }

    // GAP 5 (M) — ChaCha8 cross-version golden PIN. The first u64 drawn from
    // `stage_rng("sectorforge-test","pin","")` is pinned; a change here means
    // rand_chacha (or blake3 seeding) output drifted.
    // PIN: changing this means rand_chacha output drifted — re-derive the
    // constant and re-run golden tests.
    #[test]
    fn stage_rng_chacha8_output_is_pinned() {
        let mut r = stage_rng("sectorforge-test", "pin", "");
        assert_eq!(r.gen::<u64>(), 1_922_047_071_708_196_106_u64);
    }

    // GAP 6 (M) — `hex` is lowercase, zero-padded, two chars per byte.
    #[test]
    fn hex_is_lowercase_zero_padded() {
        assert_eq!(hex(b""), "");
        assert_eq!(hex(&[0x0f]), "0f");
        assert_eq!(hex(&[0xde, 0xad]), "dead");
        // Length is exactly 2 x input.
        assert_eq!(hex(&[1u8, 2, 3, 4]).len(), 8);
        // Output is lowercase even though the inputs read like uppercase nibbles.
        let h = hex(&[0xAB, 0xCD]);
        assert_eq!(h, "abcd");
    }

    // GAP 8 (L) — `digest_bytes` is deterministic, input-sensitive, 64-char
    // lowercase hex, and pinned to blake3("abc").
    #[test]
    fn digest_bytes_is_stable_and_pinned() {
        assert_eq!(digest_bytes(b"abc"), digest_bytes(b"abc"));
        assert_ne!(digest_bytes(b"abc"), digest_bytes(b"abd"));
        assert_eq!(digest_bytes(b"abc").len(), 64);
        assert!(
            digest_bytes(b"abc")
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        );
        // PIN: blake3("abc") lowercase hex — canary against a blake3
        // algorithm/version change.
        assert_eq!(
            digest_bytes(b"abc"),
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        );
    }

    // GAP 9 (L) — `hash_root_seed` is deterministic, seed-sensitive, and must
    // stay distinct from `derive_stage_seed(seed,"","")`. The two intentionally
    // hash different byte strings ("seed-x" vs "sectorforge:seed-x::").
    #[test]
    fn hash_root_seed_is_distinct_from_stage_seed() {
        assert_eq!(hash_root_seed("seed-x"), hash_root_seed("seed-x"));
        assert_ne!(hash_root_seed("seed-x"), hash_root_seed("seed-y"));
        // Load-bearing guard against anyone "unifying" the two derivations.
        assert_ne!(
            hash_root_seed("seed-x"),
            derive_stage_seed("seed-x", "", ""),
        );
        // Hex form is 64 chars (the [u8; 32] length is implicit in the type).
        assert_eq!(hex(&hash_root_seed("z")).len(), 64);
    }

    // GAP 7 (L) — `weighted_index` reverse-scan fallback: when only the trailing
    // entry is finite-positive, every draw returns that last index (2 here),
    // whether resolved by the main scan or the FP-edge reverse fallback.
    #[test]
    fn weighted_index_returns_trailing_positive_index() {
        let pool = vec![("a", 0.0), ("b", 0.0), ("c", 1.0)];
        let mut rng = stage_rng("g7", "rev", "");
        for _ in 0..200 {
            assert_eq!(weighted_index(&pool, &mut rng, "p").unwrap(), 2);
        }
    }
}
