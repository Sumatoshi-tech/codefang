//! A space-efficient probabilistic set-membership filter.
//!
//! A Bloom filter answers "definitely not in set" or "possibly in set" with a
//! tunable false-positive rate. It is useful as a pre-filter to avoid expensive
//! exact lookups (map access, lock acquisition, disk I/O).
//!
//! This implementation uses the double-hashing technique from Kirsch and
//! Mitzenmacher (2006): two base hashes derive `k` bit positions via
//! `h(i) = h1 + i*h2 mod m`, avoiding `k` independent hash functions.
//!
//! Compatibility: filter behavior is pinned to the reference implementation
//! (differential gate: `tests/compat`). Concretely:
//!
//! * `optimal_m` / `optimal_k` use the reference sizing formulas, including
//!   the `f64`-to-integer truncation, so [`Filter::bit_count`] and
//!   [`Filter::hash_count`] are deterministic and reproducible.
//! * The hash kernel is **FNV-128a**, splitting the 128-bit digest into two
//!   big-endian 64-bit halves, with the second half forced odd. The chosen bit
//!   positions — and therefore membership behavior — are bit-stable for any
//!   input.
//! * [`Filter::to_binary`] / [`Filter::from_binary`] use a frozen byte layout:
//!   `[m: u64 BE][k: u64 BE][count: u64 BE][bits: u64 BE...]`.
//!
//! # Thread safety
//!
//! The bare [`Filter`] has no interior mutability: mutating methods take
//! `&mut self`, and the type can be wrapped in a [`std::sync::RwLock`] when
//! shared. For a drop-in concurrent variant where every method takes `&self`,
//! use [`SyncFilter`], which embeds the lock internally.
//!
//! # Example
//!
//! ```
//! use cf_alg_bloom::Filter;
//!
//! let mut f = Filter::new_with_estimates(1000, 0.01).unwrap();
//! f.add(b"hello");
//! assert!(f.test(b"hello"));
//! assert!(!f.test(b"world")); // overwhelmingly likely false
//! ```

#![forbid(unsafe_code)]

use std::sync::RwLock;

/// Number of bits in each `u64` word.
const BITS_PER_WORD: u64 = 64;

/// `ln(2)` squared, used in the optimal bit-array size formula.
///
/// Computed from [`std::f64::consts::LN_2`] so the sizing math is
/// bit-reproducible.
const LN2_SQUARED: f64 = std::f64::consts::LN_2 * std::f64::consts::LN_2;

/// Byte size of the serialized header (`m` + `k` + `count`).
const BLOOM_HEADER_SIZE: usize = 24;

/// Byte size of a single `u64` word.
const UINT64_SIZE: usize = 8;

/// Errors returned by [`Filter`] construction and deserialization.
///
/// The [`std::fmt::Display`] texts are part of the CLI/log compatibility
/// contract and must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BloomError {
    /// `n` (expected element count) was zero.
    #[error("bloom: n must be positive")]
    ZeroN,
    /// `fp` was not in the open interval `(0, 1)`.
    #[error("bloom: fp must be in the open interval (0, 1)")]
    InvalidFp,
    /// Binary data was shorter than the fixed header.
    #[error("bloom: binary data too short")]
    BinaryDataTooShort,
    /// Binary payload length did not match the declared word count.
    #[error("bloom: binary data length mismatch")]
    BinaryDataLenMismatch,
}

/// A Bloom filter.
///
/// All mutating methods take `&mut self`; read-only methods take `&self`. For
/// a shared, internally-synchronized variant, see [`SyncFilter`].
#[derive(Debug, Clone)]
pub struct Filter {
    bits: Vec<u64>,
    /// Total bits.
    m: u64,
    /// Number of hash functions.
    k: u64,
    /// Approximate number of added elements.
    count: u64,
}

