//! Report generation in text, JSON, and summary forms.
//!
//! Port of `reporter.go`. [`Reporter`] wraps a [`Formatter`] and renders single
//! reports or cross-report comparisons.
//!
//! # Byte-identity
//!
//! The `json` form is a **binding** machine format. Go produces it with
//! `json.MarshalIndent(report, "", "  ")`; this port routes through
//! [`crate::report::report_to_json`] (the `cf-gojson` encoder) so the bytes are
//! identical — never through `serde_json` (DESIGN Rule 1). The `text` and
//! `summary` forms are human-readable and non-binding.

use crate::formatter::{extract_all_numeric_metrics, FormatConfig, Formatter, MSG_NO_REPORT_DATA};
use crate::report::{report_to_json, Report, Value};
use std::collections::BTreeMap;

const FORMAT_TEXT: &str = "text";

/// Configuration for report generation. Mirrors the Go `ReportConfig`.
#[derive(Debug, Clone, Default)]
pub struct ReportConfig {
    /// Output format: `"text"`, `"json"`, `"summary"`, or other (→ text).
    pub format: String,
    /// Sort key passed to the underlying formatter.
    pub sort_by: String,
    /// Sort order passed to the underlying formatter.
    pub sort_order: String,
    /// Numeric metric keys to extract (empty = all numeric values).
    pub metric_keys: Vec<String>,
    /// Count metric keys to extract (empty = all integer values).
    pub count_keys: Vec<String>,
    /// Maximum number of collection rows in text output.
    pub max_items: usize,
    /// Whether to include the details/tables sections in text output.
    pub include_details: bool,
}

/// Generates analyzer reports in the configured format. Mirrors `Reporter`.
#[derive(Debug, Clone)]
pub struct Reporter {
    config: ReportConfig,
    formatter: Formatter,
}

impl Reporter {
    /// Creates a reporter, deriving the [`Formatter`] config from the report
    /// config exactly as the Go `NewReporter` does.
    pub fn new(config: ReportConfig) -> Self {
        let format_config = FormatConfig {
            show_progress_bars: config.format == FORMAT_TEXT,
            show_tables: config.format == FORMAT_TEXT && config.include_details,
            show_details: config.include_details,
            max_items: config.max_items,
            sort_by: config.sort_by.clone(),
            sort_order: config.sort_order.clone(),
            skip_header: false,
        };
        Reporter {
            formatter: Formatter::new(format_config),
            config,
        }
    }

    /// Generates a report in the configured format. Mirrors `GenerateReport`.
    ///
    /// The `json` arm can fail if the encoder rejects the value (e.g. a
    /// non-finite float, which Go's `encoding/json` also errors on).
    pub fn generate_report(&self, report: Option<&Report>) -> Result<String, String> {
        match self.config.format.as_str() {
            FORMAT_TEXT => Ok(self.formatter.format_report(report)),
            "json" => self.generate_json_report(report),
            "summary" => Ok(self.generate_summary_report(report)),
            _ => Ok(self.formatter.format_report(report)),
        }
    }

    /// Generates a byte-identical JSON report via `cf-gojson`. Mirrors
    /// `generateJSONReport` (`json.MarshalIndent(report, "", "  ")`).
    fn generate_json_report(&self, report: Option<&Report>) -> Result<String, String> {
        let empty = Report::new();
        let report = report.unwrap_or(&empty);
        let bytes = report_to_json(report)
            .map_err(|e| format!("failed to marshal report to JSON: {e}"))?;
        String::from_utf8(bytes).map_err(|e| format!("failed to marshal report to JSON: {e}"))
    }

    /// Generates a concise single-line summary. Mirrors `generateSummaryReport`.
    fn generate_summary_report(&self, report: Option<&Report>) -> String {
        let report = match report {
            Some(r) => r,
            None => return MSG_NO_REPORT_DATA.to_string(),
        };

        let mut summary: Vec<String> = Vec::new();

        if let Some(Value::String(name)) = report.get("analyzer_name") {
            summary.push(format!("Analyzer: {name}"));
        }

        if let Some(Value::String(message)) = report.get("message") {
            if !message.is_empty() {
                summary.push(format!("Status: {message}"));
            }
        }

        let metrics = self.extract_key_metrics(report);
        if !metrics.is_empty() {
            let mut lines: Vec<String> = metrics
                .iter()
                .map(|(key, value)| format!("{key}: {value:.2}"))
                .collect();
            lines.sort();
            summary.push(format!("Metrics: {}", lines.join(", ")));
        }

        let counts = self.extract_counts(report);
        if !counts.is_empty() {
            let mut lines: Vec<String> = counts
                .iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            lines.sort();
            summary.push(format!("Counts: {}", lines.join(", ")));
        }

        summary.join(" | ")
    }

