//! Count-Min Sketch for frequency estimation.
//!
//! A Count-Min Sketch estimates the frequency of elements in a data stream
//! using bounded overestimation. It answers "how many times has this element
//! been seen?" with an estimate that is always `>=` the true count (for
//! positive-only additions) and bounded by `epsilon * totalCount` with
//! probability `>= 1 - delta`.
//!
//! This implementation uses multiple independent hash functions derived from
//! FNV-1a with per-row seeds mixed via a splitmix64 finalizer.
//!
//! # Byte-identity
//!
//! The column indices, per-row seeds and resulting frequency estimates are a
//! frozen compatibility contract: they flow into the Halstead analyzer's
//! machine-format reports, whose bytes are pinned against the reference
//! implementation by `tests/compat`. The hash machinery (`mix64`,
//! `generate_seeds`, `fnv64a`) is provided by the [`cf_alg_hashutil`] crate,
//! whose fixed seeds and finalizers are part of the same contract.
//!
//! # Thread safety
//!
//! All mutable state lives behind a [`std::sync::RwLock`], so a [`Sketch`] can
//! be shared across threads (`Send + Sync`) and concurrently mutated through
//! `&self`. [`Sketch::add`] takes a write lock; [`Sketch::count`] and
//! [`Sketch::total_count`] take read locks.
//!
//! # Wrapping arithmetic
//!
//! Counter and total accumulation use `i64` and `wrapping_add`: overflow wraps
//! in two's complement in every build profile (reference-implementation
//! behavior). In practice counts stay far below the overflow point, but the
//! wrapping keeps the contract exact at the edge.
//!
//! # Examples
//!
//! ```
//! use cf_alg_cms::Sketch;
//!
//! let sk = Sketch::new(0.001, 0.001).unwrap();
//! assert_eq!(sk.width(), 2719);
//! assert_eq!(sk.depth(), 7);
//!
//! sk.add(b"token-operator", 42);
//! // Count-Min never underestimates.
//! assert!(sk.count(b"token-operator") >= 42);
//! assert_eq!(sk.total_count(), 42);
//! ```

#![forbid(unsafe_code)]

use std::sync::RwLock;

use cf_alg_hashutil::{fnv64a, generate_seeds, mix64};

/// Errors returned by [`Sketch::new`].
///
/// The [`Display`](core::fmt::Display) strings are part of the CLI/log
/// compatibility contract and must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// `epsilon` was not positive.
    #[error("cms: epsilon must be positive")]
    InvalidEpsilon,

    /// `delta` was not in the open interval `(0, 1)`.
    #[error("cms: delta must be in the open interval (0, 1)")]
    InvalidDelta,
}

/// Mutable state of a [`Sketch`], guarded by a single [`RwLock`].
///
/// Grouping `counters` and `total_count` under one lock keeps them consistent:
/// a reader sees a snapshot where `total_count` reflects exactly the additions
/// visible in `counters`.
#[derive(Debug)]
struct State {
    /// Flattened 2D array: `depth` rows by `width` columns, row-major.
    counters: Vec<i64>,
    /// Sum of all counts added to the sketch.
    total_count: i64,
}

/// A thread-safe Count-Min Sketch for frequency estimation.
///
/// Construct with [`Sketch::new`] from desired error bounds; mutate with
/// [`Sketch::add`]; query with [`Sketch::count`] and [`Sketch::total_count`];
/// clear with [`Sketch::reset`].
#[derive(Debug)]
pub struct Sketch {
    /// Per-row seeds for independent hashing (one seed per row).
    ///
    /// Immutable after construction, so it lives outside the lock.
    seeds: Vec<u64>,
    /// Number of columns in the sketch.
    width: usize,
    /// Number of rows (independent hash functions) in the sketch.
    depth: usize,
    /// Lock-guarded mutable counters and running total.
    state: RwLock<State>,
}

