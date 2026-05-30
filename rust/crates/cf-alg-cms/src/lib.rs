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
//! This is a faithful, **bit-identical** port of the Go package
//! `pkg/alg/cms` (`github.com/Sumatoshi-tech/codefang`). The column indices,
//! per-row seeds and resulting frequency estimates produced here must match the
//! Go implementation exactly, because those estimates flow into the Halstead
//! analyzer's machine-format reports whose byte-identity is the project goal
//! (see `specs/rust-rewrite/DESIGN.md` §2.6: "Sketch/hash determinism ... a
//! faithful reimplementation of `cf-alg-hashutil` (Splitmix64, Mix64, fixed
//! seeds), **bit-identical**, not a dependency swap").
//!
//! The hash machinery (`mix64`, `generate_seeds`, `fnv64a`) is provided by the
//! [`cf_alg_hashutil`] crate, itself a bit-identical port of
//! `pkg/alg/internal/hashutil`.
//!
//! # Thread safety
//!
//! The Go [`Sketch`] is thread-safe via a `sync.RWMutex`. Here, all mutable
//! state lives behind a [`std::sync::RwLock`], so a [`Sketch`] can be shared
//! across threads (`Send + Sync`) and concurrently mutated through `&self`,
//! exactly like the Go type. `Add` takes a write lock; `Count`/`TotalCount`
//! take read locks.
//!
//! # Wrapping arithmetic
//!
//! Counter and total accumulation use `i64` and `wrapping_add` to match Go's
//! two's-complement `int64` overflow semantics (Go wraps; Rust panics in debug
//! and wraps in release, so `wrapping_add` guarantees identical behavior in
//! every build profile). In practice counts stay far below the overflow point,
//! but the wrapping keeps parity at the edge.
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
/// Mirrors the package-level `ErrInvalidEpsilon` / `ErrInvalidDelta` sentinels
/// in Go. The [`Display`](core::fmt::Display) strings are byte-identical to the
/// Go `errors.New` messages so any surfaced error text matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `epsilon` was not positive.
    ///
    /// Mirrors `cms.ErrInvalidEpsilon` (`"cms: epsilon must be positive"`).
    InvalidEpsilon,

    /// `delta` was not in the open interval `(0, 1)`.
    ///
    /// Mirrors `cms.ErrInvalidDelta`
    /// (`"cms: delta must be in the open interval (0, 1)"`).
    InvalidDelta,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            Error::InvalidEpsilon => "cms: epsilon must be positive",
            Error::InvalidDelta => "cms: delta must be in the open interval (0, 1)",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for Error {}

/// Mutable state of a [`Sketch`], guarded by a single [`RwLock`].
///
/// Grouping `counters` and `total_count` under one lock reproduces Go's single
/// `sync.RWMutex` guarding both fields: a writer sees a consistent snapshot and
/// `TotalCount` reflects exactly the additions visible in `counters`.
#[derive(Debug)]
struct State {
    /// Flattened 2D array: `depth` rows by `width` columns, row-major.
    counters: Vec<i64>,
    /// Sum of all counts added to the sketch.
    total_count: i64,
}

