//! Cross-commit LRU cache for git blob data.
//!
//! Faithful Rust port of the Go `internal/cache/lru.go` (`LRUBlobCache`). It
//! wraps the generic [`cf_alg_lru::Cache`] configured with size-based capacity, an
//! FNV-128a Bloom pre-filter, sampled cost-based eviction, and blob cloning on
//! insertion — exactly the four functional options the Go constructor passes to
//! `lru.New`.

use std::collections::HashMap;

use cf_alg_lru::Cache;

use crate::gitlib::{CachedBlob, GitHash};

/// Statistics snapshot type (Go `LRUStats = lru.Stats`).
pub type LruStats = cf_alg_lru::Stats;

/// Default maximum memory size for the LRU blob cache, 256 MB
/// (Go `DefaultLRUCacheSize`).
pub const DEFAULT_LRU_CACHE_SIZE: i64 = 256 * 1024 * 1024;

/// Bytes per kilobyte for eviction-cost normalization (Go `bytesPerKB`).
const BYTES_PER_KB: f64 = 1024.0;

/// Estimated average blob size in bytes for Bloom-filter sizing
/// (Go `averageBlobSizeEstimate`). Typical source files are ~4 KB.
const AVERAGE_BLOB_SIZE_ESTIMATE: i64 = 4096;

/// Minimum expected element count for the Bloom filter (Go `minBloomElements`).
const MIN_BLOOM_ELEMENTS: usize = 64;

/// Number of LRU candidates sampled for size-aware eviction
/// (Go `evictionSampleSize`).
const EVICTION_SAMPLE_SIZE: i64 = 5;

/// Returns the data length of a cached blob as `i64` (Go `blobSize`).
///
/// Go's `blobSize` guards a nil blob (`return 0`); here a non-existent blob is
/// represented by the absence of an entry, and a present [`CachedBlob`] always
/// reports its `data` length.
fn blob_size(blob: &CachedBlob) -> i64 {
    blob.data.len() as i64
}

/// Computes the cost of evicting an entry (Go `evictionCost`).
///
/// Higher cost = less desirable to evict; cost = `accessCount / sizeKB`, so
/// large, rarely-accessed items are evicted first. `sizeKB` is clamped to `>= 1`.
fn eviction_cost(access_count: i64, size_bytes: i64) -> f64 {
    if size_bytes == 0 {
        return access_count as f64;
    }

    let mut size_kb = size_bytes as f64 / BYTES_PER_KB;
    if size_kb < 1.0 {
        size_kb = 1.0;
    }

    access_count as f64 / size_kb
}

/// A cross-commit LRU cache for blob data (Go `cache.LRUBlobCache`).
///
/// Tracks memory usage and evicts least-recently-used entries when the limit is
/// exceeded; a Bloom pre-filter short-circuits `get`/`get_multi` lookups for
/// definite misses.
pub struct LruBlobCache {
    cache: Cache<GitHash, CachedBlob>,
}

impl LruBlobCache {
    /// Creates a new LRU blob cache with `max_size` bytes of capacity
    /// (Go `NewLRUBlobCache`).
    ///
    /// A non-positive `max_size` falls back to [`DEFAULT_LRU_CACHE_SIZE`]. The
    /// Bloom filter is sized for `max(max_size / 4096, 64)` expected elements,
    /// matching `max(uint(maxSize/averageBlobSizeEstimate), minBloomElements)`.
    #[must_use]
    pub fn new(max_size: i64) -> Self {
        let max_size = if max_size <= 0 {
            DEFAULT_LRU_CACHE_SIZE
        } else {
            max_size
        };

        let expected_n =
            ((max_size / AVERAGE_BLOB_SIZE_ESTIMATE) as usize).max(MIN_BLOOM_ELEMENTS);

        let cache = Cache::new(|c| {
            c.with_max_bytes(max_size, blob_size);
            c.with_bloom_filter(|h: &GitHash| h.as_bytes().to_vec(), expected_n);
            c.with_cost_eviction(EVICTION_SAMPLE_SIZE, eviction_cost);
            c.with_clone_func(CachedBlob::clone_blob);
        });

        Self { cache }
    }

