//! Locality-Sensitive Hashing (LSH) index for fast approximate
//! nearest-neighbor retrieval of MinHash signatures.
//!
//! LSH groups similar MinHash signatures into the same buckets by hashing
//! *bands* of consecutive hash values. This enables `O(N)` indexing and
//! sublinear query time, replacing the naive `O(N^2)` pairwise comparison.
//!
//! The index is parameterized by `num_bands` and `num_rows`, where
//! `num_bands * num_rows` must equal the number of hash functions in every
//! inserted/queried signature. A higher `num_bands` lowers the similarity
//! threshold for candidate retrieval.
//!
//! Determinism is exact: each band hash is computed with FNV-1a (64-bit, as
//! in [`cf_alg_hashutil::fnv64a`]) over the band index (8 bytes, big-endian,
//! for domain separation) followed by that band's slice of the signature's
//! big-endian serialized minimums (skipping the 4-byte header). Because both
//! the signature bytes ([`cf_alg_minhash`]) and the FNV hashing are
//! bit-stable, candidate sets are reproducible across runs and platforms
//! (reference-implementation behavior, pinned by golden tests below).
//!
//! This crate produces **no** MACHINE-format report bytes — its outputs are
//! in-memory candidate ID lists consumed internally by clone detection — so it
//! does not route through `cf-gojson` / `cf-goyaml` and depends on no
//! serialization machinery.
//!
//! # Thread safety
//!
//! Mutation (`insert`) takes `&mut self` and reads (`query`) take `&self`;
//! the borrow checker proves exclusivity at compile time, so no runtime lock
//! is needed. To share an [`Index`] for concurrent reads, wrap it in an
//! [`std::sync::Arc`]; for concurrent mutation, wrap it in a `Mutex`/`RwLock`.
//! Numeric results are identical regardless of strategy.
//!
//! # Examples
//!
//! ```
//! use cf_alg_lsh::Index;
//! use cf_alg_minhash::Signature;
//!
//! let mut idx = Index::new(16, 8).unwrap(); // 16 bands * 8 rows = 128 hashes
//! let mut a = Signature::new(128).unwrap();
//! let mut b = Signature::new(128).unwrap();
//! for tok in ["func", "main", "return"] {
//!     a.add(tok.as_bytes());
//!     b.add(tok.as_bytes());
//! }
//! idx.insert("funcA".to_string(), &a).unwrap();
//! let candidates = idx.query(&b).unwrap();
//! assert!(candidates.iter().any(|id| id == "funcA"));
//! ```
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fmt;

use cf_alg_minhash::{Signature, HEADER_SIZE};

/// Number of bytes per `u64` value used when slicing signature bytes for band
/// hashing.
const BYTES_PER_UINT64: usize = 8;

/// FNV-1a 64-bit offset basis (the hash of empty input; canonical constant).
/// Used as the starting state when accumulating a band hash from multiple
/// byte chunks.
const FNV64A_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime (canonical constant).
const FNV64A_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Errors returned by [`Index`] operations.
///
/// The message strings are part of the CLI/log compatibility contract and
/// must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LshError {
    /// `num_bands` or `num_rows` was not positive.
    #[error("lsh: numBands and numRows must be positive")]
    InvalidParams,
    /// A nil signature was provided.
    ///
    /// The type system makes "nil" unrepresentable for `&Signature`, so this
    /// variant exists only to keep the error surface complete.
    #[error("lsh: signature must not be nil")]
    NilSignature,
    /// Signature size did not equal `num_bands * num_rows`.
    #[error("lsh: signature size must equal numBands * numRows")]
    SizeMismatch,
}

/// A deterministic LSH index for approximate nearest-neighbor retrieval over
/// MinHash signatures.
///
/// Construct one with [`Index::new`]. The index stores, per band, a map from
/// band-hash to the set of item IDs in that bucket, plus the full signatures
/// (needed by [`Index::query_threshold`]). Inserting an existing ID replaces
/// the prior entry.
pub struct Index {
    num_bands: usize,
    num_rows: usize,
    /// One bucket map per band: band-hash -> set of item IDs.
    ///
    /// The inner map is keyed for membership; iteration order is not
    /// observable to callers ([`Index::query`] returns IDs in unspecified map
    /// order).
    bands: Vec<HashMap<u64, HashMap<String, bool>>>,
    /// All stored signatures by ID.
    sigs: HashMap<String, Signature>,
}

