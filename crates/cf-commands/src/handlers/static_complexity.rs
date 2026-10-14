//! The `static/complexity` handlers: one directory walk shared by every
//! output format, plus the JSON sections report (`--format json`, also the
//! value behind `text` / `compact`) and the raw aggregated report
//! (`--format plot` / `report.json`). The yaml and bin siblings
//! ([`super::static_complexity_yaml`], [`super::static_complexity_bin`])
//! serialize the same [`Collected`] walk products through
//! [`cf_complexity::report::computed_metrics`].
//!
//! Division of labour: [`cf_complexity`] owns every number (counting rules,
//! thresholds, aggregation, ordering, the UAST bridge); the shared static
//! walk ([`super::walk_source_files`]) owns file selection (parser support,
//! `--languages` globs, path policy, the generated-content marker, the read
//! cap); this module owns the per-file stamping and the section/value
//! shapes.

use cf_complexity::node::Node as CxNode;
use cf_complexity::report::{aggregate, function_order, Aggregate, FunctionInput};
use cf_complexity::thresholds::{
    cognitive_assessment, complexity_assessment, complexity_message, nesting_assessment,
    section_score, severity, terminal_bucket, TERMINAL_BUCKETS,
};
use cf_complexity::{AnalysisOptions, Analyzer};
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_uast::Parser;

use super::{SkipReason, SourceFile, StaticFilter};

const SECTION_TITLE: &str = "COMPLEXITY";
const ANALYZER_NAME: &str = "complexity";

const METRIC_TOTAL_FUNCTIONS: &str = "Total Functions";
const METRIC_AVG_COMPLEXITY: &str = "Avg Complexity";
const METRIC_MAX_COMPLEXITY: &str = "Max Complexity";
const METRIC_TOTAL_COMPLEXITY: &str = "Total Complexity";
const METRIC_COGNITIVE_TOTAL: &str = "Cognitive Total";
const METRIC_MAX_NESTING: &str = "Max Nesting";
const METRIC_DECISION_POINTS: &str = "Decision Points";

/// Section status when the walk analyzed no file at all.
const DEFAULT_STATUS_MESSAGE: &str = "No complexity data available";
/// Per-file status / raw-report message for a file without functions.
const MSG_NO_FUNCTIONS: &str = "No functions found";

/// Score label denominator: scores are rendered as `N/10`.
const SCORE_LABEL_SCALE: f64 = 10.0;

/// One analyzed (parsed) file and its functions, in crate order.
pub(crate) struct AnalyzedFile {
    /// Path relative to the analyzed root (the file name for a FILE root).
    location: String,
    /// Functions stamped with `location`, language and directory.
    functions: Vec<FunctionInput>,
}

/// The walk products: every parsed file in walk order, function-free files
/// included.
pub(crate) struct Collected {
    files: Vec<AnalyzedFile>,
}

impl Collected {
    /// Every function across all files, in the report order
    /// ([`function_order`]: cyclomatic desc, cognitive desc, nesting desc,
    /// name, file, line).
    pub(crate) fn functions(&self) -> Vec<FunctionInput> {
        let mut all: Vec<FunctionInput> = self
            .files
            .iter()
            .flat_map(|f| f.functions.iter().cloned())
            .collect();
        all.sort_by(function_order);
        all
    }
}

/// Walks `root_path` through the shared static walk and analyzes every
/// admitted file, or `None` when the path does not exist.
pub(crate) fn collect(root_path: &str, filter: &StaticFilter) -> Option<Collected> {
    let parser = Parser::new();
    let files = super::walk_source_files(root_path, &parser, filter)
        .ok()?
        .filter_map(|file| analyze_file(&file, &parser))
        .collect();
    Some(Collected { files })
}