impl Sketch {
    /// Creates a Count-Min Sketch with automatic sizing from the desired error
    /// bounds.
    ///
    /// `width = ceil(e / epsilon)`, `depth = ceil(ln(1 / delta))`, where `e` is
    /// Euler's number and `ln` the natural logarithm.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidEpsilon`] if `epsilon <= 0`, or
    /// [`Error::InvalidDelta`] if `delta` is not in the open interval `(0, 1)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cf_alg_cms::Sketch;
    /// let sk = Sketch::new(0.01, 0.01).unwrap();
    /// assert_eq!(sk.width(), 272);
    /// assert_eq!(sk.depth(), 5);
    /// ```
    pub fn new(epsilon: f64, delta: f64) -> Result<Self, Error> {
        if epsilon <= 0.0 {
            return Err(Error::InvalidEpsilon);
        }

        if delta <= 0.0 || delta >= 1.0 {
            return Err(Error::InvalidDelta);
        }

        // The f64-to-usize conversion truncates toward zero; ceil has already
        // made the value a non-negative whole number, so `as usize` is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = (std::f64::consts::E / epsilon).ceil() as usize;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let depth = (1.0_f64 / delta).ln().ceil() as usize;

        // CMS-style seeds use the Mix64 finalizer over the fixed seed schedule.
        let seeds = generate_seeds(depth, mix64);

        Ok(Self {
            seeds,
            width,
            depth,
            state: RwLock::new(State {
                counters: vec![0; width.saturating_mul(depth)],
                total_count: 0,
            }),
        })
    }

    /// Returns the number of columns in the sketch.
    #[inline]
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Returns the number of rows (hash functions) in the sketch.
    #[inline]
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    /// Increments the counter for `key` by `count`.
    ///
    /// A `count` of zero is a no-op (it does not even touch the total).
    /// Negative counts are permitted, though Count-Min's non-underestimation
    /// guarantee only holds for positive-only additions.
    ///
    /// # Panics
    ///
    /// Panics only if the internal lock is poisoned (a prior panic occurred
    /// while the lock was held), which cannot happen during normal use.
    pub fn add(&self, key: &[u8], count: i64) {
        if count == 0 {
            return;
        }

        let mut st = self.state.write().expect("cms: lock poisoned");

        for row in 0..self.depth {
            let col = self.hash_key(row, key);
            let idx = row * self.width + col;
            st.counters[idx] = st.counters[idx].wrapping_add(count);
        }

        st.total_count = st.total_count.wrapping_add(count);
    }

    /// Returns the estimated frequency of `key`.
    ///
    /// For positive-only additions, the estimate is always `>=` the true count.
    /// The estimate is the minimum counter across all rows.
    ///
    /// # Panics
    ///
    /// Panics only if the internal lock is poisoned.
    #[must_use]
    pub fn count(&self, key: &[u8]) -> i64 {
        let st = self.state.read().expect("cms: lock poisoned");

        // Start from i64::MAX so the first row always lowers it.
        let mut min_val = i64::MAX;

        for row in 0..self.depth {
            let col = self.hash_key(row, key);
            let val = st.counters[row * self.width + col];

            if val < min_val {
                min_val = val;
            }
        }

        min_val
    }

    /// Returns the sum of all counts added to the sketch.
    ///
    /// # Panics
    ///
    /// Panics only if the internal lock is poisoned.
    #[must_use]
    pub fn total_count(&self) -> i64 {
        self.state.read().expect("cms: lock poisoned").total_count
    }

    /// Clears all counters and the total count without reallocation.
    ///
    /// # Panics
    ///
    /// Panics only if the internal lock is poisoned.
    pub fn reset(&self) {
        let mut st = self.state.write().expect("cms: lock poisoned");

        for c in &mut st.counters {
            *c = 0;
        }

        st.total_count = 0;
    }

    /// Computes the column index for the given `row` and `key`.
    ///
    /// Frozen hashing scheme: the row seed forms an 8-byte **little-endian**
    /// prefix, followed by the key bytes; the concatenation is hashed with
    /// FNV-1a/64 and reduced modulo `width`.
    #[inline]
    fn hash_key(&self, row: usize, key: &[u8]) -> usize {
        // Seed as 8-byte little-endian prefix for per-row independence.
        let seed_buf = self.seeds[row].to_le_bytes();

        // FNV-1a over (seed_buf || key). Computing the hash incrementally over
        // the two slices is identical to hashing their concatenation, because
        // FNV-1a folds one byte at a time with no length framing.
        let mut h = fnv64a(&seed_buf);
        h = fnv64a_continue(h, key);

        // The full 64-bit hash reduced modulo width (contract: the reduction
        // happens in 64-bit space).
        (h % self.width as u64) as usize
    }
}

