//! Per-function metrics and the union aggregate.
//!
//! A set of functions (a file, or the whole codebase) is aggregated by taking
//! the **union of the operator and operand multisets**: n1 and n2 are the
//! distinct operators / operands across every function, N1 and N2 their total
//! occurrences, and V / D / E / T / B derive from those four numbers exactly
//! as they do for a single function. Alongside the union, the aggregate
//! carries per-function statistics (mean and max of volume, difficulty and
//! effort), the volume-band distribution, and the health score and message
//! of [`crate::thresholds`].

use std::collections::BTreeMap;

use cf_uast_node::Node;

use crate::calculator::{derive, DerivedMetrics, HalsteadCounts};
use crate::detector::{collect_functions, FunctionCounts};
use crate::thresholds::{health_score, is_high_effort, message, VolumeLevel};

/// One function's multisets, raw counts, and derived measures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunctionMetrics {
    /// Function name.
    pub name: String,
    /// n1, n2, N1, N2.
    pub counts: HalsteadCounts,
    /// The eight derived measures.
    pub derived: DerivedMetrics,
    /// Operator text → occurrences.
    pub operators: BTreeMap<String, i64>,
    /// Operand text → occurrences.
    pub operands: BTreeMap<String, i64>,
}

impl From<FunctionCounts> for FunctionMetrics {
    fn from(fc: FunctionCounts) -> Self {
        let counts = fc.totals();
        Self {
            name: fc.name,
            counts,
            derived: derive(counts),
            operators: fc.operators,
            operands: fc.operands,
        }
    }
}

/// Collects and measures every function of `root` (see
/// [`collect_functions`] for the `source` contract).
#[must_use]
pub fn analyze_functions(root: &Node, source: Option<&[u8]>) -> Vec<FunctionMetrics> {
    collect_functions(root, source)
        .into_iter()
        .map(FunctionMetrics::from)
        .collect()
}

/// Per-function statistics of an aggregated set.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FunctionStats {
    /// Mean per-function volume.
    pub mean_volume: f64,
    /// Largest per-function volume.
    pub max_volume: f64,
    /// Mean per-function difficulty.
    pub mean_difficulty: f64,
    /// Largest per-function difficulty.
    pub max_difficulty: f64,
    /// Mean per-function effort.
    pub mean_effort: f64,
    /// Largest per-function effort.
    pub max_effort: f64,
    /// Functions with effort at or above [`crate::thresholds::EFFORT_HIGH`].
    pub high_effort_count: i64,
}

/// The union aggregate of a set of functions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    /// Number of functions in the set.
    pub total_functions: i64,
    /// Union counts: distinct / total operators and operands across the set.
    pub counts: HalsteadCounts,
    /// Measures derived from the union counts.
    pub derived: DerivedMetrics,
    /// Per-function statistics.
    pub stats: FunctionStats,
    /// Function count per volume band, in [`VolumeLevel::ALL`] order.
    pub distribution: [i64; 4],
    /// `high_effort_count / total_functions` (0 for an empty set).
    pub high_effort_share: f64,
    /// Health score (0..=100) from the high-effort share.
    pub health_score: f64,
    /// Aggregate message from the high-effort share.
    pub message: &'static str,
}

