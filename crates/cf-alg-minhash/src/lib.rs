//! MinHash signature generation for set similarity estimation.
//!
//! MinHash compresses a set of tokens or shingles into a compact fixed-size
//! signature. The Jaccard similarity between two sets can then be estimated by
//! comparing signatures in `O(k)` time, where `k` is the number of hash
//! functions (typically 128).
//!
//! Uses FNV-1a base hashing with per-hash-function seeds mixed via a
//! SplitMix64-derived finalizer to produce `k` independent hash values from a
//! single base hash computation. The hashing primitives are shared with the
//! other probabilistic data structures via the [`cf_alg_hashutil`] crate, so
//! signatures are bit-stable for the same inputs (reference-implementation
//! behavior, pinned by the parity vector below).
//!
//! All integer arithmetic uses wrapping operators (via [`cf_alg_hashutil`]) so
//! results are identical in every build profile. See
//! `specs/rust-rewrite/DESIGN.md` §2.6 for the determinism requirements.
//!
//! # Thread safety
//!
//! Mutation requires `&mut self`, which the borrow checker proves is exclusive
//! at compile time, so no runtime lock is needed for the single-owner case.
//! Callers that genuinely need to share a signature across threads wrap it in
//! their own `Mutex`/`RwLock`. The numeric results are identical regardless of
//! locking strategy.
//!
//! # Examples
//!
//! ```
//! use cf_alg_minhash::Signature;
//!
//! let mut a = Signature::new(128).unwrap();
//! let mut b = Signature::new(128).unwrap();
//! for tok in ["func", "main", "return"] {
//!     a.add(tok.as_bytes());
//!     b.add(tok.as_bytes());
//! }
//! // Identical token sets estimate to a Jaccard similarity of 1.0.
//! assert!((a.similarity(&b).unwrap() - 1.0).abs() < 0.001);
//! ```

use std::fmt;

use cf_alg_hashutil::{fnv64a, generate_seeds, mix_hash, splitmix64};

/// Number of bytes for the `num_hashes` `u32` length prefix in serialization.
pub const HEADER_SIZE: usize = 4;

/// Number of bytes per `u64` hash value in serialization.
pub const BYTES_PER_HASH: usize = 8;

/// Errors returned by [`Signature`] operations.
///
/// The message strings are part of the CLI/log compatibility contract and
/// must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MinHashError {
    /// `num_hashes` was zero.
    #[error("minhash: numHashes must be positive")]
    ZeroNumHashes,
    /// Two signatures being compared/merged have different sizes.
    #[error("minhash: signature sizes do not match")]
    SizeMismatch,
    /// A nil signature was provided.
    ///
    /// The type system makes "nil" unrepresentable for `&Signature`, so this
    /// variant exists only to keep the error surface complete.
    #[error("minhash: signature must not be nil")]
    NilSignature,
    /// Deserialization input was invalid.
    #[error("minhash: invalid serialized data")]
    InvalidData,
}

/// A MinHash signature for Jaccard similarity estimation.
///
/// Construct one with [`Signature::new`]. Each minimum is initialized to
/// [`u64::MAX`]. The seed table is generated deterministically (via
/// [`cf_alg_hashutil::generate_seeds`] with the [`cf_alg_hashutil::splitmix64`]
/// mixer), so two signatures constructed with the same `num_hashes` produce
/// identical results for the same tokens.
#[derive(Clone)]
pub struct Signature {
    mins: Vec<u64>,
    seeds: Vec<u64>,
}

impl Signature {
    /// Creates a new MinHash signature with the given number of hash functions.
    ///
    /// Each minimum is initialized to [`u64::MAX`].
    ///
    /// # Errors
    ///
    /// Returns [`MinHashError::ZeroNumHashes`] when `num_hashes == 0`.
    pub fn new(num_hashes: usize) -> Result<Self, MinHashError> {
        if num_hashes == 0 {
            return Err(MinHashError::ZeroNumHashes);
        }

        Ok(Self {
            mins: vec![u64::MAX; num_hashes],
            seeds: generate_seeds(num_hashes, splitmix64),
        })
    }

