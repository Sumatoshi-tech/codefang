//! HyperLogLog cardinality estimator.
//!
//! HyperLogLog estimates the number of distinct elements in a multiset with
//! approximately 2% standard error using only `2^p` bytes of memory (e.g.
//! 16 KiB for precision 14). It is useful for counting unique items
//! (developers, files, tokens) without maintaining a full set.
//!
//! This implementation uses the **LogLog-Beta** bias correction from
//! Qin et al. (2016), which provides accurate estimates across all cardinality
//! ranges without the piecewise linear interpolation tables of HLL++.
//!
//! The estimator is deterministic given the same sequence of [`Sketch::add`]
//! calls, and the cardinality estimate is a frozen compatibility contract: the
//! FNV-1a + Mix64 hash kernel ([`cf_alg_hashutil`]) selects the register, the
//! LogLog-Beta polynomial and `alpha` constants are fixed, and the final
//! estimate is rounded half away from zero (`f64::round`) before truncation to
//! `u64`. Sketch estimates appear in machine-format reports whose bytes are
//! pinned against the reference implementation by `tests/compat`.
//!
//! # Thread safety
//!
//! Shared mutation is expressed through the type system rather than an
//! internal lock: [`Sketch`] is `Send + Sync` for read-only sharing, and
//! callers that need concurrent mutation wrap it in `Mutex`/`RwLock`.
//!
//! # Examples
//!
//! ```
//! use cf_alg_hll::Sketch;
//!
//! let mut sk = Sketch::new(14).expect("precision 14 is in range");
//! for i in 0u64..10_000 {
//!     sk.add(&i.to_be_bytes());
//! }
//! let estimate = sk.count();
//! // Within ~3% of the true cardinality of 10000.
//! assert!((9_700..=10_300).contains(&estimate));
//! ```

#![forbid(unsafe_code)]

use cf_alg_hashutil::{fnv64a, mix64};

/// Minimum allowed precision (`2^4` = 16 registers).
const MIN_PRECISION: u8 = 4;

/// Maximum allowed precision (`2^18` = 262144 registers).
const MAX_PRECISION: u8 = 18;

/// Total number of bits in the hash output.
const HASH_BITS: u8 = 64;

/// Precision 5 — alpha-constant lookup key.
const PRECISION_P5: u8 = 5;

/// Precision 6 — alpha-constant lookup key.
const PRECISION_P6: u8 = 6;

/// Alpha constant for `2^4` = 16 registers.
const ALPHA_P4: f64 = 0.673;

/// Alpha constant for `2^5` = 32 registers.
const ALPHA_P5: f64 = 0.697;

/// Alpha constant for `2^6` = 64 registers.
const ALPHA_P6: f64 = 0.709;

/// Numerator in the generic alpha formula.
const ALPHA_GENERIC_NUMERATOR: f64 = 0.7213;

/// Coefficient in the generic alpha denominator.
const ALPHA_GENERIC_DENOMINATOR_COEFF: f64 = 1.079;

// LogLog-Beta polynomial coefficients from Qin et al. (2016).
const BETA_C0: f64 = -0.370393911;
const BETA_C1: f64 = 0.070471823;
const BETA_C2: f64 = 0.17393686;
const BETA_C3: f64 = 0.16339839;
const BETA_C4: f64 = -0.09237745;
const BETA_C5: f64 = 0.03738027;
const BETA_C6: f64 = -0.005384159;
const BETA_C7: f64 = 0.00042419;

/// Errors returned by [`Sketch`] constructors and operations.
///
/// The [`Display`](std::fmt::Display) strings are part of the CLI/log
/// compatibility contract and must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HllError {
    /// Precision was not in `[4, 18]`.
    #[error("hll: precision must be in [4, 18]")]
    PrecisionOutOfRange,
    /// Attempted to merge sketches with different precisions.
    #[error("hll: cannot merge sketches with different precisions")]
    PrecisionMismatch,
}

/// A HyperLogLog cardinality estimator.
///
/// Allocates `2^precision` single-byte registers and updates them with the
/// observed leading-zero rank of each hashed element. See the [crate]
/// documentation for the byte-identity contract.
#[derive(Debug, Clone)]
pub struct Sketch {
    registers: Vec<u8>,
    precision: u8,
}