impl Filter {
    /// Creates a Bloom filter sized for `n` expected elements at a
    /// false-positive rate of `fp`.
    ///
    /// The bit-array size `m` and hash-function count `k` are computed with
    /// fixed formulas (including a deliberate `f64`-to-integer truncation), so
    /// [`bit_count`](Self::bit_count) and [`hash_count`](Self::hash_count) are
    /// fully determined by `n` and `fp`.
    ///
    /// # Errors
    ///
    /// Returns [`BloomError::ZeroN`] if `n` is zero, or
    /// [`BloomError::InvalidFp`] if `fp` is not in the open interval `(0, 1)`.
    pub fn new_with_estimates(n: u64, fp: f64) -> Result<Self, BloomError> {
        if n == 0 {
            return Err(BloomError::ZeroN);
        }

        if fp <= 0.0 || fp >= 1.0 {
            return Err(BloomError::InvalidFp);
        }

        let m = optimal_m(n, fp);
        let k = optimal_k(m, n);
        let words = m.div_ceil(BITS_PER_WORD);

        Ok(Self {
            bits: vec![0u64; words as usize],
            m,
            k,
            count: 0,
        })
    }

    /// Returns the size of the bit array in bits (`m`).
    #[inline]
    #[must_use]
    pub const fn bit_count(&self) -> u64 {
        self.m
    }

    /// Returns the number of hash functions used by the filter (`k`).
    #[inline]
    #[must_use]
    pub const fn hash_count(&self) -> u64 {
        self.k
    }

    /// Inserts `data` into the filter.
    pub fn add(&mut self, data: &[u8]) {
        let (h1, h2) = hash_kernel(data);
        set_bits(&mut self.bits, self.m, self.k, h1, h2);
        self.count += 1;
    }

    /// Reports whether `data` is possibly in the filter.
    ///
    /// A return value of `false` guarantees the element was never added. A
    /// return value of `true` means the element might have been added (subject
    /// to the configured false-positive rate).
    #[must_use]
    pub fn test(&self, data: &[u8]) -> bool {
        let (h1, h2) = hash_kernel(data);
        test_bits(&self.bits, self.m, self.k, h1, h2)
    }

    /// Tests for membership and then adds the element.
    ///
    /// Returns `true` if the element was possibly already present before this
    /// call.
    pub fn test_and_add(&mut self, data: &[u8]) -> bool {
        let (h1, h2) = hash_kernel(data);

        let mut present = true;

        for i in 0..self.k {
            let pos = (h1.wrapping_add(i.wrapping_mul(h2))) % self.m;
            let word_idx = (pos / BITS_PER_WORD) as usize;
            let bit_mask = 1u64 << (pos % BITS_PER_WORD);

            if self.bits[word_idx] & bit_mask == 0 {
                present = false;
                self.bits[word_idx] |= bit_mask;
            }
        }

        self.count += 1;

        present
    }

    /// Inserts multiple elements into the filter.
    ///
    /// An empty `items` slice is a no-op.
    pub fn add_bulk(&mut self, items: &[&[u8]]) {
        if items.is_empty() {
            return;
        }

        for item in items {
            let (h1, h2) = hash_kernel(item);
            set_bits(&mut self.bits, self.m, self.k, h1, h2);
            self.count += 1;
        }
    }

    /// Tests multiple elements for membership.
    ///
    /// Returns `None` when `items` is empty; otherwise returns a `Vec<bool>`
    /// the same length as `items`, where each entry indicates possible
    /// presence.
    #[must_use]
    pub fn test_bulk(&self, items: &[&[u8]]) -> Option<Vec<bool>> {
        if items.is_empty() {
            return None;
        }

        let results = items
            .iter()
            .map(|item| {
                let (h1, h2) = hash_kernel(item);
                test_bits(&self.bits, self.m, self.k, h1, h2)
            })
            .collect();

        Some(results)
    }

    /// Returns an approximation of the number of elements added to the filter.
    #[inline]
    #[must_use]
    pub const fn estimated_count(&self) -> u64 {
        self.count
    }

    /// Returns the fraction of bits that are set, in the range `[0, 1]`.
    #[must_use]
    pub fn fill_ratio(&self) -> f64 {
        let total: u64 = self.bits.iter().map(|w| u64::from(w.count_ones())).sum();
        total as f64 / self.m as f64
    }

