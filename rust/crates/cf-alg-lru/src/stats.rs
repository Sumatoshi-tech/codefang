//! Cache performance metrics.
//!
//! Rust port of the Go `stats.go` file: the [`Stats`] snapshot struct, its
//! [`Stats::hit_rate`] derivation, and the [`Cache::stats`],
//! [`Cache::cache_hits`], and [`Cache::cache_misses`] accessors.

use std::hash::Hash;
use std::sync::atomic::Ordering;

use crate::Cache;

/// A snapshot of cache performance metrics.
///
/// Mirrors Go's `Stats` struct field-for-field. Field types match the Go
/// counterparts (`int64` → [`i64`], `int` → [`i64`] for the entry count to
/// avoid platform-width ambiguity; the values are always small and
/// non-negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// Total number of cache hits.
    pub hits: i64,
    /// Total number of cache misses.
    pub misses: i64,
    /// Lookups short-circuited by the Bloom pre-filter.
    pub bloom_filtered: i64,
    /// Current number of entries in the cache.
    pub entries: i64,
    /// Current total size in bytes.
    pub current_size: i64,
    /// Maximum number of entries; 0 when the count-based limit is not set.
    pub max_entries: i64,
    /// Maximum total size in bytes; 0 when the size-based limit is not set.
    pub max_size: i64,
}

impl Stats {
    /// Returns the cache hit rate as a fraction in `[0.0, 1.0]`.
    ///
    /// Returns `0.0` when there have been no lookups. Mirrors Go's
    /// `Stats.HitRate`.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }

        self.hits as f64 / total as f64
    }
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Returns current cache statistics. Mirrors Go's `Stats`.
    #[must_use]
    pub fn stats(&self) -> Stats {
        let inner = self.read_inner_for_stats();

        Stats {
            hits: self.hits_atomic().load(Ordering::Relaxed),
            misses: self.misses_atomic().load(Ordering::Relaxed),
            bloom_filtered: self.bloom_filtered_atomic().load(Ordering::Relaxed),
            entries: inner.0 as i64,
            current_size: inner.1,
            max_entries: self.max_entries_value(),
            max_size: self.max_size_value(),
        }
    }

    /// Returns the total cache hit count (atomic, lock-free). Mirrors Go's
    /// `CacheHits`.
    #[must_use]
    pub fn cache_hits(&self) -> i64 {
        self.hits_atomic().load(Ordering::Relaxed)
    }

    /// Returns the total cache miss count (atomic, lock-free). Mirrors Go's
    /// `CacheMisses`.
    #[must_use]
    pub fn cache_misses(&self) -> i64 {
        self.misses_atomic().load(Ordering::Relaxed)
    }
}
