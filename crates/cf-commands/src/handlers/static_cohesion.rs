//! Report paths for the `static/cohesion` analyzer.
//!
//! Pipeline (shared with the other static analyzers):
//!
//!  1. The shared static walk ([`super::walk_source_files`]) admits every
//!     UAST-supported file that passes the run filter (`--languages`, path
//!     policy, generated-content marker, read cap); each admitted file is
//!     parsed. Unreadable, oversized and unparseable files are skipped and
//!     counted for the end-of-run `progress: skipped N ...` diagnostic.
//!  2. Each parsed file yields one [`cf_cohesion::FileReport`] via
//!     [`cf_cohesion::Analyzer::analyze_file`].
//!  3. [`cf_cohesion::aggregate`] folds the files: every function is kept,
//!     scalars are function-weighted, files without functions carry no data.
//!  4. `json` / `text` / `compact` render the section tree built here (one
//!     scored section: metrics, distribution, issues, optional `--per-file`
//!     entries); `yaml` / `bin` marshal [`cf_cohesion::ComputedMetrics`];
//!     `plot` consumes the raw aggregated report.

use super::{SkipReason, StaticFilter};
use cf_cohesion::aggregator::NO_DATA_MESSAGE;
use cf_cohesion::report_section::{self, Bucket, Issue, SECTION_TITLE};
use cf_cohesion::{aggregate, Aggregate, AnalyzedFile, Analyzer, ComputedMetrics};
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_uast::Parser;

const METRIC_TOTAL_FUNCTIONS: &str = "Total Functions";
const METRIC_LCOM: &str = "LCOM Score";
const METRIC_COHESION_SCORE: &str = "Cohesion Score";
const METRIC_FUNCTION_COHESION: &str = "Avg Cohesion";

/// Walks `root_path` through the shared static walk and analyzes every
/// admitted file (a parse failure is skipped and counted), or `None` when
/// the path does not exist.
fn analyze_tree(root_path: &str, filter: &StaticFilter) -> Option<Vec<AnalyzedFile>> {
    let parser = Parser::new();
    let analyzer = Analyzer::new();
    let files = super::walk_source_files(root_path, &parser, filter)
        .ok()?
        .filter_map(|file| {
            let Ok(uast_root) = parser.parse(&file.rel_path, &file.bytes) else {
                super::note_skipped(&file.path, SkipReason::Unparseable);
                return None;
            };
            Some(AnalyzedFile {
                path: file.rel_path,
                report: analyzer.analyze_file(&uast_root),
            })
        })
        .collect();
    Some(files)
}

// === JSON / text / compact (renderer section) ===

/// The `static/cohesion --format json` report bytes, or `None` when the path
/// does not exist. `per_file` adds one `files` entry per analyzed file.
#[must_use]
pub fn cohesion_report_json_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let report = cohesion_report_value_flags(root_path, filter, per_file)?;
    Some(
        Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode_to_vec(&report),
    )
}

/// The `static/cohesion` single-section report tree
/// (`{overall_score_label, sections, overall_score}`), shared by the
/// single-analyzer JSON path and the multi-analyzer merge. `per_file` adds
/// one entry per analyzed file (function-free files included, with
/// `No functions found`) under the section's `files`.
#[must_use]
pub fn cohesion_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let files = analyze_tree(root_path, filter)?;
    let agg = aggregate(&files);
    let file_entries = per_file.then(|| files.iter().map(build_file_entry).collect());
    Some(build_json_report(&agg, file_entries))
}

/// The section tree the `text` / `compact` renderers consume: the same
/// metrics, distribution and issues as the JSON report.
#[must_use]
pub fn cohesion_report_value_summary(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    cohesion_report_value_flags(root_path, filter, false)
}