    /// Updates all hash function minimums with the given token.
    ///
    /// The base FNV-1a hash of `token` is mixed with each per-function seed; for
    /// each function the running minimum is lowered if the mixed value is
    /// smaller. An empty token is valid and still updates the minimums.
    pub fn add(&mut self, token: &[u8]) {
        let base_hash = fnv64a(token);
        for (min, &seed) in self.mins.iter_mut().zip(self.seeds.iter()) {
            let h = mix_hash(base_hash, seed);
            if h < *min {
                *min = h;
            }
        }
    }

    /// Returns the estimated Jaccard index between this signature and `other`.
    ///
    /// This is the fraction of matching minimum positions.
    ///
    /// # Errors
    ///
    /// Returns [`MinHashError::SizeMismatch`] if the signatures have different
    /// lengths.
    pub fn similarity(&self, other: &Self) -> Result<f64, MinHashError> {
        // Identity shortcut: a signature is always fully similar to itself.
        if std::ptr::eq(self, other) {
            return Ok(1.0);
        }

        if self.mins.len() != other.mins.len() {
            return Err(MinHashError::SizeMismatch);
        }

        let matches = self
            .mins
            .iter()
            .zip(other.mins.iter())
            .filter(|(a, b)| a == b)
            .count();

        Ok(matches as f64 / self.mins.len() as f64)
    }

    /// Merges `other` into this signature, taking the element-wise minimum.
    ///
    /// This is the set-union operation on MinHash signatures: the merged
    /// signature estimates the Jaccard similarity of the union of the two
    /// underlying sets.
    ///
    /// # Errors
    ///
    /// Returns [`MinHashError::SizeMismatch`] if the signatures have different
    /// lengths.
    pub fn merge(&mut self, other: &Self) -> Result<(), MinHashError> {
        // Identity shortcut: self-merge is a no-op.
        if std::ptr::eq(self, other) {
            return Ok(());
        }

        if self.mins.len() != other.mins.len() {
            return Err(MinHashError::SizeMismatch);
        }

        for (min, &o) in self.mins.iter_mut().zip(other.mins.iter()) {
            if o < *min {
                *min = o;
            }
        }

        Ok(())
    }

    /// Serializes the signature to a compact binary format.
    ///
    /// Format (frozen): `[num_hashes as u32 big-endian (4 bytes)] +
    /// [mins as []u64 big-endian]`.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(HEADER_SIZE + self.mins.len() * BYTES_PER_HASH);
        // num_hashes as u32 BE. The cast truncates by design; lengths beyond
        // u32 are not reachable in practice.
        data.extend_from_slice(&(self.mins.len() as u32).to_be_bytes());
        for &v in &self.mins {
            data.extend_from_slice(&v.to_be_bytes());
        }
        data
    }

    /// Deserializes a signature from the compact binary format produced by
    /// [`Signature::bytes`].
    ///
    /// The seed table is regenerated deterministically, so the restored
    /// signature compares/merges correctly with signatures of the same size.
    ///
    /// # Errors
    ///
    /// - [`MinHashError::InvalidData`] if `data` is shorter than [`HEADER_SIZE`]
    ///   or its length does not match the declared header.
    /// - [`MinHashError::ZeroNumHashes`] if the header declares zero hashes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, MinHashError> {
        if data.len() < HEADER_SIZE {
            return Err(MinHashError::InvalidData);
        }

        let header: [u8; HEADER_SIZE] = data[..HEADER_SIZE].try_into().expect("header slice");
        let num_hashes = u32::from_be_bytes(header) as usize;
        if num_hashes == 0 {
            return Err(MinHashError::ZeroNumHashes);
        }

        let expected_len = HEADER_SIZE + num_hashes * BYTES_PER_HASH;
        if data.len() != expected_len {
            return Err(MinHashError::InvalidData);
        }

        let mut mins = vec![0u64; num_hashes];
        for (i, slot) in mins.iter_mut().enumerate() {
            let offset = HEADER_SIZE + i * BYTES_PER_HASH;
            let chunk: [u8; BYTES_PER_HASH] = data[offset..offset + BYTES_PER_HASH]
                .try_into()
                .expect("8-byte chunk");
            *slot = u64::from_be_bytes(chunk);
        }

        Ok(Self {
            mins,
            seeds: generate_seeds(num_hashes, splitmix64),
        })
    }

    /// Resets all minimums back to [`u64::MAX`], emptying the signature while
    /// preserving its size and seeds.
    pub fn reset(&mut self) {
        for m in &mut self.mins {
            *m = u64::MAX;
        }
    }

    /// Returns `true` if no tokens have been added since construction or the
    /// last [`Signature::reset`] (all minimums are still [`u64::MAX`]).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mins.iter().all(|&v| v == u64::MAX)
    }

    /// Returns the number of hash functions in the signature.
    #[must_use]
    pub fn len(&self) -> usize {
        self.mins.len()
    }
}

