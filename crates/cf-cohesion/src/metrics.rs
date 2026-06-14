//! Computed metrics: the report view serialized to the machine formats.
//!
//! [`ComputedMetrics`] is the struct that is actually serialized to JSON / YAML
//! / binary. Its shape is **byte-identity critical** (pinned by the
//! differential gate):
//!
//! * Fields serialize in **declaration order**, honoring `omitempty` for the
//!   optional string fields.
//! * `distribution` is a string-keyed map; its keys are byte-sorted on encode.
//! * Floats render with shortest-round-trip `%g` semantics; HTML escaping is
//!   ON; the JSON path is two-space-indented with **no trailing newline**; the
//!   binary path is the CFB1 envelope over compact JSON.
//!
//! The encoding goes through [`crate::serialize::to_go_value`], which builds
//! the value tree with struct-origin objects (declaration order) and one
//! map-origin object (the byte-sorted `distribution`).

use crate::report_value::{Report, ReportValue};
use std::collections::BTreeMap;

/// Stamped source-file key.
pub const SOURCE_FILE_KEY: &str = "_source_file";
/// Stamped language key.
pub const LANGUAGE_KEY: &str = "_language";
/// Stamped directory key.
pub const DIRECTORY_KEY: &str = "_directory";

// --- Quality thresholds ---

/// Cohesion at or above this is "Excellent".
pub const COHESION_THRESHOLD_EXCELLENT: f64 = 0.6;
/// Cohesion at or above this is "Good".
pub const COHESION_THRESHOLD_GOOD: f64 = 0.4;
/// Cohesion at or above this is "Fair".
pub const COHESION_THRESHOLD_FAIR: f64 = 0.3;
/// Health score = cohesion score x this multiplier.
pub const HEALTH_SCORE_MULTIPLIER: f64 = 100.0;

// --- Distribution keys ---

/// Distribution bucket key.
pub const METRIC_DIST_EXCELLENT: &str = "excellent";
/// Distribution bucket key.
pub const METRIC_DIST_GOOD: &str = "good";
/// Distribution bucket key.
pub const METRIC_DIST_FAIR: &str = "fair";
/// Distribution bucket key.
pub const METRIC_DIST_POOR: &str = "poor";

// === Input data types ===

/// Parsed input for metrics computation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReportData {
    /// `total_functions`.
    pub total_functions: i64,
    /// `lcom`.
    pub lcom: f64,
    /// `cohesion_score`.
    pub cohesion_score: f64,
    /// `function_cohesion`.
    pub function_cohesion: f64,
    /// Per-function data.
    pub functions: Vec<FunctionData>,
    /// `message`.
    pub message: String,
}

/// Cohesion data for one function.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunctionData {
    /// `name`.
    pub name: String,
    /// `_source_file`.
    pub source_file: String,
    /// `_language`.
    pub language: String,
    /// `_directory`.
    pub directory: String,
    /// `cohesion`.
    pub cohesion: f64,
}

/// Extracts [`ReportData`] from an analyzer [`Report`].
///
/// Type checks are strict (report-format contract): a key that is missing or
/// of the wrong dynamic type is left at its zero value.
#[must_use]
pub fn parse_report_data(report: &Report) -> ReportData {
    let mut data = ReportData::default();
    if let Some(v) = report.get("total_functions").and_then(ReportValue::as_int) {
        data.total_functions = v;
    }
    if let Some(v) = report.get("lcom").and_then(ReportValue::as_float) {
        data.lcom = v;
    }
    if let Some(v) = report.get("cohesion_score").and_then(ReportValue::as_float) {
        data.cohesion_score = v;
    }
    if let Some(v) = report
        .get("function_cohesion")
        .and_then(ReportValue::as_float)
    {
        data.function_cohesion = v;
    }
    if let Some(v) = report.get("message").and_then(ReportValue::as_str) {
        data.message = v.to_string();
    }
    data.functions = parse_report_functions(report);
    data
}

fn parse_report_functions(report: &Report) -> Vec<FunctionData> {
    let Some(functions) = report.get("functions").and_then(ReportValue::as_functions) else {
        return Vec::new();
    };
    functions.iter().map(parse_function_data).collect()
}