    /// Encodes the filter into a binary format.
    ///
    /// Layout (frozen compatibility contract):
    /// `[m: u64 BE][k: u64 BE][count: u64 BE][bits: u64 BE ...]`.
    #[must_use]
    pub fn to_binary(&self) -> Vec<u8> {
        let mut buf = vec![0u8; BLOOM_HEADER_SIZE + self.bits.len() * UINT64_SIZE];
        buf[0..UINT64_SIZE].copy_from_slice(&self.m.to_be_bytes());
        buf[UINT64_SIZE..2 * UINT64_SIZE].copy_from_slice(&self.k.to_be_bytes());
        buf[2 * UINT64_SIZE..BLOOM_HEADER_SIZE].copy_from_slice(&self.count.to_be_bytes());

        for (i, word) in self.bits.iter().enumerate() {
            let start = BLOOM_HEADER_SIZE + i * UINT64_SIZE;
            buf[start..start + UINT64_SIZE].copy_from_slice(&word.to_be_bytes());
        }

        buf
    }

    /// Decodes a filter from the binary format produced by
    /// [`to_binary`](Self::to_binary).
    ///
    /// # Errors
    ///
    /// Returns [`BloomError::BinaryDataTooShort`] if `data` is shorter than the
    /// fixed header, or [`BloomError::BinaryDataLenMismatch`] if the payload
    /// length does not match the declared word count.
    pub fn from_binary(data: &[u8]) -> Result<Self, BloomError> {
        if data.len() < BLOOM_HEADER_SIZE {
            return Err(BloomError::BinaryDataTooShort);
        }

        let m = read_be_u64(&data[0..UINT64_SIZE]);
        let k = read_be_u64(&data[UINT64_SIZE..2 * UINT64_SIZE]);
        let count = read_be_u64(&data[2 * UINT64_SIZE..BLOOM_HEADER_SIZE]);

        let words = m.div_ceil(BITS_PER_WORD);

        if (data.len() - BLOOM_HEADER_SIZE) as u64 != words * UINT64_SIZE as u64 {
            return Err(BloomError::BinaryDataLenMismatch);
        }

        let mut bits = vec![0u64; words as usize];
        for (i, slot) in bits.iter_mut().enumerate() {
            let start = BLOOM_HEADER_SIZE + i * UINT64_SIZE;
            *slot = read_be_u64(&data[start..start + UINT64_SIZE]);
        }

        Ok(Self { bits, m, k, count })
    }

    /// Clears the filter without reallocating the bit array.
    pub fn reset(&mut self) {
        for w in &mut self.bits {
            *w = 0;
        }
        self.count = 0;
    }
}

/// Sets the `k` bit positions derived from `h1` and `h2` in the bit array.
fn set_bits(arr: &mut [u64], m: u64, k: u64, h1: u64, h2: u64) {
    for i in 0..k {
        let pos = (h1.wrapping_add(i.wrapping_mul(h2))) % m;
        arr[(pos / BITS_PER_WORD) as usize] |= 1u64 << (pos % BITS_PER_WORD);
    }
}

/// Returns `true` if all `k` bit positions derived from `h1` and `h2` are set.
fn test_bits(arr: &[u64], m: u64, k: u64, h1: u64, h2: u64) -> bool {
    for i in 0..k {
        let pos = (h1.wrapping_add(i.wrapping_mul(h2))) % m;
        if arr[(pos / BITS_PER_WORD) as usize] & (1u64 << (pos % BITS_PER_WORD)) == 0 {
            return false;
        }
    }
    true
}

/// Computes the optimal bit-array size for `n` elements at false-positive rate
/// `fp` using the formula `m = ceil(-n * ln(fp) / ln(2)^2)`.
///
/// The `f64`-to-integer conversion deliberately truncates toward zero
/// (reference-implementation behavior).
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn optimal_m(n: u64, fp: f64) -> u64 {
    (-(n as f64) * fp.ln() / LN2_SQUARED).ceil() as u64
}

/// Computes the optimal number of hash functions using the formula
/// `k = round(m/n * ln(2))`, clamped to a minimum of 1.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn optimal_k(m: u64, n: u64) -> u64 {
    let k = (m as f64 / n as f64 * std::f64::consts::LN_2).round() as u64;
    k.max(1)
}

/// Computes two independent 64-bit hashes from `data` using FNV-128a.
///
/// The 128-bit digest is split into two big-endian 64-bit halves. The second
/// half is forced odd so the step through the bit array is coprime with any
/// even `m`.
fn hash_kernel(data: &[u8]) -> (u64, u64) {
    let digest = fnv128a(data);

    let h1 = read_be_u64(&digest[0..8]);
    let mut h2 = read_be_u64(&digest[8..16]);

    // Force h2 odd so gcd(h2, m) avoids degenerate cycling.
    h2 |= 1;

    (h1, h2)
}