    /// Retrieves a blob, returning `None` if absent (Go `Get`).
    ///
    /// Uses the Bloom filter to skip lock acquisition for definite misses.
    #[must_use]
    pub fn get(&self, hash: &GitHash) -> Option<CachedBlob> {
        self.cache.get(hash)
    }

    /// Adds a blob to the cache (Go `Put`).
    ///
    /// A `nil` blob in Go is a no-op; here that maps to passing `None`.
    /// Oversize blobs (larger than the whole cache) are silently skipped by the
    /// underlying LRU, exactly as in Go.
    pub fn put(&self, hash: GitHash, blob: Option<CachedBlob>) {
        if let Some(b) = blob {
            self.cache.put(hash, b);
        }
    }

    /// Retrieves multiple blobs, returning found pairs and missing hashes
    /// (Go `GetMulti`).
    #[must_use]
    pub fn get_multi(
        &self,
        hashes: &[GitHash],
    ) -> (HashMap<GitHash, CachedBlob>, Vec<GitHash>) {
        self.cache.get_multi(hashes)
    }

    /// Adds multiple blobs to the cache (Go `PutMulti`).
    pub fn put_multi(&self, blobs: HashMap<GitHash, CachedBlob>) {
        self.cache.put_multi(blobs);
    }

    /// Adds multiple blobs without cloning (Go `PutMultiOwned`).
    ///
    /// The caller guarantees the blobs are exclusively owned heap copies.
    pub fn put_multi_owned(&self, blobs: HashMap<GitHash, CachedBlob>) {
        self.cache.put_multi_owned(blobs);
    }

    /// Returns cache statistics (Go `Stats`).
    #[must_use]
    pub fn stats(&self) -> LruStats {
        self.cache.stats()
    }

    /// Returns the total cache hit count (Go `CacheHits`).
    #[must_use]
    pub fn cache_hits(&self) -> i64 {
        self.cache.cache_hits()
    }

    /// Returns the total cache miss count (Go `CacheMisses`).
    #[must_use]
    pub fn cache_misses(&self) -> i64 {
        self.cache.cache_misses()
    }

