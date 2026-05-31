//! Halstead derived-metric calculation (`metrics.go`).
//!
//! The Go code expresses the calculation against a set of getter/setter
//! interfaces so the same routine drives both the file-level `Metrics` and the
//! per-function `FunctionHalsteadMetrics`. In Rust we model the four inputs as a
//! small [`HalsteadCounts`] value and return the derived measures, which both
//! aggregate types then store. This is behaviorally identical and avoids the
//! interface indirection.

/// Standard constant used in time-to-program estimation (18 seconds).
pub const TIME_CONSTANT: f64 = 18.0;

/// Standard constant used in delivered-bugs estimation (`B = V / 3000`).
pub const BUG_CONSTANT: f64 = 3000.0;

/// Divisor in the difficulty formula `D = (n1 / 2) · (N2 / n2)`.
pub const DIFFICULTY_DIVISOR: f64 = 2.0;

/// The four raw Halstead counts that feed the derived measures.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HalsteadCounts {
    /// n1 — distinct operators.
    pub distinct_operators: i64,
    /// n2 — distinct operands.
    pub distinct_operands: i64,
    /// N1 — total operators.
    pub total_operators: i64,
    /// N2 — total operands.
    pub total_operands: i64,
}

/// The eight derived Halstead measures.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DerivedMetrics {
    /// n — vocabulary = n1 + n2.
    pub vocabulary: i64,
    /// N — length = N1 + N2.
    pub length: i64,
    /// Estimated length = n1·log2(n1) + n2·log2(n2) (0 when either is 0).
    pub estimated_length: f64,
    /// V — volume = N · log2(n) (0 when n == 0).
    pub volume: f64,
    /// D — difficulty = (n1/2)·(N2/n2) (0 when n2 == 0).
    pub difficulty: f64,
    /// E — effort = V · D.
    pub effort: f64,
    /// T — time to program = E / 18.
    pub time_to_program: f64,
    /// B — delivered bugs = V / 3000.
    pub delivered_bugs: f64,
}

/// Stateless calculator of Halstead derived metrics (`MetricsCalculator`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsCalculator;

impl MetricsCalculator {
    /// Creates a new calculator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Sums all values in an integer map (`SumMap`).
    #[must_use]
    pub fn sum_map<S: ::std::hash::BuildHasher>(
        &self,
        m: &std::collections::HashMap<String, i64, S>,
    ) -> i64 {
        m.values().sum()
    }

    /// Calculates all derived Halstead metrics from the four raw counts.
    ///
    /// Reproduces `CalculateHalsteadMetrics` exactly, including the zero-guards:
    /// estimated length is 0 unless both distinct counts are positive; volume is
    /// 0 when vocabulary is 0; difficulty is 0 when distinct operands is 0.
    #[must_use]
    pub fn calculate(&self, counts: HalsteadCounts) -> DerivedMetrics {
        let distinct_ops = counts.distinct_operators;
        let distinct_opnds = counts.distinct_operands;
        let total_ops = counts.total_operators;
        let total_opnds = counts.total_operands;

        let vocabulary = distinct_ops + distinct_opnds;
        let length = total_ops + total_opnds;

        let estimated_length = if distinct_ops > 0 && distinct_opnds > 0 {
            (distinct_ops as f64) * (distinct_ops as f64).log2()
                + (distinct_opnds as f64) * (distinct_opnds as f64).log2()
        } else {
            0.0
        };

        let volume = if vocabulary > 0 {
            (length as f64) * (vocabulary as f64).log2()
        } else {
            0.0
        };

        let difficulty = if distinct_opnds > 0 {
            (distinct_ops as f64 / DIFFICULTY_DIVISOR) * (total_opnds as f64 / distinct_opnds as f64)
        } else {
            0.0
        };

        let effort = volume * difficulty;
        let time_to_program = effort / TIME_CONSTANT;
        let delivered_bugs = volume / BUG_CONSTANT;

        DerivedMetrics {
            vocabulary,
            length,
            estimated_length,
            volume,
            difficulty,
            effort,
            time_to_program,
            delivered_bugs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `TestAnalyzer_CalculateMetrics`: vocabulary and length are
    /// exact sums; the derived measures are positive.
    #[test]
    fn calculate_metrics_known_values() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts {
            distinct_operators: 3,
            distinct_operands: 4,
            total_operators: 6,
            total_operands: 8,
        });
        assert_eq!(d.vocabulary, 7);
        assert_eq!(d.length, 14);
        assert!(d.volume > 0.0);
        assert!(d.difficulty > 0.0);
        assert!(d.effort > 0.0);
    }

    /// Ported from `TestMetricsCalculator_DeliveredBugsUsesVolume`.
    #[test]
    fn delivered_bugs_uses_volume() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts {
            distinct_operators: 2,
            distinct_operands: 3,
            total_operators: 4,
            total_operands: 6,
        });
        let expected = d.volume / BUG_CONSTANT;
        assert!((expected - d.delivered_bugs).abs() < 1e-12);
    }

    #[test]
    fn zero_counts_are_zero() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts::default());
        assert_eq!(d.vocabulary, 0);
        assert_eq!(d.length, 0);
        assert_eq!(d.estimated_length, 0.0);
        assert_eq!(d.volume, 0.0);
        assert_eq!(d.difficulty, 0.0);
        assert_eq!(d.effort, 0.0);
        assert_eq!(d.delivered_bugs, 0.0);
    }
}