impl Sketch {
    /// Creates a HyperLogLog sketch with the given precision `p`.
    ///
    /// Precision must be in `[4, 18]`. The sketch allocates `2^p` registers
    /// (one byte each).
    ///
    /// # Errors
    ///
    /// Returns [`HllError::PrecisionOutOfRange`] when `precision` is outside
    /// `[4, 18]`.
    pub fn new(precision: u8) -> Result<Self, HllError> {
        if !(MIN_PRECISION..=MAX_PRECISION).contains(&precision) {
            return Err(HllError::PrecisionOutOfRange);
        }
        let reg_count = 1usize << precision;
        Ok(Self {
            registers: vec![0u8; reg_count],
            precision,
        })
    }

    /// Inserts `data` into the sketch by hashing it and updating the
    /// appropriate register with the observed number of leading zeros.
    ///
    /// Hashing uses FNV-1a followed by the Mix64 finalizer (via
    /// [`cf_alg_hashutil`]); the kernel is frozen.
    pub fn add(&mut self, data: &[u8]) {
        let hash_val = hash64(data);
        let idx = (hash_val >> (HASH_BITS - self.precision)) as usize;

        // Mask out the upper p bits to get the remaining (64-p) bits, then count
        // the position of the leftmost 1-bit (rho = leading zeros + 1). When all
        // remaining bits are zero, rho = 64-p+1 (the maximum).
        let remaining: u32 = (HASH_BITS - self.precision) as u32;
        let mask: u64 = (1u64 << remaining) - 1;
        let w = hash_val & mask;

        // Number of bits required to represent w (0 for w == 0).
        let len64 = (64 - w.leading_zeros()) as u8;
        let rho = (remaining as u8 - len64) + 1;

        if rho > self.registers[idx] {
            self.registers[idx] = rho;
        }
    }

    /// Returns the estimated number of distinct elements added to the sketch.
    ///
    /// Uses the LogLog-Beta formula
    /// `alpha * m * (m - ez) / (beta(ez) + sum)`, where `ez` is the number of
    /// zero registers and `sum` is the harmonic sum of `2^-M[j]`. The result is
    /// rounded half away from zero and truncated to `u64` (frozen rounding
    /// rule).
    #[must_use]
    pub fn count(&self) -> u64 {
        let reg_count = (1usize << self.precision) as f64;
        let zeros = count_zero_registers(&self.registers) as f64;

        if zeros == reg_count {
            return 0;
        }

        let alpha_m = alpha(self.precision);
        let harmonic_sum = compute_harmonic_sum(&self.registers);
        let beta_val = beta_correction(zeros);
        let estimate = alpha_m * reg_count * (reg_count - zeros) / (beta_val + harmonic_sum);

        // Round half away from zero, then truncate (the value is already an
        // integer-valued f64 after rounding). This rounding rule is part of
        // the report compatibility contract.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            estimate.round() as u64
        }
    }

    /// Combines another sketch into this one by taking the element-wise maximum
    /// of registers. Both sketches must have the same precision.
    ///
    /// # Errors
    ///
    /// Returns [`HllError::PrecisionMismatch`] if the precisions differ.
    pub fn merge(&mut self, other: &Self) -> Result<(), HllError> {
        if self.precision != other.precision {
            return Err(HllError::PrecisionMismatch);
        }
        for (i, &val) in other.registers.iter().enumerate() {
            if val > self.registers[i] {
                self.registers[i] = val;
            }
        }
        Ok(())
    }

    /// Clears all registers without reallocating the underlying array.
    pub fn reset(&mut self) {
        for r in &mut self.registers {
            *r = 0;
        }
    }

    /// Returns a deep copy of the sketch.
    ///
    /// Equivalent to the derived [`Clone`] implementation; retained for API
    /// stability.
    #[must_use]
    pub fn clone_sketch(&self) -> Self {
        self.clone()
    }

    /// Returns the configured precision of the sketch.
    #[must_use]
    pub const fn precision(&self) -> u8 {
        self.precision
    }

    /// Returns the number of registers (`2^p`).
    #[must_use]
    pub const fn register_count(&self) -> usize {
        1usize << self.precision
    }

    /// Serializes the sketch state into a byte vector.
    ///
    /// The format is `[precision:1][registers:m]`; provided for cross-boundary
    /// persisted state.
    #[must_use]
    pub fn marshal_binary(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.registers.len());
        buf.push(self.precision);
        buf.extend_from_slice(&self.registers);
        buf
    }

    /// Restores sketch state from bytes produced by
    /// [`marshal_binary`](Self::marshal_binary).
    ///
    /// # Errors
    ///
    /// Returns [`HllError::PrecisionOutOfRange`] if the encoded precision is
    /// outside `[4, 18]` or the payload length is inconsistent with it.
    pub fn unmarshal_binary(data: &[u8]) -> Result<Self, HllError> {
        if data.is_empty() {
            return Err(HllError::PrecisionOutOfRange);
        }
        let precision = data[0];
        if !(MIN_PRECISION..=MAX_PRECISION).contains(&precision) {
            return Err(HllError::PrecisionOutOfRange);
        }
        let reg_count = 1usize << precision;
        if data.len() != 1 + reg_count {
            return Err(HllError::PrecisionOutOfRange);
        }
        Ok(Self {
            registers: data[1..].to_vec(),
            precision,
        })
    }
}

