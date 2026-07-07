//! Static-analysis-phase budget solver.
//!
//! Derives the worker cap and spill threshold for the static (no-libgit2)
//! analysis phase from a memory budget.

use crate::solver::SLACK_PERCENT;
use crate::units::MIB;
use crate::{num_cpu, PERCENT_DIVISOR};

// --- Static analysis cost model constants (empirically measured) ---

/// The fixed runtime + loaded-analyzers overhead.
///
/// Lower than history's `BASE_OVERHEAD` because no libgit2 repo is opened.
pub const STATIC_BASE_OVERHEAD: i64 = 150 * MIB;

/// The per-worker memory for parser + tree-sitter native tree + UAST node
/// tree + file content buffer.
pub const STATIC_WORKER_FOOTPRINT: i64 = 50 * MIB;

/// The average serialized size of a report item (a string-keyed map with ~8
/// keys). Used to estimate the spill threshold.
pub const STATIC_AVG_ITEM_BYTES: i64 = 512;

/// The number of static analyzers that use the spillable data collector
/// (complexity, halstead, comments, cohesion, clones, imports).
pub const STATIC_ANALYZER_COUNT: i64 = 6;

/// The smallest budget that produces a non-zero config.
///
/// Must cover base overhead plus at least one worker.
pub const MIN_STATIC_BUDGET: i64 = STATIC_BASE_OVERHEAD + STATIC_WORKER_FOOTPRINT + 10 * MIB;

/// Caps workers even with large budgets.
pub const MAX_STATIC_WORKERS: i64 = 16;

/// The floor for the spill threshold.
pub const MIN_STATIC_SPILL_THRESHOLD: i64 = 1000;

/// The ceiling for the spill threshold.
pub const MAX_STATIC_SPILL_THRESHOLD: i64 = 100_000;

/// Budget-derived parameters for the static analysis phase.
///
/// Zero values mean "use defaults" — no override applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StaticBudgetConfig {
    /// Maximum number of static workers.
    pub max_workers: i64,
    /// Item count at which collectors spill to disk.
    pub spill_threshold: i64,
}

/// Derives static analysis parameters from a memory budget.
///
/// Returns a zero-value config when the budget is zero, negative, or below
/// [`MIN_STATIC_BUDGET`].
#[must_use]
pub fn solve_static_budget(budget_bytes: i64) -> StaticBudgetConfig {
    if budget_bytes < MIN_STATIC_BUDGET {
        return StaticBudgetConfig::default();
    }

    let usable = budget_bytes * (PERCENT_DIVISOR - SLACK_PERCENT) / PERCENT_DIVISOR;
    let available = usable - STATIC_BASE_OVERHEAD;

    if available <= 0 {
        return StaticBudgetConfig::default();
    }

    let workers = solve_static_workers(available);
    let worker_alloc = workers * STATIC_WORKER_FOOTPRINT;
    let remaining = available - worker_alloc;
    let spill_threshold = solve_static_spill_threshold(remaining);

    StaticBudgetConfig {
        max_workers: workers,
        spill_threshold,
    }
}

/// Computes the number of workers from available memory.
fn solve_static_workers(available: i64) -> i64 {
    let cpu_cap = num_cpu().min(MAX_STATIC_WORKERS);
    let budget_workers = available / STATIC_WORKER_FOOTPRINT;
    1.max(budget_workers.min(cpu_cap))
}

/// Computes the spill threshold from memory remaining after worker allocation.
fn solve_static_spill_threshold(remaining: i64) -> i64 {
    if remaining <= 0 {
        return MIN_STATIC_SPILL_THRESHOLD;
    }

    let per_analyzer = remaining / STATIC_ANALYZER_COUNT;
    let threshold = per_analyzer / STATIC_AVG_ITEM_BYTES;

    MIN_STATIC_SPILL_THRESHOLD.max(threshold.min(MAX_STATIC_SPILL_THRESHOLD))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num_cpu;
    use crate::units::GIB;

    #[test]
    fn solve_static_budget_zero_budget() {
        let cfg = solve_static_budget(0);
        assert_eq!(cfg.max_workers, 0);
        assert_eq!(cfg.spill_threshold, 0);
    }

    #[test]
    fn solve_static_budget_negative_budget() {
        let cfg = solve_static_budget(-1);
        assert_eq!(cfg.max_workers, 0);
        assert_eq!(cfg.spill_threshold, 0);
    }

    #[test]
    fn solve_static_budget_below_minimum() {
        let cfg = solve_static_budget(MIN_STATIC_BUDGET - 1);
        assert_eq!(cfg.max_workers, 0);
        assert_eq!(cfg.spill_threshold, 0);
    }

    #[test]
    fn solve_static_budget_at_minimum() {
        let cfg = solve_static_budget(MIN_STATIC_BUDGET);
        assert!(cfg.max_workers >= 1);
        assert!(cfg.spill_threshold >= MIN_STATIC_SPILL_THRESHOLD);
    }

    #[test]
    fn solve_static_budget_medium_budget() {
        let cfg = solve_static_budget(GIB);
        assert!(cfg.max_workers >= 1);
        assert!(cfg.max_workers <= MAX_STATIC_WORKERS);
        assert!(cfg.spill_threshold >= MIN_STATIC_SPILL_THRESHOLD);
        assert!(cfg.spill_threshold <= MAX_STATIC_SPILL_THRESHOLD);
    }

    #[test]
    fn solve_static_budget_large_budget() {
        let cfg = solve_static_budget(4 * GIB);
        // With 4 GiB, workers should be capped at MAX_STATIC_WORKERS or NumCPU.
        let max_expected = num_cpu().min(MAX_STATIC_WORKERS);
        assert!(cfg.max_workers <= max_expected);
        // Spill threshold should be at max.
        assert_eq!(cfg.spill_threshold, MAX_STATIC_SPILL_THRESHOLD);
    }

    #[test]
    fn solve_static_budget_workers_scale_with_budget() {
        let small_cfg = solve_static_budget(MIN_STATIC_BUDGET);
        let large_cfg = solve_static_budget(4 * GIB);
        assert!(
            large_cfg.max_workers >= small_cfg.max_workers,
            "larger budget should allow at least as many workers"
        );
    }

    #[test]
    fn solve_static_budget_spill_scales_with_budget() {
        let small_cfg = solve_static_budget(MIN_STATIC_BUDGET);
        let large_cfg = solve_static_budget(4 * GIB);
        assert!(
            large_cfg.spill_threshold >= small_cfg.spill_threshold,
            "larger budget should allow at least as large a spill threshold"
        );
    }

    #[test]
    fn solve_static_budget_workers_capped_by_cpu() {
        // Even with unlimited budget, workers must not exceed
        // min(NumCPU, MAX_STATIC_WORKERS).
        let cfg = solve_static_budget(16 * GIB);
        let max_expected = num_cpu().min(MAX_STATIC_WORKERS);
        assert!(cfg.max_workers <= max_expected);
    }
}
