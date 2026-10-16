//! Locality-Sensitive Hashing (LSH) index for fast approximate
//! nearest-neighbor retrieval of MinHash signatures.
//!
//! LSH groups similar MinHash signatures into the same buckets by hashing
//! *bands* of consecutive hash values. This enables `O(N)` indexing and
//! sublinear candidate generation, replacing the naive `O(N^2)` pairwise
//! comparison.
//!
//! The index is parameterized by `num_bands` and `num_rows`, where
//! `num_bands * num_rows` must equal the number of hash functions in every
//! inserted/queried signature. Two signatures with Jaccard similarity `s`
//! share at least one band bucket with probability
//! `1 - (1 - s^num_rows)^num_bands` — see [`candidate_probability`]. More bands
//! (fewer rows each) move that S-curve to the left, i.e. lower the
//! similarity at which a pair becomes a candidate.
//!
//! Items are identified by dense `usize` ids assigned by the caller in
//! insertion order (strictly increasing), so the index stores only bucket
//! membership — no per-item strings and no signature copies. Querying with
//! [`Index::query_after`] yields only candidates with a larger id than the
//! item being queried, which lets a caller enumerate every unordered pair
//! exactly once without a `seen` set.
//!
//! Determinism is exact: each band hash is computed with FNV-1a (64-bit, as
//! in [`cf_alg_hashutil::fnv64a`]) over the band index (8 bytes, big-endian,
//! for domain separation) followed by that band's slice of the signature's
//! big-endian serialized minimums (skipping the 4-byte header). Because both
//! the signature bytes ([`cf_alg_minhash`]) and the FNV hashing are
//! bit-stable, candidate sets are reproducible across runs and platforms.
//!
//! This crate produces **no** machine-format report bytes — its outputs are
//! in-memory candidate id lists consumed by clone detection — so it does not
//! depend on any serialization machinery.
//!
//! # Thread safety
//!
//! Mutation (`insert`) takes `&mut self` and reads (`query*`) take `&self`;
//! each query needs its own [`Scratch`]. To share an [`Index`] for concurrent
//! reads, wrap it in an [`std::sync::Arc`] and give every thread a `Scratch`.
//!
//! # Examples
//!
//! ```
//! use cf_alg_lsh::{Index, Scratch};
//! use cf_alg_minhash::Signature;
//!
//! let mut idx = Index::new(32, 4).unwrap(); // 32 bands * 4 rows = 128 hashes
//! let mut a = Signature::new(128).unwrap();
//! let mut b = Signature::new(128).unwrap();
//! for tok in ["func", "main", "return"] {
//!     a.add(tok.as_bytes());
//!     b.add(tok.as_bytes());
//! }
//! idx.insert(0, &a).unwrap();
//! idx.insert(1, &b).unwrap();
//!
//! // Every candidate of `b`, and only those strictly after item 0.
//! assert_eq!(idx.query(&b).unwrap(), vec![0, 1]);
//! let mut scratch = Scratch::default();
//! assert_eq!(idx.query_after(&a, 0, &mut scratch).unwrap(), vec![1]);
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LshError {
    /// `num_bands` or `num_rows` was not positive.
    #[error("lsh: numBands and numRows must be positive")]
    InvalidParams,
    /// Signature size did not equal `num_bands * num_rows`.
    #[error("lsh: signature size must equal numBands * numRows")]
    SizeMismatch,
    /// An id was inserted that is not larger than every id already stored.
    #[error("lsh: ids must be inserted in strictly increasing order")]
    IdOrder,
}