/// The AGGREGATED RAW cohesion report consumed by `--format plot` and
/// `report.json`: `analyzer_name`, `total_functions`, `lcom`,
/// `cohesion_score`, `function_cohesion`, `message` and the full `functions`
/// table (every function, walk order, `_source_file`-stamped).
#[must_use]
pub fn cohesion_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let files = analyze_tree(root_path, filter)?;
    let agg = aggregate(&files);
    let functions: Vec<GoValue> = agg
        .functions
        .iter()
        .map(|f| {
            let metrics = cf_cohesion::FunctionMetrics {
                name: f.name.clone(),
                start_line: f.start_line,
                line_count: f.line_count,
                variable_count: f.variable_count,
                cohesion: f.cohesion,
            };
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("name", GoValue::Str(f.name.clone()));
            m.push("start_line", GoValue::Int(f.start_line as i64));
            m.push("line_count", GoValue::Int(f.line_count as i64));
            m.push("variable_count", GoValue::Int(f.variable_count as i64));
            m.push("cohesion", GoValue::Float(f.cohesion));
            m.push(
                "cohesion_assessment",
                GoValue::Str(metrics.cohesion_assessment().to_string()),
            );
            m.push(
                "variable_assessment",
                GoValue::Str(metrics.variable_assessment().to_string()),
            );
            m.push(
                "size_assessment",
                GoValue::Str(metrics.size_assessment().to_string()),
            );
            m.push("_source_file", GoValue::Str(f.source_file.clone()));
            GoValue::Map(m)
        })
        .collect();

    let mut m = GoMap::new(MapOrigin::Map);
    m.push(
        "analyzer_name",
        GoValue::Str(cf_cohesion::ANALYZER_NAME.to_string()),
    );
    m.push("total_functions", GoValue::Int(agg.total_functions as i64));
    m.push("functions", GoValue::Array(functions));
    m.push("message", GoValue::Str(agg.message.clone()));
    m.push("lcom", GoValue::Float(agg.lcom));
    m.push("cohesion_score", GoValue::Float(agg.cohesion_score));
    m.push("function_cohesion", GoValue::Float(agg.function_cohesion));
    Some(GoValue::Map(m))
}

/// One `--per-file` entry: the file's own scalars, distribution and issues;
/// a function-free file reports `No functions found`.
fn build_file_entry(file: &AnalyzedFile) -> GoValue {
    let report = &file.report;
    let cohesions: Vec<f64> = report.functions.iter().map(|f| f.cohesion).collect();
    let issues = report_section::issues(
        report
            .functions
            .iter()
            .map(|f| (f.name.as_str(), file.path.as_str(), f.cohesion)),
    );

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(file.path.clone()));
    entry.push(
        "score_label",
        GoValue::Str(report_section::score_label(report.cohesion_score)),
    );
    entry.push("status", GoValue::Str(report.message.clone()));
    entry.push(
        "metrics",
        metrics_value(
            report.total_functions,
            report.lcom,
            report.cohesion_score,
            report.function_cohesion,
        ),
    );
    push_distribution_and_issues(
        &mut entry,
        &report_section::distribution(&cohesions),
        &issues,
    );
    entry.push("score", GoValue::Float(report.cohesion_score));
    GoValue::Map(entry)
}

/// The single-section report tree of an aggregate.
fn build_json_report(agg: &Aggregate, file_entries: Option<Vec<GoValue>>) -> GoValue {
    let cohesions: Vec<f64> = agg.functions.iter().map(|f| f.cohesion).collect();
    let issues = report_section::issues(
        agg.functions
            .iter()
            .map(|f| (f.name.as_str(), f.source_file.as_str(), f.cohesion)),
    );
    let status = if agg.has_data() {
        agg.message.clone()
    } else {
        NO_DATA_MESSAGE.to_string()
    };
    let label = report_section::score_label(agg.cohesion_score);

    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(label.clone()));
    section.push("status", GoValue::Str(status));
    section.push(
        "metrics",
        metrics_value(
            agg.total_functions,
            agg.lcom,
            agg.cohesion_score,
            agg.function_cohesion,
        ),
    );
    push_distribution_and_issues(
        &mut section,
        &report_section::distribution(&cohesions),
        &issues,
    );
    // `--per-file` entries sit between `issues` and `score`.
    if let Some(entries) = file_entries {
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(agg.cohesion_score));

    let mut report = GoMap::new(MapOrigin::Struct);
    report.push("overall_score_label", GoValue::Str(label));
    report.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    report.push("overall_score", GoValue::Float(agg.cohesion_score));
    GoValue::Map(report)
}