fn parse_function_data(fn_map: &BTreeMap<String, ReportValue>) -> FunctionData {
    let mut fd = FunctionData::default();
    if let Some(v) = fn_map.get("name").and_then(ReportValue::as_str) {
        fd.name = v.to_string();
    }
    if let Some(v) = fn_map.get(SOURCE_FILE_KEY).and_then(ReportValue::as_str) {
        fd.source_file = v.to_string();
    }
    if let Some(v) = fn_map.get(LANGUAGE_KEY).and_then(ReportValue::as_str) {
        fd.language = v.to_string();
    }
    if let Some(v) = fn_map.get(DIRECTORY_KEY).and_then(ReportValue::as_str) {
        fd.directory = v.to_string();
    }
    if let Some(v) = fn_map.get("cohesion").and_then(ReportValue::as_float) {
        fd.cohesion = v;
    }
    fd
}

// === Output data types ===

/// Per-function cohesion output.
///
/// Field order = JSON/YAML key order: `name`, `source_file?`, `language?`,
/// `directory?`, `cohesion`, `quality_level`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunctionCohesionData {
    /// `name`.
    pub name: String,
    /// `source_file` (`omitempty`).
    pub source_file: String,
    /// `language` (`omitempty`).
    pub language: String,
    /// `directory` (`omitempty`).
    pub directory: String,
    /// `cohesion`.
    pub cohesion: f64,
    /// `quality_level`.
    pub quality_level: String,
}

/// Low-cohesion function output.
///
/// Field order: `name`, `source_file?`, `language?`, `directory?`, `cohesion`,
/// `risk_level`, `recommendation`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LowCohesionFunctionData {
    /// `name`.
    pub name: String,
    /// `source_file` (`omitempty`).
    pub source_file: String,
    /// `language` (`omitempty`).
    pub language: String,
    /// `directory` (`omitempty`).
    pub directory: String,
    /// `cohesion`.
    pub cohesion: f64,
    /// `risk_level`.
    pub risk_level: String,
    /// `recommendation`.
    pub recommendation: String,
}

/// Aggregate summary statistics.
///
/// Field order: `total_functions`, `lcom`, `lcom_variant`, `cohesion_score`,
/// `function_cohesion`, `health_score`, `message`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// `total_functions`.
    pub total_functions: i64,
    /// `lcom`.
    pub lcom: f64,
    /// `lcom_variant`.
    pub lcom_variant: String,
    /// `cohesion_score`.
    pub cohesion_score: f64,
    /// `function_cohesion`.
    pub function_cohesion: f64,
    /// `health_score`.
    pub health_score: f64,
    /// `message`.
    pub message: String,
}

/// All computed metric results.
///
/// Field order = top-level JSON/YAML key order: `function_cohesion`,
/// `distribution`, `low_cohesion_functions`, `aggregate`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// `function_cohesion`.
    pub function_cohesion: Vec<FunctionCohesionData>,
    /// `distribution` — string-keyed counts, byte-sorted keys on encode.
    pub distribution: BTreeMap<String, i64>,
    /// `low_cohesion_functions`.
    pub low_cohesion_functions: Vec<LowCohesionFunctionData>,
    /// `aggregate`.
    pub aggregate: AggregateData,
}

impl ComputedMetrics {
    /// The analyzer name.
    #[must_use]
    pub fn analyzer_name(&self) -> &'static str {
        crate::ANALYZER_NAME
    }
}

// === Metric implementations ===

/// Classifies a cohesion score into a quality label: the first label whose
/// limit the score meets, scanning highest first; "Poor" otherwise.
#[must_use]
pub fn classify_cohesion_quality(cohesion: f64) -> &'static str {
    if cohesion >= COHESION_THRESHOLD_EXCELLENT {
        "Excellent"
    } else if cohesion >= COHESION_THRESHOLD_GOOD {
        "Good"
    } else if cohesion >= COHESION_THRESHOLD_FAIR {
        "Fair"
    } else {
        "Poor"
    }
}