// Manual Debug avoids dumping the large min/seed vectors while remaining
// useful.
impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signature")
            .field("num_hashes", &self.mins.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NUM_HASHES: usize = 128;
    const TEST_SMALL_NUM_HASHES: usize = 16;
    const TEST_OVERLAP_SET_SIZE: usize = 1000;
    const TEST_OVERLAP_TOLERANCE: f64 = 0.1;
    const TEST_DISJOINT_THRESHOLD: f64 = 0.1;

    fn approx(a: f64, b: f64, delta: f64) -> bool {
        (a - b).abs() <= delta
    }

    // --- Constructor Tests ---

    #[test]
    fn test_new_valid_num_hashes() {
        let sig = Signature::new(TEST_NUM_HASHES).expect("valid");
        assert_eq!(sig.len(), TEST_NUM_HASHES);
    }

    #[test]
    fn test_new_small_num_hashes() {
        let sig = Signature::new(1).expect("valid");
        assert_eq!(sig.len(), 1);
    }

    #[test]
    fn test_new_zero_num_hashes() {
        let err = Signature::new(0).unwrap_err();
        assert_eq!(err, MinHashError::ZeroNumHashes);
    }

    #[test]
    fn test_new_large_num_hashes() {
        let sig = Signature::new(1024).expect("valid");
        assert_eq!(sig.len(), 1024);
    }

    // --- Add Tests ---

    #[test]
    fn test_add_single_token() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(b"hello");
        assert!(!sig.is_empty(), "signature should not be empty after add");
    }

    #[test]
    fn test_add_nil_token() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        // Adding an empty token must not panic.
        sig.add(&[]);
    }

    #[test]
    fn test_add_empty_token() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(&[]);
        // An empty byte slice is a valid token and updates the minimums.
        assert!(!sig.is_empty());
    }

    // --- Similarity Tests ---

    #[test]
    fn test_similarity_identical() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        for tok in ["func", "main", "return", "if", "else"] {
            a.add(tok.as_bytes());
            b.add(tok.as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        assert!(approx(1.0, sim, 0.001), "identical sets sim = {sim}");
    }

    #[test]
    fn test_similarity_disjoint() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        for i in 0..TEST_OVERLAP_SET_SIZE {
            a.add(format!("tokenA_{i}").as_bytes());
            b.add(format!("tokenB_{i}").as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        assert!(sim < TEST_DISJOINT_THRESHOLD, "disjoint sim = {sim}");
    }

    #[test]
    fn test_similarity_partial_overlap() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        let half = TEST_OVERLAP_SET_SIZE / 2;
        for i in 0..half {
            let shared = format!("shared_{i}");
            a.add(shared.as_bytes());
            b.add(shared.as_bytes());
        }
        for i in 0..half {
            a.add(format!("uniqueA_{i}").as_bytes());
            b.add(format!("uniqueB_{i}").as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        // Jaccard = 500 / 1500 ≈ 0.333.
        assert!(
            approx(1.0 / 3.0, sim, TEST_OVERLAP_TOLERANCE),
            "partial overlap sim = {sim}"
        );
    }

    #[test]
    fn test_similarity_high_overlap() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        for i in 0..900 {
            let shared = format!("shared_{i}");
            a.add(shared.as_bytes());
            b.add(shared.as_bytes());
        }
        for i in 0..100 {
            a.add(format!("uniqueA_{i}").as_bytes());
            b.add(format!("uniqueB_{i}").as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        // Jaccard = 900 / 1100 ≈ 0.818.
        assert!(
            approx(900.0 / 1100.0, sim, TEST_OVERLAP_TOLERANCE),
            "high overlap sim = {sim}"
        );
    }

    #[test]
    fn test_similarity_size_mismatch() {
        let a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let b = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        let err = a.similarity(&b).unwrap_err();
        assert_eq!(err, MinHashError::SizeMismatch);
    }

    #[test]
    fn test_similarity_empty() {
        let a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let b = Signature::new(TEST_NUM_HASHES).expect("valid");
        let sim = a.similarity(&b).expect("ok");
        assert!(approx(1.0, sim, 0.001), "two empty sigs sim = {sim}");
    }

    #[test]
    fn test_similarity_self() {
        // The self-comparison shortcut must return exactly 1.0.
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        a.add(b"x");
        let sim = a.similarity(&a).expect("ok");
        assert_eq!(sim, 1.0);
    }

    // --- Merge Tests ---

    #[test]
    fn test_merge_basic() {
        let mut a = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        a.add(b"alpha");
        b.add(b"beta");
        a.merge(&b).expect("ok");

        let mut combined = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        combined.add(b"alpha");
        combined.add(b"beta");

        let sim = a.similarity(&combined).expect("ok");
        assert!(approx(1.0, sim, 0.001), "merged vs combined sim = {sim}");
    }

    #[test]
    fn test_merge_size_mismatch() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let b = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        let err = a.merge(&b).unwrap_err();
        assert_eq!(err, MinHashError::SizeMismatch);
    }

    #[test]
    fn test_merge_idempotent_with_clone() {
        // Merging a signature with a copy of itself must leave it unchanged,
        // exercising the element-wise-min logic on equal inputs. (A literal
        // self-merge cannot be expressed in safe Rust because it would alias
        // `&mut self` with `&other`; the `ptr::eq` guard in `merge` covers
        // that exact path, and this asserts the numeric invariant.)
        let mut a = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        a.add(b"x");
        let copy = a.clone();
        let before = a.bytes();
        a.merge(&copy).expect("ok");
        assert_eq!(before, a.bytes());
    }

    // --- Serialization Tests ---

    #[test]
    fn test_bytes_from_bytes_round_trip() {
        let mut sig = Signature::new(TEST_NUM_HASHES).expect("valid");
        sig.add(b"hello");
        sig.add(b"world");
        let data = sig.bytes();
        let restored = Signature::from_bytes(&data).expect("valid");
        assert_eq!(sig.len(), restored.len());
        let sim = sig.similarity(&restored).expect("ok");
        assert!(approx(1.0, sim, 0.001), "round-trip sim = {sim}");
    }

    #[test]
    fn test_from_bytes_invalid_data_too_short() {
        let err = Signature::from_bytes(&[1, 2]).unwrap_err();
        assert_eq!(err, MinHashError::InvalidData);
    }

    #[test]
    fn test_from_bytes_invalid_data_wrong_length() {
        // Header says 128 hashes but only 10 bytes of data follow.
        let mut data = vec![0u8; HEADER_SIZE + 10];
        data[3] = TEST_NUM_HASHES as u8;
        let err = Signature::from_bytes(&data).unwrap_err();
        assert_eq!(err, MinHashError::InvalidData);
    }

    #[test]
    fn test_from_bytes_zero_hashes() {
        let data = vec![0u8; HEADER_SIZE];
        let err = Signature::from_bytes(&data).unwrap_err();
        assert_eq!(err, MinHashError::ZeroNumHashes);
    }

    #[test]
    fn test_bytes_correct_size() {
        let sig = Signature::new(TEST_NUM_HASHES).expect("valid");
        let data = sig.bytes();
        assert_eq!(data.len(), HEADER_SIZE + TEST_NUM_HASHES * BYTES_PER_HASH);
    }

    // --- Reset Tests ---

    #[test]
    fn test_reset() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(b"token");
        assert!(!sig.is_empty());
        sig.reset();
        assert!(sig.is_empty(), "signature should be empty after reset");
    }

    #[test]
    fn test_is_empty_after_reset() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(b"token");
        sig.reset();
        // Every minimum should be back to u64::MAX in the serialized form.
        let data = sig.bytes();
        for i in 0..TEST_SMALL_NUM_HASHES {
            let offset = HEADER_SIZE + i * BYTES_PER_HASH;
            let chunk: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
            assert_eq!(u64::from_be_bytes(chunk), u64::MAX, "min[{i}] after reset");
        }
    }

    // --- Clone Tests ---

    #[test]
    fn test_clone() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(b"hello");
        let mut cloned = sig.clone();
        let sim = sig.similarity(&cloned).expect("ok");
        assert!(approx(1.0, sim, 0.001));

        // Modifying the clone must not affect the original.
        cloned.add(b"world");
        let sim2 = sig.similarity(&cloned).expect("ok");
        assert!(sim2 < 1.0, "clone should be independent, sim2 = {sim2}");
    }

    // --- IsEmpty Tests ---

    #[test]
    fn test_is_empty_new() {
        let sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        assert!(sig.is_empty());
    }

    #[test]
    fn test_is_empty_after_add() {
        let mut sig = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        sig.add(b"token");
        assert!(!sig.is_empty());
    }

    // --- Determinism Tests ---

    #[test]
    fn test_deterministic() {
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        for tok in ["func", "main", "return", "if", "else", "for", "range"] {
            a.add(tok.as_bytes());
            b.add(tok.as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        assert!(approx(1.0, sim, 0.001), "deterministic sim = {sim}");
    }

    #[test]
    fn test_seed_generation_deterministic() {
        let mut a = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_SMALL_NUM_HASHES).expect("valid");
        a.add(b"test");
        b.add(b"test");
        let sim = a.similarity(&b).expect("ok");
        assert!(approx(1.0, sim, 0.001), "deterministic seeds sim = {sim}");
    }

    // --- Len Tests ---

    #[test]
    fn test_len() {
        let sig = Signature::new(TEST_NUM_HASHES).expect("valid");
        assert_eq!(sig.len(), TEST_NUM_HASHES);
    }

    // --- Accuracy Tests ---

    #[test]
    fn test_accuracy_known_jaccard() {
        // A = element_0..element_99, B = element_50..element_149.
        // |A ∩ B| = 50, |A ∪ B| = 150, Jaccard = 1/3.
        let mut a = Signature::new(TEST_NUM_HASHES).expect("valid");
        let mut b = Signature::new(TEST_NUM_HASHES).expect("valid");
        let set_size = 100usize;
        for i in 0..set_size {
            a.add(format!("element_{i}").as_bytes());
        }
        for i in 0..set_size {
            b.add(format!("element_{}", i + set_size / 2).as_bytes());
        }
        let sim = a.similarity(&b).expect("ok");
        let expected = (set_size / 2) as f64 / (set_size + set_size / 2) as f64;
        assert!(
            approx(expected, sim, TEST_OVERLAP_TOLERANCE),
            "expected ~{expected}, got {sim}"
        );
    }

    // --- Byte-parity fixed vector ---
    //
    // The expected bytes below were captured from the reference implementation
    // (an 8-hash signature after adding "a", "b", "c", "d"), confirming the
    // seed sequence + FNV-1a + mix pipeline + big-endian serialization are
    // bit-exact.
    #[test]
    fn test_reference_parity_fixed_vector() {
        let mut sig = Signature::new(8).expect("valid");
        for tok in ["a", "b", "c", "d"] {
            sig.add(tok.as_bytes());
        }
        let expected_hex = "000000082b6f8f5c860519cf663c8930b083847b2879c5e3f62f41d6113d66bba91cae1234667cd1f127248875599e283ca0811f423a92627a434a9915d10f7d0fc711cd";
        let got_hex: String = sig.bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(got_hex, expected_hex, "parity byte vector mismatch");
    }
}