/// Continues an in-progress FNV-1a/64 hash over additional bytes.
///
/// FNV-1a has no finalization step and no length framing, so feeding bytes in
/// two passes (`fnv64a(prefix)` then `fnv64a_continue(h, rest)`) yields the
/// same 64-bit value as hashing the concatenation `prefix || rest` in one
/// pass.
#[inline]
fn fnv64a_continue(mut h: u64, data: &[u8]) -> u64 {
    /// FNV-1a 64-bit prime (canonical constant).
    const FNV64A_PRIME: u64 = 0x0000_0100_0000_01b3;

    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(FNV64A_PRIME);
    }

    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::thread;

    // Standard config: width=ceil(e/0.001)=2719, depth=ceil(ln(1/0.001))=7.
    const STANDARD_EPSILON: f64 = 0.001;
    const STANDARD_DELTA: f64 = 0.001;
    const EXPECTED_WIDTH: usize = 2719;
    const EXPECTED_DEPTH: usize = 7;

    // Loose config for faster tests.
    const LOOSE_EPSILON: f64 = 0.01;
    const LOOSE_DELTA: f64 = 0.01;
    const LOOSE_WIDTH: usize = 272;
    const LOOSE_DEPTH: usize = 5;

    // Concurrency test parameters.
    const CONC_THREADS: usize = 100;
    const CONC_OPS_PER_THREAD: usize = 1000;

    // Overestimation test parameters.
    const OVEREST_N: usize = 10_000;
    const OVEREST_FREQ: i64 = 100;

    /// Converts a `u64` to an 8-byte big-endian array.
    ///
    /// (This is the *key* encoding used by the tests; it is independent of the
    /// little-endian *seed* encoding inside `hash_key`.)
    fn u64_to_bytes(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    /// Generates a deterministic test key from a prefix and index.
    fn test_key(prefix: &str, idx: usize) -> Vec<u8> {
        format!("{prefix}-{idx}").into_bytes()
    }

    #[test]
    fn new_parameters() {
        let cases = [
            (
                "standard",
                STANDARD_EPSILON,
                STANDARD_DELTA,
                EXPECTED_WIDTH,
                EXPECTED_DEPTH,
            ),
            (
                "loose",
                LOOSE_EPSILON,
                LOOSE_DELTA,
                LOOSE_WIDTH,
                LOOSE_DEPTH,
            ),
        ];
        for (name, eps, delta, want_w, want_d) in cases {
            let sk = Sketch::new(eps, delta).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(sk.width(), want_w, "{name}: width");
            assert_eq!(sk.depth(), want_d, "{name}: depth");
        }
    }

    #[test]
    fn new_edge_cases() {
        assert_eq!(
            Sketch::new(0.0, STANDARD_DELTA).unwrap_err(),
            Error::InvalidEpsilon
        );
        assert_eq!(
            Sketch::new(-0.01, STANDARD_DELTA).unwrap_err(),
            Error::InvalidEpsilon
        );
        assert_eq!(
            Sketch::new(STANDARD_EPSILON, 0.0).unwrap_err(),
            Error::InvalidDelta
        );
        assert_eq!(
            Sketch::new(STANDARD_EPSILON, -0.01).unwrap_err(),
            Error::InvalidDelta
        );
        assert_eq!(
            Sketch::new(STANDARD_EPSILON, 1.0).unwrap_err(),
            Error::InvalidDelta
        );
        assert_eq!(
            Sketch::new(STANDARD_EPSILON, 1.5).unwrap_err(),
            Error::InvalidDelta
        );
    }

    // The error Display strings are frozen (CLI/log compatibility contract).
    #[test]
    fn error_messages_are_frozen() {
        assert_eq!(
            Error::InvalidEpsilon.to_string(),
            "cms: epsilon must be positive"
        );
        assert_eq!(
            Error::InvalidDelta.to_string(),
            "cms: delta must be in the open interval (0, 1)"
        );
    }

    #[test]
    fn add_count_single_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        let key = b"token-operator";
        let add_count: i64 = 42;
        sk.add(key, add_count);
        assert!(
            sk.count(key) >= add_count,
            "CMS count must be >= true count"
        );
    }

    #[test]
    fn add_count_multiple_keys() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        let keys: [(&str, i64); 4] = [
            ("operator-plus", 100),
            ("operator-minus", 50),
            ("operand-x", 200),
            ("operand-y", 75),
        ];
        for (key, count) in keys {
            sk.add(key.as_bytes(), count);
        }
        for (key, true_count) in keys {
            let count = sk.count(key.as_bytes());
            assert!(
                count >= true_count,
                "CMS count for {key:?} must be >= true count {true_count}, got {count}"
            );
        }
    }

    #[test]
    fn count_never_added() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(b"exists", 100);
        assert!(
            sk.count(b"never-added") >= 0,
            "count of absent key must be >= 0"
        );
    }

    #[test]
    fn overestimation_bounded() {
        let sk = Sketch::new(STANDARD_EPSILON, STANDARD_DELTA).unwrap();
        let mut true_freqs: HashMap<Vec<u8>, i64> = HashMap::with_capacity(OVEREST_N);
        for i in 0..OVEREST_N {
            let key = test_key("key", i);
            sk.add(&key, OVEREST_FREQ);
            true_freqs.insert(key, OVEREST_FREQ);
        }

        let total_count = sk.total_count();
        let max_overest = total_count as f64 * STANDARD_EPSILON;
        let mut violations = 0usize;
        for (key, true_freq) in &true_freqs {
            let estimated = sk.count(key);
            let overestimation = (estimated - true_freq) as f64;
            if overestimation > max_overest {
                violations += 1;
            }
        }

        // With delta=0.001, we expect <0.1% violations.
        let max_violations = (OVEREST_N as f64 * STANDARD_DELTA * 10.0).ceil() as usize;
        assert!(
            violations <= max_violations,
            "too many overestimation violations: {violations} > {max_violations} \
             (max_overest={max_overest:.2}, total_count={total_count})"
        );
    }

    #[test]
    fn add_zero_count() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(b"key", 0);
        assert_eq!(sk.total_count(), 0);
        assert_eq!(sk.count(b"key"), 0);
    }

    // The empty-slice key must hash like any other key and must not panic.
    #[test]
    fn nil_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(&[], 5);
        assert!(sk.count(&[]) >= 5);
    }

    #[test]
    fn empty_slice_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(&[], 3);
        assert!(sk.count(&[]) >= 3);
    }

    #[test]
    fn reset() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(b"key", 100);
        assert!(sk.count(b"key") > 0);
        assert!(sk.total_count() > 0);

        sk.reset();

        assert_eq!(sk.count(b"key"), 0);
        assert_eq!(sk.total_count(), 0);
    }

    #[test]
    fn total_count() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        assert_eq!(sk.total_count(), 0);
        sk.add(b"a", 10);
        sk.add(b"b", 20);
        sk.add(b"a", 5);
        assert_eq!(sk.total_count(), 35);
    }

    // Two independently constructed sketches use the same fixed seeds, so
    // estimates must match exactly.
    #[test]
    fn determinism() {
        let sk1 = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        let sk2 = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        for i in 0..100 {
            let key = test_key("det", i);
            sk1.add(&key, (i + 1) as i64);
            sk2.add(&key, (i + 1) as i64);
        }
        for i in 0..100 {
            let key = test_key("det", i);
            assert_eq!(
                sk1.count(&key),
                sk2.count(&key),
                "determinism violated for key {i}"
            );
        }
    }

    // Many threads add disjoint keys concurrently while reading; the total
    // must be exact.
    #[test]
    fn concurrent_add_count() {
        let sk = Arc::new(Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap());
        let mut handles = Vec::with_capacity(CONC_THREADS);
        for g in 0..CONC_THREADS {
            let sk = Arc::clone(&sk);
            handles.push(thread::spawn(move || {
                for i in 0..CONC_OPS_PER_THREAD {
                    let key = u64_to_bytes((g * CONC_OPS_PER_THREAD + i) as u64);
                    sk.add(&key, 1);
                }
                // Read while others are writing.
                let _ = sk.count(&u64_to_bytes(g as u64));
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        let expected_total = (CONC_THREADS * CONC_OPS_PER_THREAD) as i64;
        assert_eq!(sk.total_count(), expected_total);
    }

    // The counter array sizing is width*depth.
    #[test]
    fn memory_usage() {
        let sk = Sketch::new(STANDARD_EPSILON, STANDARD_DELTA).unwrap();
        assert_eq!(sk.width(), EXPECTED_WIDTH);
        assert_eq!(sk.depth(), EXPECTED_DEPTH);
        // Counter array should be width * depth * 8 bytes (i64).
        let expected_bytes = EXPECTED_WIDTH * EXPECTED_DEPTH * 8;
        assert_eq!(expected_bytes, 2719 * 7 * 8);
    }

    #[test]
    fn multiple_adds_accumulate() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        let key = b"accumulate";
        for _ in 0..100 {
            sk.add(key, 1);
        }
        assert!(sk.count(key) >= 100);
    }

    // Extra parity guard: the incremental two-slice FNV-1a used by hash_key must
    // equal hashing the concatenation in one pass.
    #[test]
    fn fnv_two_pass_equals_concat() {
        let seed: u64 = 0x0123_4567_89ab_cdef;
        let key = b"some-key-bytes";
        let mut concat = Vec::new();
        concat.extend_from_slice(&seed.to_le_bytes());
        concat.extend_from_slice(key);

        let one_pass = fnv64a(&concat);
        let two_pass = fnv64a_continue(fnv64a(&seed.to_le_bytes()), key);
        assert_eq!(one_pass, two_pass);
    }
}
