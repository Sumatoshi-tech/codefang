//! Memory budget calculation and auto-tuning (`cf-budget`).
//!
//! Rust port of the Go package `internal/budget` (DESIGN.md §1). Three modules
//! mirror the three Go source files:
//!
//! * [`model`] — `model.go`: the empirically measured component cost model and
//!   [`model::native_limits_for_budget`], which derives the libgit2 native
//!   memory limits applied on every history run
//!   (`cmd/codefang/commands/run.go` `configureLibgit2MemoryLimits` →
//!   `gitlib.ConfigureMemoryLimits`).
//! * [`solver`] — `solver.go`: [`solver::solve_for_budget`] distributes a
//!   `--memory-budget` across workers/caches/buffers and derives a
//!   [`framework::CoordinatorConfig`]. It is the `BudgetSolver` callback that
//!   `cf_framework::config::build_config_from_params` accepts (Go run.go:1410
//!   `framework.BuildConfigFromParams(params, budget.SolveForBudget)`).
//! * [`static_solver`] — `static_solver.go`:
//!   [`static_solver::solve_static_budget`] derives the static-analysis worker
//!   cap and spill threshold (Go run.go `applyStaticBudgetConfig`).
//!
//! Every knob tunes parallelism, cache sizes, or native memory limits only —
//! none of them can change machine-report bytes.

pub mod model;
pub mod solver;
pub mod static_solver;

pub use model::{estimate_memory_usage, native_limits_for_budget, NativeLimits};
pub use solver::{solve_for_budget, SolveError, MINIMUM_BUDGET};
pub use static_solver::{solve_static_budget, StaticBudgetConfig, MIN_STATIC_BUDGET};

/// Divisor for integer percentage calculations (Go `percentDivisor`).
pub const PERCENT_DIVISOR: i64 = 100;

/// Logical CPU count as `i64`, mirroring Go's `runtime.NumCPU()`. Uses the std
/// parallelism hint, falling back to 1 (same fallback as
/// `cf_framework::coordinator`).
#[must_use]
pub fn num_cpu() -> i64 {
    std::thread::available_parallelism().map_or(1, |n| n.get() as i64)
}

/// Byte-size multipliers, re-exported so the modules mirror Go's
/// `pkg/units` import path as `crate::units`.
pub mod units {
    pub use cf_units::{GIB, KIB, MIB};
}

/// Bridge to the `cf-framework` coordinator types, mirroring Go's
/// `internal/framework` import.
pub mod framework {
    pub use cf_framework::coordinator::CoordinatorConfig;

    /// The default coordinator configuration (Go
    /// `framework.DefaultCoordinatorConfig()`); `CoordinatorConfig::default()`
    /// implements it.
    #[must_use]
    pub fn default_coordinator_config() -> CoordinatorConfig {
        CoordinatorConfig::default()
    }
}