/// Parses and measures one admitted file; `None` when it fails to parse
/// (counted as skipped).
fn analyze_file(file: &SourceFile, parser: &Parser) -> Option<AnalyzedFile> {
    let Ok(tree) = super::parse_for_functions(parser, file) else {
        super::note_skipped(&file.path, SkipReason::Unparseable);
        return None;
    };
    let root = CxNode::from_uast(&tree);
    let metrics = Analyzer.function_metrics_with(
        Some(&root),
        &AnalysisOptions {
            source: Some(&file.bytes),
            find_depth: None,
        },
    );
    let directory = file.directory();
    let functions = metrics
        .iter()
        .map(|m| FunctionInput::stamped(m, &file.rel_path, &file.language, &directory))
        .collect();
    Some(AnalyzedFile {
        location: file.rel_path.clone(),
        functions,
    })
}

/// `static/complexity --format json` bytes: the sections report, with the
/// `--per-file` enrichment when `per_file` is set. `None` when the path does
/// not exist.
#[must_use]
pub fn complexity_report_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let report = complexity_report_value_flags(root_path, filter, per_file)?;
    Some(
        Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode_to_vec(&report),
    )
}

/// The sections report value (one scored `COMPLEXITY` section) with the
/// optional `--per-file` `files` array. Shared by the single-analyzer JSON
/// path and the multi-analyzer static-JSON merge.
#[must_use]
pub fn complexity_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let collected = collect(root_path, filter)?;
    Some(build_json_report(&collected, per_file))
}

/// The section value the `text` / `compact` renderers consume: the same
/// section as `--format json` (key metrics, distribution and the ranked
/// function rows the text renderer prints as "Top Issues"), never per-file.
#[must_use]
pub fn complexity_report_value_summary(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    complexity_report_value_flags(root_path, filter, false)
}

/// The raw aggregated report (`--format plot` input and `report.json`):
/// a map-origin object with `analyzer_name`, the count/sum/max scalars,
/// every function (`functions`, with assessments and the `_source_file` /
/// `_language` / `_directory` stamps), `average_complexity`,
/// `max_complexity` and `message`. With no parsed file the empty shape
/// (`total_functions`, `average_complexity`, `max_complexity`,
/// `total_complexity`, `message`) is returned.
#[must_use]
pub fn complexity_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let collected = collect(root_path, filter)?;
    let functions = collected.functions();
    let agg = aggregate(&functions);
    let mut m = GoMap::new(MapOrigin::Map);
    if collected.files.is_empty() {
        m.push("total_functions", GoValue::Int(0));
        m.push("average_complexity", GoValue::Float(0.0));
        m.push("max_complexity", GoValue::Int(0));
        m.push("total_complexity", GoValue::Int(0));
        m.push("message", GoValue::Str(MSG_NO_FUNCTIONS.to_string()));
        return Some(GoValue::Map(m));
    }
    m.push("analyzer_name", GoValue::Str(ANALYZER_NAME.to_string()));
    m.push("total_functions", GoValue::Int(agg.total_functions));
    m.push("total_complexity", GoValue::Int(agg.total_complexity));
    m.push("decision_points", GoValue::Int(agg.decision_points));
    m.push(
        "cognitive_complexity",
        GoValue::Int(agg.cognitive_complexity),
    );
    m.push("nesting_depth", GoValue::Int(agg.nesting_depth));
    m.push(
        "functions",
        GoValue::Array(functions.iter().map(raw_function_item).collect()),
    );
    m.push("average_complexity", GoValue::Float(agg.average_complexity));
    m.push("max_complexity", GoValue::Int(agg.max_complexity));
    m.push("message", GoValue::Str(agg.message.to_string()));
    Some(GoValue::Map(m))
}

