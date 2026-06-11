//! History-coordinator budget solver.
//!
//! Port of `internal/budget/solver.go`. Distributes a memory budget across
//! workers, caches and buffers and derives a [`CoordinatorConfig`].

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use crate::framework::CoordinatorConfig;
use crate::model::{
    AVG_COMMIT_DATA_SIZE, AVG_DIFF_SIZE, BASE_OVERHEAD, MAX_BLOB_CACHE_SIZE, MAX_DIFF_CACHE_ENTRIES,
    REPO_HANDLE_SIZE, WORKER_NATIVE_OVERHEAD,
};
use crate::units::MIB;
use crate::{num_cpu, PERCENT_DIVISOR};

// --- Allocation proportions for budget distribution ---

/// The percentage of available budget for caches.
pub const CACHE_ALLOCATION_PERCENT: i64 = 60;
/// The percentage of available budget for workers.
pub const WORKER_ALLOCATION_PERCENT: i64 = 30;
/// The percentage of available budget for buffers.
pub const BUFFER_ALLOCATION_PERCENT: i64 = 10;
/// Percentage reserved for runtime overhead.
pub const SLACK_PERCENT: i64 = 5;
/// The portion (percent) of cache allocation for the blob cache.
pub const BLOB_CACHE_RATIO: i64 = 80;
/// The portion (percent) of cache allocation for the diff cache.
pub const DIFF_CACHE_RATIO: i64 = 20;

// --- Float64 weights derived from the integer percentage constants above ---
//
// These mirror Go's `float64(CONST) / percentDivisor` exactly: the literal
// `100.0` divisor and identical IEEE-754 division give bit-identical weights,
// so `(total as f64 * weight) as i64` matches `int64(float64(total) * weight)`.

/// Cache weight (`CACHE_ALLOCATION_PERCENT / 100`).
pub const CACHE_WEIGHT: f64 = CACHE_ALLOCATION_PERCENT as f64 / PERCENT_DIVISOR as f64;
/// Worker weight (`WORKER_ALLOCATION_PERCENT / 100`).
pub const WORKER_WEIGHT: f64 = WORKER_ALLOCATION_PERCENT as f64 / PERCENT_DIVISOR as f64;
/// Buffer weight (`BUFFER_ALLOCATION_PERCENT / 100`).
pub const BUFFER_WEIGHT: f64 = BUFFER_ALLOCATION_PERCENT as f64 / PERCENT_DIVISOR as f64;
/// Blob sub-cache weight (`BLOB_CACHE_RATIO / 100`).
pub const BLOB_WEIGHT: f64 = BLOB_CACHE_RATIO as f64 / PERCENT_DIVISOR as f64;
/// Diff sub-cache weight (`DIFF_CACHE_RATIO / 100`).
pub const DIFF_WEIGHT: f64 = DIFF_CACHE_RATIO as f64 / PERCENT_DIVISOR as f64;

// --- Bucket name constants for `allocate_proportionally` ---

const BUCKET_CACHE: &str = "cache";
const BUCKET_WORKER: &str = "worker";
const BUCKET_BUFFER: &str = "buffer";
const BUCKET_BLOB: &str = "blob";
const BUCKET_DIFF: &str = "diff";

// --- Solver constraints ---

/// The smallest budget the solver will accept.
///
/// Must exceed `BASE_OVERHEAD` (250 MiB) plus room for at least 1 worker.
pub const MINIMUM_BUDGET: i64 = 512 * MIB;

/// The default blob arena size.
///
/// 8 MiB reduces fallback to per-blob C malloc (which accumulates in glibc
/// arenas as retained native RSS) by fitting ~97% of blob batches.
pub const DEFAULT_ARENA_SIZE: i64 = 8 * MIB;

/// The maximum arena size allowed.
pub const MAX_ARENA_SIZE: i64 = 16 * MIB;

/// Commit batch size used for all budget-derived configs.
pub const DEFAULT_COMMIT_BATCH_SIZE: i64 = 100;

