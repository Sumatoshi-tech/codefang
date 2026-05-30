//! Computed-metric collection (`computed_metrics.go`).
//!
//! A [`MetricSet`] holds the results of running a list of metric computers
//! against a report and exposes the `analyzer_name` / `to_json` / `to_yaml`
//! surface that `analyze.BaseHistoryAnalyzer` requires of computed metrics.
//!
//! Serialization note: in Go `ToJSON`/`ToYAML` return a `map[string]any` that
//! is later marshaled by `encoding/json` / `yaml.v3`. Here they return a
//! [`Report`] (a byte-sorted map); the actual bytes are produced downstream by
//! the `cf-gojson` / `cf-goyaml` encoders per DESIGN §2.3. Routing the map
//! through those encoders is the caller's responsibility and is tracked in the
//! crate-level roadmap note in `lib.rs` until those encoder crates land.

use crate::report::{Report, Value};

/// A single computed metric with its metadata.
///
/// Mirrors `common.MetricResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricResult {
    /// Machine-readable identifier (e.g. `"typo_list"`).
    pub name: String,
    /// Human-readable label (e.g. `"Typo List"`).
    pub display: String,
    /// Short description of what the metric measures.
    pub description: String,
    /// Data-type hint (e.g. `"list"`, `"aggregate"`, `"scalar"`).
    pub metric_type: String,
    /// The computed value.
    pub value: Value,
}

impl MetricResult {
    /// Convenience constructor for a name/value metric with empty metadata,
    /// matching the common `MetricResult{Name, Value}` literal pattern in tests.
    pub fn new(name: impl Into<String>, value: Value) -> Self {
        MetricResult {
            name: name.into(),
            display: String::new(),
            description: String::new(),
            metric_type: String::new(),
            value,
        }
    }
}

/// Computed metrics for one analyzer.
///
/// Mirrors `common.MetricSet`.
#[derive(Debug, Clone)]
pub struct MetricSet {
    analyzer: String,
    results: Vec<MetricResult>,
}

impl MetricSet {
    /// Returns the name of the analyzer that produced these metrics.
    #[must_use]
    pub fn analyzer_name(&self) -> &str {
        &self.analyzer
    }

    /// Returns a map keyed by metric name for JSON serialization.
    ///
    /// Mirrors `common.MetricSet.ToJSON`. Later metrics with a duplicate name
    /// overwrite earlier ones, matching Go's `m[r.Name] = r.Value`.
    #[must_use]
    pub fn to_json(&self) -> Report {
        let mut m = Report::new();
        for r in &self.results {
            m.insert(r.name.clone(), r.value.clone());
        }
        m
    }

    /// Returns the same map as [`MetricSet::to_json`] for YAML serialization.
    #[must_use]
    pub fn to_yaml(&self) -> Report {
        self.to_json()
    }

    /// Returns the underlying metric results.
    #[must_use]
    pub fn metrics(&self) -> &[MetricResult] {
        &self.results
    }
}

/// Evaluates each computer against the report and collects the results.
///
/// Mirrors `common.ComputeAllMetrics`. Computers run in order; the resulting
/// [`MetricSet`] preserves that order.
pub fn compute_all_metrics<F>(
    analyzer_name: impl Into<String>,
    computers: &[F],
    report: &Report,
) -> MetricSet
where
    F: Fn(&Report) -> MetricResult,
{
    let results = computers.iter().map(|c| c(report)).collect();
    MetricSet {
        analyzer: analyzer_name.into(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Computer = Box<dyn Fn(&Report) -> MetricResult>;

    #[test]
    fn compute_all_metrics_runs_computers() {
        let computers: Vec<Computer> = vec![
            Box::new(|_: &Report| MetricResult::new("metric1", Value::Int(42))),
            Box::new(|_: &Report| MetricResult::new("metric2", Value::Str("hello".into()))),
        ];
        let report = Report::new();
        let ms = compute_all_metrics("test_analyzer", &computers, &report);

        assert_eq!(ms.analyzer_name(), "test_analyzer");
        assert_eq!(ms.metrics().len(), 2);
    }

    #[test]
    fn to_json_maps_name_to_value() {
        let computers: Vec<Computer> = vec![
            Box::new(|_: &Report| MetricResult::new("count", Value::Int(10))),
            Box::new(|_: &Report| MetricResult::new("label", Value::Str("test".into()))),
        ];
        let ms = compute_all_metrics("analyzer", &computers, &Report::new());
        let json = ms.to_json();

        assert_eq!(json.get("count"), Some(&Value::Int(10)));
        assert_eq!(json.get("label"), Some(&Value::Str("test".into())));
        assert_eq!(json.len(), 2);
    }

    #[test]
    fn to_yaml_matches_to_json() {
        let computers: Vec<Computer> =
            vec![Box::new(|_: &Report| MetricResult::new("value", Value::Float(3.14)))];
        let ms = compute_all_metrics("analyzer", &computers, &Report::new());
        assert_eq!(ms.to_json(), ms.to_yaml());
    }

    #[test]
    fn empty_computers() {
        let computers: Vec<Computer> = vec![];
        let ms = compute_all_metrics("analyzer", &computers, &Report::new());
        assert_eq!(ms.analyzer_name(), "analyzer");
        assert_eq!(ms.metrics().len(), 0);
        assert_eq!(ms.to_json().len(), 0);
    }
}
