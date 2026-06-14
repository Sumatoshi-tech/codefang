//! Memory budget calculation and auto-tuning (`cf-budget`).
//!
//! Turns a single memory-budget value (bytes) into concrete tuning knobs:
//!
//! * [`model`] — the empirically measured component cost model and
//!   [`model::native_limits_for_budget`], which derives the libgit2 native
//!   memory limits applied on every history run.
//! * [`solver`] — [`solver::solve_for_budget`] distributes a
//!   `--memory-budget` across workers/caches/buffers and derives a
//!   [`framework::CoordinatorConfig`]. It is the budget-solver callback that
//!   `cf_framework::config::build_config_from_params` accepts.
//! * [`static_solver`] — [`static_solver::solve_static_budget`] derives the
//!   static-analysis worker cap and spill threshold.
//!
//! Every knob tunes parallelism, cache sizes, or native memory limits only —
//! none of them can change machine-report bytes.
//!
//! # Example
//!
//! ```
//! use cf_budget::{solve_for_budget, solve_static_budget, SolveError, MINIMUM_BUDGET};
//!
//! // A budget below the minimum is rejected.
//! assert_eq!(solve_for_budget(MINIMUM_BUDGET - 1), Err(SolveError::BudgetTooSmall));
//!
//! // A valid budget yields a coordinator config with at least one worker and a
//! // non-empty blob cache.
//! let cfg = solve_for_budget(2 * 1024 * 1024 * 1024).unwrap();
//! assert!(cfg.workers >= 1);
//! assert!(cfg.blob_cache_size > 0);
//!
//! // The static solver returns a zero-value config ("use defaults") when the
//! // budget is below its own minimum.
//! let static_cfg = solve_static_budget(0);
//! assert_eq!(static_cfg.max_workers, 0);
//! assert_eq!(static_cfg.spill_threshold, 0);
//! ```

pub mod model;
pub mod solver;
pub mod static_solver;

pub use model::{estimate_memory_usage, native_limits_for_budget, NativeLimits};
pub use solver::{solve_for_budget, SolveError, MINIMUM_BUDGET};
pub use static_solver::{solve_static_budget, StaticBudgetConfig, MIN_STATIC_BUDGET};

/// Divisor for integer percentage calculations.
pub const PERCENT_DIVISOR: i64 = 100;

/// Logical CPU count as `i64`. Uses the std parallelism hint, falling back to
/// 1 (same fallback as `cf_framework::coordinator`).
#[must_use]
pub fn num_cpu() -> i64 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as i64)
}

/// Byte-size multipliers, re-exported as `crate::units` for the cost-model
/// constants.
pub mod units {
    pub use cf_units::{GIB, KIB, MIB};
}

/// Bridge to the `cf-framework` coordinator types.
pub mod framework {
    pub use cf_framework::coordinator::CoordinatorConfig;

    /// The default coordinator configuration; `CoordinatorConfig::default()`
    /// implements it.
    #[must_use]
    pub fn default_coordinator_config() -> CoordinatorConfig {
        CoordinatorConfig::default()
    }
}