/// The four key metrics, floats rendered with two decimals.
fn metrics_value(total: usize, lcom: f64, score: f64, avg: f64) -> GoValue {
    GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &total.to_string()),
        metric(METRIC_LCOM, &report_section::format_metric(lcom)),
        metric(METRIC_COHESION_SCORE, &report_section::format_metric(score)),
        metric(
            METRIC_FUNCTION_COHESION,
            &report_section::format_metric(avg),
        ),
    ])
}

/// Pushes the `distribution` (omitted when empty) and `issues` keys.
fn push_distribution_and_issues(target: &mut GoMap, buckets: &[Bucket], issues: &[Issue]) {
    if !buckets.is_empty() {
        target.push(
            "distribution",
            GoValue::Array(buckets.iter().map(dist_item).collect()),
        );
    }
    target.push(
        "issues",
        GoValue::Array(issues.iter().map(issue_item).collect()),
    );
}

fn metric(label: &str, value: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("label", GoValue::Str(label.to_string()));
    m.push("value", GoValue::Str(value.to_string()));
    GoValue::Map(m)
}

fn dist_item(bucket: &Bucket) -> GoValue {
    let mut d = GoMap::new(MapOrigin::Struct);
    d.push("label", GoValue::Str(bucket.label.to_string()));
    d.push("percent", GoValue::Float(bucket.percent));
    d.push("count", GoValue::Int(bucket.count as i64));
    GoValue::Map(d)
}

fn issue_item(issue: &Issue) -> GoValue {
    let mut i = GoMap::new(MapOrigin::Struct);
    i.push("name", GoValue::Str(issue.name.clone()));
    i.push("location", GoValue::Str(issue.location.clone()));
    i.push(
        "value",
        GoValue::Str(report_section::format_metric(issue.cohesion)),
    );
    i.push("severity", GoValue::Str(issue.severity.to_string()));
    GoValue::Map(i)
}

// === YAML / binary (ComputedMetrics machine format) ===

/// The `static/cohesion --format yaml` report bytes, or `None` when the path
/// does not exist.
#[must_use]
pub fn cohesion_report_yaml(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let metrics = computed_metrics(root_path, filter)?;
    Some(cf_goyaml::marshal(&computed_metrics_go_value(&metrics)))
}

/// The `static/cohesion --format bin` report bytes (`CFB1` envelope over the
/// compact JSON of [`ComputedMetrics`]), or `None` when the path does not
/// exist.
#[must_use]
pub fn cohesion_report_bin(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let metrics = computed_metrics(root_path, filter)?;
    cf_reportutil::encode_binary_envelope(&computed_metrics_go_value(&metrics)).ok()
}

fn computed_metrics(root_path: &str, filter: &StaticFilter) -> Option<ComputedMetrics> {
    let files = analyze_tree(root_path, filter)?;
    Some(cf_cohesion::metrics::compute_metrics(&aggregate(&files)))
}

