//! Numeric/count metric accumulation (`metrics_processor.go`).
//!
//! Accumulates configured numeric metrics (summed, then averaged over the
//! report count) and configured count metrics (summed) across many per-file
//! reports. Only keys listed in `numeric_keys` / `count_keys` are accumulated;
//! values are coerced via the `safeconv`-compatible [`Value::to_float64`] /
//! [`Value::to_int`].

use std::collections::BTreeMap;

use crate::report::{Report, Value};

/// Estimated per-entry memory for a map entry (string key pointer + numeric
/// value). Mirrors `common.metricsEntryBytes`.
const METRICS_ENTRY_BYTES: i64 = 16;

/// Accumulates and averages metrics extracted from reports.
///
/// Mirrors `common.MetricsProcessor`.
#[derive(Debug, Clone)]
pub struct MetricsProcessor {
    metrics: BTreeMap<String, f64>,
    counts: BTreeMap<String, i64>,
    numeric_keys: Vec<String>,
    count_keys: Vec<String>,
    report_count: i64,
}

impl MetricsProcessor {
    /// Creates a processor that accumulates the given numeric and count keys.
    ///
    /// Mirrors `common.NewMetricsProcessor`.
    #[must_use]
    pub fn new(numeric_keys: Vec<String>, count_keys: Vec<String>) -> Self {
        MetricsProcessor {
            metrics: BTreeMap::new(),
            counts: BTreeMap::new(),
            numeric_keys,
            count_keys,
            report_count: 0,
        }
    }

    /// Accumulates numeric and count metrics from a single report.
    ///
    /// Mirrors `common.MetricsProcessor.ProcessReport`. A key may be both a
    /// numeric and a count key, in which case it is accumulated into both maps,
    /// matching the Go code's two independent `if` checks.
    pub fn process_report(&mut self, report: &Report) {
        self.report_count += 1;

        for (key, value) in report {
            if self.is_numeric_metric(key) {
                if let Some(f) = value.to_float64() {
                    *self.metrics.entry(key.clone()).or_insert(0.0) += f;
                }
            }
            if self.is_count_metric(key) {
                if let Some(i) = value.to_int() {
                    *self.counts.entry(key.clone()).or_insert(0) += i;
                }
            }
        }
    }

    /// Returns the per-key averages (sum / report count).
    ///
    /// Mirrors `common.MetricsProcessor.CalculateAverages`. Keys are only
    /// emitted when the report count is positive.
    #[must_use]
    pub fn calculate_averages(&self) -> BTreeMap<String, f64> {
        let mut averages = BTreeMap::new();
        if self.report_count > 0 {
            for (key, total) in &self.metrics {
                averages.insert(key.clone(), total / self.report_count as f64);
            }
        }
        averages
    }

    /// Returns the accumulated count totals.
    #[must_use]
    pub fn get_counts(&self) -> &BTreeMap<String, i64> {
        &self.counts
    }

    /// Returns the number of reports processed.
    #[must_use]
    pub fn get_report_count(&self) -> i64 {
        self.report_count
    }

    /// Returns the summed total for a numeric metric key (0 if absent).
    #[must_use]
    pub fn get_metric(&self, key: &str) -> f64 {
        self.metrics.get(key).copied().unwrap_or(0.0)
    }

    /// Returns the summed total for a count key (0 if absent).
    #[must_use]
    pub fn get_count(&self, key: &str) -> i64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Returns the estimated in-memory state size in bytes.
    ///
    /// Mirrors `common.MetricsProcessor.EstimatedStateBytes`.
    #[must_use]
    pub fn estimated_state_bytes(&self) -> i64 {
        (self.metrics.len() + self.counts.len()) as i64 * METRICS_ENTRY_BYTES
    }

    fn is_numeric_metric(&self, key: &str) -> bool {
        self.numeric_keys.iter().any(|k| k == key)
    }

    fn is_count_metric(&self, key: &str) -> bool {
        self.count_keys.iter().any(|k| k == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(ks: &[&str]) -> Vec<String> {
        ks.iter().map(|s| s.to_string()).collect()
    }

    fn report(pairs: &[(&str, Value)]) -> Report {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn process_report_accumulates() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), keys(&["count"]));
        mp.process_report(&report(&[("score", Value::Float(0.8)), ("count", Value::Int(5))]));

        assert_eq!(mp.get_report_count(), 1);
        assert_eq!(mp.get_metric("score"), 0.8);
        assert_eq!(mp.get_count("count"), 5);
    }

    #[test]
    fn calculate_averages() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), vec![]);
        mp.process_report(&report(&[("score", Value::Float(0.8))]));
        mp.process_report(&report(&[("score", Value::Float(0.4))]));

        let averages = mp.calculate_averages();
        assert_eq!(averages.get("score"), Some(&0.6));
    }

    #[test]
    fn multiple_reports_sum_counts() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), keys(&["count"]));
        mp.process_report(&report(&[("score", Value::Float(0.5)), ("count", Value::Int(2))]));
        mp.process_report(&report(&[("score", Value::Float(0.5)), ("count", Value::Int(3))]));

        assert_eq!(mp.get_report_count(), 2);
        assert_eq!(mp.get_count("count"), 5);
    }

    #[test]
    fn empty_report_counts_but_adds_nothing() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), keys(&["count"]));
        mp.process_report(&Report::new());
        assert_eq!(mp.get_report_count(), 1);
        assert_eq!(mp.get_metric("score"), 0.0);
    }

    #[test]
    fn get_counts_returns_all() {
        let mut mp = MetricsProcessor::new(vec![], keys(&["a", "b"]));
        mp.process_report(&report(&[("a", Value::Int(1)), ("b", Value::Int(2))]));
        let counts = mp.get_counts();
        assert_eq!(counts.get("a"), Some(&1));
        assert_eq!(counts.get("b"), Some(&2));
    }

    #[test]
    fn non_numeric_values_ignored() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), keys(&["count"]));
        mp.process_report(&report(&[
            ("score", Value::Str("not a number".into())),
            ("count", Value::Str("also not".into())),
        ]));
        assert_eq!(mp.get_metric("score"), 0.0);
    }

    #[test]
    fn estimated_state_bytes_positive() {
        let mut mp = MetricsProcessor::new(keys(&["score"]), keys(&["count"]));
        mp.process_report(&report(&[("score", Value::Float(0.8)), ("count", Value::Int(5))]));
        assert!(mp.estimated_state_bytes() > 0);
    }
}