/// One raw `functions` item: metrics, assessments and the file stamps
/// (map-origin, so keys byte-sort on encode).
fn raw_function_item(f: &FunctionInput) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("name", GoValue::Str(f.name.clone()));
    m.push(
        "cyclomatic_complexity",
        GoValue::Int(f.cyclomatic_complexity),
    );
    m.push("cognitive_complexity", GoValue::Int(f.cognitive_complexity));
    m.push("nesting_depth", GoValue::Int(f.nesting_depth));
    m.push("lines_of_code", GoValue::Int(f.lines_of_code));
    m.push(
        "complexity_assessment",
        GoValue::Str(complexity_assessment(f.cyclomatic_complexity).to_string()),
    );
    m.push(
        "cognitive_assessment",
        GoValue::Str(cognitive_assessment(f.cognitive_complexity).to_string()),
    );
    m.push(
        "nesting_assessment",
        GoValue::Str(nesting_assessment(f.nesting_depth).to_string()),
    );
    m.push("_source_file", GoValue::Str(f.source_file.clone()));
    if !f.language.is_empty() {
        m.push("_language", GoValue::Str(f.language.clone()));
    }
    m.push("_directory", GoValue::Str(f.directory.clone()));
    GoValue::Map(m)
}

/// The sections report: `{overall_score_label, sections: [section],
/// overall_score}`.
fn build_json_report(collected: &Collected, per_file: bool) -> GoValue {
    let functions = collected.functions();
    let agg = aggregate(&functions);
    let score = section_score(agg.average_complexity);
    let status = if collected.files.is_empty() {
        DEFAULT_STATUS_MESSAGE.to_string()
    } else {
        agg.message.to_string()
    };

    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(score_label(score)));
    section.push("status", GoValue::Str(status));
    section.push("metrics", key_metrics(&agg));
    if let Some(dist) = distribution(&functions) {
        section.push("distribution", dist);
    }
    section.push("issues", issues(&functions));
    if per_file {
        section.push(
            "files",
            GoValue::Array(collected.files.iter().map(file_entry).collect()),
        );
    }
    section.push("score", GoValue::Float(score));

    let mut report = GoMap::new(MapOrigin::Struct);
    report.push("overall_score_label", GoValue::Str(score_label(score)));
    report.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    report.push("overall_score", GoValue::Float(score));
    GoValue::Map(report)
}

/// One `--per-file` entry: the file's own section-shaped summary
/// (`file_path`, `score_label`, `status`, `metrics`, `distribution` when it
/// has functions, `issues`, `score`).
fn file_entry(file: &AnalyzedFile) -> GoValue {
    let mut functions = file.functions.clone();
    functions.sort_by(function_order);
    let agg = aggregate(&functions);
    let score = section_score(agg.average_complexity);
    let status = if functions.is_empty() {
        MSG_NO_FUNCTIONS.to_string()
    } else {
        complexity_message(agg.average_complexity).to_string()
    };
    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(file.location.clone()));
    entry.push("score_label", GoValue::Str(score_label(score)));
    entry.push("status", GoValue::Str(status));
    entry.push("metrics", key_metrics(&agg));
    if let Some(dist) = distribution(&functions) {
        entry.push("distribution", dist);
    }
    entry.push("issues", issues(&functions));
    entry.push("score", GoValue::Float(score));
    GoValue::Map(entry)
}

/// The key metrics rows, as the terminal and JSON renderers print them.
fn key_metrics(agg: &Aggregate) -> GoValue {
    GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &agg.total_functions.to_string()),
        metric(
            METRIC_AVG_COMPLEXITY,
            &format!("{:.1}", agg.average_complexity),
        ),
        metric(METRIC_MAX_COMPLEXITY, &agg.max_complexity.to_string()),
        metric(METRIC_TOTAL_COMPLEXITY, &agg.total_complexity.to_string()),
        metric(
            METRIC_COGNITIVE_TOTAL,
            &agg.cognitive_complexity.to_string(),
        ),
        metric(METRIC_MAX_NESTING, &agg.nesting_depth.to_string()),
        metric(METRIC_DECISION_POINTS, &agg.decision_points.to_string()),
    ])
}

