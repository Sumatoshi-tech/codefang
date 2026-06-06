//! Static-analysis report computation for `static/complexity`.
//!
//! Ports the metric-computation + serialization view (`metrics.go`:
//! `ParseReportData` → `ComputeAllMetrics` → `ComputedMetrics`) used by the
//! static pipeline's `--format json|yaml|bin` captures. The streaming static
//! framework (walk → per-file UAST parse → analyzer → stamp → aggregate) is
//! reproduced by the caller; this module takes the aggregated, file-stamped
//! per-function inputs and builds the exact [`cf_gojson::GoValue`] that Go's
//! `ComputedMetrics` marshals to.
//!
//! Field/key ordering matches Go's `encoding/json`: the top-level struct and the
//! per-function / aggregate structs keep declaration order (struct-origin maps),
//! while `distribution` (a `map[string]int`) byte-sorts. `function_complexity`
//! and `high_risk_functions` are ordered by Go's `sort.Slice` (pdqsort) over the
//! aggregation-order input — see [`crate::gosort`].

use crate::gosort::go_sort_slice;
use cf_gojson::{GoMap, GoValue, MapOrigin};

/// One function's aggregated, file-stamped complexity input (the parsed
/// `FunctionData` the metric computers read). `source_file` / `language` /
/// `directory` are stamped by the static pipeline.
#[derive(Debug, Clone)]
pub struct FunctionInput {
    /// Function name.
    pub name: String,
    /// Relative source file (stamped; `_source_file`).
    pub source_file: String,
    /// Language (stamped; `_language`).
    pub language: String,
    /// Directory of the source file (stamped; `_directory`).
    pub directory: String,
    /// Cyclomatic complexity.
    pub cyclomatic_complexity: i64,
    /// Cognitive complexity.
    pub cognitive_complexity: i64,
    /// Maximum nesting depth.
    pub nesting_depth: i64,
    /// Estimated lines of code.
    pub lines_of_code: i64,
}

/// Aggregated report-level scalars (the parsed `ReportData` scalars, summed
/// across files by the aggregator). Note: the Go aggregator stores
/// `cognitive_complexity` / `nesting_depth` as float sums, which
/// `parseReportScalars`'s `.(int)` assertion then drops to 0 — so the aggregate
/// view always reports those two as 0.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportScalars {
    /// Total functions across all files.
    pub total_functions: i64,
    /// Average complexity (`total_complexity / total_functions`).
    pub average_complexity: f64,
    /// Maximum per-function cyclomatic complexity across all files.
    pub max_complexity: i64,
    /// Total cyclomatic complexity across all files.
    pub total_complexity: i64,
    /// Total decision points across all files.
    pub decision_points: i64,
}

// --- Thresholds (metrics.go) ---
const CYCLOMATIC_THRESHOLD_HIGH: i64 = 10;
const CYCLOMATIC_THRESHOLD_MODERATE: i64 = 5;
const COGNITIVE_THRESHOLD_HIGH: i64 = 15;
const COGNITIVE_THRESHOLD_MODERATE: i64 = 7;
const NESTING_THRESHOLD_HIGH: i64 = 5;
const NESTING_THRESHOLD_MODERATE: i64 = 3;
const RISK_SCORE_CRITICAL: i64 = 5;
const RISK_SCORE_HIGH: i64 = 3;

/// Mirrors the aggregator's `buildComplexityMessage` (aggregator.go), which the
/// static pipeline writes into the report `message` key consumed by the
/// aggregate metric. Note this differs from the single-file analyzer's
/// `getComplexityMessage` wording for the mid bands.
fn build_complexity_message(score: f64) -> &'static str {
    if score <= 1.0 {
        "Excellent complexity - functions are simple and maintainable"
    } else if score <= 3.0 {
        "Good complexity - functions have reasonable complexity"
    } else if score <= 7.0 {
        "Fair complexity - some functions could be simplified"
    } else {
        "High complexity - functions are complex and should be refactored"
    }
}