/// FNV-128a (Fowler–Noll–Vo, 128-bit, variant 1a) with the canonical
/// constants:
/// * offset basis `0x6c62272e07bb014262b821756295c58d`;
/// * prime `0x0000000001000000000000000000013b` (`2^88 + 2^8 + 0x3b`).
///
/// For each input byte: XOR into the low-order octet, then multiply the 128-bit
/// accumulator by the FNV prime modulo `2^128`. The digest is serialized
/// big-endian.
fn fnv128a(data: &[u8]) -> [u8; 16] {
    // 128-bit FNV offset basis and prime (canonical constants).
    const OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    let mut hash: u128 = OFFSET_BASIS;
    for &byte in data {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }

    hash.to_be_bytes()
}

/// Reads a big-endian `u64` from the first 8 bytes of `b`.
#[inline]
fn read_be_u64(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[0..8]);
    u64::from_be_bytes(a)
}

/// A thread-safe wrapper around [`Filter`] where every method takes `&self`
/// and synchronization is internal.
///
/// Read methods take a read lock; mutating methods take a write lock.
#[derive(Debug)]
pub struct SyncFilter {
    inner: RwLock<Filter>,
}

impl SyncFilter {
    /// Creates a [`SyncFilter`]; see [`Filter::new_with_estimates`].
    ///
    /// # Errors
    ///
    /// See [`Filter::new_with_estimates`].
    pub fn new_with_estimates(n: u64, fp: f64) -> Result<Self, BloomError> {
        Ok(Self {
            inner: RwLock::new(Filter::new_with_estimates(n, fp)?),
        })
    }

    /// See [`Filter::bit_count`].
    #[must_use]
    pub fn bit_count(&self) -> u64 {
        self.inner.read().expect("bloom lock poisoned").bit_count()
    }

    /// See [`Filter::hash_count`].
    #[must_use]
    pub fn hash_count(&self) -> u64 {
        self.inner.read().expect("bloom lock poisoned").hash_count()
    }

    /// See [`Filter::add`].
    pub fn add(&self, data: &[u8]) {
        self.inner.write().expect("bloom lock poisoned").add(data);
    }

    /// See [`Filter::test`].
    #[must_use]
    pub fn test(&self, data: &[u8]) -> bool {
        self.inner.read().expect("bloom lock poisoned").test(data)
    }

    /// See [`Filter::test_and_add`].
    pub fn test_and_add(&self, data: &[u8]) -> bool {
        self.inner
            .write()
            .expect("bloom lock poisoned")
            .test_and_add(data)
    }

    /// See [`Filter::add_bulk`].
    pub fn add_bulk(&self, items: &[&[u8]]) {
        self.inner
            .write()
            .expect("bloom lock poisoned")
            .add_bulk(items);
    }

    /// See [`Filter::test_bulk`].
    #[must_use]
    pub fn test_bulk(&self, items: &[&[u8]]) -> Option<Vec<bool>> {
        self.inner
            .read()
            .expect("bloom lock poisoned")
            .test_bulk(items)
    }

    /// See [`Filter::estimated_count`].
    #[must_use]
    pub fn estimated_count(&self) -> u64 {
        self.inner
            .read()
            .expect("bloom lock poisoned")
            .estimated_count()
    }

    /// See [`Filter::fill_ratio`].
    #[must_use]
    pub fn fill_ratio(&self) -> f64 {
        self.inner.read().expect("bloom lock poisoned").fill_ratio()
    }

    /// See [`Filter::to_binary`].
    #[must_use]
    pub fn to_binary(&self) -> Vec<u8> {
        self.inner.read().expect("bloom lock poisoned").to_binary()
    }

    /// See [`Filter::from_binary`].
    ///
    /// # Errors
    ///
    /// See [`Filter::from_binary`].
    pub fn from_binary(data: &[u8]) -> Result<Self, BloomError> {
        Ok(Self {
            inner: RwLock::new(Filter::from_binary(data)?),
        })
    }