/// Counts registers that are still at zero.
fn count_zero_registers(registers: &[u8]) -> usize {
    registers.iter().filter(|&&v| v == 0).count()
}

/// Computes the sum of `2^-M[j]` for all registers.
fn compute_harmonic_sum(registers: &[u8]) -> f64 {
    let mut sum = 0.0f64;
    for &val in registers {
        // exp2 (not powf) keeps the estimate bit-reproducible.
        sum += (-f64::from(val)).exp2();
    }
    sum
}

/// Returns the `alpha_m` constant used in the HLL estimate formula.
///
/// For `m >= 128`, `alpha_m = 0.7213 / (1 + 1.079/m)`.
fn alpha(precision: u8) -> f64 {
    let reg_count = (1usize << precision) as f64;
    match precision {
        MIN_PRECISION => ALPHA_P4,
        PRECISION_P5 => ALPHA_P5,
        PRECISION_P6 => ALPHA_P6,
        _ => ALPHA_GENERIC_NUMERATOR / (1.0 + ALPHA_GENERIC_DENOMINATOR_COEFF / reg_count),
    }
}

/// Computes the LogLog-Beta bias-correction term (Qin et al. 2016).
///
/// A polynomial approximation in `ln(zeroCount + 1)` that corrects estimator
/// bias across all cardinality ranges.
fn beta_correction(zero_count: f64) -> f64 {
    let zl = (zero_count + 1.0).ln();
    let zl2 = zl * zl;
    let zl3 = zl2 * zl;
    let zl4 = zl3 * zl;
    let zl5 = zl4 * zl;
    let zl6 = zl5 * zl;
    let zl7 = zl6 * zl;

    BETA_C0 * zero_count
        + BETA_C1 * zl
        + BETA_C2 * zl2
        + BETA_C3 * zl3
        + BETA_C4 * zl4
        + BETA_C5 * zl5
        + BETA_C6 * zl6
        + BETA_C7 * zl7
}

