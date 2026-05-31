//! Minimal cross-crate interfaces for the not-yet-ported dependencies.
//!
//! The framework's concrete pipeline stages reference types owned by other Go
//! packages that are still stubs in the Rust workspace (`cf-gitlib`,
//! `cf-cache`, `cf-uast`, `cf-plumbing`, `cf-analyze`). Until those crates
//! land, the dependency-light modules in this crate need just enough of those
//! shapes to compile and be tested. The definitions here mirror the Go shapes
//! exactly so that, when the upstream crates are ported, callers can switch
//! `use cf_framework::interfaces::Hash` to `use cf_gitlib::Hash` mechanically.
//!
//! These are intentionally NOT re-exported as the crate's public model; they
//! are a temporary seam (see the crate-level "Port status" docs).

/// Number of bytes in a git object hash (SHA-1). Mirrors `gitlib.HashSize`.
pub const HASH_SIZE: usize = 20;

/// A git object hash. Mirrors Go `gitlib.Hash` (`type Hash [20]byte`).
///
/// This is the only `cf-gitlib` shape the ported modules need (it is the key
/// material for [`crate::diff_cache::DiffKey`]). The real `cf-gitlib::Hash`
/// will replace this with an identical byte layout.
pub type Hash = [u8; HASH_SIZE];

/// Cache hit/miss counter provider. Mirrors Go's private `cacheStatsProvider`
/// interface in `coordinator.go`, used by the generic `cacheStats` helper.
///
/// Both the blob cache (`cf-cache::LRUBlobCache`) and the [`crate::diff_cache`]
/// implement this; the coordinator reads deltas across a run.
pub trait CacheStatsProvider {
    /// Total cache hits (atomic, lock-free in the Go impl).
    fn cache_hits(&self) -> i64;
    /// Total cache misses (atomic, lock-free in the Go impl).
    fn cache_misses(&self) -> i64;
}

/// Returns the current hit/miss counters, or `(0, 0)` when absent.
///
/// Mirrors Go's `cacheStats[T cacheStatsProvider](c T)`: a nil cache reports
/// zero. In Rust the "nil" case is modeled by `Option`.
#[must_use]
pub fn cache_stats<C: CacheStatsProvider>(cache: Option<&C>) -> (i64, i64) {
    match cache {
        Some(c) => (c.cache_hits(), c.cache_misses()),
        None => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeCache {
        hits: i64,
        misses: i64,
    }

    impl CacheStatsProvider for FakeCache {
        fn cache_hits(&self) -> i64 {
            self.hits
        }
        fn cache_misses(&self) -> i64 {
            self.misses
        }
    }

    #[test]
    fn hash_size_is_twenty() {
        let h: Hash = [0u8; HASH_SIZE];
        assert_eq!(h.len(), 20);
    }

    #[test]
    fn cache_stats_none_is_zero() {
        assert_eq!(cache_stats::<FakeCache>(None), (0, 0));
    }

    #[test]
    fn cache_stats_some_reports_counters() {
        let c = FakeCache {
            hits: 7,
            misses: 3,
        };
        assert_eq!(cache_stats(Some(&c)), (7, 3));
    }
}