/// Mirrors `classifyFunctionRisk`.
fn classify_function_risk(cyclomatic: i64, cognitive: i64, nesting: i64) -> &'static str {
    let mut score = 0;
    if cyclomatic >= CYCLOMATIC_THRESHOLD_HIGH {
        score += 2;
    } else if cyclomatic >= CYCLOMATIC_THRESHOLD_MODERATE {
        score += 1;
    }
    if cognitive >= COGNITIVE_THRESHOLD_HIGH {
        score += 2;
    } else if cognitive >= COGNITIVE_THRESHOLD_MODERATE {
        score += 1;
    }
    if nesting >= NESTING_THRESHOLD_HIGH {
        score += 2;
    } else if nesting >= NESTING_THRESHOLD_MODERATE {
        score += 1;
    }
    if score >= RISK_SCORE_CRITICAL {
        "CRITICAL"
    } else if score >= RISK_SCORE_HIGH {
        "HIGH"
    } else if score >= 1 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

/// Mirrors `metrics.RiskPriority` (CRITICAL < HIGH < MEDIUM < LOW).
fn risk_priority(level: &str) -> i32 {
    match level {
        "CRITICAL" => 0,
        "HIGH" => 1,
        "MEDIUM" => 2,
        _ => 3,
    }
}

/// Mirrors `classifyComplexityLevel`: distribution bucket label.
fn classify_complexity_level(cyclomatic: i64) -> &'static str {
    if cyclomatic <= CYCLOMATIC_THRESHOLD_MODERATE {
        "simple"
    } else if cyclomatic <= CYCLOMATIC_THRESHOLD_HIGH {
        "moderate"
    } else {
        "complex"
    }
}

// --- Health score (metrics.go) ---
fn calculate_health_score(avg: f64) -> f64 {
    if avg <= 1.0 {
        100.0
    } else if avg <= 3.0 {
        80.0 + (3.0 - avg) * 10.0
    } else if avg <= 7.0 {
        50.0 + (7.0 - avg) * 7.5
    } else {
        f64::max(0.0, 50.0 - (avg - 7.0) * 5.0)
    }
}

/// Inserts a string field only when non-empty (Go `omitempty` for strings).
fn insert_omitempty_str(m: &mut GoMap, key: &str, value: &str) {
    if !value.is_empty() {
        m.insert(key, GoValue::Str(value.to_string()));
    }
}