/// The minimum number of workers.
pub const MIN_WORKERS: i64 = 1;
/// The minimum buffer size.
pub const MIN_BUFFER_SIZE: i64 = 2;
/// The minimum diff cache entries.
pub const MIN_DIFF_CACHE_SIZE: i64 = 100;
/// The minimum blob cache size, in bytes.
pub const MIN_BLOB_CACHE_SIZE: i64 = MIB;

/// The percentage of CPU cores to use for workers.
///
/// Testing shows ~60% provides optimal performance due to contention overhead.
pub const OPTIMAL_WORKER_RATIO: i64 = 60;
/// The percentage of CPU cores for UAST pipeline workers.
pub const UAST_PIPELINE_WORKER_RATIO: i64 = 40;
/// Controls leaf worker count: `NumCPU / divisor`.
pub const LEAF_WORKER_DIVISOR: i64 = 3;
/// The minimum number of leaf workers.
pub const MIN_LEAF_WORKERS: i64 = 4;

/// Solver error.
///
/// Mirrors the single sentinel `ErrBudgetTooSmall` from `solver.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveError {
    /// The budget is below the minimum required.
    BudgetTooSmall,
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Exact wording matches Go's `errors.New("memory budget is too small")`.
            SolveError::BudgetTooSmall => f.write_str("memory budget is too small"),
        }
    }
}

impl std::error::Error for SolveError {}

/// Distributes `total` bytes across named buckets by weight.
///
/// Weights must be in `[0, 1]` and should sum to `<= 1.0`. Returns a map from
/// bucket name to allocated bytes (truncated toward zero, matching Go's
/// `int64(float64(total) * weight)`).
pub(crate) fn allocate_proportionally(
    total: i64,
    weights: &HashMap<String, f64>,
) -> HashMap<String, i64> {
    let mut result = HashMap::with_capacity(weights.len());
    for (name, &weight) in weights {
        result.insert(name.clone(), (total as f64 * weight) as i64);
    }
    result
}

/// Calculates the optimal [`CoordinatorConfig`] for the given memory budget.
///
/// The solver distributes available memory across workers, caches and buffers
/// while ensuring the total estimated usage stays within budget.
///
/// # Errors
///
/// Returns [`SolveError::BudgetTooSmall`] when the budget is below
/// [`MINIMUM_BUDGET`] or when no memory remains after slack and base overhead.
pub fn solve_for_budget(budget: i64) -> Result<CoordinatorConfig, SolveError> {
    if budget < MINIMUM_BUDGET {
        return Err(SolveError::BudgetTooSmall);
    }

    let usable_budget = budget * (PERCENT_DIVISOR - SLACK_PERCENT) / PERCENT_DIVISOR;
    let available = usable_budget - BASE_OVERHEAD;

    if available <= 0 {
        return Err(SolveError::BudgetTooSmall);
    }

    let weights: HashMap<String, f64> = [
        (BUCKET_CACHE.to_string(), CACHE_WEIGHT),
        (BUCKET_WORKER.to_string(), WORKER_WEIGHT),
        (BUCKET_BUFFER.to_string(), BUFFER_WEIGHT),
    ]
    .into_iter()
    .collect();
    let allocs = allocate_proportionally(available, &weights);

    let cfg = derive_knobs(
        allocs[BUCKET_CACHE],
        allocs[BUCKET_WORKER],
        allocs[BUCKET_BUFFER],
    );

    Ok(cfg)
}

