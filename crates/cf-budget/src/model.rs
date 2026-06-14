//! Component memory cost model and libgit2 native memory limits.

use crate::units::{GIB, KIB, MIB};
use crate::PERCENT_DIVISOR;

// --- Component memory sizes (empirically measured) ---

/// The fixed runtime + libgit2 overhead.
///
/// Includes shared mmap of pack files (~200 MB for large repos).
pub const BASE_OVERHEAD: i64 = 250 * MIB;

/// The heap memory per worker for the libgit2 repository handle.
pub const REPO_HANDLE_SIZE: i64 = 10 * MIB;

/// The per-worker C/mmap overhead from libgit2.
///
/// Each worker opens the repo and mmaps pack index files; the OS faults in pack
/// data pages during object lookups. Empirically ~50-100 MB per worker on large
/// repos due to shared pack page cache pressure.
pub const WORKER_NATIVE_OVERHEAD: i64 = 50 * MIB;

/// The average size of a cached diff entry.
pub const AVG_DIFF_SIZE: i64 = 2 * KIB;

/// The average size of in-flight commit data.
pub const AVG_COMMIT_DATA_SIZE: i64 = 64 * KIB;

/// Caps the blob cache to avoid dominating the budget.
///
/// Beyond 256 MB the hit rate improvement is marginal for most repositories.
pub const MAX_BLOB_CACHE_SIZE: i64 = 256 * MIB;

/// Caps the diff cache.
///
/// Beyond 20K entries the benefit is marginal and memory cost grows linearly.
pub const MAX_DIFF_CACHE_ENTRIES: i64 = 20000;

/// libgit2's default mmap limit (8 GiB on 64-bit).
///
/// This allows pack file windows to consume enormous RSS on large repos.
pub const DEFAULT_MWINDOW_MAPPED_LIMIT: i64 = 8 * GIB;

/// libgit2's default object cache (256 MiB).
pub const DEFAULT_LIBGIT2_CACHE_SIZE: i64 = 256 * MIB;

/// The fraction (percent) of the budget reserved for libgit2 native memory
/// (mwindow + object cache + decompression buffers).
///
/// The rest is available to the heap, caches, and buffers.
pub const NATIVE_MEMORY_PERCENT: i64 = 25;

/// Controls how the native allocation is split: 30% for mwindow (mmap'd pack
/// data), 70% for object cache.
///
/// Lowered from 80 to reduce RSS from pack file mmap windows. The larger object
/// cache compensates by keeping decompressed objects longer, reducing
/// re-decompression overhead.
pub const MWINDOW_CACHE_RATIO: i64 = 30;

/// Limits glibc malloc arenas to prevent RSS bloat.
///
/// glibc defaults to 8*cores which retains freed memory across ~192 arenas on a
/// 24-core machine, inflating RSS by 3-4x. A value of 2 reduces peak RSS by
/// ~60% vs default, with minimal throughput impact when combined with
/// `malloc_trim(0)` between chunks to reclaim freed arena memory.
pub const DEFAULT_MALLOC_ARENA_MAX: i64 = 2;

/// libgit2 global memory limits derived from the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NativeLimits {
    /// `mwindow` mapped-memory limit, in bytes.
    pub mwindow_mapped_limit: i64,
    /// Object cache maximum size, in bytes.
    pub cache_max_size: i64,
    /// glibc `MALLOC_ARENA_MAX` value.
    pub malloc_arena_max: i64,
}

/// Computes libgit2 memory limits proportional to the memory budget.
///
/// Returns zero values when no budget is set (use defaults), i.e. when
/// `budget <= 0`.
#[must_use]
pub fn native_limits_for_budget(budget: i64) -> NativeLimits {
    if budget <= 0 {
        return NativeLimits::default();
    }

    let native_alloc = budget * NATIVE_MEMORY_PERCENT / PERCENT_DIVISOR;
    let mwindow = native_alloc * MWINDOW_CACHE_RATIO / PERCENT_DIVISOR;
    let cache = native_alloc - mwindow;

    NativeLimits {
        mwindow_mapped_limit: mwindow,
        cache_max_size: cache,
        malloc_arena_max: DEFAULT_MALLOC_ARENA_MAX,
    }
}