impl Index {
    /// Creates a new LSH index with the given number of bands and rows per band.
    ///
    /// Every signature inserted or queried must have exactly
    /// `num_bands * num_rows` hash functions.
    ///
    /// # Errors
    ///
    /// Returns [`LshError::InvalidParams`] if `num_bands == 0` or
    /// `num_rows == 0`.
    pub fn new(num_bands: usize, num_rows: usize) -> Result<Self, LshError> {
        if num_bands == 0 || num_rows == 0 {
            return Err(LshError::InvalidParams);
        }

        let mut bands = Vec::with_capacity(num_bands);
        for _ in 0..num_bands {
            bands.push(HashMap::new());
        }

        Ok(Self {
            num_bands,
            num_rows,
            bands,
            sigs: HashMap::new(),
        })
    }

    /// Inserts a signature into the index under the given `id`.
    ///
    /// If `id` already exists, its previous signature is removed from all band
    /// buckets before the new one is inserted (an update, not a duplicate).
    ///
    /// # Errors
    ///
    /// - [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`.
    ///
    /// ([`LshError::NilSignature`] is unreachable here because `sig` is a
    /// non-null reference.)
    pub fn insert(&mut self, id: String, sig: &Signature) -> Result<(), LshError> {
        let expected_size = self.num_bands * self.num_rows;
        if sig.len() != expected_size {
            return Err(LshError::SizeMismatch);
        }

        let band_hashes = self.compute_band_hashes(sig);

        // Remove the old entry if the ID already exists.
        if let Some(old_sig) = self.sigs.get(&id) {
            let old_hashes = self.compute_band_hashes(old_sig);
            self.remove(&id, &old_hashes);
        }

        self.sigs.insert(id.clone(), sig.clone());

        for (b, &h) in band_hashes.iter().enumerate() {
            self.bands[b].entry(h).or_default().insert(id.clone(), true);
        }

        Ok(())
    }

    /// Returns deduplicated candidate IDs whose signatures share at least one
    /// band hash with the query signature.
    ///
    /// The order of returned IDs is unspecified (it follows hash-map iteration
    /// order).
    ///
    /// # Errors
    ///
    /// [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`.
    pub fn query(&self, sig: &Signature) -> Result<Vec<String>, LshError> {
        let expected_size = self.num_bands * self.num_rows;
        if sig.len() != expected_size {
            return Err(LshError::SizeMismatch);
        }

        let band_hashes = self.compute_band_hashes(sig);

        // Deduplicate via a membership map.
        let mut seen: HashMap<String, bool> = HashMap::new();
        for (b, &h) in band_hashes.iter().enumerate() {
            if let Some(bucket) = self.bands[b].get(&h) {
                for id in bucket.keys() {
                    seen.insert(id.clone(), true);
                }
            }
        }

        Ok(seen.into_keys().collect())
    }

    /// Returns candidate IDs whose exact MinHash similarity with the query
    /// signature is at or above `threshold`.
    ///
    /// This first retrieves LSH candidates via [`Index::query`], then filters
    /// them by computing the exact MinHash similarity against each stored
    /// signature, keeping those `>= threshold`. Candidates whose similarity
    /// computation fails (e.g. size mismatch) are skipped.
    ///
    /// # Errors
    ///
    /// [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`
    /// (propagated from the inner [`Index::query`]).
    pub fn query_threshold(
        &self,
        sig: &Signature,
        threshold: f64,
    ) -> Result<Vec<String>, LshError> {
        let candidates = self.query(sig)?;

        let mut result = Vec::new();
        for id in candidates {
            let Some(stored) = self.sigs.get(&id) else {
                continue;
            };
            // Entries whose similarity computation errors are skipped.
            if let Ok(sim) = sig.similarity(stored) {
                if sim >= threshold {
                    result.push(id);
                }
            }
        }

        Ok(result)
    }