/// A thread-safe Count-Min Sketch for frequency estimation.
///
/// Mirrors the Go `cms.Sketch`. Construct with [`Sketch::new`] from desired
/// error bounds; mutate with [`Sketch::add`]; query with [`Sketch::count`] and
/// [`Sketch::total_count`]; clear with [`Sketch::reset`].
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
    /// Euler's number and `ln` the natural logarithm — matching Go's
    /// `math.Ceil(math.E / epsilon)` and `math.Ceil(math.Log(1 / delta))`.
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

        // Match Go's uint(math.Ceil(...)) conversion. Go truncates the f64
        // toward zero when converting to uint; ceil has already made the value
        // a non-negative whole number, so `as usize` reproduces it exactly.
        let width = (std::f64::consts::E / epsilon).ceil() as usize;
        let depth = (1.0_f64 / delta).ln().ceil() as usize;

        // CMS-style seeds use the Mix64 finalizer (cf. Go: GenerateSeeds(depth, Mix64)).
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
    ///
    /// Mirrors `Sketch.Width()`.
    #[inline]
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the number of rows (hash functions) in the sketch.
    ///
    /// Mirrors `Sketch.Depth()`.
    #[inline]
    #[must_use]
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Increments the counter for `key` by `count`.
    ///
    /// A `count` of zero is a no-op (it does not even touch the total),
    /// matching Go. Negative counts are permitted (as in Go's `int64`
    /// parameter), though Count-Min's non-underestimation guarantee only holds
    /// for positive-only additions.
    ///
    /// Mirrors `Sketch.Add(key, count)`.
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
    /// Mirrors `Sketch.Count(key)`.
    ///
    /// # Panics
    ///
    /// Panics only if the internal lock is poisoned.
    #[must_use]
    pub fn count(&self, key: &[u8]) -> i64 {
        let st = self.state.read().expect("cms: lock poisoned");

        // Start from i64::MAX so the first row always lowers it, mirroring Go's
        // `minVal := int64(math.MaxInt64)`.
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
    /// Mirrors `Sketch.TotalCount()`.
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
    /// Mirrors `Sketch.Reset()`.
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
    /// Reproduces Go's `hashKey` exactly: write the row seed as an 8-byte
    /// **little-endian** prefix (Go: `binary.LittleEndian.PutUint64`), then the
    /// key bytes, hash the concatenation with FNV-1a/64, and reduce modulo
    /// `width`.
    #[inline]
    fn hash_key(&self, row: usize, key: &[u8]) -> usize {
        // Seed as 8-byte little-endian prefix for per-row independence.
        let seed_buf = self.seeds[row].to_le_bytes();

        // FNV-1a over (seed_buf || key). Computing the hash incrementally over
        // the two slices is identical to hashing their concatenation, because
        // FNV-1a folds one byte at a time with no length framing.
        let mut h = fnv64a(&seed_buf);
        h = fnv64a_continue(h, key);

        // Go computes `uint(h.Sum64()) % s.width`. On the 64-bit targets
        // codefang runs on, Go's `uint` is 64-bit, so this is the full 64-bit
        // hash reduced modulo width.
        (h % self.width as u64) as usize
    }
}