/// Calculates the estimated memory usage for a given configuration.
///
/// The inverse of the solver's cost model; the solver tests use it to verify
/// that derived configs never exceed their budget, and callers can use it to
/// estimate a config's footprint.
#[must_use]
pub const fn estimate_memory_usage(cfg: &crate::framework::CoordinatorConfig) -> i64 {
    let worker_memory = cfg.workers * (REPO_HANDLE_SIZE + cfg.blob_arena_size);
    let native_memory = cfg.workers * WORKER_NATIVE_OVERHEAD;
    let cache_memory = cfg.blob_cache_size + cfg.diff_cache_size * AVG_DIFF_SIZE;
    let buffer_memory = cfg.buffer_size * AVG_COMMIT_DATA_SIZE;

    BASE_OVERHEAD + worker_memory + native_memory + cache_memory + buffer_memory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::{default_coordinator_config, CoordinatorConfig};
    use crate::units::MIB;

    #[test]
    fn estimate_memory_usage_default_config() {
        let cfg = default_coordinator_config();
        let estimate = estimate_memory_usage(&cfg);
        // Estimate should be positive and reasonable (at least base overhead).
        assert!(estimate > 0, "estimate should be positive");
        assert!(
            estimate >= BASE_OVERHEAD,
            "estimate should include base overhead"
        );
    }

    #[test]
    fn estimate_memory_usage_minimal_config() {
        let minimal_cfg = CoordinatorConfig {
            workers: 1,
            buffer_size: 1,
            blob_cache_size: MIB,
            diff_cache_size: 100,
            blob_arena_size: MIB,
            ..CoordinatorConfig::default()
        };
        let default_cfg = default_coordinator_config();

        let minimal_estimate = estimate_memory_usage(&minimal_cfg);
        let default_estimate = estimate_memory_usage(&default_cfg);

        // Minimal config should use less memory than default.
        assert!(
            minimal_estimate < default_estimate,
            "minimal config should use less memory"
        );
        // But still include base overhead.
        assert!(
            minimal_estimate >= BASE_OVERHEAD,
            "should include base overhead"
        );
    }

    #[test]
    fn estimate_memory_usage_monotonic_workers() {
        let base_cfg = CoordinatorConfig {
            workers: 2,
            buffer_size: 4,
            blob_cache_size: 100 * MIB,
            diff_cache_size: 1000,
            blob_arena_size: 4 * MIB,
            ..CoordinatorConfig::default()
        };
        let mut more_cfg = base_cfg.clone();
        more_cfg.workers = 4;

        assert!(
            estimate_memory_usage(&more_cfg) > estimate_memory_usage(&base_cfg),
            "more workers should increase memory"
        );
    }

    #[test]
    fn estimate_memory_usage_monotonic_blob_cache() {
        let base_cfg = CoordinatorConfig {
            workers: 2,
            buffer_size: 4,
            blob_cache_size: 100 * MIB,
            diff_cache_size: 1000,
            blob_arena_size: 4 * MIB,
            ..CoordinatorConfig::default()
        };
        let mut more_cfg = base_cfg.clone();
        more_cfg.blob_cache_size = 500 * MIB;

        assert!(
            estimate_memory_usage(&more_cfg) > estimate_memory_usage(&base_cfg),
            "larger blob cache should increase memory"
        );
    }

    #[test]
    fn estimate_memory_usage_monotonic_diff_cache() {
        let base_cfg = CoordinatorConfig {
            workers: 2,
            buffer_size: 4,
            blob_cache_size: 100 * MIB,
            diff_cache_size: 1000,
            blob_arena_size: 4 * MIB,
            ..CoordinatorConfig::default()
        };
        let mut more_cfg = base_cfg.clone();
        more_cfg.diff_cache_size = 10000;

        assert!(
            estimate_memory_usage(&more_cfg) > estimate_memory_usage(&base_cfg),
            "larger diff cache should increase memory"
        );
    }

    #[test]
    fn native_limits_zero_budget() {
        assert_eq!(native_limits_for_budget(0), NativeLimits::default());
        assert_eq!(native_limits_for_budget(-1), NativeLimits::default());
    }

    #[test]
    fn native_limits_split() {
        // 1 GiB budget: native = 25% = 256 MiB; mwindow = 30% of that; cache = rest.
        let budget = GIB;
        let limits = native_limits_for_budget(budget);
        let native_alloc = budget * NATIVE_MEMORY_PERCENT / PERCENT_DIVISOR;
        let expected_mwindow = native_alloc * MWINDOW_CACHE_RATIO / PERCENT_DIVISOR;
        assert_eq!(limits.mwindow_mapped_limit, expected_mwindow);
        assert_eq!(limits.cache_max_size, native_alloc - expected_mwindow);
        assert_eq!(limits.malloc_arena_max, DEFAULT_MALLOC_ARENA_MAX);
        // mwindow + cache must equal the native allocation exactly.
        assert_eq!(
            limits.mwindow_mapped_limit + limits.cache_max_size,
            native_alloc
        );
    }
}