/// Probability that two items with Jaccard similarity `similarity` share at
/// least one band bucket in a `num_bands` × `num_rows` index:
/// `1 - (1 - s^rows)^bands`. This is the recall of the candidate stage at
/// that similarity; its 50% point is roughly `(1/bands)^(1/rows)`.
///
/// ```
/// use cf_alg_lsh::candidate_probability;
///
/// // 32 x 4: pairs at the 0.5 Jaccard threshold are candidates ~87% of the
/// // time, pairs at 0.64 practically always, unrelated pairs rarely.
/// assert!(candidate_probability(0.5, 32, 4) > 0.85);
/// assert!(candidate_probability(0.64, 32, 4) > 0.99);
/// assert!(candidate_probability(0.2, 32, 4) < 0.06);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // band/row counts are tiny
pub fn candidate_probability(similarity: f64, num_bands: usize, num_rows: usize) -> f64 {
    let band_match = similarity.powf(num_rows as f64);
    1.0 - (1.0 - band_match).powf(num_bands as f64)
}

/// Reusable per-query workspace for [`Index::query_after`]: a
/// generation-stamped mark per item id, so deduplicating the candidates of a
/// query costs one array write per bucket member instead of a hash-set
/// insert, and no allocation after the first query.
#[derive(Debug, Default)]
pub struct Scratch {
    marks: Vec<u32>,
    generation: u32,
}

impl Scratch {
    /// Starts a new query generation; returns the stamp to mark seen ids with.
    fn next_generation(&mut self) -> u32 {
        if self.generation == u32::MAX {
            self.marks.iter_mut().for_each(|m| *m = 0);
            self.generation = 0;
        }
        self.generation += 1;
        self.generation
    }

    /// Marks `id` as seen in the current generation; `true` the first time.
    fn mark(&mut self, id: usize, generation: u32) -> bool {
        if id >= self.marks.len() {
            self.marks.resize(id + 1, 0);
        }
        if self.marks[id] == generation {
            return false;
        }
        self.marks[id] = generation;
        true
    }
}