    /// See [`Filter::reset`].
    pub fn reset(&self) {
        self.inner.write().expect("bloom lock poisoned").reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converts a `u64` to an 8-byte big-endian array.
    fn uint64_to_bytes(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    /// Generates a deterministic test key from a prefix and index.
    fn test_key(prefix: &str, idx: i32) -> Vec<u8> {
        format!("{prefix}-{idx}").into_bytes()
    }

    // ---- Hash-parity fixtures ----------------------------------------------
    //
    // These golden vectors were computed with the reference implementation's
    // FNV-128a and pin our kernel byte-for-byte. If these pass, the chosen bit
    // positions — and thus all membership behavior — match the reference
    // binary.

    fn fnv128a_hex(data: &[u8]) -> String {
        let d = fnv128a(data);
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Independent, obviously-correct FNV-128a reference written from the
    /// published definition. If it agrees with the production [`fnv128a`]
    /// across a corpus, the production kernel is correct — without relying on
    /// hand-transcribed golden hex.
    fn fnv128a_reference(data: &[u8]) -> [u8; 16] {
        // FNV-128 offset basis and prime (canonical constants).
        let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
        const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
        for &b in data {
            hash ^= u128::from(b);
            hash = hash.wrapping_mul(PRIME);
        }
        hash.to_be_bytes()
    }

    #[test]
    fn fnv128a_empty_is_offset_basis() {
        // FNV-128a("") is, by definition, the FNV-128 offset basis. This anchor
        // is certain regardless of any external golden vectors.
        assert_eq!(fnv128a_hex(b""), "6c62272e07bb014262b821756295c58d");
    }

    #[test]
    fn fnv128a_matches_independent_reference() {
        let corpus: &[&[u8]] = &[
            b"",
            b"a",
            b"ab",
            b"abc",
            b"foobar",
            b"hello world",
            b"\x00\x01\x02\xff\xfe",
            b"The quick brown fox jumps over the lazy dog",
            &[0xAA; 256],
        ];
        for &input in corpus {
            assert_eq!(
                fnv128a(input),
                fnv128a_reference(input),
                "FNV-128a mismatch for input len {}",
                input.len()
            );
        }
    }

    /// Well-published FNV-1a 128-bit test vectors. Kept as a hard golden so a
    /// silent regression in *both* implementations would still be caught.
    #[test]
    fn fnv128a_known_vectors() {
        assert_eq!(fnv128a_hex(b"a"), "d228cb696f1a8caf78912b704e4a8964");
        assert_eq!(fnv128a_hex(b"foobar"), "343e1662793c64bf6f0d3597ba446f18");
    }

    // ---- Behavioral suite ---------------------------------------------------

    const STANDARD_N: u64 = 10_000_000;
    const STANDARD_FP: f64 = 0.01;
    const SMALL_N: u64 = 1000;
    const TIGHT_N: u64 = 100;
    const TIGHT_FP: f64 = 0.001;
    const FP_TEST_N: u64 = 100_000;
    const FP_TEST_FP: f64 = 0.01;
    const FP_TEST_PROBE_N: u64 = 200_000;
    const FP_MARGIN: f64 = 1.5;

    const EXPECTED_M_10M_1PCT: u64 = 95_850_584;
    const EXPECTED_K_10M_1PCT: u64 = 7;
    const EXPECTED_M_1K_1PCT: u64 = 9586;
    const EXPECTED_K_1K_1PCT: u64 = 7;
    const EXPECTED_M_100_01PCT: u64 = 1438;
    const EXPECTED_K_100_01PCT: u64 = 10;

    #[test]
    fn new_with_estimates_parameters() {
        let cases = [
            (
                STANDARD_N,
                STANDARD_FP,
                EXPECTED_M_10M_1PCT,
                EXPECTED_K_10M_1PCT,
            ),
            (SMALL_N, STANDARD_FP, EXPECTED_M_1K_1PCT, EXPECTED_K_1K_1PCT),
            (
                TIGHT_N,
                TIGHT_FP,
                EXPECTED_M_100_01PCT,
                EXPECTED_K_100_01PCT,
            ),
        ];
        for (n, fp, want_m, want_k) in cases {
            let f = Filter::new_with_estimates(n, fp).unwrap();
            assert_eq!(f.bit_count(), want_m, "m for n={n} fp={fp}");
            assert_eq!(f.hash_count(), want_k, "k for n={n} fp={fp}");
        }
    }

    #[test]
    fn new_with_estimates_edge_cases() {
        assert_eq!(
            Filter::new_with_estimates(0, STANDARD_FP).unwrap_err(),
            BloomError::ZeroN
        );
        assert_eq!(
            Filter::new_with_estimates(SMALL_N, 0.0).unwrap_err(),
            BloomError::InvalidFp
        );
        assert_eq!(
            Filter::new_with_estimates(SMALL_N, 1.0).unwrap_err(),
            BloomError::InvalidFp
        );
        assert_eq!(
            Filter::new_with_estimates(SMALL_N, 1.5).unwrap_err(),
            BloomError::InvalidFp
        );
        assert_eq!(
            Filter::new_with_estimates(SMALL_N, -0.01).unwrap_err(),
            BloomError::InvalidFp
        );
    }

    #[test]
    fn add_test_no_false_negatives() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        for i in 0..SMALL_N {
            f.add(&uint64_to_bytes(i));
        }
        for i in 0..SMALL_N {
            assert!(
                f.test(&uint64_to_bytes(i)),
                "false negative for element {i}"
            );
        }
    }

    #[test]
    fn test_definite_absence() {
        let f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        assert!(!f.test(b"never-added"));
        assert!(!f.test(&uint64_to_bytes(42)));
    }

    #[test]
    fn test_and_add_first_and_second_call() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        let data = b"unique-element";
        assert!(!f.test_and_add(data));
        assert!(f.test_and_add(data));
    }

    #[test]
    fn add_bulk_test_bulk() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        const BULK_SIZE: usize = 500;
        let owned: Vec<[u8; 8]> = (0..BULK_SIZE as u64).map(uint64_to_bytes).collect();
        let items: Vec<&[u8]> = owned.iter().map(|b| b.as_slice()).collect();
        f.add_bulk(&items);

        let results = f.test_bulk(&items).expect("non-empty");
        assert_eq!(results.len(), BULK_SIZE);
        for (i, present) in results.iter().enumerate() {
            assert!(present, "false negative in bulk test for element {i}");
        }
    }