/// The value tree of [`ComputedMetrics`] (struct field order), the single
/// value both machine formats encode.
fn computed_metrics_go_value(m: &ComputedMetrics) -> GoValue {
    let mut root = GoMap::new(MapOrigin::Struct);

    let function_cohesion: Vec<GoValue> = m
        .function_cohesion
        .iter()
        .map(|f| {
            let mut o = GoMap::new(MapOrigin::Struct);
            o.push("name", GoValue::Str(f.name.clone()));
            o.push("source_file", GoValue::Str(f.source_file.clone()));
            o.push("cohesion", GoValue::Float(f.cohesion));
            o.push("quality_level", GoValue::Str(f.quality_level.clone()));
            GoValue::Map(o)
        })
        .collect();
    root.push("function_cohesion", GoValue::Array(function_cohesion));

    let mut dist = GoMap::new(MapOrigin::Map);
    for (k, v) in &m.distribution {
        dist.push(k, GoValue::Int(*v));
    }
    root.push("distribution", GoValue::Map(dist));

    let low: Vec<GoValue> = m
        .low_cohesion_functions
        .iter()
        .map(|f| {
            let mut o = GoMap::new(MapOrigin::Struct);
            o.push("name", GoValue::Str(f.name.clone()));
            o.push("source_file", GoValue::Str(f.source_file.clone()));
            o.push("cohesion", GoValue::Float(f.cohesion));
            o.push("risk_level", GoValue::Str(f.risk_level.clone()));
            o.push("recommendation", GoValue::Str(f.recommendation.clone()));
            GoValue::Map(o)
        })
        .collect();
    root.push("low_cohesion_functions", GoValue::Array(low));

    let a = &m.aggregate;
    let mut agg = GoMap::new(MapOrigin::Struct);
    agg.push("total_functions", GoValue::Int(a.total_functions));
    agg.push("lcom", GoValue::Float(a.lcom));
    agg.push("lcom_variant", GoValue::Str(a.lcom_variant.clone()));
    agg.push("cohesion_score", GoValue::Float(a.cohesion_score));
    agg.push("function_cohesion", GoValue::Float(a.function_cohesion));
    agg.push("health_score", GoValue::Float(a.health_score));
    agg.push("message", GoValue::Str(a.message.clone()));
    root.push("aggregate", GoValue::Map(agg));

    GoValue::Map(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// `f` uses {x, y}, `g` uses {x}: LCOM 0.25, f 0.5, g 1.0.
    const SHARE_GO: &str = "package main\n\nfunc f() int {\n\tx := 1\n\ty := 2\n\treturn x + y\n}\n\nfunc g() int {\n\tx := 3\n\treturn x\n}\n";
    /// `p` and `q` are disjoint: LCOM 0.5, both 0.0.
    const DISJOINT_GO: &str =
        "package main\n\nfunc p1() int {\n\tp := 1\n\treturn p\n}\n\nfunc q1() int {\n\tq := 2\n\treturn q\n}\n";
    const TYPES_GO: &str = "package main\n\ntype Config struct {\n\tName string\n}\n";

    fn write(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    fn section(dir: &Path, per_file: bool) -> GoValue {
        let report =
            cohesion_report_value_flags(dir.to_str().unwrap(), &StaticFilter::default(), per_file)
                .expect("existing path");
        match report.as_map().and_then(|m| m.get("sections")) {
            Some(GoValue::Array(sections)) => sections[0].clone(),
            other => panic!("no sections in {other:?}"),
        }
    }

    fn get<'a>(v: &'a GoValue, key: &str) -> &'a GoValue {
        v.as_map()
            .and_then(|m| m.get(key))
            .unwrap_or_else(|| panic!("key {key} missing in {v:?}"))
    }

    fn metric_value(section: &GoValue, label: &str) -> String {
        let GoValue::Array(metrics) = get(section, "metrics") else {
            panic!("metrics is not an array")
        };
        metrics
            .iter()
            .find(|m| get(m, "label").as_str() == Some(label))
            .and_then(|m| get(m, "value").as_str())
            .unwrap_or_else(|| panic!("metric {label} missing"))
            .to_string()
    }

    fn float(v: &GoValue) -> f64 {
        match v {
            GoValue::Float(f) => *f,
            GoValue::Int(i) => *i as f64,
            other => panic!("not a number: {other:?}"),
        }
    }

    /// Scalars are function-weighted over the files WITH functions, never a
    /// mean over files, and function-free files are excluded.
    #[test]
    fn aggregation_is_function_weighted_and_skips_function_free_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "share.go", SHARE_GO);
        write(dir.path(), "disjoint.go", DISJOINT_GO);
        for i in 0..5 {
            write(dir.path(), &format!("t{i}.go"), TYPES_GO);
        }
        let section = section(dir.path(), false);
        // (0.25 * 2 + 0.5 * 2) / 4 = 0.375; file-mean would be far lower.
        assert_eq!(metric_value(&section, METRIC_TOTAL_FUNCTIONS), "4");
        assert!((float(get(&section, "score")) - 0.625).abs() < 1e-9);
        assert_eq!(metric_value(&section, METRIC_LCOM), "0.38");
        // (0.5 + 1.0 + 0 + 0) / 4
        assert_eq!(metric_value(&section, METRIC_FUNCTION_COHESION), "0.38");
        assert_eq!(get(&section, "score_label").as_str(), Some("6/10"));
    }

    /// An existing empty directory and a tree of function-free files both
    /// yield the "no data" report.
    #[test]
    fn empty_and_function_free_trees_are_no_data() {
        let empty = tempfile::tempdir().unwrap();
        let empty_section = section(empty.path(), false);
        assert_eq!(
            get(&empty_section, "status").as_str(),
            Some(NO_DATA_MESSAGE)
        );
        assert_eq!(metric_value(&empty_section, METRIC_TOTAL_FUNCTIONS), "0");
        assert_eq!(get(&empty_section, "score_label").as_str(), Some("0/10"));
        assert!(empty_section
            .as_map()
            .unwrap()
            .get("distribution")
            .is_none());

        let structs = tempfile::tempdir().unwrap();
        write(structs.path(), "a.go", TYPES_GO);
        write(structs.path(), "b.go", "");
        let structs_section = section(structs.path(), false);
        assert_eq!(
            get(&structs_section, "status").as_str(),
            Some(NO_DATA_MESSAGE)
        );
        assert_eq!(float(get(&structs_section, "score")), 0.0);
    }

    /// A path that does not exist is reported to the caller as `None`.
    #[test]
    fn missing_path_is_none() {
        assert!(
            cohesion_report_value_flags("/nonexistent/zz", &StaticFilter::default(), false)
                .is_none()
        );
        assert!(cohesion_report_yaml("/nonexistent/zz", &StaticFilter::default()).is_none());
    }

    /// `text` / `compact` render the same section as JSON: the distribution
    /// and the issues are present.
    #[test]
    fn summary_section_keeps_distribution_and_issues() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "disjoint.go", DISJOINT_GO);
        let summary =
            cohesion_report_value_summary(dir.path().to_str().unwrap(), &StaticFilter::default())
                .unwrap();
        let full = cohesion_report_value_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            false,
        )
        .unwrap();
        assert_eq!(summary, full);
        let section = section(dir.path(), false);
        let GoValue::Array(issues) = get(&section, "issues") else {
            panic!("issues missing")
        };
        assert_eq!(issues.len(), 2, "both 0.0 functions are issues");
        assert!(section.as_map().unwrap().get("distribution").is_some());
    }

    /// Same-named functions in different files (or types) are all kept in the
    /// raw report and the distribution.
    #[test]
    fn raw_report_keeps_same_named_functions() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", SHARE_GO);
        write(dir.path(), "b.go", SHARE_GO);
        let raw = cohesion_raw_report_value(dir.path().to_str().unwrap(), &StaticFilter::default())
            .unwrap();
        let GoValue::Array(functions) = get(&raw, "functions") else {
            panic!("functions missing")
        };
        assert_eq!(functions.len(), 4);
        assert_eq!(get(&raw, "total_functions"), &GoValue::Int(4));
        let section = section(dir.path(), false);
        let GoValue::Array(dist) = get(&section, "distribution") else {
            panic!("distribution missing")
        };
        let total: i64 = dist
            .iter()
            .map(|d| match get(d, "count") {
                GoValue::Int(n) => *n,
                other => panic!("count {other:?}"),
            })
            .sum();
        assert_eq!(total, 4);
    }

    #[test]
    fn per_file_flag_emits_files_entries() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", SHARE_GO);
        write(dir.path(), "types.go", TYPES_GO);
        let bytes = cohesion_report_json_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            true,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(json.contains("\"files\""), "files key missing:\n{json}");
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "a.go entry missing:\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"types.go\""),
            "types.go entry missing:\n{json}"
        );
        assert!(
            json.contains("\"status\": \"No functions found\""),
            "per-file empty-result status missing:\n{json}"
        );
        assert!(
            json.contains("\"value\": \"0.25\""),
            "per-file LCOM missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.go", SHARE_GO);
        let bytes = cohesion_report_json_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            false,
        )
        .unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
    }
}