/// Builds the full `ComputedMetrics` value tree (the bin/json payload root).
///
/// `functions` is the aggregation-order list (walk order × per-file analyzer
/// order); the per-metric `sort.Slice` calls are applied here exactly as Go does.
#[must_use]
pub fn computed_metrics(functions: &[FunctionInput], scalars: &ReportScalars) -> GoValue {
    // --- function_complexity: per-function with density + risk, sorted by
    // cyclomatic desc via Go's pdqsort. ---
    struct FcRow<'a> {
        f: &'a FunctionInput,
        density: f64,
        risk: &'static str,
    }
    let mut fc: Vec<FcRow> = functions
        .iter()
        .map(|f| {
            let density = if f.lines_of_code > 0 {
                f.cyclomatic_complexity as f64 / f.lines_of_code as f64
            } else {
                0.0
            };
            let risk = classify_function_risk(
                f.cyclomatic_complexity,
                f.cognitive_complexity,
                f.nesting_depth,
            );
            FcRow { f, density, risk }
        })
        .collect();
    go_sort_slice(&mut fc, |a, b| {
        a.f.cyclomatic_complexity > b.f.cyclomatic_complexity
    });

    let mut fc_arr: Vec<GoValue> = Vec::with_capacity(fc.len());
    for row in &fc {
        let mut o = GoMap::new(MapOrigin::Struct);
        o.insert("name", GoValue::Str(row.f.name.clone()));
        insert_omitempty_str(&mut o, "source_file", &row.f.source_file);
        insert_omitempty_str(&mut o, "language", &row.f.language);
        insert_omitempty_str(&mut o, "directory", &row.f.directory);
        o.insert("cyclomatic_complexity", GoValue::Int(row.f.cyclomatic_complexity));
        o.insert("cognitive_complexity", GoValue::Int(row.f.cognitive_complexity));
        o.insert("nesting_depth", GoValue::Int(row.f.nesting_depth));
        o.insert("lines_of_code", GoValue::Int(row.f.lines_of_code));
        o.insert("complexity_density", GoValue::Float(row.density));
        o.insert("risk_level", GoValue::Str(row.risk.to_string()));
        fc_arr.push(GoValue::Map(o));
    }

    // --- distribution: map[string]int -> byte-sorted keys. ---
    let mut simple = 0i64;
    let mut moderate = 0i64;
    let mut complex = 0i64;
    for f in functions {
        match classify_complexity_level(f.cyclomatic_complexity) {
            "simple" => simple += 1,
            "moderate" => moderate += 1,
            _ => complex += 1,
        }
    }
    let mut dist = GoMap::new(MapOrigin::Map);
    // stats.Distribution returns a map with only the labels that occurred; all
    // three buckets are present whenever any function falls into them. We always
    // include the buckets that have a non-zero count plus the ones Go emits.
    // stats.Distribution only adds a key when count > 0, but here every bucket is
    // exercised on real input; match Go by inserting only non-zero buckets.
    if complex > 0 {
        dist.insert("complex", GoValue::Int(complex));
    }
    if moderate > 0 {
        dist.insert("moderate", GoValue::Int(moderate));
    }
    if simple > 0 {
        dist.insert("simple", GoValue::Int(simple));
    }

    // --- high_risk_functions: functions with issues, sorted by risk priority. ---
    struct HrRow<'a> {
        f: &'a FunctionInput,
        risk: &'static str,
        issues: Vec<&'static str>,
    }
    let mut hr: Vec<HrRow> = Vec::new();
    for f in functions {
        let mut issues: Vec<&'static str> = Vec::new();
        if f.cyclomatic_complexity >= CYCLOMATIC_THRESHOLD_HIGH {
            issues.push("High cyclomatic complexity");
        }
        if f.cognitive_complexity >= COGNITIVE_THRESHOLD_HIGH {
            issues.push("High cognitive complexity");
        }
        if f.nesting_depth >= NESTING_THRESHOLD_HIGH {
            issues.push("Deep nesting");
        }
        if issues.is_empty() {
            continue;
        }
        let risk = classify_function_risk(
            f.cyclomatic_complexity,
            f.cognitive_complexity,
            f.nesting_depth,
        );
        hr.push(HrRow { f, risk, issues });
    }
    go_sort_slice(&mut hr, |a, b| {
        risk_priority(a.risk) < risk_priority(b.risk)
    });

    let mut hr_arr: Vec<GoValue> = Vec::with_capacity(hr.len());
    for row in &hr {
        let mut o = GoMap::new(MapOrigin::Struct);
        o.insert("name", GoValue::Str(row.f.name.clone()));
        insert_omitempty_str(&mut o, "source_file", &row.f.source_file);
        insert_omitempty_str(&mut o, "language", &row.f.language);
        insert_omitempty_str(&mut o, "directory", &row.f.directory);
        o.insert("cyclomatic_complexity", GoValue::Int(row.f.cyclomatic_complexity));
        o.insert("cognitive_complexity", GoValue::Int(row.f.cognitive_complexity));
        o.insert("risk_level", GoValue::Str(row.risk.to_string()));
        o.insert(
            "issues",
            GoValue::Array(row.issues.iter().map(|s| GoValue::Str((*s).to_string())).collect()),
        );
        hr_arr.push(GoValue::Map(o));
    }

    // --- aggregate. ---
    let mut agg = GoMap::new(MapOrigin::Struct);
    agg.insert("total_functions", GoValue::Int(scalars.total_functions));
    agg.insert("average_complexity", GoValue::Float(scalars.average_complexity));
    agg.insert("max_complexity", GoValue::Int(scalars.max_complexity));
    agg.insert("total_complexity", GoValue::Int(scalars.total_complexity));
    // The aggregator's float-summed cognitive/nesting are dropped to 0 by the
    // int type assertion in parseReportScalars (see ReportScalars docs).
    agg.insert("cognitive_complexity", GoValue::Int(0));
    agg.insert("nesting_depth", GoValue::Int(0));
    agg.insert("decision_points", GoValue::Int(scalars.decision_points));
    agg.insert(
        "health_score",
        GoValue::Float(calculate_health_score(scalars.average_complexity)),
    );
    agg.insert(
        "message",
        GoValue::Str(build_complexity_message(scalars.average_complexity).to_string()),
    );

    // --- root struct. ---
    let mut root = GoMap::new(MapOrigin::Struct);
    root.insert("function_complexity", GoValue::Array(fc_arr));
    root.insert("distribution", GoValue::Map(dist));
    root.insert("high_risk_functions", GoValue::Array(hr_arr));
    root.insert("aggregate", GoValue::Map(agg));
    GoValue::Map(root)
}