    #[test]
    fn add_bulk_empty_slice() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        f.add_bulk(&[]);
        assert_eq!(f.estimated_count(), 0);
    }

    #[test]
    fn test_bulk_empty_slice() {
        let f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        assert!(f.test_bulk(&[]).is_none());
    }

    #[test]
    fn estimated_count() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        assert_eq!(f.estimated_count(), 0);
        const INSERT_COUNT: u64 = 42;
        for i in 0..INSERT_COUNT {
            f.add(&uint64_to_bytes(i));
        }
        assert_eq!(f.estimated_count(), INSERT_COUNT);
    }

    #[test]
    fn reset() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        let data = b"to-be-reset";
        f.add(data);
        assert!(f.test(data));
        assert_eq!(f.estimated_count(), 1);

        f.reset();

        assert!(!f.test(data));
        assert_eq!(f.estimated_count(), 0);
        assert!(f.fill_ratio().abs() < 0.0001);
    }

    #[test]
    fn fill_ratio() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        assert!(f.fill_ratio().abs() < 0.0001);
        for i in 0..SMALL_N {
            f.add(&uint64_to_bytes(i));
        }
        let ratio = f.fill_ratio();
        assert!(ratio > 0.0);
        assert!(ratio <= 1.0);
    }

    #[test]
    fn nil_data() {
        // Adding the empty slice must behave like any other key and must not
        // panic.
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        f.add(&[]);
        assert!(f.test(&[]));

        let empty: &[u8] = b"";
        let mut f2 = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        f2.add(empty);
        assert!(f2.test(empty));
    }

    #[test]
    fn false_positive_rate() {
        let mut f = Filter::new_with_estimates(FP_TEST_N, FP_TEST_FP).unwrap();
        for i in 0..FP_TEST_N {
            f.add(&uint64_to_bytes(i));
        }

        let mut false_positives = 0u64;
        for i in FP_TEST_N..FP_TEST_N + FP_TEST_PROBE_N {
            if f.test(&uint64_to_bytes(i)) {
                false_positives += 1;
            }
        }

        let observed_rate = false_positives as f64 / FP_TEST_PROBE_N as f64;
        let max_allowed = FP_TEST_FP * FP_MARGIN;
        assert!(
            observed_rate <= max_allowed,
            "FP rate {observed_rate:.4} exceeds maximum {max_allowed:.4}"
        );
    }

    #[test]
    fn concurrent_add_test() {
        use std::sync::Arc;
        use std::thread;

        const CONC_THREADS: u64 = 100;
        const CONC_OPS_PER_THREAD: u64 = 1000;

        let f = Arc::new(
            SyncFilter::new_with_estimates(CONC_THREADS * CONC_OPS_PER_THREAD, STANDARD_FP)
                .unwrap(),
        );

        let handles: Vec<_> = (0..CONC_THREADS)
            .map(|g| {
                let f = Arc::clone(&f);
                thread::spawn(move || {
                    let base = g * CONC_OPS_PER_THREAD;
                    for i in 0..CONC_OPS_PER_THREAD {
                        f.add(&uint64_to_bytes(base + i));
                    }
                    for i in 0..CONC_OPS_PER_THREAD {
                        assert!(f.test(&uint64_to_bytes(base + i)));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(f.estimated_count(), CONC_THREADS * CONC_OPS_PER_THREAD);
    }

    #[test]
    fn memory_usage_10m_1pct() {
        let f = Filter::new_with_estimates(STANDARD_N, STANDARD_FP).unwrap();
        const MAX_BYTES: u64 = 15 * 1024 * 1024;
        let actual_bytes = f.bit_count() / 8;
        assert!(
            actual_bytes <= MAX_BYTES,
            "filter uses {actual_bytes} bytes, exceeding {MAX_BYTES} byte limit"
        );
    }

    #[test]
    fn test_bulk_mixed_presence() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        const HALF: i32 = 50;

        for i in 0..HALF {
            f.add(&test_key("member", i));
        }

        let mut owned: Vec<Vec<u8>> = Vec::with_capacity((HALF * 2) as usize);
        for i in 0..HALF {
            owned.push(test_key("member", i));
        }
        for i in 0..HALF {
            owned.push(test_key("nonmember", i));
        }
        let queries: Vec<&[u8]> = owned.iter().map(|v| v.as_slice()).collect();

        let results = f.test_bulk(&queries).expect("non-empty");
        assert_eq!(results.len(), (HALF * 2) as usize);
        for (i, present) in results.iter().take(HALF as usize).enumerate() {
            assert!(present, "member {i} should be present");
        }
    }

    // ---- Binary round-trip (serialized-layout contract) ---------------------

    #[test]
    fn binary_round_trip() {
        let mut f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        for i in 0..SMALL_N {
            f.add(&uint64_to_bytes(i));
        }

        let encoded = f.to_binary();
        // Header layout is byte-exact: m, k, count as big-endian u64.
        assert_eq!(read_be_u64(&encoded[0..8]), f.bit_count());
        assert_eq!(read_be_u64(&encoded[8..16]), f.hash_count());
        assert_eq!(read_be_u64(&encoded[16..24]), f.estimated_count());

        let decoded = Filter::from_binary(&encoded).unwrap();
        assert_eq!(decoded.bit_count(), f.bit_count());
        assert_eq!(decoded.hash_count(), f.hash_count());
        assert_eq!(decoded.estimated_count(), f.estimated_count());
        // Re-encoding must be byte-identical (deterministic).
        assert_eq!(decoded.to_binary(), encoded);

        // Membership preserved across the round trip.
        for i in 0..SMALL_N {
            assert!(decoded.test(&uint64_to_bytes(i)));
        }
    }

    #[test]
    fn binary_errors() {
        assert_eq!(
            Filter::from_binary(&[0u8; 4]).unwrap_err(),
            BloomError::BinaryDataTooShort
        );

        // Valid header claiming m bits but truncated payload.
        let f = Filter::new_with_estimates(SMALL_N, STANDARD_FP).unwrap();
        let mut encoded = f.to_binary();
        encoded.truncate(encoded.len() - 8); // drop one word
        assert_eq!(
            Filter::from_binary(&encoded).unwrap_err(),
            BloomError::BinaryDataLenMismatch
        );
    }
}