    /// Removes a signature's ID from all band buckets given its precomputed
    /// band hashes, dropping now-empty buckets and the stored signature.
    fn remove(&mut self, id: &str, band_hashes: &[u64]) {
        for (b, &h) in band_hashes.iter().enumerate() {
            if let Some(bucket) = self.bands[b].get_mut(&h) {
                bucket.remove(id);
                if bucket.is_empty() {
                    self.bands[b].remove(&h);
                }
            }
        }
        self.sigs.remove(id);
    }

    /// Computes the FNV-1a (64-bit) hash for each band of the signature.
    ///
    /// For band `b`: the 8-byte big-endian encoding of `b` is hashed first (for
    /// domain separation), followed by that band's `num_rows * 8` bytes from the
    /// signature's serialized form (after skipping the [`HEADER_SIZE`]-byte
    /// header). The scheme is frozen (pinned by the golden test below).
    fn compute_band_hashes(&self, sig: &Signature) -> Vec<u64> {
        let data = sig.bytes();
        // Skip the 4-byte header (num_hashes prefix).
        let hash_data = &data[HEADER_SIZE..];

        let mut hashes = Vec::with_capacity(self.num_bands);
        for b in 0..self.num_bands {
            // FNV-1a over: [band index BE u64] ++ [band rows bytes].
            let mut h = FNV64A_OFFSET;

            // Domain separation: write the band index as 8 big-endian bytes.
            for byte in (b as u64).to_be_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(FNV64A_PRIME);
            }

            // Write this band's rows.
            let start = b * self.num_rows * BYTES_PER_UINT64;
            let end = start + self.num_rows * BYTES_PER_UINT64;
            for &byte in &hash_data[start..end] {
                h ^= u64::from(byte);
                h = h.wrapping_mul(FNV64A_PRIME);
            }

            hashes.push(h);
        }
        hashes
    }

    /// Returns the number of signatures currently stored in the index.
    #[must_use]
    pub fn size(&self) -> usize {
        self.sigs.len()
    }

    /// Returns `true` if the index contains no signatures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sigs.is_empty()
    }

    /// Removes all signatures and empties every band bucket, preserving the
    /// `num_bands` / `num_rows` configuration.
    pub fn clear(&mut self) {
        for band in &mut self.bands {
            band.clear();
        }
        self.sigs.clear();
    }

    /// Returns the configured number of bands.
    #[must_use]
    pub const fn num_bands(&self) -> usize {
        self.num_bands
    }

    /// Returns the configured number of rows per band.
    #[must_use]
    pub const fn num_rows(&self) -> usize {
        self.num_rows
    }
}

impl fmt::Debug for Index {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Index")
            .field("num_bands", &self.num_bands)
            .field("num_rows", &self.num_rows)
            .field("size", &self.sigs.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_alg_hashutil::fnv64a;

    // Test constants.
    const TEST_BANDS: usize = 16;
    const TEST_ROWS: usize = 8;
    const TEST_NUM_HASHES: usize = TEST_BANDS * TEST_ROWS;
    const TEST_LARGE_INDEX_SIZE: usize = 1000;
    const TEST_HIGH_THRESHOLD: f64 = 0.8;
    const TEST_LOW_THRESHOLD: f64 = 0.0;
    const TEST_CONCURRENT_THREADS: usize = 50;
    const TEST_CONCURRENT_OPS_PER_THREAD: usize = 20;

    fn new_sig(n: usize) -> Signature {
        Signature::new(n).expect("valid signature")
    }

    // --- Constructor Tests ---

    #[test]
    fn test_new_valid() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert_eq!(idx.num_bands(), TEST_BANDS);
        assert_eq!(idx.num_rows(), TEST_ROWS);
        assert_eq!(idx.size(), 0);
    }

    #[test]
    fn test_new_zero_bands() {
        let err = Index::new(0, TEST_ROWS).unwrap_err();
        assert_eq!(err, LshError::InvalidParams);
    }

    #[test]
    fn test_new_zero_rows() {
        let err = Index::new(TEST_BANDS, 0).unwrap_err();
        assert_eq!(err, LshError::InvalidParams);
    }