/// Calculates individual configuration knobs from allocation budgets.
pub(crate) fn derive_knobs(
    cache_alloc: i64,
    worker_alloc: i64,
    buffer_alloc: i64,
) -> CoordinatorConfig {
    // Workers: maximize within allocation, minimum 1, cap at optimal ratio of
    // CPU cores. Include native overhead (C/mmap) per worker in the cost.
    let max_workers = MIN_WORKERS.max(num_cpu() * OPTIMAL_WORKER_RATIO / PERCENT_DIVISOR);
    let worker_cost = REPO_HANDLE_SIZE + DEFAULT_ARENA_SIZE + WORKER_NATIVE_OVERHEAD;
    let workers = MIN_WORKERS.max(max_workers.min(worker_alloc / worker_cost));

    // Split cache allocation into blob and diff sub-budgets.
    let cache_weights: HashMap<String, f64> = [
        (BUCKET_BLOB.to_string(), BLOB_WEIGHT),
        (BUCKET_DIFF.to_string(), DIFF_WEIGHT),
    ]
    .into_iter()
    .collect();
    let cache_allocs = allocate_proportionally(cache_alloc, &cache_weights);

    // Blob cache: capped to avoid dominating the budget.
    let mut blob_cache_size = MIN_BLOB_CACHE_SIZE.max(cache_allocs[BUCKET_BLOB]);
    blob_cache_size = blob_cache_size.min(MAX_BLOB_CACHE_SIZE);

    // Diff cache: converted to entries, capped.
    let diff_cache_alloc = cache_allocs[BUCKET_DIFF];
    let mut diff_cache_size = MIN_DIFF_CACHE_SIZE.max(diff_cache_alloc / AVG_DIFF_SIZE);
    diff_cache_size = diff_cache_size.min(MAX_DIFF_CACHE_ENTRIES);

    // Buffer size: based on allocation and workers.
    let buffer_size = MIN_BUFFER_SIZE.max(buffer_alloc / AVG_COMMIT_DATA_SIZE);

    // Use default arena size.
    let arena_size = DEFAULT_ARENA_SIZE;

    // UAST pipeline workers: use the UAST pipeline ratio of CPU cores.
    let uast_workers = 1.max(num_cpu() * UAST_PIPELINE_WORKER_RATIO / PERCENT_DIVISOR);

    // Leaf workers: CPU / leafWorkerDivisor, with a floor.
    let leaf_workers = MIN_LEAF_WORKERS.max(num_cpu() / LEAF_WORKER_DIVISOR);

    CoordinatorConfig {
        workers,
        buffer_size,
        commit_batch_size: DEFAULT_COMMIT_BATCH_SIZE,
        blob_cache_size,
        diff_cache_size,
        blob_arena_size: arena_size,
        uast_pipeline_workers: uast_workers,
        leaf_workers,
        ..zero_coordinator_config()
    }
}