    /// Extracts numeric metrics, honoring configured `metric_keys` or all
    /// numeric values. Mirrors `extractKeyMetrics`.
    fn extract_key_metrics(&self, report: &Report) -> BTreeMap<String, f64> {
        if self.config.metric_keys.is_empty() {
            return extract_all_numeric_metrics(report);
        }
        let mut metrics = BTreeMap::new();
        for key in &self.config.metric_keys {
            if let Some(value) = report.get(key) {
                if let Some(score) = value.to_float64() {
                    metrics.insert(key.clone(), score);
                }
            }
        }
        metrics
    }

    /// Extracts count metrics, honoring configured `count_keys` or all integer
    /// values. Mirrors `extractCounts`.
    fn extract_counts(&self, report: &Report) -> BTreeMap<String, i64> {
        let mut counts = BTreeMap::new();
        if self.config.count_keys.is_empty() {
            for (key, value) in report {
                if let Some(count) = value.to_int() {
                    counts.insert(key.clone(), count);
                }
            }
            return counts;
        }
        for key in &self.config.count_keys {
            if let Some(value) = report.get(key) {
                if let Some(count) = value.to_int() {
                    counts.insert(key.clone(), count);
                }
            }
        }
        counts
    }

    /// Generates a comparison report across multiple named reports. Mirrors
    /// `GenerateComparisonReport`.
    pub fn generate_comparison_report(
        &self,
        reports: &BTreeMap<String, Report>,
    ) -> Result<String, String> {
        if reports.is_empty() {
            return Ok("No reports to compare".to_string());
        }
        match self.config.format.as_str() {
            FORMAT_TEXT => Ok(self.generate_text_comparison_report(reports)),
            "json" => self.generate_json_comparison_report(reports),
            "summary" => Ok(self.generate_summary_comparison_report(reports)),
            _ => Ok(self.generate_text_comparison_report(reports)),
        }
    }

    /// Mirrors `generateTextComparisonReport`.
    fn generate_text_comparison_report(&self, reports: &BTreeMap<String, Report>) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(reports.len() + 2);
        parts.push("=== COMPARISON REPORT ===".to_string());

        let comparison = self.compare_metrics(reports);
        if !comparison.is_empty() {
            parts.push(comparison);
        }

        for (name, report) in reports {
            parts.push(format!("\n--- {name} ---"));
            parts.push(self.formatter.format_report(Some(report)));
        }