/// Distribution label for a function.
#[must_use]
pub fn classify_cohesion_level(cohesion: f64) -> &'static str {
    if cohesion >= COHESION_THRESHOLD_EXCELLENT {
        METRIC_DIST_EXCELLENT
    } else if cohesion >= COHESION_THRESHOLD_GOOD {
        METRIC_DIST_GOOD
    } else if cohesion >= COHESION_THRESHOLD_FAIR {
        METRIC_DIST_FAIR
    } else {
        METRIC_DIST_POOR
    }
}

/// Computes the per-function cohesion list, sorted by cohesion ascending.
#[must_use]
pub fn compute_function_cohesion(input: &ReportData) -> Vec<FunctionCohesionData> {
    let mut result: Vec<FunctionCohesionData> = input
        .functions
        .iter()
        .map(|fd| FunctionCohesionData {
            name: fd.name.clone(),
            source_file: fd.source_file.clone(),
            language: fd.language.clone(),
            directory: fd.directory.clone(),
            cohesion: fd.cohesion,
            quality_level: classify_cohesion_quality(fd.cohesion).to_string(),
        })
        .collect();
    // The reference sort is an unstable quicksort with a strict-less comparator
    // on cohesion. We use a total order on cohesion; equal-cohesion ordering is
    // a canonicalized golden path (function-table nondeterminism).
    result.sort_by(|a, b| a.cohesion.total_cmp(&b.cohesion));
    result
}

/// Computes the cohesion distribution counts. Only buckets that occur are
/// inserted (absent buckets are not emitted — report-format contract).
#[must_use]
pub fn compute_distribution(input: &ReportData) -> BTreeMap<String, i64> {
    let mut dist: BTreeMap<String, i64> = BTreeMap::new();
    for fd in &input.functions {
        let label = classify_cohesion_level(fd.cohesion);
        *dist.entry(label.to_string()).or_insert(0) += 1;
    }
    dist
}

/// Computes the low-cohesion function list.
///
/// Includes only functions below [`COHESION_THRESHOLD_GOOD`]; high risk below
/// [`COHESION_THRESHOLD_FAIR`]. NOTE: only `name`, `source_file`, `cohesion`,
/// `risk_level`, `recommendation` are populated here — `language`/`directory`
/// stay empty and are omitted via `omitempty` (report-format contract; pinned
/// by the differential gate).
#[must_use]
pub fn compute_low_cohesion_functions(input: &ReportData) -> Vec<LowCohesionFunctionData> {
    let mut result: Vec<LowCohesionFunctionData> = Vec::new();
    for fd in &input.functions {
        if fd.cohesion >= COHESION_THRESHOLD_GOOD {
            continue;
        }
        // The risk-level strings are UPPERCASE ("HIGH"/"MEDIUM") in this
        // report (CLI contract).
        let (risk_level, recommendation) = if fd.cohesion < COHESION_THRESHOLD_FAIR {
            ("HIGH", "Consider splitting into multiple focused functions")
        } else {
            (
                "MEDIUM",
                "Review function responsibilities for possible separation",
            )
        };
        result.push(LowCohesionFunctionData {
            name: fd.name.clone(),
            source_file: fd.source_file.clone(),
            language: String::new(),
            directory: String::new(),
            cohesion: fd.cohesion,
            risk_level: risk_level.to_string(),
            recommendation: recommendation.to_string(),
        });
    }
    result.sort_by(|a, b| a.cohesion.total_cmp(&b.cohesion));
    result
}

/// Computes aggregate statistics.
#[must_use]
pub fn compute_aggregate(input: &ReportData) -> AggregateData {
    AggregateData {
        total_functions: input.total_functions,
        lcom: input.lcom,
        lcom_variant: "LCOM-HS (Henderson-Sellers)".to_string(),
        cohesion_score: input.cohesion_score,
        function_cohesion: input.function_cohesion,
        health_score: input.cohesion_score * HEALTH_SCORE_MULTIPLIER,
        message: input.message.clone(),
    }
}