/// Continues an in-progress FNV-1a/64 hash over additional bytes.
///
/// FNV-1a has no finalization step and no length framing, so feeding bytes in
/// two passes (`fnv64a(prefix)` then `fnv64a_continue(h, rest)`) yields the same
/// 64-bit value as hashing the concatenation `prefix || rest` in one pass —
/// which is what Go's `h.Write(seedBuf); h.Write(key)` does.
#[inline]
fn fnv64a_continue(mut h: u64, data: &[u8]) -> u64 {
    /// FNV-1a 64-bit prime (matches Go's `hash/fnv` `prime64`).
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
    const CONC_GOROUTINES: usize = 100;
    const CONC_OPS_PER_G: usize = 1000;

    // Overestimation test parameters.
    const OVEREST_N: usize = 10_000;
    const OVEREST_FREQ: i64 = 100;

    /// Converts a `u64` to an 8-byte big-endian slice.
    ///
    /// Ported from the Go test helper `uint64ToBytes`, which uses
    /// `binary.BigEndian.PutUint64`. (Note: this is the *key* encoding used by
    /// the tests; it is independent of the little-endian *seed* encoding inside
    /// `hash_key`.)
    fn u64_to_bytes(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    /// Generates a deterministic test key from a prefix and index.
    ///
    /// Ported from the Go test helper `testKey` (`fmt.Appendf(nil, "%s-%d", ...)`).
    fn test_key(prefix: &str, idx: usize) -> Vec<u8> {
        format!("{prefix}-{idx}").into_bytes()
    }

    // Ported from TestNew_Parameters.
    #[test]
    fn new_parameters() {
        let cases = [
            ("standard", STANDARD_EPSILON, STANDARD_DELTA, EXPECTED_WIDTH, EXPECTED_DEPTH),
            ("loose", LOOSE_EPSILON, LOOSE_DELTA, LOOSE_WIDTH, LOOSE_DEPTH),
        ];
        for (name, eps, delta, want_w, want_d) in cases {
            let sk = Sketch::new(eps, delta).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(sk.width(), want_w, "{name}: width");
            assert_eq!(sk.depth(), want_d, "{name}: depth");
        }
    }

    // Ported from TestNew_EdgeCases.
    #[test]
    fn new_edge_cases() {
        assert_eq!(Sketch::new(0.0, STANDARD_DELTA).unwrap_err(), Error::InvalidEpsilon);
        assert_eq!(Sketch::new(-0.01, STANDARD_DELTA).unwrap_err(), Error::InvalidEpsilon);
        assert_eq!(Sketch::new(STANDARD_EPSILON, 0.0).unwrap_err(), Error::InvalidDelta);
        assert_eq!(Sketch::new(STANDARD_EPSILON, -0.01).unwrap_err(), Error::InvalidDelta);
        assert_eq!(Sketch::new(STANDARD_EPSILON, 1.0).unwrap_err(), Error::InvalidDelta);
        assert_eq!(Sketch::new(STANDARD_EPSILON, 1.5).unwrap_err(), Error::InvalidDelta);
    }

    // The error Display strings must match Go's errors.New text byte-for-byte.
    #[test]
    fn error_messages_match_go() {
        assert_eq!(Error::InvalidEpsilon.to_string(), "cms: epsilon must be positive");
        assert_eq!(
            Error::InvalidDelta.to_string(),
            "cms: delta must be in the open interval (0, 1)"
        );
    }

    // Ported from TestAdd_Count_SingleKey.
    #[test]
    fn add_count_single_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        let key = b"token-operator";
        let add_count: i64 = 42;
        sk.add(key, add_count);
        assert!(sk.count(key) >= add_count, "CMS count must be >= true count");
    }

    // Ported from TestAdd_Count_MultipleKeys.
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

    // Ported from TestCount_NeverAdded.
    #[test]
    fn count_never_added() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(b"exists", 100);
        assert!(sk.count(b"never-added") >= 0, "count of absent key must be >= 0");
    }

    // Ported from TestOverestimation_Bounded.
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

    // Ported from TestAdd_ZeroCount.
    #[test]
    fn add_zero_count() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(b"key", 0);
        assert_eq!(sk.total_count(), 0);
        assert_eq!(sk.count(b"key"), 0);
    }

    // Ported from TestNilKey: the Rust equivalent of a Go nil key is an empty
    // slice; it must not panic.
    #[test]
    fn nil_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(&[], 5);
        assert!(sk.count(&[]) >= 5);
    }

    // Ported from TestEmptySliceKey.
    #[test]
    fn empty_slice_key() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        sk.add(&[], 3);
        assert!(sk.count(&[]) >= 3);
    }

    // Ported from TestReset.
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

    // Ported from TestTotalCount.
    #[test]
    fn total_count() {
        let sk = Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap();
        assert_eq!(sk.total_count(), 0);
        sk.add(b"a", 10);
        sk.add(b"b", 20);
        sk.add(b"a", 5);
        assert_eq!(sk.total_count(), 35);
    }

    // Ported from TestDeterminism: two independently constructed sketches use
    // the same fixed seeds, so estimates must match exactly.
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
            assert_eq!(sk1.count(&key), sk2.count(&key), "determinism violated for key {i}");
        }
    }

    // Ported from TestConcurrent_AddCount: many threads add disjoint keys
    // concurrently while reading; the total must be exact.
    #[test]
    fn concurrent_add_count() {
        let sk = Arc::new(Sketch::new(LOOSE_EPSILON, LOOSE_DELTA).unwrap());
        let mut handles = Vec::with_capacity(CONC_GOROUTINES);
        for g in 0..CONC_GOROUTINES {
            let sk = Arc::clone(&sk);
            handles.push(thread::spawn(move || {
                for i in 0..CONC_OPS_PER_G {
                    let key = u64_to_bytes((g * CONC_OPS_PER_G + i) as u64);
                    sk.add(&key, 1);
                }
                // Read while others are writing.
                let _ = sk.count(&u64_to_bytes(g as u64));
            }));
        }
        for h in handles {
            h.join().expect("thread panicked");
        }
        let expected_total = (CONC_GOROUTINES * CONC_OPS_PER_G) as i64;
        assert_eq!(sk.total_count(), expected_total);
    }

    // Ported from TestMemoryUsage: the counter array sizing is width*depth.
    #[test]
    fn memory_usage() {
        let sk = Sketch::new(STANDARD_EPSILON, STANDARD_DELTA).unwrap();
        assert_eq!(sk.width(), EXPECTED_WIDTH);
        assert_eq!(sk.depth(), EXPECTED_DEPTH);
        // Counter array should be width * depth * 8 bytes (i64).
        let expected_bytes = EXPECTED_WIDTH * EXPECTED_DEPTH * 8;
        assert_eq!(expected_bytes, 2719 * 7 * 8);
    }

    // Ported from TestMultipleAddsAccumulate.
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