        parts.join("\n")
    }

    /// Mirrors `generateJSONComparisonReport`. Routes through `cf-gojson` so the
    /// comparison JSON is byte-identical to Go's `json.MarshalIndent`.
    fn generate_json_comparison_report(
        &self,
        reports: &BTreeMap<String, Report>,
    ) -> Result<String, String> {
        // Build a Report-shaped object: {"comparison": {...}, "reports": {...}}.
        let mut comparison_report = Report::new();
        comparison_report.insert("comparison".into(), self.compare_metrics_data(reports));

        let reports_map: BTreeMap<String, Value> = reports
            .iter()
            .map(|(name, r)| (name.clone(), Value::Map(r.clone())))
            .collect();
        comparison_report.insert("reports".into(), Value::Map(reports_map));

        let bytes = report_to_json(&comparison_report)
            .map_err(|e| format!("failed to marshal comparison report to JSON: {e}"))?;
        String::from_utf8(bytes)
            .map_err(|e| format!("failed to marshal comparison report to JSON: {e}"))
    }

    /// Mirrors `generateSummaryComparisonReport`.
    fn generate_summary_comparison_report(&self, reports: &BTreeMap<String, Report>) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(reports.len() + 2);
        parts.push("Comparison Summary:".to_string());

        let comparison = self.compare_metrics(reports);
        if !comparison.is_empty() {
            parts.push(comparison);
        }

        for (name, report) in reports {
            let summary = self.generate_summary_report(Some(report));
            parts.push(format!("{name}: {summary}"));
        }

        parts.join("\n")
    }

    /// Returns the metric keys to compare. Mirrors `resolveMetricKeys`: the
    /// configured keys when set, else every key that is numeric in any report,
    /// sorted (`mapx.SortedKeys`).
    fn resolve_metric_keys(&self, reports: &BTreeMap<String, Report>) -> Vec<String> {
        if !self.config.metric_keys.is_empty() {
            return self.config.metric_keys.clone();
        }
        let mut key_set = std::collections::BTreeSet::new();
        for report in reports.values() {
            for (key, value) in report {
                if value.to_float64().is_some() {
                    key_set.insert(key.clone());
                }
            }
        }
        key_set.into_iter().collect()
    }

    /// Collects a metric's float values across named reports. Mirrors
    /// `collectMetricValues`.
    fn collect_metric_values(
        &self,
        metric_key: &str,
        reports: &BTreeMap<String, Report>,
    ) -> (BTreeMap<String, f64>, bool) {
        let mut values = BTreeMap::new();
        let mut has_values = false;
        for (name, report) in reports {
            if let Some(value) = report.get(metric_key) {
                if let Some(score) = value.to_float64() {
                    values.insert(name.clone(), score);
                    has_values = true;
                }
            }
        }
        (values, has_values)
    }

    /// Compares metrics across reports as text. Mirrors `compareMetrics`;
    /// requires at least two reports.
    fn compare_metrics(&self, reports: &BTreeMap<String, Report>) -> String {
        if reports.len() < 2 {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        for metric_key in self.resolve_metric_keys(reports) {
            let (values, has_values) = self.collect_metric_values(&metric_key, reports);
            if has_values {
                let comparison = self.format_metric_comparison(&metric_key, &values);
                if !comparison.is_empty() {
                    parts.push(comparison);
                }
            }
        }
        parts.join("\n")
    }

    /// Builds comparison data for JSON output. Mirrors `compareMetricsData`.
    fn compare_metrics_data(&self, reports: &BTreeMap<String, Report>) -> Value {
        let mut comparison = BTreeMap::new();
        for metric_key in self.resolve_metric_keys(reports) {
            let (values, has_values) = self.collect_metric_values(&metric_key, reports);
            if has_values {
                let inner: BTreeMap<String, Value> = values
                    .into_iter()
                    .map(|(k, v)| (k, Value::Float(v)))
                    .collect();
                comparison.insert(metric_key, Value::Map(inner));
            }
        }
        Value::Map(comparison)
    }

    /// Formats a single metric comparison, sorted by value descending. Mirrors
    /// `formatMetricComparison`.
    fn format_metric_comparison(&self, metric_key: &str, values: &BTreeMap<String, f64>) -> String {
        if values.is_empty() {
            return String::new();
        }
        let mut lines: Vec<String> = Vec::with_capacity(values.len() + 1);
        lines.push(format!("{metric_key}:"));

        let mut sorted: Vec<(&String, &f64)> = values.iter().collect();
        sorted.sort_by(|a, b| {
            b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        for (key, value) in sorted {
            lines.push(format!("  {key}: {value:.3}"));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ANALYZER_NAME: &str = "test_analyzer";

    fn report(pairs: &[(&str, Value)]) -> Report {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // Ported from reporter_test.go: TestNewReporter
    #[test]
    fn new_reporter() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            include_details: true,
            sort_by: "name".into(),
            sort_order: "asc".into(),
            max_items: 10,
            ..Default::default()
        });
        assert_eq!(r.config.format, "text");
    }

    // Ported from reporter_test.go: TestReporter_GenerateReport_Text
    #[test]
    fn generate_report_text() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            ..Default::default()
        });
        let rep = report(&[
            ("analyzer_name", Value::String(TEST_ANALYZER_NAME.into())),
            ("message", Value::String("Test completed".into())),
            ("score", Value::Float(0.85)),
        ]);
        let out = r.generate_report(Some(&rep)).unwrap();
        assert!(!out.is_empty());
    }

    // Ported from reporter_test.go: TestReporter_GenerateReport_JSON
    #[test]
    fn generate_report_json_is_valid_and_byte_shaped() {
        let r = Reporter::new(ReportConfig {
            format: "json".into(),
            ..Default::default()
        });
        let rep = report(&[
            ("analyzer_name", Value::String(TEST_ANALYZER_NAME.into())),
            ("message", Value::String("Test completed".into())),
            ("score", Value::Float(0.85)),
        ]);
        let out = r.generate_report(Some(&rep)).unwrap();
        // Keys are byte-sorted (map-origin) and indented two spaces, matching
        // Go's json.MarshalIndent(report, "", "  ").
        let expected = "{\n  \"analyzer_name\": \"test_analyzer\",\n  \"message\": \"Test completed\",\n  \"score\": 0.85\n}";
        assert_eq!(out, expected);
    }

    // Ported from reporter_test.go: TestReporter_GenerateReport_Summary
    #[test]
    fn generate_report_summary() {
        let r = Reporter::new(ReportConfig {
            format: "summary".into(),
            ..Default::default()
        });
        let rep = report(&[
            ("analyzer_name", Value::String(TEST_ANALYZER_NAME.into())),
            ("message", Value::String("Test completed".into())),
            ("score", Value::Float(0.85)),
            ("total_items", Value::Int(10)),
        ]);
        let out = r.generate_report(Some(&rep)).unwrap();
        assert!(out.contains(TEST_ANALYZER_NAME));
        assert!(out.contains("Test completed"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateReport_DefaultFormat
    #[test]
    fn generate_report_default_format() {
        let r = Reporter::new(ReportConfig {
            format: "unknown".into(),
            ..Default::default()
        });
        let rep = report(&[("message", Value::String("Test".into()))]);
        let out = r.generate_report(Some(&rep)).unwrap();
        assert!(!out.is_empty());
    }

    // Ported from reporter_test.go: TestReporter_GenerateSummaryReport_NilReport
    #[test]
    fn generate_summary_nil_report() {
        let r = Reporter::new(ReportConfig {
            format: "summary".into(),
            ..Default::default()
        });
        let out = r.generate_report(None).unwrap();
        assert!(out.contains("No report data available"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateSummaryReport_WithMetricKeys
    #[test]
    fn generate_summary_with_metric_keys() {
        let r = Reporter::new(ReportConfig {
            format: "summary".into(),
            metric_keys: vec!["score".into()],
            count_keys: vec!["total_items".into()],
            ..Default::default()
        });
        let rep = report(&[
            ("analyzer_name", Value::String(TEST_ANALYZER_NAME.into())),
            ("score", Value::Float(0.85)),
            ("total_items", Value::Int(10)),
            ("ignored_field", Value::String("should not appear".into())),
        ]);
        let out = r.generate_report(Some(&rep)).unwrap();
        assert!(out.contains("score"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateComparisonReport_Empty
    #[test]
    fn comparison_empty() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            ..Default::default()
        });
        let out = r.generate_comparison_report(&BTreeMap::new()).unwrap();
        assert!(out.contains("No reports to compare"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateComparisonReport_Text
    #[test]
    fn comparison_text() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert(
            "report1".to_string(),
            report(&[("score", Value::Float(0.85)), ("count", Value::Int(10))]),
        );
        reports.insert(
            "report2".to_string(),
            report(&[("score", Value::Float(0.92)), ("count", Value::Int(15))]),
        );
        let out = r.generate_comparison_report(&reports).unwrap();
        assert!(out.contains("COMPARISON REPORT"));
        assert!(out.contains("report1"));
        assert!(out.contains("report2"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateComparisonReport_JSON
    #[test]
    fn comparison_json() {
        let r = Reporter::new(ReportConfig {
            format: "json".into(),
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert("report1".to_string(), report(&[("score", Value::Float(0.85))]));
        reports.insert("report2".to_string(), report(&[("score", Value::Float(0.92))]));
        let out = r.generate_comparison_report(&reports).unwrap();
        assert!(out.contains("\"comparison\""));
        assert!(out.contains("\"reports\""));
    }

    // Ported from reporter_test.go: TestReporter_GenerateComparisonReport_Summary
    #[test]
    fn comparison_summary() {
        let r = Reporter::new(ReportConfig {
            format: "summary".into(),
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert(
            "report1".to_string(),
            report(&[
                ("analyzer_name", Value::String("analyzer1".into())),
                ("score", Value::Float(0.85)),
            ]),
        );
        reports.insert(
            "report2".to_string(),
            report(&[
                ("analyzer_name", Value::String("analyzer2".into())),
                ("score", Value::Float(0.92)),
            ]),
        );
        let out = r.generate_comparison_report(&reports).unwrap();
        assert!(out.contains("Comparison Summary"));
    }

    // Ported from reporter_test.go: TestReporter_GenerateComparisonReport_SingleReport
    #[test]
    fn comparison_single_report() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            metric_keys: vec!["score".into()],
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert("report1".to_string(), report(&[("score", Value::Float(0.85))]));
        let out = r.generate_comparison_report(&reports).unwrap();
        assert!(out.contains("report1"));
    }

    // Ported from reporter_test.go: TestReporter_ExtractKeyMetrics_WithConfiguredKeys
    #[test]
    fn extract_key_metrics_configured() {
        let r = Reporter::new(ReportConfig {
            metric_keys: vec!["score".into(), "complexity".into()],
            ..Default::default()
        });
        let rep = report(&[
            ("score", Value::Float(0.85)),
            ("complexity", Value::Float(15.0)),
            ("other_metric", Value::Float(100.0)),
            ("string_field", Value::String("not a metric".into())),
        ]);
        let metrics = r.extract_key_metrics(&rep);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics.get("score"), Some(&0.85));
        assert_eq!(metrics.get("complexity"), Some(&15.0));
        assert!(!metrics.contains_key("other_metric"));
    }

    // Ported from reporter_test.go: TestReporter_ExtractKeyMetrics_WithoutConfiguredKeys
    #[test]
    fn extract_key_metrics_unconfigured() {
        let r = Reporter::new(ReportConfig::default());
        let rep = report(&[
            ("score", Value::Float(0.85)),
            ("count", Value::Int(10)),
            ("string_field", Value::String("not a metric".into())),
        ]);
        let metrics = r.extract_key_metrics(&rep);
        assert!(metrics.contains_key("score"));
        assert!(metrics.contains_key("count"));
    }

    // Ported from reporter_test.go: TestReporter_ExtractCounts_WithConfiguredKeys
    #[test]
    fn extract_counts_configured() {
        let r = Reporter::new(ReportConfig {
            count_keys: vec!["total_items".into()],
            ..Default::default()
        });
        let rep = report(&[
            ("total_items", Value::Int(10)),
            ("other_count", Value::Int(20)),
            ("string_field", Value::String("not a count".into())),
        ]);
        let counts = r.extract_counts(&rep);
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("total_items"), Some(&10));
    }

    // Ported from reporter_test.go: TestReporter_CompareMetrics_WithConfiguredKeys
    #[test]
    fn compare_metrics_configured() {
        let r = Reporter::new(ReportConfig {
            format: "text".into(),
            metric_keys: vec!["score".into()],
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert(
            "report1".to_string(),
            report(&[("score", Value::Float(0.85)), ("other", Value::Float(1.0))]),
        );
        reports.insert(
            "report2".to_string(),
            report(&[("score", Value::Float(0.92)), ("other", Value::Float(2.0))]),
        );
        let out = r.compare_metrics(&reports);
        assert!(out.contains("score"));
        assert!(!out.contains("other"));
    }

    // Ported from reporter_test.go: TestReporter_CompareMetricsData
    #[test]
    fn compare_metrics_data_values() {
        let r = Reporter::new(ReportConfig {
            metric_keys: vec!["score".into()],
            ..Default::default()
        });
        let mut reports = BTreeMap::new();
        reports.insert("report1".to_string(), report(&[("score", Value::Float(0.85))]));
        reports.insert("report2".to_string(), report(&[("score", Value::Float(0.92))]));
        let data = r.compare_metrics_data(&reports);
        if let Value::Map(m) = data {
            if let Some(Value::Map(score)) = m.get("score") {
                assert_eq!(score.get("report1"), Some(&Value::Float(0.85)));
                assert_eq!(score.get("report2"), Some(&Value::Float(0.92)));
            } else {
                panic!("expected score comparison data");
            }
        } else {
            panic!("expected map");
        }
    }

    // Ported from reporter_test.go: TestReporter_FormatMetricComparison_Empty
    #[test]
    fn format_metric_comparison_empty() {
        let r = Reporter::new(ReportConfig::default());
        let out = r.format_metric_comparison("test", &BTreeMap::new());
        assert_eq!(out, "");
    }

    // Ported from reporter_test.go: TestReporter_FormatMetricComparison_Sorted
    #[test]
    fn format_metric_comparison_sorted() {
        let r = Reporter::new(ReportConfig::default());
        let mut values = BTreeMap::new();
        values.insert("low".to_string(), 0.5);
        values.insert("medium".to_string(), 0.75);
        values.insert("high".to_string(), 0.9);
        let out = r.format_metric_comparison("score", &values);
        let lines: Vec<&str> = out.split('\n').collect();
        assert!(lines.len() >= 4);
        assert!(lines[0].contains("score"));
        assert!(lines[1].contains("high"));
    }
}