/// Aggregates `functions` by the union of their multisets.
///
/// ```
/// use cf_halstead::aggregate::{summarize, FunctionMetrics};
/// use cf_halstead::detector::FunctionCounts;
///
/// let f = |ops: &[(&str, i64)], opnds: &[(&str, i64)]| {
///     FunctionMetrics::from(FunctionCounts {
///         name: "f".into(),
///         operators: ops.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
///         operands: opnds.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
///     })
/// };
/// let a = f(&[("+", 1), ("return", 1)], &[("x", 2)]);
/// let b = f(&[("*", 1), ("return", 1)], &[("x", 1), ("y", 1)]);
/// let s = summarize([&a, &b]);
/// assert_eq!(s.total_functions, 2);
/// assert_eq!(s.counts.distinct_operators, 3);  // +, return, *
/// assert_eq!(s.counts.total_operators, 4);
/// assert_eq!(s.counts.distinct_operands, 2);   // x, y
/// assert_eq!(s.counts.total_operands, 4);
/// assert_eq!(s.derived.length, 8);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // function counts are far below 2^52
pub fn summarize<'a, I>(functions: I) -> Summary
where
    I: IntoIterator<Item = &'a FunctionMetrics>,
{
    let mut operators: BTreeMap<&str, i64> = BTreeMap::new();
    let mut operands: BTreeMap<&str, i64> = BTreeMap::new();
    let mut stats = FunctionStats::default();
    let mut distribution = [0i64; 4];
    let mut total_functions: i64 = 0;

    for f in functions {
        total_functions += 1;
        for (k, v) in &f.operators {
            *operators.entry(k).or_insert(0) += v;
        }
        for (k, v) in &f.operands {
            *operands.entry(k).or_insert(0) += v;
        }
        let d = &f.derived;
        stats.mean_volume += d.volume;
        stats.mean_difficulty += d.difficulty;
        stats.mean_effort += d.effort;
        stats.max_volume = stats.max_volume.max(d.volume);
        stats.max_difficulty = stats.max_difficulty.max(d.difficulty);
        stats.max_effort = stats.max_effort.max(d.effort);
        stats.high_effort_count += i64::from(is_high_effort(d.effort));
        distribution[VolumeLevel::of(d.volume).index()] += 1;
    }

    let high_effort_share = if total_functions > 0 {
        let n = total_functions as f64;
        stats.mean_volume /= n;
        stats.mean_difficulty /= n;
        stats.mean_effort /= n;
        stats.high_effort_count as f64 / n
    } else {
        0.0
    };

    let counts = HalsteadCounts {
        distinct_operators: i64::try_from(operators.len()).unwrap_or(i64::MAX),
        distinct_operands: i64::try_from(operands.len()).unwrap_or(i64::MAX),
        total_operators: operators.values().sum(),
        total_operands: operands.values().sum(),
    };

    Summary {
        total_functions,
        counts,
        derived: derive(counts),
        stats,
        distribution,
        high_effort_share,
        health_score: health_score(high_effort_share),
        message: message(total_functions, high_effort_share),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thresholds::{EFFORT_HIGH, HEALTH_MAX, MSG_LOW, MSG_NO_FUNCTIONS, MSG_VERY_HIGH};

    fn metrics(name: &str, ops: &[(&str, i64)], opnds: &[(&str, i64)]) -> FunctionMetrics {
        FunctionMetrics::from(FunctionCounts {
            name: name.into(),
            operators: ops.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
            operands: opnds.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        })
    }

    #[test]
    fn per_function_metrics_derive_from_the_multisets() {
        let hal = metrics(
            "hal",
            &[(":=", 1), ("+", 1), ("*", 1), ("return", 1)],
            &[("a", 1), ("b", 1), ("c", 2), ("2", 1)],
        );
        assert_eq!(hal.counts.distinct_operators, 4);
        assert_eq!(hal.counts.total_operands, 5);
        assert_eq!(hal.derived.volume, 27.0);
        assert_eq!(hal.derived.effort, 67.5);
    }

    #[test]
    fn empty_set_summary() {
        let s = summarize(std::iter::empty());
        assert_eq!(s.total_functions, 0);
        assert_eq!(s.counts, HalsteadCounts::default());
        assert_eq!(s.derived, DerivedMetrics::default());
        assert_eq!(s.distribution, [0; 4]);
        assert_eq!(s.health_score, HEALTH_MAX);
        assert_eq!(s.message, MSG_NO_FUNCTIONS);
    }

    /// Two files' worth of functions: the aggregate is the UNION, so shared
    /// operators are distinct once and totals add up — never an average.
    #[test]
    fn union_counts_distinct_once_and_totals_add() {
        let a = metrics("a", &[("+", 2), ("return", 1)], &[("x", 3)]);
        let b = metrics("b", &[("return", 1), ("*", 1)], &[("x", 1), ("y", 2)]);
        let s = summarize([&a, &b]);
        assert_eq!(s.total_functions, 2);
        assert_eq!(s.counts.distinct_operators, 3);
        assert_eq!(s.counts.total_operators, 5);
        assert_eq!(s.counts.distinct_operands, 2);
        assert_eq!(s.counts.total_operands, 6);
        assert_eq!(s.derived.vocabulary, 5);
        assert_eq!(s.derived.length, 11);
        assert_eq!(s.derived.volume, 11.0 * 5f64.log2());
        assert_eq!(s.derived.difficulty, 1.5 * 3.0);
        assert_eq!(s.distribution, [2, 0, 0, 0]);
        assert_eq!(s.message, MSG_LOW);
    }

    #[test]
    fn per_function_stats_mean_and_max() {
        let a = metrics("a", &[("+", 1)], &[("x", 1), ("y", 1)]);
        let b = metrics("b", &[("+", 1), ("*", 1)], &[("x", 1), ("y", 1), ("z", 1)]);
        let s = summarize([&a, &b]);
        let (va, vb) = (a.derived.volume, b.derived.volume);
        assert!((s.stats.mean_volume - (va + vb) / 2.0).abs() < 1e-12);
        assert_eq!(s.stats.max_volume, va.max(vb));
        assert_eq!(
            s.stats.max_difficulty,
            a.derived.difficulty.max(b.derived.difficulty)
        );
        assert_eq!(s.stats.max_effort, a.derived.effort.max(b.derived.effort));
        assert_eq!(s.stats.high_effort_count, 0);
    }

    #[test]
    fn high_effort_share_drives_health_and_message() {
        // One huge function: many distinct operands used many times.
        let ops: Vec<(String, i64)> = (0..40).map(|i| (format!("op{i}"), 20)).collect();
        let opnds: Vec<(String, i64)> = (0..60).map(|i| (format!("v{i}"), 20)).collect();
        let big = FunctionMetrics::from(FunctionCounts {
            name: "big".into(),
            operators: ops.into_iter().collect(),
            operands: opnds.into_iter().collect(),
        });
        assert!(
            big.derived.effort >= EFFORT_HIGH,
            "fixture must be high-effort"
        );
        let s = summarize([&big]);
        assert_eq!(s.stats.high_effort_count, 1);
        assert_eq!(s.high_effort_share, 1.0);
        assert_eq!(s.health_score, 0.0);
        assert_eq!(s.message, MSG_VERY_HIGH);
        assert_eq!(
            s.distribution[VolumeLevel::of(big.derived.volume).index()],
            1
        );
    }
}