/// Computes a 64-bit hash of `data` using FNV-1a followed by the Mix64
/// bit-mixing finalizer.
///
/// The finalizer ensures good avalanche across all bit positions, which is
/// critical for HyperLogLog where both the high bits (register index) and the
/// low bits (leading zeros) must be well distributed.
fn hash64(data: &[u8]) -> u64 {
    mix64(fnv64a(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_PRECISION: u8 = 14;
    const MIN_PREC: u8 = 4;
    const MAX_PREC: u8 = 18;
    const BELOW_MIN_PREC: u8 = 3;
    const ABOVE_MAX_PREC: u8 = 19;

    const REGISTERS_P4: usize = 1 << 4; // 16
    const REGISTERS_P14: usize = 1 << 14; // 16384
    const REGISTERS_P18: usize = 1 << 18; // 262144

    const ACCURACY_MAX_ERROR: f64 = 0.03; // 3% relative error

    const DUPLICATE_COUNT: usize = 1000;
    const DUPLICATE_MAX_RESULT: u64 = 2;

    const CARD_N1K: usize = 1_000;
    const CARD_N10K: usize = 10_000;
    const CARD_N100K: usize = 100_000;

    /// Converts a `u64` to an 8-byte big-endian array.
    fn uint64_to_bytes(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    #[test]
    fn test_new_parameters() {
        let cases = [
            ("min_precision_4", MIN_PREC, REGISTERS_P4),
            ("default_precision_14", DEFAULT_PRECISION, REGISTERS_P14),
            ("max_precision_18", MAX_PREC, REGISTERS_P18),
        ];
        for (name, precision, want_reg_cnt) in cases {
            let sk = Sketch::new(precision).expect("valid precision");
            assert_eq!(sk.precision(), precision, "{name}: precision");
            assert_eq!(sk.register_count(), want_reg_cnt, "{name}: register_count");
        }
    }

    #[test]
    fn test_new_edge_cases() {
        assert_eq!(
            Sketch::new(BELOW_MIN_PREC).unwrap_err(),
            HllError::PrecisionOutOfRange,
            "below min precision"
        );
        assert_eq!(
            Sketch::new(ABOVE_MAX_PREC).unwrap_err(),
            HllError::PrecisionOutOfRange,
            "above max precision"
        );
        assert_eq!(
            Sketch::new(0).unwrap_err(),
            HllError::PrecisionOutOfRange,
            "zero precision"
        );
    }

    #[test]
    fn test_count_empty_sketch() {
        let sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        assert_eq!(sk.count(), 0);
    }

    #[test]
    fn test_add_count_single_element() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        sk.add(b"hello");
        let count = sk.count();
        assert!(count >= 1, "count={count}, want >= 1");
        assert!(count <= 2, "count={count}, want <= 2");
    }

    #[test]
    fn test_add_count_duplicate_elements() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        let data = b"same-element";
        for _ in 0..DUPLICATE_COUNT {
            sk.add(data);
        }
        let count = sk.count();
        assert!(
            count <= DUPLICATE_MAX_RESULT,
            "adding same element {DUPLICATE_COUNT} times should produce count <= {DUPLICATE_MAX_RESULT}, got {count}"
        );
    }

    // Accuracy across cardinality ranges (subset up to 100K to keep the unit
    // test fast; larger ranges are exercised by the differential harness).
    #[test]
    fn test_accuracy_ranges() {
        for &n in &[100usize, CARD_N1K, CARD_N10K, CARD_N100K] {
            let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
            for i in 0..n {
                sk.add(&uint64_to_bytes(i as u64));
            }
            let count = sk.count();
            let expected = n as f64;
            let relative_error = (count as f64 - expected).abs() / expected;
            assert!(
                relative_error <= ACCURACY_MAX_ERROR,
                "relative error {relative_error:.4} exceeds maximum {ACCURACY_MAX_ERROR:.4} for n={n} (count={count})"
            );
        }
    }

    #[test]
    fn test_determinism() {
        let mut sk1 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let mut sk2 = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            let data = uint64_to_bytes(i as u64);
            sk1.add(&data);
            sk2.add(&data);
        }
        assert_eq!(sk1.count(), sk2.count());
    }

    #[test]
    fn test_merge_disjoint_sets() {
        let mut sk1 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let mut sk2 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let half = CARD_N10K / 2;
        for i in 0..half {
            sk1.add(&uint64_to_bytes(i as u64));
        }
        for i in half..CARD_N10K {
            sk2.add(&uint64_to_bytes(i as u64));
        }
        sk1.merge(&sk2).expect("same precision merge");
        let count = sk1.count();
        let expected = CARD_N10K as f64;
        let relative_error = (count as f64 - expected).abs() / expected;
        assert!(
            relative_error <= ACCURACY_MAX_ERROR,
            "merge error {relative_error:.4} exceeds maximum {ACCURACY_MAX_ERROR:.4} (count={count})"
        );
    }

    #[test]
    fn test_merge_overlapping_sets() {
        let mut sk1 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let mut sk2 = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            sk1.add(&uint64_to_bytes(i as u64));
        }
        let overlap = CARD_N1K / 2;
        for i in overlap..(CARD_N1K + overlap) {
            sk2.add(&uint64_to_bytes(i as u64));
        }
        sk1.merge(&sk2).expect("same precision merge");
        let count = sk1.count();
        let expected = (CARD_N1K + overlap) as f64; // union [0, 1500)
        let relative_error = (count as f64 - expected).abs() / expected;
        assert!(
            relative_error <= ACCURACY_MAX_ERROR,
            "overlapping merge error {relative_error:.4} (count={count})"
        );
    }

    #[test]
    fn test_merge_precision_mismatch() {
        let mut sk1 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let sk2 = Sketch::new(MIN_PREC).unwrap();
        assert_eq!(
            sk1.merge(&sk2).unwrap_err(),
            HllError::PrecisionMismatch
        );
    }

    #[test]
    fn test_merge_empty_sketch() {
        let mut sk1 = Sketch::new(DEFAULT_PRECISION).unwrap();
        let sk2 = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            sk1.add(&uint64_to_bytes(i as u64));
        }
        let count_before = sk1.count();
        sk1.merge(&sk2).expect("same precision merge");
        assert_eq!(count_before, sk1.count());
    }

    // Must not panic on empty data; a single (empty) element yields
    // count >= 1.
    #[test]
    fn test_nil_data() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        sk.add(&[]);
        assert!(sk.count() >= 1);
    }

    #[test]
    fn test_reset() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            sk.add(&uint64_to_bytes(i as u64));
        }
        assert!(sk.count() > 0);
        sk.reset();
        assert_eq!(sk.count(), 0);
        assert_eq!(sk.precision(), DEFAULT_PRECISION);
    }

    #[test]
    fn test_clone() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            sk.add(&uint64_to_bytes(i as u64));
        }
        let mut clone = sk.clone_sketch();
        assert_eq!(sk.count(), clone.count());
        assert_eq!(sk.precision(), clone.precision());

        // Modifying the clone must not affect the original.
        for i in CARD_N1K..CARD_N10K {
            clone.add(&uint64_to_bytes(i as u64));
        }
        let original_count = sk.count();
        let clone_count = clone.count();
        assert!(
            clone_count > original_count,
            "clone should have more elements after additional adds (clone={clone_count}, original={original_count})"
        );
    }

    #[test]
    fn test_memory_usage_p14() {
        let sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        assert_eq!(sk.register_count(), REGISTERS_P14);
    }

    // The empty slice is a regular, hashable element.
    #[test]
    fn test_empty_slice_data() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        sk.add(&[]);
        assert!(sk.count() >= 1);
    }

    // Alpha constants spot-check.
    #[test]
    fn test_alpha_constants() {
        assert_eq!(alpha(4), ALPHA_P4);
        assert_eq!(alpha(5), ALPHA_P5);
        assert_eq!(alpha(6), ALPHA_P6);
        let reg_count = (1usize << 14) as f64;
        assert_eq!(
            alpha(14),
            ALPHA_GENERIC_NUMERATOR / (1.0 + ALPHA_GENERIC_DENOMINATOR_COEFF / reg_count)
        );
    }

    // Binary state round-trip and error cases.
    #[test]
    fn test_marshal_unmarshal_round_trip() {
        let mut sk = Sketch::new(DEFAULT_PRECISION).unwrap();
        for i in 0..CARD_N1K {
            sk.add(&uint64_to_bytes(i as u64));
        }
        let data = sk.marshal_binary();
        let restored = Sketch::unmarshal_binary(&data).expect("round-trip");
        assert_eq!(restored.precision(), sk.precision());
        assert_eq!(restored.count(), sk.count());
    }

    #[test]
    fn test_unmarshal_errors() {
        // Empty data.
        assert!(Sketch::unmarshal_binary(&[]).is_err());
        // Precision too low / too high.
        assert!(Sketch::unmarshal_binary(&[3]).is_err());
        assert!(Sketch::unmarshal_binary(&[19]).is_err());
        // Truncated registers for a valid precision byte.
        assert!(Sketch::unmarshal_binary(&[14, 0, 0, 0]).is_err());
    }

    // Hash kernel parity guard: hash64 == mix64(fnv64a(data)).
    #[test]
    fn test_hash64_kernel() {
        let data = b"developer@example.com";
        assert_eq!(hash64(data), mix64(fnv64a(data)));
    }
}
