//! Minimal cross-crate interface shapes shared with the pipeline stages.
//!
//! The dependency-light modules in this crate need just enough of the
//! git/cache shapes to compile and be tested without pulling in the heavier
//! crates. The byte layouts match the owning crates' types exactly (e.g.
//! [`Hash`] is layout-identical to `cf_gitlib::Hash`), so values convert
//! mechanically at the boundary.
//!
//! These are intentionally NOT re-exported as the crate's public model; they
//! are a boundary seam.

/// Number of bytes in a git object hash (SHA-1).
pub const HASH_SIZE: usize = 20;

/// A git object hash (raw SHA-1 bytes).
///
/// This is the only git shape these modules need (it is the key material for
/// [`crate::diff_cache::DiffKey`]); it is layout-identical to the gitlib hash
/// type.
pub type Hash = [u8; HASH_SIZE];

/// Cache hit/miss counter provider.
///
/// Both the blob cache and the [`crate::diff_cache`] implement this; the
/// coordinator reads deltas across a run.
pub trait CacheStatsProvider {
    /// Total cache hits.
    fn cache_hits(&self) -> i64;
    /// Total cache misses.
    fn cache_misses(&self) -> i64;
}

/// Returns the current hit/miss counters, or `(0, 0)` when no cache is
/// configured.
#[must_use]
pub fn cache_stats<C: CacheStatsProvider>(cache: Option<&C>) -> (i64, i64) {
    cache.map_or((0, 0), |c| (c.cache_hits(), c.cache_misses()))
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
        let c = FakeCache { hits: 7, misses: 3 };
        assert_eq!(cache_stats(Some(&c)), (7, 3));
    }
}