/// The zero-valued `CoordinatorConfig`, mirroring Go's zero
/// `framework.CoordinatorConfig{}`.
///
/// Go's `deriveKnobs` returns a *partial* struct literal, which leaves every
/// unnamed field at its zero value. Rust's `CoordinatorConfig::default()` is
/// `DefaultCoordinatorConfig` (NOT zero), so the remaining fields must be
/// zero-filled explicitly to stay faithful to the Go solver's output.
fn zero_coordinator_config() -> CoordinatorConfig {
    CoordinatorConfig {
        commit_batch_size: 0,
        workers: 0,
        buffer_size: 0,
        blob_cache_size: 0,
        diff_cache_size: 0,
        blob_arena_size: 0,
        uast_pipeline_workers: 0,
        leaf_workers: 0,
        gc_percent: 0,
        ballast_size: 0,
        first_parent: false,
        worker_timeout: Duration::ZERO,
        uast_spill_threshold: 0,
        intra_commit_parallel_threshold: 0,
        max_intra_commit_workers: 0,
        max_uast_blob_size: 0,
        uast_parse_timeout: Duration::ZERO,
        max_changes_per_commit: 0,
        tree_diff_pathspec: Vec::new(),
        max_diff_batch_size: 0,
        memory_limit_ratio: 0,
        uast_spill_trim_interval: 0,
        native_trim_interval: 0,
        max_streaming_buffering: 0,
        drain_prefetch_timeout: Duration::ZERO,
        sampler_interval: Duration::ZERO,
        worker_ratio: 0,
        uast_worker_ratio: 0,
        leaf_worker_divisor: 0,
        min_leaf_workers: 0,
        buffer_size_multiplier: 0,
        budget_limit_ratio: 0,
        system_ram_limit_ratio: 0,
        diff_job_buffer_multiplier: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::estimate_memory_usage;
    use crate::num_cpu;
    use crate::units::{GIB, KIB, MIB};

    #[test]
    fn solve_for_budget_medium_budget() {
        let cfg = solve_for_budget(GIB).expect("1 GiB should solve");
        assert!(cfg.workers > 0, "should have at least 1 worker");
        assert!(cfg.buffer_size > 0, "should have positive buffer size");
        assert!(cfg.blob_cache_size > 0, "should have positive blob cache");
        assert!(cfg.diff_cache_size > 0, "should have positive diff cache");
        assert!(cfg.blob_arena_size > 0, "should have positive arena size");
    }

    #[test]
    fn solve_for_budget_small_budget() {
        let cfg = solve_for_budget(MINIMUM_BUDGET).expect("minimum should solve");
        assert!(cfg.workers >= MIN_WORKERS, "should have minimum workers");
        assert!(cfg.buffer_size >= MIN_BUFFER_SIZE, "should have minimum buffer");
    }

    #[test]
    fn solve_for_budget_large_budget() {
        let cfg = solve_for_budget(4 * GIB).expect("4 GiB should solve");
        assert!(cfg.workers > 0);
        assert!(
            cfg.blob_cache_size > 100 * MIB,
            "large budget should have significant cache"
        );
    }

    #[test]
    fn solve_for_budget_too_small() {
        let tiny_budget = 64 * MIB; // Below MINIMUM_BUDGET.
        let err = solve_for_budget(tiny_budget).unwrap_err();
        assert_eq!(err, SolveError::BudgetTooSmall);
    }

    #[test]
    fn solve_for_budget_exactly_minimum() {
        let cfg = solve_for_budget(MINIMUM_BUDGET).expect("minimum should solve");
        assert!(cfg.workers > 0, "should work at minimum budget");
    }

    #[test]
    fn solve_for_budget_never_exceeds_budget() {
        let budgets = [MINIMUM_BUDGET, GIB, 2 * GIB, 4 * GIB];
        for budget in budgets {
            let cfg = solve_for_budget(budget).unwrap_or_else(|_| panic!("budget {budget} should succeed"));
            let estimate = estimate_memory_usage(&cfg);
            assert!(
                estimate <= budget,
                "estimate {estimate} should not exceed budget {budget}"
            );
        }
    }

    #[test]
    fn solve_for_budget_maintains_slack() {
        // Fuzz-style test: verify the solver maintains >5% slack across many
        // budgets, ensuring we never get too close to the limit.
        const SLACK: i64 = 5;
        let mut budget = MINIMUM_BUDGET;
        while budget <= 8 * GIB {
            let cfg = solve_for_budget(budget).unwrap_or_else(|_| panic!("budget {budget} should succeed"));
            let estimate = estimate_memory_usage(&cfg);
            let max_allowed = budget * (PERCENT_DIVISOR - SLACK) / PERCENT_DIVISOR;
            assert!(
                estimate <= max_allowed,
                "estimate {estimate} should be <= {max_allowed} (budget {budget} with {SLACK}% slack)"
            );
            budget += 64 * MIB;
        }
    }

    #[test]
    fn solve_for_budget_deterministic() {
        let budget = GIB;
        let cfg1 = solve_for_budget(budget).unwrap();
        let cfg2 = solve_for_budget(budget).unwrap();
        assert_eq!(cfg1.workers, cfg2.workers);
        assert_eq!(cfg1.buffer_size, cfg2.buffer_size);
        assert_eq!(cfg1.blob_cache_size, cfg2.blob_cache_size);
        assert_eq!(cfg1.diff_cache_size, cfg2.diff_cache_size);
        assert_eq!(cfg1.blob_arena_size, cfg2.blob_arena_size);
    }

    #[test]
    fn solve_for_budget_larger_budget_more_resources() {
        let small_cfg = solve_for_budget(MINIMUM_BUDGET).unwrap();
        let large_cfg = solve_for_budget(2 * GIB).unwrap();
        assert!(
            large_cfg.blob_cache_size >= small_cfg.blob_cache_size,
            "larger budget should have larger or equal blob cache"
        );
        assert!(
            large_cfg.diff_cache_size >= small_cfg.diff_cache_size,
            "larger budget should have larger or equal diff cache"
        );
    }

    #[test]
    fn solve_for_budget_workers_capped_at_cpu_count() {
        let huge_budget = 64 * GIB;
        let cfg = solve_for_budget(huge_budget).unwrap();
        assert!(
            cfg.workers <= num_cpu(),
            "workers should not exceed CPU count"
        );
    }

    #[test]
    fn solve_for_budget_minimum_values_enforced() {
        let cfg = solve_for_budget(MINIMUM_BUDGET).unwrap();
        assert!(cfg.workers >= MIN_WORKERS, "should enforce min workers");
        assert!(cfg.buffer_size >= MIN_BUFFER_SIZE, "should enforce min buffer");
        assert!(
            cfg.diff_cache_size >= MIN_DIFF_CACHE_SIZE,
            "should enforce min diff cache"
        );
        assert!(
            cfg.blob_cache_size >= MIN_BLOB_CACHE_SIZE,
            "should enforce min blob cache"
        );
    }

    #[test]
    fn derive_knobs_zero_allocations() {
        let cfg = derive_knobs(0, 0, 0);
        assert_eq!(cfg.workers, MIN_WORKERS, "should use min workers");
        assert_eq!(cfg.buffer_size, MIN_BUFFER_SIZE, "should use min buffer");
        assert_eq!(cfg.diff_cache_size, MIN_DIFF_CACHE_SIZE, "should use min diff cache");
        assert_eq!(cfg.blob_cache_size, MIN_BLOB_CACHE_SIZE, "should use min blob cache");
    }

    #[test]
    fn derive_knobs_tiny_allocations() {
        let cfg = derive_knobs(KIB, KIB, KIB);
        assert!(cfg.workers >= MIN_WORKERS);
        assert!(cfg.buffer_size >= MIN_BUFFER_SIZE);
        assert!(cfg.diff_cache_size >= MIN_DIFF_CACHE_SIZE);
        assert!(cfg.blob_cache_size >= MIN_BLOB_CACHE_SIZE);
    }

    #[test]
    fn derive_knobs_huge_worker_allocation() {
        let cfg = derive_knobs(100 * MIB, 100 * GIB, 10 * MIB);
        assert!(
            cfg.workers <= num_cpu(),
            "workers capped at CPU count"
        );
    }

    #[test]
    fn allocate_proportionally_single_weight() {
        let total: i64 = 1000;
        let weights: HashMap<String, f64> = [("a".to_string(), 0.6)].into_iter().collect();
        let result = allocate_proportionally(total, &weights);
        assert_eq!(result["a"], 600);
    }

    #[test]
    fn allocate_proportionally_multiple_weights() {
        let total: i64 = 1000;
        let weights: HashMap<String, f64> = [
            ("cache".to_string(), 0.6),
            ("worker".to_string(), 0.3),
            ("buffer".to_string(), 0.1),
        ]
        .into_iter()
        .collect();
        let result = allocate_proportionally(total, &weights);
        assert_eq!(result["cache"], 600);
        assert_eq!(result["worker"], 300);
        assert_eq!(result["buffer"], 100);
    }

    #[test]
    fn allocate_proportionally_zero_total() {
        let weights: HashMap<String, f64> = [("a".to_string(), 0.5)].into_iter().collect();
        let result = allocate_proportionally(0, &weights);
        assert_eq!(result["a"], 0);
    }

    #[test]
    fn allocate_proportionally_nil_weights() {
        let weights: HashMap<String, f64> = HashMap::new();
        let result = allocate_proportionally(1000, &weights);
        assert!(result.is_empty());
    }

    #[test]
    fn allocate_proportionally_truncation() {
        // 1001 * 0.3 = 300.3 -> truncated to 300 (matches Go int64(float64*...)).
        let total: i64 = 1001;
        let weights: HashMap<String, f64> = [("a".to_string(), 0.3)].into_iter().collect();
        let result = allocate_proportionally(total, &weights);
        assert_eq!(result["a"], 300);
    }
}
