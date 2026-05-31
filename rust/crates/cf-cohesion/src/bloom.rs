//! Self-contained Bloom filter mirroring the surface of Go `pkg/alg/bloom`
//! (`NewWithEstimates` / `Add` / `Test`).
//!
//! # Bit-identity seam (IMPORTANT)
//!
//! Bloom membership decisions feed cohesion scores that appear in machine output, so
//! per DESIGN §2.6 the filter must be **bit-identical** to the shared sketch crate
//! (`cf-alg-bloom`, itself a faithful re-implementation of `cf-alg-hashutil`). This
//! module implements the classic bits-and-blooms sizing
//! (`m = ceil(-n*ln(p)/ln2^2)`, `k = round((m/n)*ln2)`) with double hashing. It
//! produces the correct *behavior shape* and lets `cf-cohesion` build and be tested
//! independently, but its hash family is not guaranteed to match Go's, so false
//! positives can differ.
//!
//! Integration step (tracked in the crate todos): replace `crate::bloom::Filter`
//! usages in `calc.rs` with `cf_alg_bloom::Filter` once that crate's API is wired,
//! and delete this module.

/// Natural log of 2.
const LN2: f64 = std::f64::consts::LN_2;

/// A classic Bloom filter over a fixed bit array.
#[derive(Debug, Clone)]
pub struct Filter {
    bits: Vec<u64>,
    m: u64,
    k: u32,
}

/// Error returned by [`Filter::new_with_estimates`] for invalid parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BloomError;

impl std::fmt::Display for BloomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid bloom filter parameters")
    }
}

impl std::error::Error for BloomError {}

impl Filter {
    /// Constructs a filter sized for `n` expected elements at false-positive rate
    /// `p` (Go `bloom.NewWithEstimates`).
    ///
    /// # Errors
    ///
    /// Returns [`BloomError`] when `n == 0` or `p` is not in `(0, 1)`.
    pub fn new_with_estimates(n: u64, p: f64) -> Result<Self, BloomError> {
        if n == 0 || !(p > 0.0 && p < 1.0) {
            return Err(BloomError);
        }
        let nf = n as f64;
        let m = (-(nf * p.ln()) / (LN2 * LN2)).ceil().max(1.0) as u64;
        let k = (((m as f64) / nf) * LN2).round().max(1.0) as u32;
        let words = (m / 64 + 1) as usize;
        Ok(Filter {
            bits: vec![0u64; words],
            m,
            k,
        })
    }

    /// Adds `data` to the filter (Go `Filter.Add`).
    pub fn add(&mut self, data: &[u8]) {
        let (h1, h2) = double_hash(data);
        for i in 0..self.k {
            let idx = self.index(h1, h2, i);
            self.bits[(idx / 64) as usize] |= 1u64 << (idx % 64);
        }
    }

    /// Tests `data` for membership (Go `Filter.Test`). May return false positives.
    #[must_use]
    pub fn test(&self, data: &[u8]) -> bool {
        let (h1, h2) = double_hash(data);
        for i in 0..self.k {
            let idx = self.index(h1, h2, i);
            if self.bits[(idx / 64) as usize] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }

    fn index(&self, h1: u64, h2: u64, i: u32) -> u64 {
        h1.wrapping_add((i as u64).wrapping_mul(h2)) % self.m
    }
}

/// FNV-1a 64-bit plus a mixed second hash for double hashing.
fn double_hash(data: &[u8]) -> (u64, u64) {
    // FNV-1a.
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h1 ^= u64::from(b);
        h1 = h1.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // splitmix64-style mix of h1 for the second hash.
    let mut z = h1.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let h2 = z ^ (z >> 31);
    (h1, h2 | 1) // ensure h2 is odd/non-zero so steps cover the array
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_test_is_positive() {
        let mut f = Filter::new_with_estimates(64, 0.01).unwrap();
        f.add(b"hello");
        f.add(b"world");
        assert!(f.test(b"hello"));
        assert!(f.test(b"world"));
    }

    #[test]
    fn absent_is_usually_negative() {
        let mut f = Filter::new_with_estimates(64, 0.01).unwrap();
        f.add(b"present");
        // A clearly-absent key should not be present in a small, low-FP filter.
        assert!(!f.test(b"definitely-not-in-the-filter-xyzzy"));
    }

    #[test]
    fn invalid_params_error() {
        assert!(Filter::new_with_estimates(0, 0.01).is_err());
        assert!(Filter::new_with_estimates(10, 0.0).is_err());
        assert!(Filter::new_with_estimates(10, 1.0).is_err());
    }
}
