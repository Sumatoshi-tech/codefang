//! Budget hooks — port of the `BudgetHook` interface and `BudgetSnapshot`
//! struct from `internal/framework/runner.go`.
//!
//! During a streaming run the runner periodically samples heap usage against a
//! configured memory budget. A [`BudgetHook`] is notified when the budget is
//! crossed (and when it returns to OK), and can supply its own snapshot. The
//! concrete sampling loop that invokes the hook lives in the (currently
//! blocked) `runner`/`streaming` modules; this module is the dependency-free
//! interface that those modules and external callers share.

/// Captures memory state at a point in time for budget decisions.
///
/// Mirrors Go `framework.BudgetSnapshot` (`runner.go`). Field order and types
/// are preserved so that any future serialization matches the Go struct.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetSnapshot {
    /// Current Go-heap allocation in bytes (`runtime.MemStats.HeapAlloc`).
    pub heap_alloc_bytes: i64,
    /// The configured memory budget in bytes (0 = no budget).
    pub budget_bytes: i64,
    /// `heap_alloc_bytes / budget_bytes`; 0 when no budget is set.
    pub usage_ratio: f64,
    /// Whether [`Self::heap_alloc_bytes`] exceeds [`Self::budget_bytes`].
    pub over_budget: bool,
    /// Number of commits processed at the moment of the check.
    pub commits_at_check: i64,
}

impl Default for BudgetSnapshot {
    fn default() -> Self {
        Self {
            heap_alloc_bytes: 0,
            budget_bytes: 0,
            usage_ratio: 0.0,
            over_budget: false,
            commits_at_check: 0,
        }
    }
}

impl BudgetSnapshot {
    /// Computes a snapshot from raw heap/budget/commit counters.
    ///
    /// Reproduces the runner's derivation: `usage_ratio` is the heap/budget
    /// fraction (0 when `budget_bytes <= 0`), and `over_budget` is
    /// `heap_alloc_bytes > budget_bytes` (only meaningful with a positive
    /// budget).
    #[must_use]
    pub fn compute(heap_alloc_bytes: i64, budget_bytes: i64, commits_at_check: i64) -> Self {
        let (usage_ratio, over_budget) = if budget_bytes > 0 {
            (
                heap_alloc_bytes as f64 / budget_bytes as f64,
                heap_alloc_bytes > budget_bytes,
            )
        } else {
            (0.0, false)
        };

        Self {
            heap_alloc_bytes,
            budget_bytes,
            usage_ratio,
            over_budget,
            commits_at_check,
        }
    }
}

/// Invoked when memory budget thresholds are crossed.
///
/// Mirrors Go `framework.BudgetHook`:
///
/// ```text
/// type BudgetHook interface {
///     OnBudgetExceeded(snapshot BudgetSnapshot)
///     OnBudgetOK(snapshot BudgetSnapshot)
///     BudgetSnapshot() BudgetSnapshot
/// }
/// ```
pub trait BudgetHook {
    /// Called when the heap first crosses above the budget.
    fn on_budget_exceeded(&mut self, snapshot: BudgetSnapshot);
    /// Called when the heap returns to within budget after being over.
    fn on_budget_ok(&mut self, snapshot: BudgetSnapshot);
    /// Returns the hook's own current snapshot (used by the runner to report
    /// final budget state).
    fn budget_snapshot(&self) -> BudgetSnapshot;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_no_budget_is_neutral() {
        let s = BudgetSnapshot::compute(1024, 0, 5);
        assert_eq!(s.usage_ratio, 0.0);
        assert!(!s.over_budget);
        assert_eq!(s.commits_at_check, 5);
    }

    #[test]
    fn compute_under_budget() {
        let s = BudgetSnapshot::compute(50, 100, 1);
        assert!((s.usage_ratio - 0.5).abs() < 1e-12);
        assert!(!s.over_budget);
    }

    #[test]
    fn compute_over_budget() {
        let s = BudgetSnapshot::compute(150, 100, 9);
        assert!((s.usage_ratio - 1.5).abs() < 1e-12);
        assert!(s.over_budget);
    }

    /// A trivial hook that records the last events, exercising the trait object
    /// path the runner uses (`budgetHook BudgetHook`).
    struct RecordingHook {
        last: BudgetSnapshot,
        exceeded: u32,
        ok: u32,
    }

    impl BudgetHook for RecordingHook {
        fn on_budget_exceeded(&mut self, snapshot: BudgetSnapshot) {
            self.last = snapshot;
            self.exceeded += 1;
        }
        fn on_budget_ok(&mut self, snapshot: BudgetSnapshot) {
            self.last = snapshot;
            self.ok += 1;
        }
        fn budget_snapshot(&self) -> BudgetSnapshot {
            self.last
        }
    }

    #[test]
    fn hook_trait_object_dispatch() {
        let mut hook = RecordingHook {
            last: BudgetSnapshot::default(),
            exceeded: 0,
            ok: 0,
        };
        let h: &mut dyn BudgetHook = &mut hook;
        h.on_budget_exceeded(BudgetSnapshot::compute(150, 100, 1));
        h.on_budget_ok(BudgetSnapshot::compute(50, 100, 2));
        assert_eq!(hook.exceeded, 1);
        assert_eq!(hook.ok, 1);
        assert_eq!(hook.budget_snapshot().commits_at_check, 2);
    }
}
