//! Result aggregation — port of `internal/analyzers/cohesion/aggregator.go`.
//!
//! When cohesion runs over many files (and when it feeds the quality analyzer /
//! timeseries), per-file reports are merged into one. The Go code delegates the
//! heavy lifting to `common.Aggregator` (numeric mean over `lcom`/`cohesion_score`/
//! `function_cohesion`, sum over `total_functions`, concatenation + de-dup of the
//! `functions` table keyed by `("_source_file", "name")`, and a recomputed message),
//! while retaining per-file reports via `PerFileRetainer`.
//!
//! Until the shared `cf-analyzers-common::Aggregator` is available this module
//! reproduces the merge semantics directly. The aggregation keys and the message
//! labeler are byte-faithful to the Go config; the per-file retention hook is a seam
//! (see crate todos).

use crate::report_value::{Report, ReportValue};
use std::collections::{BTreeMap, BTreeSet};

/// `scoreThresholdHigh` (aggregator.go).
const SCORE_THRESHOLD_HIGH: f64 = 0.7;
/// `scoreThresholdMedium`.
const SCORE_THRESHOLD_MEDIUM: f64 = 0.4;
/// `scoreThresholdLow`.
const SCORE_THRESHOLD_LOW: f64 = 0.3;

/// Numeric keys averaged across reports (Go `getNumericKeys`).
pub const NUMERIC_KEYS: [&str; 3] = ["lcom", "cohesion_score", "function_cohesion"];
/// Count keys summed across reports (Go `getCountKeys`).
pub const COUNT_KEYS: [&str; 1] = ["total_functions"];
/// The collection key holding the per-function table (Go `"functions"`).
pub const COLLECTION_KEY: &str = "functions";
/// The composite identity used to de-duplicate function rows (Go
/// `[]string{"_source_file", "name"}`).
pub const DEDUP_KEYS: [&str; 2] = ["_source_file", "name"];

/// Overall-cohesion message keyed by the aggregated score (Go `getCohesionMessage`
/// in aggregator.go — distinct wording from the per-analyzer message).
#[must_use]
pub fn get_cohesion_message(score: f64) -> &'static str {
    if score >= SCORE_THRESHOLD_HIGH {
        "Excellent overall cohesion across all analyzed code"
    } else if score >= SCORE_THRESHOLD_MEDIUM {
        "Good overall cohesion with room for improvement"
    } else if score >= SCORE_THRESHOLD_LOW {
        "Fair overall cohesion - consider refactoring some functions"
    } else {
        "Poor overall cohesion - significant refactoring recommended"
    }
}

/// The empty aggregated result (Go `createEmptyResult`).
#[must_use]
pub fn create_empty_result() -> Report {
    let mut r = Report::new();
    r.insert("total_functions".into(), ReportValue::Int(0));
    r.insert("lcom".into(), ReportValue::Float(0.0));
    r.insert("cohesion_score".into(), ReportValue::Float(1.0));
    r.insert("function_cohesion".into(), ReportValue::Float(1.0));
    r.insert(
        "message".into(),
        ReportValue::Str("No functions found".into()),
    );
    r
}