    // --- Insert and Query Tests ---

    #[test]
    fn test_insert_query_duplicate() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig_a = new_sig(TEST_NUM_HASHES);
        let mut sig_b = new_sig(TEST_NUM_HASHES);

        let tokens = [
            "func", "main", "return", "if", "else", "for", "range", "var", "int", "string",
        ];
        for tok in tokens {
            sig_a.add(tok.as_bytes());
            sig_b.add(tok.as_bytes());
        }

        idx.insert("funcA".to_string(), &sig_a).expect("insert ok");

        let candidates = idx.query(&sig_b).expect("query ok");
        assert!(
            candidates.iter().any(|c| c == "funcA"),
            "expected funcA in {candidates:?}"
        );
    }

    #[test]
    fn test_insert_query_dissimilar() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig_a = new_sig(TEST_NUM_HASHES);
        let mut sig_b = new_sig(TEST_NUM_HASHES);

        for i in 0..TEST_LARGE_INDEX_SIZE {
            sig_a.add(format!("tokenA_{i}").as_bytes());
            sig_b.add(format!("tokenB_{i}").as_bytes());
        }

        idx.insert("funcA".to_string(), &sig_a).expect("insert ok");

        let candidates = idx.query(&sig_b).expect("query ok");
        assert!(
            !candidates.iter().any(|c| c == "funcA"),
            "dissimilar signatures should not collide: {candidates:?}"
        );
    }

    #[test]
    fn test_insert_query_similar_pair() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig_a = new_sig(TEST_NUM_HASHES);
        let mut sig_b = new_sig(TEST_NUM_HASHES);

        let shared_count = 900;
        let unique_count = 100;
        for i in 0..shared_count {
            let shared = format!("shared_{i}");
            sig_a.add(shared.as_bytes());
            sig_b.add(shared.as_bytes());
        }
        for i in 0..unique_count {
            sig_a.add(format!("uniqueA_{i}").as_bytes());
            sig_b.add(format!("uniqueB_{i}").as_bytes());
        }

        idx.insert("funcA".to_string(), &sig_a).expect("insert ok");

        let candidates = idx.query(&sig_b).expect("query ok");
        assert!(
            candidates.iter().any(|c| c == "funcA"),
            "similar signatures should be candidates: {candidates:?}"
        );
    }

    // --- QueryThreshold Tests ---

    #[test]
    fn test_query_threshold_filters_correctly() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig_similar = new_sig(TEST_NUM_HASHES);
        let mut sig_different = new_sig(TEST_NUM_HASHES);
        let mut sig_query = new_sig(TEST_NUM_HASHES);

        for i in 0..900 {
            let shared = format!("shared_{i}");
            sig_similar.add(shared.as_bytes());
            sig_query.add(shared.as_bytes());
        }
        for i in 0..100 {
            sig_similar.add(format!("simUnique_{i}").as_bytes());
            sig_query.add(format!("queryUnique_{i}").as_bytes());
        }
        for i in 0..TEST_LARGE_INDEX_SIZE {
            sig_different.add(format!("different_{i}").as_bytes());
        }

        idx.insert("similar".to_string(), &sig_similar).expect("ok");
        idx.insert("different".to_string(), &sig_different).expect("ok");

        let results = idx
            .query_threshold(&sig_query, TEST_HIGH_THRESHOLD)
            .expect("ok");
        assert!(results.iter().any(|c| c == "similar"), "{results:?}");
        assert!(!results.iter().any(|c| c == "different"), "{results:?}");
    }

    #[test]
    fn test_query_threshold_zero_threshold() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        sig.add(b"token");
        idx.insert("funcA".to_string(), &sig).expect("ok");

        let results = idx.query_threshold(&sig, TEST_LOW_THRESHOLD).expect("ok");
        assert!(results.iter().any(|c| c == "funcA"), "{results:?}");
    }

    // --- Empty Index Tests ---

    #[test]
    fn test_query_empty_index() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        sig.add(b"token");

        let candidates = idx.query(&sig).expect("ok");
        assert!(candidates.is_empty(), "{candidates:?}");
    }

    // --- Size Mismatch Tests ---

    #[test]
    fn test_insert_size_mismatch() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let wrong_sig = new_sig(TEST_NUM_HASHES + 1);
        let err = idx.insert("funcA".to_string(), &wrong_sig).unwrap_err();
        assert_eq!(err, LshError::SizeMismatch);
    }

    #[test]
    fn test_query_size_mismatch() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let wrong_sig = new_sig(TEST_NUM_HASHES + 1);
        let err = idx.query(&wrong_sig).unwrap_err();
        assert_eq!(err, LshError::SizeMismatch);
    }

    // --- Size and Clear Tests ---

    #[test]
    fn test_size() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert_eq!(idx.size(), 0);

        let mut sig = new_sig(TEST_NUM_HASHES);
        sig.add(b"token");

        idx.insert("funcA".to_string(), &sig).expect("ok");
        assert_eq!(idx.size(), 1);

        idx.insert("funcB".to_string(), &sig).expect("ok");
        assert_eq!(idx.size(), 2);
    }

    #[test]
    fn test_clear() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        sig.add(b"token");

        idx.insert("funcA".to_string(), &sig).expect("ok");
        idx.clear();

        assert_eq!(idx.size(), 0);
        let candidates = idx.query(&sig).expect("ok");
        assert!(candidates.is_empty(), "{candidates:?}");
    }

    // --- Duplicate Insert Test ---

    #[test]
    fn test_insert_duplicate_id() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        sig.add(b"token");

        idx.insert("funcA".to_string(), &sig).expect("ok");
        // Insert same ID again — should update, not duplicate.
        idx.insert("funcA".to_string(), &sig).expect("ok");

        assert_eq!(idx.size(), 1);

        let candidates = idx.query(&sig).expect("ok");
        let count = candidates.iter().filter(|&c| c == "funcA").count();
        assert_eq!(count, 1, "duplicate ID should appear once: {candidates:?}");
    }

    // --- NumBands / NumRows Tests ---

    #[test]
    fn test_num_bands() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert_eq!(idx.num_bands(), TEST_BANDS);
    }

    #[test]
    fn test_num_rows() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert_eq!(idx.num_rows(), TEST_ROWS);
    }

    // --- Large Index Test ---

    #[test]
    fn test_insert_query_large_index() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");

        for i in 0..TEST_LARGE_INDEX_SIZE {
            let mut sig = new_sig(TEST_NUM_HASHES);
            for j in 0..10 {
                sig.add(format!("sig_{i}_tok_{j}").as_bytes());
            }
            idx.insert(format!("func_{i}"), &sig).expect("ok");
        }

        assert_eq!(idx.size(), TEST_LARGE_INDEX_SIZE);

        let mut query_sig = new_sig(TEST_NUM_HASHES);
        for j in 0..10 {
            query_sig.add(format!("sig_{}_tok_{j}", 0).as_bytes());
        }

        let candidates = idx.query(&query_sig).expect("ok");
        assert!(candidates.iter().any(|c| c == "func_0"), "{candidates:?}");
    }

    // --- Determinism Test (band-hash bit-identity) ---

    /// Two indexes built identically must return the same candidate set,
    /// confirming deterministic band hashing.
    #[test]
    fn test_deterministic_hashing() {
        let mut a = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut b = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        for tok in ["alpha", "beta", "gamma", "delta"] {
            sig.add(tok.as_bytes());
        }
        a.insert("x".to_string(), &sig).expect("ok");
        b.insert("x".to_string(), &sig).expect("ok");

        let mut ca = a.query(&sig).expect("ok");
        let mut cb = b.query(&sig).expect("ok");
        ca.sort();
        cb.sort();
        assert_eq!(ca, cb, "non-deterministic candidate sets");
    }

    // --- Concurrent Access Test ---

    /// Concurrent mutation requires a lock; wrap the index in a `Mutex` and
    /// exercise mixed insert/query from many threads, then assert the index is
    /// non-empty.
    #[test]
    fn test_concurrent_insert_query() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        let idx = Arc::new(Mutex::new(Index::new(TEST_BANDS, TEST_ROWS).expect("valid")));

        let handles: Vec<_> = (0..TEST_CONCURRENT_THREADS)
            .map(|g| {
                let idx = Arc::clone(&idx);
                thread::spawn(move || {
                    for i in 0..TEST_CONCURRENT_OPS_PER_THREAD {
                        let mut sig = match Signature::new(TEST_NUM_HASHES) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        sig.add(format!("thread_{g}_token_{i}").as_bytes());

                        if g % 2 == 0 {
                            let _ = idx.lock().unwrap().insert(format!("func_{g}_{i}"), &sig);
                        } else {
                            let _ = idx.lock().unwrap().query(&sig);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("worker thread panicked");
        }

        assert!(idx.lock().unwrap().size() > 0, "index should not be empty");
    }

    // --- Band-hash byte-parity cross-check ---
    //
    // Confirms the accumulating FNV-1a path matches a reference one-shot FNV
    // over the concatenated [band-index BE u64] ++ [band bytes].
    #[test]
    fn test_compute_band_hashes_matches_reference_fnv() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut sig = new_sig(TEST_NUM_HASHES);
        for tok in ["a", "b", "c"] {
            sig.add(tok.as_bytes());
        }
        let data = sig.bytes();
        let hash_data = &data[HEADER_SIZE..];

        let computed = idx.compute_band_hashes(&sig);
        assert_eq!(computed.len(), TEST_BANDS);

        for (b, &got) in computed.iter().enumerate() {
            // Reference: build the exact byte stream and run fnv64a once.
            let mut buf: Vec<u8> = Vec::new();
            buf.extend_from_slice(&(b as u64).to_be_bytes());
            let start = b * TEST_ROWS * BYTES_PER_UINT64;
            let end = start + TEST_ROWS * BYTES_PER_UINT64;
            buf.extend_from_slice(&hash_data[start..end]);
            assert_eq!(got, fnv64a(&buf), "band {b} hash mismatch");
        }
    }

    /// Hard golden: the band hashes for signature `["a","b","c"]` with `16`
    /// bands × `8` rows must equal the exact `u64` values captured from the
    /// reference implementation. This proves byte-for-byte determinism
    /// end-to-end: MinHash seeds + FNV-1a base hash + mix + big-endian
    /// serialization + band-index domain separation + band FNV-1a.
    #[test]
    fn test_band_hashes_reference_golden() {
        const REFERENCE_BAND_HASHES: [u64; 16] = [
            5_179_986_799_449_527_917,
            2_684_347_889_088_141_715,
            8_071_132_555_451_129_258,
            12_186_593_400_230_371_248,
            9_526_441_772_001_931_491,
            18_022_818_885_551_654_373,
            8_475_595_975_851_155_518,
            7_742_516_436_093_151_802,
            15_880_334_081_806_128_271,
            12_015_151_828_204_407_170,
            12_112_662_855_795_310_576,
            2_992_170_943_111_823_601,
            16_575_047_986_323_955_151,
            10_791_085_982_235_387_036,
            17_715_564_219_214_662_133,
            3_458_076_640_252_412_449,
        ];

        let idx = Index::new(16, 8).expect("valid");
        let mut sig = Signature::new(128).expect("valid");
        for tok in ["a", "b", "c"] {
            sig.add(tok.as_bytes());
        }
        let got = idx.compute_band_hashes(&sig);
        assert_eq!(
            got, REFERENCE_BAND_HASHES,
            "band hashes diverged from the reference golden"
        );
    }

    /// Band-index domain separation: different band slots hash an empty
    /// signature to different values (no two bands collide on identical zero
    /// rows), confirming the band index is folded into the hash.
    #[test]
    fn test_band_index_domain_separation() {
        let idx = Index::new(4, TEST_ROWS).expect("valid");
        let sig = new_sig(4 * TEST_ROWS); // all rows = u64::MAX, identical per band
        let hashes = idx.compute_band_hashes(&sig);
        // With identical row bytes, only the band index differs; all four hashes
        // must still be distinct because the index is part of the hash input.
        for i in 0..hashes.len() {
            for j in (i + 1)..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "bands {i} and {j} collided");
            }
        }
    }
}
