//! Diff cache keying and statistics — port of the self-contained parts of
//! `internal/framework/diff_cache.go`.
//!
//! The Go `DiffCache` is an LRU of `plumbing.FileDiffData` keyed by
//! `(old_hash, new_hash)`, fronted by a Bloom pre-filter. The LRU + Bloom
//! machinery is owned by `cf-alg-lru` (already ported) and the cached value
//! type is `cf-plumbing::FileDiffData` (not yet ported). What is **fully
//! self-contained** — and ported here verbatim — is:
//!
//! - [`DiffKey`] and its exact Bloom-filter byte layout ([`diff_key_to_bytes`]),
//!   which is byte-identity-relevant because it feeds the Bloom hash.
//! - [`DiffCacheStats`] and [`DiffCacheStats::hit_rate`].
//! - [`DEFAULT_DIFF_CACHE_SIZE`].
//!
//! The concrete `DiffCache` (LRU wiring over `cf-alg-lru` + `FileDiffData`) is
//! deferred until `cf-plumbing` lands; see the crate-level port notes.

use crate::interfaces::{Hash as GitHash, HASH_SIZE};

/// Default maximum number of diff entries to cache. Mirrors Go
/// `DefaultDiffCacheSize`.
pub const DEFAULT_DIFF_CACHE_SIZE: usize = 10000;

/// Uniquely identifies a diff computation by blob hashes. Mirrors Go
/// `framework.DiffKey`.
///
/// Implements [`std::hash::Hash`] (hand-written rather than derived, to avoid
/// the derive macro resolving the field type name) so it can key an in-memory
/// map/LRU — the Go type is a comparable struct used as a Go map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffKey {
    /// The old (pre-image) blob hash.
    pub old_hash: GitHash,
    /// The new (post-image) blob hash.
    pub new_hash: GitHash,
}

impl std::hash::Hash for DiffKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write(&self.old_hash);
        state.write(&self.new_hash);
    }
}

/// Returns the concatenated hash bytes for Bloom filter lookup.
///
/// Mirrors Go `diffKeyToBytes`: a fixed `2 * HashSize` buffer with `old_hash`
/// in the low half and `new_hash` in the high half. The exact ordering matters
/// because these bytes are hashed by the Bloom pre-filter.
#[must_use]
pub fn diff_key_to_bytes(key: &DiffKey) -> [u8; 2 * HASH_SIZE] {
    let mut buf = [0u8; 2 * HASH_SIZE];
    buf[..HASH_SIZE].copy_from_slice(&key.old_hash);
    buf[HASH_SIZE..].copy_from_slice(&key.new_hash);
    buf
}

/// Statistics about diff cache usage. Mirrors Go `framework.DiffCacheStats`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffCacheStats {
    /// Number of lookups that found an entry.
    pub hits: i64,
    /// Number of lookups that missed.
    pub misses: i64,
    /// Lookups short-circuited by the Bloom pre-filter.
    pub bloom_skips: i64,
    /// Current number of entries.
    pub entries: usize,
    /// Maximum number of entries.
    pub max_entries: usize,
}

impl DiffCacheStats {
    /// Returns the cache hit rate as a fraction. Mirrors Go `HitRate()`:
    /// `hits / (hits + misses)`, or `0` when there have been no lookups.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_byte_layout_is_old_then_new() {
        let key = DiffKey {
            old_hash: [0xAA; HASH_SIZE],
            new_hash: [0xBB; HASH_SIZE],
        };
        let bytes = diff_key_to_bytes(&key);
        assert_eq!(&bytes[..HASH_SIZE], &[0xAA; HASH_SIZE]);
        assert_eq!(&bytes[HASH_SIZE..], &[0xBB; HASH_SIZE]);
        assert_eq!(bytes.len(), 40);
    }

    #[test]
    fn key_equality_and_hashing() {
        use std::collections::HashSet;
        let a = DiffKey {
            old_hash: [1; HASH_SIZE],
            new_hash: [2; HASH_SIZE],
        };
        let b = DiffKey {
            old_hash: [1; HASH_SIZE],
            new_hash: [2; HASH_SIZE],
        };
        let c = DiffKey {
            old_hash: [2; HASH_SIZE],
            new_hash: [1; HASH_SIZE],
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    #[test]
    fn hit_rate_zero_when_no_lookups() {
        let s = DiffCacheStats::default();
        assert_eq!(s.hit_rate(), 0.0);
    }

    #[test]
    fn hit_rate_fraction() {
        let s = DiffCacheStats {
            hits: 3,
            misses: 1,
            ..DiffCacheStats::default()
        };
        assert!((s.hit_rate() - 0.75).abs() < 1e-12);
    }
}