    /// Removes all entries and resets the Bloom filter (Go `Clear`).
    pub fn clear(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn make_test_blob(data: &[u8]) -> CachedBlob {
        CachedBlob::for_test(data.to_vec())
    }

    fn make_test_hash(b: u8) -> GitHash {
        let mut h = GitHash::default();
        h.0[0] = b;
        h
    }

    /// makeTestHashU16: big-endian u16 in the first two bytes, for wider variety.
    fn make_test_hash_u16(val: u16) -> GitHash {
        let mut h = GitHash::default();
        let be = val.to_be_bytes();
        h.0[0] = be[0];
        h.0[1] = be[1];
        h
    }

    // TestLRUBlobCache_GetPut
    #[test]
    fn get_put() {
        let c = LruBlobCache::new(1024);
        let hash = make_test_hash(1);
        let blob = make_test_blob(b"hello world");

        assert!(c.get(&hash).is_none());
        c.put(hash, Some(blob.clone()));
        let got = c.get(&hash).expect("present");
        assert_eq!(got.data, blob.data);
    }

    // TestLRUBlobCache_LRUEviction
    #[test]
    fn lru_eviction() {
        let c = LruBlobCache::new(100);
        let (h1, h2, h3) = (make_test_hash(1), make_test_hash(2), make_test_hash(3));
        let blob = make_test_blob(&[0u8; 40]);

        c.put(h1, Some(blob.clone()));
        c.put(h2, Some(blob.clone()));
        assert!(c.get(&h1).is_some());
        assert!(c.get(&h2).is_some());

        // Re-access h2 so h1 becomes LRU.
        c.get(&h2);
        c.put(h3, Some(blob.clone()));

        assert!(c.get(&h1).is_none(), "hash1 should be evicted");
        assert!(c.get(&h2).is_some(), "hash2 should still be in cache");
        assert!(c.get(&h3).is_some(), "hash3 should be in cache");
    }

    // TestLRUBlobCache_SkipLargeBlobs
    #[test]
    fn skip_large_blobs() {
        let c = LruBlobCache::new(100);
        let hash = make_test_hash(1);
        c.put(hash, Some(make_test_blob(&[0u8; 200])));
        assert!(c.get(&hash).is_none());
    }

    // TestLRUBlobCache_NilBlob
    #[test]
    fn nil_blob() {
        let c = LruBlobCache::new(1024);
        let hash = make_test_hash(1);
        c.put(hash, None);
        assert!(c.get(&hash).is_none());
    }

    // TestLRUBlobCache_DuplicatePut
    #[test]
    fn duplicate_put() {
        let c = LruBlobCache::new(1024);
        let hash = make_test_hash(1);
        let blob = make_test_blob(b"data");
        c.put(hash, Some(blob.clone()));
        c.put(hash, Some(blob));
        assert_eq!(c.stats().entries, 1);
    }

    // TestLRUBlobCache_GetMulti
    #[test]
    fn get_multi() {
        let c = LruBlobCache::new(1024);
        let (h1, h2, h3) = (make_test_hash(1), make_test_hash(2), make_test_hash(3));
        c.put(h1, Some(make_test_blob(b"blob1")));
        c.put(h2, Some(make_test_blob(b"blob2")));

        let (found, missing) = c.get_multi(&[h1, h2, h3]);
        assert_eq!(found.len(), 2);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], h3);
        assert!(found.contains_key(&h1));
        assert!(found.contains_key(&h2));
    }

    // TestLRUBlobCache_PutMulti
    #[test]
    fn put_multi() {
        let c = LruBlobCache::new(1024);
        let (h1, h2) = (make_test_hash(1), make_test_hash(2));
        let mut blobs = HashMap::new();
        blobs.insert(h1, make_test_blob(b"blob1"));
        blobs.insert(h2, make_test_blob(b"blob2"));
        c.put_multi(blobs);

        assert_eq!(c.stats().entries, 2);
        assert!(c.get(&h1).is_some());
        assert!(c.get(&h2).is_some());
    }

    // TestLRUBlobCache_Stats
    #[test]
    fn stats() {
        let c = LruBlobCache::new(1024);
        let (h1, h2) = (make_test_hash(1), make_test_hash(2));
        c.put(h1, Some(make_test_blob(b"hello")));
        c.get(&h1); // hit
        c.get(&h2); // miss

        let s = c.stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.entries, 1);
        assert!((s.hit_rate() - 0.5).abs() < 0.001);
    }

    // TestLRUBlobCache_Clear
    #[test]
    fn clear() {
        let c = LruBlobCache::new(1024);
        let hash = make_test_hash(1);
        c.put(hash, Some(make_test_blob(b"data")));
        assert!(c.get(&hash).is_some());

        c.clear();
        assert!(c.get(&hash).is_none());
        let s = c.stats();
        assert_eq!(s.entries, 0);
        assert_eq!(s.current_size, 0);
    }

    // TestLRUStats_HitRate_Empty
    #[test]
    fn stats_hit_rate_empty() {
        assert!((LruStats::default().hit_rate() - 0.0).abs() < 0.001);
    }

    // TestLRUBlobCache_DefaultSize
    #[test]
    fn default_size() {
        let c = LruBlobCache::new(0);
        assert_eq!(c.stats().max_size, DEFAULT_LRU_CACHE_SIZE);
    }

    // TestLRUBlobCache_BloomFiltersAbsentKeys
    #[test]
    fn bloom_filters_absent_keys() {
        const CACHE_SIZE: i64 = 64 * 1024;
        const INSERT: usize = 100;
        const PROBE: usize = 200;
        let c = LruBlobCache::new(CACHE_SIZE);

        for i in 0..INSERT {
            c.put(
                make_test_hash_u16(i as u16),
                Some(make_test_blob(b"bloom-test-data")),
            );
        }
        for i in INSERT..INSERT + PROBE {
            assert!(c.get(&make_test_hash_u16(i as u16)).is_none());
        }
        assert!(c.stats().bloom_filtered > 0);
    }

    // TestLRUBlobCache_BloomNoFalseNegatives
    #[test]
    fn bloom_no_false_negatives() {
        const CACHE_SIZE: i64 = 64 * 1024;
        const INSERT: usize = 100;
        let c = LruBlobCache::new(CACHE_SIZE);

        for i in 0..INSERT {
            c.put(
                make_test_hash_u16(i as u16),
                Some(make_test_blob(b"bloom-test-data")),
            );
        }
        for i in 0..INSERT {
            assert!(
                c.get(&make_test_hash_u16(i as u16)).is_some(),
                "inserted key {i} must be found (no false negatives)"
            );
        }
    }

    // TestLRUBlobCache_BloomFilteredStats
    #[test]
    fn bloom_filtered_stats() {
        const CACHE_SIZE: i64 = 64 * 1024;
        const PROBE: usize = 200;
        let c = LruBlobCache::new(CACHE_SIZE);

        for i in 0..PROBE {
            c.get(&make_test_hash_u16(i as u16));
        }
        let s = c.stats();
        assert_eq!(s.misses, PROBE as i64);
        assert_eq!(
            s.bloom_filtered, PROBE as i64,
            "all lookups on empty cache should be Bloom-filtered"
        );
    }

    // TestLRUBlobCache_BloomResetOnClear
    #[test]
    fn bloom_reset_on_clear() {
        const CACHE_SIZE: i64 = 64 * 1024;
        let c = LruBlobCache::new(CACHE_SIZE);
        let hash = make_test_hash(1);
        c.put(hash, Some(make_test_blob(b"bloom-test-data")));
        assert!(c.get(&hash).is_some());

        c.clear();
        assert!(c.get(&hash).is_none(), "cleared key should not be found");
        assert!(
            c.stats().bloom_filtered > 0,
            "lookup after clear should be Bloom-filtered"
        );
    }

    // TestLRUBlobCache_GetMultiBloomFiltering
    #[test]
    fn get_multi_bloom_filtering() {
        const CACHE_SIZE: i64 = 64 * 1024;
        const INSERT: usize = 100;
        let c = LruBlobCache::new(CACHE_SIZE);

        for i in 0..INSERT {
            c.put(
                make_test_hash_u16((i * 2) as u16),
                Some(make_test_blob(b"bloom-test-data")),
            );
        }

        let mut hashes = Vec::with_capacity(INSERT * 2);
        for i in 0..INSERT {
            hashes.push(make_test_hash_u16((i * 2) as u16)); // present
            hashes.push(make_test_hash_u16((i * 2 + 1) as u16)); // absent
        }

        let (found, missing) = c.get_multi(&hashes);
        assert_eq!(found.len(), INSERT, "all inserted hashes should be found");
        assert_eq!(missing.len(), INSERT, "all absent hashes should be missing");
        assert!(c.stats().bloom_filtered > 0);
    }

    // TestLRUBlobCache_BloomAfterEviction
    #[test]
    fn bloom_after_eviction() {
        let c = LruBlobCache::new(100);
        let (h1, h2, h3) = (make_test_hash(1), make_test_hash(2), make_test_hash(3));
        let blob40 = make_test_blob(&[0u8; 40]);

        c.put(h1, Some(blob40.clone()));
        c.put(h2, Some(blob40.clone()));
        c.get(&h2); // make h1 LRU
        c.put(h3, Some(blob40)); // evicts h1

        assert!(c.get(&h1).is_none(), "evicted key should return nil");
        assert!(c.get(&h2).is_some(), "hash2 should still be in cache");
        assert!(c.get(&h3).is_some(), "hash3 should be in cache");
    }

    // TestLRUBlobCache_ConcurrentAccess
    #[test]
    fn concurrent_access() {
        const GOROUTINES: usize = 50;
        const OPERATIONS: usize = 100;
        let c = Arc::new(LruBlobCache::new(10 * 1024));

        let mut handles = Vec::with_capacity(GOROUTINES);
        for g in 0..GOROUTINES {
            let c = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for i in 0..OPERATIONS {
                    let hash = make_test_hash(((g * OPERATIONS + i) % 256) as u8);
                    c.put(hash, Some(make_test_blob(b"data")));
                    c.get(&hash);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let s = c.stats();
        assert!(s.entries > 0);
        assert!(s.current_size <= s.max_size);
    }
}