/// A deterministic LSH index for approximate nearest-neighbor retrieval over
/// MinHash signatures, keyed by dense `usize` item ids.
///
/// Construct one with [`Index::new`]. The index stores, per band, a map from
/// band-hash to the (ascending) list of item ids in that bucket.
pub struct Index {
    num_bands: usize,
    num_rows: usize,
    /// One bucket map per band: band-hash -> ascending item ids.
    bands: Vec<HashMap<u64, Vec<usize>>>,
    /// The largest id inserted so far (ids are strictly increasing).
    last_id: Option<usize>,
    /// Number of inserted items.
    size: usize,
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
        Ok(Self {
            num_bands,
            num_rows,
            bands: (0..num_bands).map(|_| HashMap::new()).collect(),
            last_id: None,
            size: 0,
        })
    }

    /// Inserts a signature under item `id`.
    ///
    /// Ids must be inserted in strictly increasing order; this keeps every
    /// bucket sorted, which [`Index::query_after`] relies on.
    ///
    /// # Errors
    ///
    /// - [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`;
    /// - [`LshError::IdOrder`] if `id` is not larger than the last inserted id.
    pub fn insert(&mut self, id: usize, sig: &Signature) -> Result<(), LshError> {
        self.check_size(sig)?;
        if self.last_id.is_some_and(|last| id <= last) {
            return Err(LshError::IdOrder);
        }
        let band_hashes = self.compute_band_hashes(sig);
        for (band, hash) in self.bands.iter_mut().zip(band_hashes) {
            band.entry(hash).or_default().push(id);
        }
        self.last_id = Some(id);
        self.size += 1;
        Ok(())
    }

    /// Returns every item id whose signature shares at least one band bucket
    /// with `sig`, ascending and deduplicated.
    ///
    /// # Errors
    ///
    /// [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`.
    pub fn query(&self, sig: &Signature) -> Result<Vec<usize>, LshError> {
        let mut scratch = Scratch::default();
        let mut ids = self.collect_candidates(sig, 0, &mut scratch)?;
        ids.sort_unstable();
        Ok(ids)
    }

    /// Returns the deduplicated candidate ids of `sig` that are **greater
    /// than** `id` — the candidates of item `id` that come after it.
    ///
    /// Querying every item in id order with its own id yields each unordered
    /// candidate pair exactly once (bucket sharing is symmetric, so the pair
    /// `(i, j)`, `i < j`, surfaces when `i` is queried and is skipped when
    /// `j` is). The order of the returned ids is deterministic but
    /// unspecified.
    ///
    /// # Errors
    ///
    /// [`LshError::SizeMismatch`] if `sig.len() != num_bands * num_rows`.
    pub fn query_after(
        &self,
        sig: &Signature,
        id: usize,
        scratch: &mut Scratch,
    ) -> Result<Vec<usize>, LshError> {
        self.collect_candidates(sig, id.saturating_add(1), scratch)
    }

    /// Collects the unique ids `>= lower_bound` across all band buckets of
    /// `sig`, in band/bucket order.
    fn collect_candidates(
        &self,
        sig: &Signature,
        lower_bound: usize,
        scratch: &mut Scratch,
    ) -> Result<Vec<usize>, LshError> {
        self.check_size(sig)?;
        let generation = scratch.next_generation();
        let mut out = Vec::new();
        for (band, hash) in self.bands.iter().zip(self.compute_band_hashes(sig)) {
            let Some(bucket) = band.get(&hash) else {
                continue;
            };
            // Buckets are ascending (ids are inserted in increasing order), so
            // the candidates above the bound are a suffix.
            let start = bucket.partition_point(|&member| member < lower_bound);
            for &member in &bucket[start..] {
                if scratch.mark(member, generation) {
                    out.push(member);
                }
            }
        }
        Ok(out)
    }

    fn check_size(&self, sig: &Signature) -> Result<(), LshError> {
        if sig.len() == self.num_bands * self.num_rows {
            Ok(())
        } else {
            Err(LshError::SizeMismatch)
        }
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

    /// Returns the number of items currently stored in the index.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns `true` if the index contains no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Removes every item and empties every band bucket, preserving the
    /// `num_bands` / `num_rows` configuration.
    pub fn clear(&mut self) {
        for band in &mut self.bands {
            band.clear();
        }
        self.last_id = None;
        self.size = 0;
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
            .field("size", &self.size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_alg_hashutil::fnv64a;

    // Test constants.
    const TEST_BANDS: usize = 32;
    const TEST_ROWS: usize = 4;
    const TEST_NUM_HASHES: usize = TEST_BANDS * TEST_ROWS;
    const TEST_LARGE_INDEX_SIZE: usize = 1000;
    const TEST_CONCURRENT_THREADS: usize = 8;
    const TEST_CONCURRENT_QUERIES_PER_THREAD: usize = 20;

    fn new_sig(n: usize) -> Signature {
        Signature::new(n).expect("valid signature")
    }

    /// A signature over `count` tokens drawn from `prefix`.
    fn sig_of(prefix: &str, count: usize) -> Signature {
        let mut sig = new_sig(TEST_NUM_HASHES);
        for i in 0..count {
            sig.add(format!("{prefix}_{i}").as_bytes());
        }
        sig
    }

    // --- Constructor Tests ---

    #[test]
    fn new_valid() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert_eq!(idx.num_bands(), TEST_BANDS);
        assert_eq!(idx.num_rows(), TEST_ROWS);
        assert_eq!(idx.size(), 0);
        assert!(idx.is_empty());
    }

    #[test]
    fn new_rejects_zero_bands_or_rows() {
        assert_eq!(
            Index::new(0, TEST_ROWS).unwrap_err(),
            LshError::InvalidParams
        );
        assert_eq!(
            Index::new(TEST_BANDS, 0).unwrap_err(),
            LshError::InvalidParams
        );
    }

    // --- Insert and Query Tests ---

    #[test]
    fn identical_signatures_are_candidates() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("tok", 10);
        idx.insert(0, &sig).expect("insert ok");
        assert_eq!(idx.query(&sig).expect("query ok"), vec![0]);
    }

    #[test]
    fn dissimilar_signatures_are_not_candidates() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        idx.insert(0, &sig_of("a", TEST_LARGE_INDEX_SIZE))
            .expect("insert ok");
        let candidates = idx
            .query(&sig_of("b", TEST_LARGE_INDEX_SIZE))
            .expect("query ok");
        assert!(candidates.is_empty(), "{candidates:?}");
    }

    #[test]
    fn similar_signatures_are_candidates() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut a = sig_of("shared", 900);
        let mut b = sig_of("shared", 900);
        for i in 0..100 {
            a.add(format!("uniqueA_{i}").as_bytes());
            b.add(format!("uniqueB_{i}").as_bytes());
        }
        idx.insert(0, &a).expect("insert ok");
        assert_eq!(idx.query(&b).expect("query ok"), vec![0]);
    }

    #[test]
    fn query_after_returns_only_larger_ids() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("tok", 10);
        for id in 0..5 {
            idx.insert(id, &sig).expect("insert ok");
        }
        let mut scratch = Scratch::default();
        let mut after_two = idx.query_after(&sig, 2, &mut scratch).expect("ok");
        after_two.sort_unstable();
        assert_eq!(after_two, vec![3, 4]);
        assert!(idx
            .query_after(&sig, 4, &mut scratch)
            .expect("ok")
            .is_empty());
        // The scratch is reusable across queries.
        let mut all = idx.query_after(&sig, 0, &mut scratch).expect("ok");
        all.sort_unstable();
        assert_eq!(all, vec![1, 2, 3, 4]);
    }

    #[test]
    fn candidates_are_deduplicated_across_bands() {
        // Identical signatures collide in EVERY band; the id must still
        // surface once.
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("tok", 3);
        idx.insert(7, &sig).expect("insert ok");
        let mut scratch = Scratch::default();
        assert_eq!(idx.query_after(&sig, 0, &mut scratch).expect("ok"), vec![7]);
        assert_eq!(idx.query(&sig).expect("ok"), vec![7]);
    }

    #[test]
    fn insert_rejects_out_of_order_ids() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("tok", 3);
        idx.insert(5, &sig).expect("insert ok");
        assert_eq!(idx.insert(5, &sig).unwrap_err(), LshError::IdOrder);
        assert_eq!(idx.insert(4, &sig).unwrap_err(), LshError::IdOrder);
        assert_eq!(idx.size(), 1);
        idx.insert(6, &sig).expect("larger id ok");
        assert_eq!(idx.size(), 2);
    }

    #[test]
    fn size_mismatch_is_rejected() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let wrong = new_sig(TEST_NUM_HASHES + 1);
        assert_eq!(idx.insert(0, &wrong).unwrap_err(), LshError::SizeMismatch);
        assert_eq!(idx.query(&wrong).unwrap_err(), LshError::SizeMismatch);
        let mut scratch = Scratch::default();
        assert_eq!(
            idx.query_after(&wrong, 0, &mut scratch).unwrap_err(),
            LshError::SizeMismatch
        );
    }

    #[test]
    fn query_empty_index() {
        let idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        assert!(idx.query(&sig_of("tok", 1)).expect("ok").is_empty());
    }

    #[test]
    fn clear_empties_the_index_and_resets_id_order() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("tok", 1);
        idx.insert(3, &sig).expect("ok");
        idx.clear();
        assert_eq!(idx.size(), 0);
        assert!(idx.query(&sig).expect("ok").is_empty());
        idx.insert(0, &sig).expect("ids restart after clear");
    }

    #[test]
    fn large_index_finds_the_matching_item() {
        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        for i in 0..TEST_LARGE_INDEX_SIZE {
            idx.insert(i, &sig_of(&format!("sig_{i}"), 10)).expect("ok");
        }
        assert_eq!(idx.size(), TEST_LARGE_INDEX_SIZE);
        let candidates = idx.query(&sig_of("sig_0", 10)).expect("ok");
        assert!(candidates.contains(&0), "{candidates:?}");
    }

    #[test]
    fn scratch_generation_wrap_is_safe() {
        let mut scratch = Scratch {
            marks: vec![u32::MAX - 1, 0],
            generation: u32::MAX - 1,
        };
        let g = scratch.next_generation();
        assert_eq!(g, u32::MAX);
        assert!(scratch.mark(0, g));
        let g2 = scratch.next_generation();
        assert_eq!(g2, 1, "generation wraps to 1 after clearing the marks");
        assert!(scratch.mark(0, g2), "stale marks never count as seen");
        assert!(!scratch.mark(0, g2));
    }

    // --- S-curve ---

    #[test]
    fn candidate_probability_matches_the_banded_formula() {
        // One band, one row: p == s.
        assert!((candidate_probability(0.3, 1, 1) - 0.3).abs() < 1e-12);
        // 32 x 4: the published operating points of the clone engine.
        assert!(candidate_probability(0.5, 32, 4) > 0.85);
        assert!(candidate_probability(0.641, 32, 4) > 0.99);
        assert!(candidate_probability(0.2, 32, 4) < 0.06);
        // The old 16 x 8 curve missed most pairs at the 0.5 threshold.
        assert!(candidate_probability(0.5, 16, 8) < 0.07);
    }

    // --- Determinism Test (band-hash bit-identity) ---

    #[test]
    fn deterministic_hashing() {
        let mut a = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let mut b = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        let sig = sig_of("alpha", 4);
        a.insert(0, &sig).expect("ok");
        b.insert(0, &sig).expect("ok");
        assert_eq!(a.query(&sig).expect("ok"), b.query(&sig).expect("ok"));
    }

    // --- Concurrent read test ---

    #[test]
    fn concurrent_queries_share_the_index() {
        use std::sync::Arc;
        use std::thread;

        let mut idx = Index::new(TEST_BANDS, TEST_ROWS).expect("valid");
        for i in 0..TEST_CONCURRENT_THREADS {
            idx.insert(i, &sig_of(&format!("t{i}"), 5)).expect("ok");
        }
        let idx = Arc::new(idx);
        let handles: Vec<_> = (0..TEST_CONCURRENT_THREADS)
            .map(|t| {
                let idx = Arc::clone(&idx);
                thread::spawn(move || {
                    let mut scratch = Scratch::default();
                    let sig = sig_of(&format!("t{t}"), 5);
                    for _ in 0..TEST_CONCURRENT_QUERIES_PER_THREAD {
                        assert!(idx.query(&sig).expect("ok").contains(&t));
                        let after = idx.query_after(&sig, t, &mut scratch).expect("ok");
                        assert!(!after.contains(&t));
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().expect("worker thread panicked");
        }
    }

    // --- Band-hash byte-parity cross-check ---
    //
    // Confirms the accumulating FNV-1a path matches a reference one-shot FNV
    // over the concatenated [band-index BE u64] ++ [band bytes].
    #[test]
    fn compute_band_hashes_matches_reference_fnv() {
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
            let mut buf: Vec<u8> = Vec::new();
            buf.extend_from_slice(&(b as u64).to_be_bytes());
            let start = b * TEST_ROWS * BYTES_PER_UINT64;
            let end = start + TEST_ROWS * BYTES_PER_UINT64;
            buf.extend_from_slice(&hash_data[start..end]);
            assert_eq!(got, fnv64a(&buf), "band {b} hash mismatch");
        }
    }

    /// Hard golden: the band hashes for signature `["a","b","c"]` with `16`
    /// bands × `8` rows must equal the exact `u64` values captured when the
    /// scheme was frozen. This proves bit-determinism end-to-end: MinHash
    /// seeds + FNV-1a base hash + mix + big-endian serialization + band-index
    /// domain separation + band FNV-1a.
    #[test]
    fn band_hashes_reference_golden() {
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
            "band hashes diverged from the golden"
        );
    }

    /// Band-index domain separation: different band slots hash an empty
    /// signature to different values (no two bands collide on identical zero
    /// rows), confirming the band index is folded into the hash.
    #[test]
    fn band_index_domain_separation() {
        let idx = Index::new(4, TEST_ROWS).expect("valid");
        let sig = new_sig(4 * TEST_ROWS); // all rows = u64::MAX, identical per band
        let hashes = idx.compute_band_hashes(&sig);
        for i in 0..hashes.len() {
            for j in (i + 1)..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "bands {i} and {j} collided");
            }
        }
    }
}