/// The four terminal buckets with their share of all functions; `None`
/// without functions (the key is omitted).
fn distribution(functions: &[FunctionInput]) -> Option<GoValue> {
    if functions.is_empty() {
        return None;
    }
    let mut counts = [0i64; TERMINAL_BUCKETS.len()];
    for f in functions {
        counts[terminal_bucket(f.cyclomatic_complexity)] += 1;
    }
    let total = functions.len() as f64;
    Some(GoValue::Array(
        TERMINAL_BUCKETS
            .iter()
            .zip(counts)
            .map(|(label, count)| {
                let mut d = GoMap::new(MapOrigin::Struct);
                d.push("label", GoValue::Str((*label).to_string()));
                d.push("percent", GoValue::Float(count as f64 / total));
                d.push("count", GoValue::Int(count));
                GoValue::Map(d)
            })
            .collect(),
    ))
}

/// Every function as an issue row (`name`, `location`, `CC=.. | Cog=.. |
/// Nest=..`, `severity`), in the given (already ranked) order.
fn issues(functions: &[FunctionInput]) -> GoValue {
    GoValue::Array(
        functions
            .iter()
            .map(|f| {
                let mut iss = GoMap::new(MapOrigin::Struct);
                iss.push("name", GoValue::Str(f.name.clone()));
                iss.push("location", GoValue::Str(f.source_file.clone()));
                iss.push(
                    "value",
                    GoValue::Str(format!(
                        "CC={} | Cog={} | Nest={}",
                        f.cyclomatic_complexity, f.cognitive_complexity, f.nesting_depth
                    )),
                );
                iss.push(
                    "severity",
                    GoValue::Str(severity(f.cyclomatic_complexity).to_string()),
                );
                GoValue::Map(iss)
            })
            .collect(),
    )
}

/// A 0..=1 score rendered as `N/10`.
fn score_label(score: f64) -> String {
    format!("{}/10", (score * SCORE_LABEL_SCALE).round() as i64)
}

fn metric(label: &str, value: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("label", GoValue::Str(label.to_string()));
    m.push("value", GoValue::Str(value.to_string()));
    GoValue::Map(m)
}

#[cfg(test)]
mod tests {
    use super::{complexity_report_value_flags, score_label};
    use crate::handlers::StaticFilter;
    use cf_gojson::GoValue;

    /// A FILE root stamps the file's name (never `.`) on the per-file entry
    /// and on every function's location, and a nested file is stamped
    /// root-relative with its directory.
    #[test]
    fn stamps_are_root_relative_and_a_file_root_stamps_the_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("pkg")).unwrap();
        let src =
            "package main\n\nfunc f(a int) int {\n\tif a > 0 {\n\t\treturn a\n\t}\n\treturn 0\n}\n";
        std::fs::write(dir.path().join("pkg/a.go"), src).unwrap();
        let file = dir.path().join("pkg/a.go");
        let report =
            complexity_report_value_flags(file.to_str().unwrap(), &StaticFilter::default(), true)
                .expect("file root");
        let json = String::from_utf8(cf_gojson::marshal(&report)).unwrap();
        assert!(json.contains("\"file_path\":\"a.go\""), "{json}");
        assert!(json.contains("\"location\":\"a.go\""), "{json}");
        assert!(!json.contains("\"file_path\":\".\""), "{json}");

        let raw = super::complexity_raw_report_value(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
        )
        .expect("dir root");
        let functions = match raw.as_map().and_then(|m| m.get("functions")) {
            Some(GoValue::Array(items)) => items.clone(),
            other => panic!("functions missing: {other:?}"),
        };
        let first = functions[0].as_map().expect("function map");
        assert_eq!(
            first.get("_source_file"),
            Some(&GoValue::Str("pkg/a.go".to_string()))
        );
        assert_eq!(
            first.get("_directory"),
            Some(&GoValue::Str("pkg".to_string()))
        );
    }

    #[test]
    fn score_labels_round_to_tenths() {
        assert_eq!(score_label(1.0), "10/10");
        assert_eq!(score_label(0.6), "6/10");
        assert_eq!(score_label(0.1), "1/10");
    }
}