/// Aggregates multiple per-file cohesion reports into one (Go
/// `(*Aggregator).Aggregate` + `common.Aggregator`).
///
/// Semantics reproduced:
/// * `total_functions` = sum across reports.
/// * `lcom`, `cohesion_score`, `function_cohesion` = arithmetic mean across reports
///   that carry the key.
/// * `functions` = concatenation of every report's table, de-duplicated by the
///   `(_source_file, name)` identity (first occurrence wins).
/// * `message` = recomputed from the aggregated `cohesion_score` via
///   [`get_cohesion_message`].
///
/// An empty input yields [`create_empty_result`].
#[must_use]
pub fn aggregate(reports: &[Report]) -> Report {
    if reports.is_empty() {
        return create_empty_result();
    }

    let mut total_functions = 0i64;
    let mut numeric_sums: BTreeMap<&str, f64> = BTreeMap::new();
    let mut numeric_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut functions: Vec<BTreeMap<String, ReportValue>> = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    for report in reports {
        for k in COUNT_KEYS {
            if let Some(v) = report.get(k).and_then(ReportValue::as_int) {
                total_functions += v;
            }
        }
        for k in NUMERIC_KEYS {
            if let Some(v) = report.get(k).and_then(ReportValue::as_float) {
                *numeric_sums.entry(k).or_insert(0.0) += v;
                *numeric_counts.entry(k).or_insert(0) += 1;
            }
        }
        if let Some(rows) = report.get(COLLECTION_KEY).and_then(ReportValue::as_functions) {
            for row in rows {
                let sf = row
                    .get(DEDUP_KEYS[0])
                    .and_then(ReportValue::as_str)
                    .unwrap_or("")
                    .to_string();
                let name = row
                    .get(DEDUP_KEYS[1])
                    .and_then(ReportValue::as_str)
                    .unwrap_or("")
                    .to_string();
                if seen.insert((sf, name)) {
                    functions.push(row.clone());
                }
            }
        }
    }

    let mean = |k: &str| -> f64 {
        let count = numeric_counts.get(k).copied().unwrap_or(0);
        if count == 0 {
            0.0
        } else {
            numeric_sums.get(k).copied().unwrap_or(0.0) / count as f64
        }
    };

    let cohesion_score = mean("cohesion_score");

    let mut out = Report::new();
    out.insert("total_functions".into(), ReportValue::Int(total_functions));
    out.insert("lcom".into(), ReportValue::Float(mean("lcom")));
    out.insert(
        "cohesion_score".into(),
        ReportValue::Float(cohesion_score),
    );
    out.insert(
        "function_cohesion".into(),
        ReportValue::Float(mean("function_cohesion")),
    );
    out.insert("functions".into(), ReportValue::Functions(functions));
    out.insert(
        "message".into(),
        ReportValue::Str(get_cohesion_message(cohesion_score).into()),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(total: i64, lcom: f64, score: f64, fcoh: f64) -> Report {
        let mut r = Report::new();
        r.insert("total_functions".into(), ReportValue::Int(total));
        r.insert("lcom".into(), ReportValue::Float(lcom));
        r.insert("cohesion_score".into(), ReportValue::Float(score));
        r.insert("function_cohesion".into(), ReportValue::Float(fcoh));
        r
    }

    #[test]
    fn empty_input_is_empty_result() {
        let r = aggregate(&[]);
        assert_eq!(r.get("total_functions"), Some(&ReportValue::Int(0)));
        assert_eq!(r.get("cohesion_score"), Some(&ReportValue::Float(1.0)));
    }

    #[test]
    fn sums_counts_and_means_numerics() {
        let a = report(2, 0.2, 0.8, 0.6);
        let b = report(3, 0.4, 0.6, 0.4);
        let r = aggregate(&[a, b]);
        assert_eq!(r.get("total_functions"), Some(&ReportValue::Int(5)));
        assert!((r.get("lcom").unwrap().as_float().unwrap() - 0.3).abs() < 1e-9);
        assert!((r.get("cohesion_score").unwrap().as_float().unwrap() - 0.7).abs() < 1e-9);
        assert!((r.get("function_cohesion").unwrap().as_float().unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn message_recomputed_from_score() {
        let r = aggregate(&[report(1, 0.0, 0.75, 1.0)]);
        assert_eq!(
            r.get("message"),
            Some(&ReportValue::Str(
                "Excellent overall cohesion across all analyzed code".into()
            ))
        );
    }

    #[test]
    fn functions_deduped_by_source_and_name() {
        let mut a = report(1, 0.0, 0.5, 0.5);
        let mut b = report(1, 0.0, 0.5, 0.5);
        let mut row = BTreeMap::new();
        row.insert("_source_file".to_string(), ReportValue::Str("a.go".into()));
        row.insert("name".to_string(), ReportValue::Str("f".into()));
        a.insert("functions".into(), ReportValue::Functions(vec![row.clone()]));
        b.insert("functions".into(), ReportValue::Functions(vec![row]));
        let r = aggregate(&[a, b]);
        let funcs = r.get("functions").unwrap().as_functions().unwrap();
        assert_eq!(funcs.len(), 1);
    }
}