/// Runs all cohesion metrics.
#[must_use]
pub fn compute_all_metrics(report: &Report) -> ComputedMetrics {
    let input = parse_report_data(report);
    ComputedMetrics {
        function_cohesion: compute_function_cohesion(&input),
        distribution: compute_distribution(&input),
        low_cohesion_functions: compute_low_cohesion_functions(&input),
        aggregate: compute_aggregate(&input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_with(funcs: &[(&str, f64)], total: i64, lcom: f64, score: f64, fcoh: f64) -> Report {
        let mut r = Report::new();
        r.insert("total_functions".into(), ReportValue::Int(total));
        r.insert("lcom".into(), ReportValue::Float(lcom));
        r.insert("cohesion_score".into(), ReportValue::Float(score));
        r.insert("function_cohesion".into(), ReportValue::Float(fcoh));
        r.insert("message".into(), ReportValue::Str("msg".into()));
        let functions: Vec<BTreeMap<String, ReportValue>> = funcs
            .iter()
            .map(|(name, coh)| {
                let mut m = BTreeMap::new();
                m.insert("name".into(), ReportValue::Str((*name).into()));
                m.insert("cohesion".into(), ReportValue::Float(*coh));
                m
            })
            .collect();
        r.insert("functions".into(), ReportValue::Functions(functions));
        r
    }

    #[test]
    fn parse_round_trips_scalars() {
        let r = report_with(&[("f", 0.5)], 1, 0.25, 0.75, 0.5);
        let d = parse_report_data(&r);
        assert_eq!(d.total_functions, 1);
        assert!((d.lcom - 0.25).abs() < 1e-9);
        assert!((d.cohesion_score - 0.75).abs() < 1e-9);
        assert_eq!(d.message, "msg");
        assert_eq!(d.functions.len(), 1);
        assert_eq!(d.functions[0].name, "f");
    }

    #[test]
    fn quality_classification() {
        assert_eq!(classify_cohesion_quality(0.6), "Excellent");
        assert_eq!(classify_cohesion_quality(0.4), "Good");
        assert_eq!(classify_cohesion_quality(0.3), "Fair");
        assert_eq!(classify_cohesion_quality(0.0), "Poor");
    }

    #[test]
    fn distribution_only_includes_present_buckets() {
        let r = report_with(&[("a", 0.9), ("b", 0.5), ("c", 0.5)], 3, 0.0, 1.0, 0.6);
        let d = parse_report_data(&r);
        let dist = compute_distribution(&d);
        assert_eq!(dist.get("excellent"), Some(&1));
        assert_eq!(dist.get("good"), Some(&2));
        assert_eq!(dist.get("fair"), None);
        assert_eq!(dist.get("poor"), None);
    }

    #[test]
    fn low_cohesion_thresholds_and_sort() {
        let r = report_with(
            &[("good", 0.9), ("mid", 0.35), ("bad", 0.1)],
            3,
            0.0,
            1.0,
            0.5,
        );
        let d = parse_report_data(&r);
        let low = compute_low_cohesion_functions(&d);
        // "good" (0.9) excluded; sorted ascending -> bad, mid.
        assert_eq!(low.len(), 2);
        assert_eq!(low[0].name, "bad");
        assert_eq!(low[0].risk_level, "HIGH");
        assert_eq!(
            low[0].recommendation,
            "Consider splitting into multiple focused functions"
        );
        assert_eq!(low[1].name, "mid");
        assert_eq!(low[1].risk_level, "MEDIUM");
        assert_eq!(
            low[1].recommendation,
            "Review function responsibilities for possible separation"
        );
    }

    #[test]
    fn aggregate_health_score() {
        let r = report_with(&[], 0, 0.2, 0.8, 1.0);
        let d = parse_report_data(&r);
        let agg = compute_aggregate(&d);
        assert!((agg.health_score - 80.0).abs() < 1e-9);
        assert_eq!(agg.lcom_variant, "LCOM-HS (Henderson-Sellers)");
    }

    #[test]
    fn compute_all_assembles_struct() {
        let r = report_with(&[("f", 0.5), ("g", 0.1)], 2, 0.25, 0.75, 0.3);
        let m = compute_all_metrics(&r);
        assert_eq!(m.function_cohesion.len(), 2);
        // function_cohesion sorted ascending.
        assert_eq!(m.function_cohesion[0].name, "g");
        assert_eq!(m.analyzer_name(), "cohesion");
    }
}
