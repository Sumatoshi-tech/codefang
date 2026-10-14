//! Report path for the `static/halstead` analyzer.
//!
//! The handler owns serialization only: the shared static walk
//! ([`super::walk_source_files`]) selects and reads the files, this module
//! parses each one with `cf_uast::Parser`, hands each tree **and its source
//! bytes** to `cf_halstead`, and serializes the results. All measurement — the operator
//! / operand classification, the per-function and union aggregates, every
//! threshold, label, health score and message — lives in the `cf-halstead`
//! crate (`cf_halstead::classify`, `::aggregate`, `::thresholds`), so each
//! format below reports the same numbers.
//!
//! ## Aggregation
//!
//! The codebase aggregate (and each `--per-file` entry) is the UNION of the
//! operator / operand multisets of its functions: `n1` / `n2` are distinct
//! across every function, `N1` / `N2` are the summed occurrences, and
//! V / D / E / T / B derive from those four numbers. Per-function statistics
//! (mean / max volume, difficulty, effort; high-effort count and share) sit
//! alongside; the status message, health score and section score come from
//! the share of high-effort functions (`cf_halstead::thresholds`).
//!
//! ## Formats
//!
//! * **json** — the structured `run` report: one `HALSTEAD` section with key
//!   metrics, the per-function volume distribution, every function as an
//!   issue (effort descending), and — with `--per-file` — one entry per
//!   analyzed file. `--per-file` is json-only (the dispatch passes the flag
//!   to the json builder alone).
//! * **yaml** / **bin** — the `ComputedMetrics` value: `function_halstead`,
//!   `distribution`, `high_effort_functions` (effort ≥ the high-effort bound)
//!   and `aggregate`; bin wraps it in the CFB1 envelope.
//! * **text** / **compact** — the summary-only section (metrics + status).
//! * **plot** — the raw report (`halstead_raw_report_value`): every function
//!   with its operator / operand maps.
//!
//! Function lists are ordered by one total key everywhere: effort desc,
//! volume desc, name asc, file asc. A folder with no analyzable files, or
//! none containing functions, yields the empty report (zeros, status
//! `No functions found`) with exit 0; only a non-existent path yields `None`.

use std::cmp::Ordering;

use super::StaticFilter;
use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_halstead::aggregate::{analyze_functions, summarize, FunctionMetrics, Summary};
use cf_halstead::thresholds::{
    difficulty_assessment, effort_assessment, is_high_effort, risk_level, severity,
    volume_assessment, VolumeLevel,
};
use cf_halstead::ANALYZER_NAME;
use cf_uast::{Node, Parser};

/// Section title in the structured report.
const SECTION_TITLE: &str = "HALSTEAD";
/// Score scale of the `N/10` labels.
const SCORE_MAX: f64 = 10.0;
/// Health score scale (the section score is `health / 100`).
const HEALTH_SCALE: f64 = 100.0;

/// One function stamped with its file location.
struct FunctionRecord {
    metrics: FunctionMetrics,
    /// Path relative to the analyzed root.
    source_file: String,
    language: String,
    directory: String,
}

/// One analyzed file: its union summary and the range of its functions
/// inside [`Aggregate::functions`] (empty for function-free files).
struct FileReport {
    rel: String,
    summary: Summary,
    start: usize,
    end: usize,
}

/// The whole analyzed tree.
struct Aggregate {
    /// Union summary over every function of every file.
    summary: Summary,
    /// Every function in walk order (files lexical, functions post-order).
    functions: Vec<FunctionRecord>,
    /// Per-file reports in walk order.
    files: Vec<FileReport>,
}

/// Measures one parsed file.
fn analyze_file(
    root: &Node,
    content: &[u8],
    rel: &str,
    language: &str,
    directory: &str,
) -> Vec<FunctionRecord> {
    analyze_functions(root, Some(content))
        .into_iter()
        .map(|metrics| FunctionRecord {
            metrics,
            source_file: rel.to_string(),
            language: language.to_string(),
            directory: directory.to_string(),
        })
        .collect()
}

/// Walks `root_path` through the shared static walk, parses each admitted
/// file, and aggregates. `None` only when the path does not exist.
fn aggregate_opts(root_path: &str, filter: &StaticFilter) -> Option<Aggregate> {
    let parser = Parser::new();
    let walk = super::walk_source_files(root_path, &parser, filter).ok()?;

    let mut functions: Vec<FunctionRecord> = Vec::new();
    let mut file_reports: Vec<FileReport> = Vec::new();
    for file in walk {
        let directory = file.directory();
        // A supported file whose grammar is not wired (or that fails to
        // parse) still counts as an analyzed, function-free file.
        let records = match super::parse_for_functions(&parser, &file) {
            Ok(node) => analyze_file(
                &node,
                &file.bytes,
                &file.rel_path,
                &file.language,
                &directory,
            ),
            Err(_) => Vec::new(),
        };
        let start = functions.len();
        let summary = summarize(records.iter().map(|r| &r.metrics));
        functions.extend(records);
        file_reports.push(FileReport {
            rel: file.rel_path,
            summary,
            start,
            end: functions.len(),
        });
    }

    Some(Aggregate {
        summary: summarize(functions.iter().map(|r| &r.metrics)),
        functions,
        files: file_reports,
    })
}

/// The one total order of function lists: effort desc, volume desc, name
/// asc, file asc.
fn function_order(a: &FunctionRecord, b: &FunctionRecord) -> Ordering {
    let (da, db) = (&a.metrics.derived, &b.metrics.derived);
    db.effort
        .total_cmp(&da.effort)
        .then_with(|| db.volume.total_cmp(&da.volume))
        .then_with(|| a.metrics.name.cmp(&b.metrics.name))
        .then_with(|| a.source_file.cmp(&b.source_file))
}

fn sorted(records: &[FunctionRecord]) -> Vec<&FunctionRecord> {
    let mut out: Vec<&FunctionRecord> = records.iter().collect();
    out.sort_by(|a, b| function_order(a, b));
    out
}

// ---------------------------------------------------------------------------
// shared serialization pieces
// ---------------------------------------------------------------------------

/// Section score (0..=1) and its `N/10` label from the health score.
fn score_of(summary: &Summary) -> (f64, String) {
    let score = summary.health_score / HEALTH_SCALE;
    let scaled = (score * SCORE_MAX).round();
    (score, format!("{scaled}/{SCORE_MAX}"))
}

/// Pushes the union counts and derived measures under their machine keys.
fn push_counts(m: &mut GoMap, summary: &Summary) {
    let (c, d) = (&summary.counts, &summary.derived);
    m.push("distinct_operators", GoValue::Int(c.distinct_operators));
    m.push("distinct_operands", GoValue::Int(c.distinct_operands));
    m.push("total_operators", GoValue::Int(c.total_operators));
    m.push("total_operands", GoValue::Int(c.total_operands));
    m.push("vocabulary", GoValue::Int(d.vocabulary));
    m.push("length", GoValue::Int(d.length));
    m.push("estimated_length", GoValue::Float(d.estimated_length));
}

/// Pushes the per-function statistics under their machine keys.
fn push_stats(m: &mut GoMap, summary: &Summary) {
    let s = &summary.stats;
    m.push("mean_volume", GoValue::Float(s.mean_volume));
    m.push("max_volume", GoValue::Float(s.max_volume));
    m.push("mean_difficulty", GoValue::Float(s.mean_difficulty));
    m.push("max_difficulty", GoValue::Float(s.max_difficulty));
    m.push("mean_effort", GoValue::Float(s.mean_effort));
    m.push("max_effort", GoValue::Float(s.max_effort));
    m.push("high_effort_functions", GoValue::Int(s.high_effort_count));
    m.push(
        "high_effort_share",
        GoValue::Float(summary.high_effort_share),
    );
}

/// Pushes the derived scalars (volume … bugs) under their machine keys.
fn push_derived(m: &mut GoMap, summary: &Summary) {
    let d = &summary.derived;
    m.push("volume", GoValue::Float(d.volume));
    m.push("difficulty", GoValue::Float(d.difficulty));
    m.push("effort", GoValue::Float(d.effort));
    m.push("time_to_program", GoValue::Float(d.time_to_program));
    m.push("delivered_bugs", GoValue::Float(d.delivered_bugs));
}

/// The key-metrics list of a section or file entry.
fn metrics_list(summary: &Summary) -> Vec<GoValue> {
    let fmt_f = cf_reportutil::format_float;
    let (c, d, s) = (&summary.counts, &summary.derived, &summary.stats);
    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    vec![
        metric("Total Functions", summary.total_functions.to_string()),
        metric("Distinct Operators (n1)", c.distinct_operators.to_string()),
        metric("Distinct Operands (n2)", c.distinct_operands.to_string()),
        metric("Total Operators (N1)", c.total_operators.to_string()),
        metric("Total Operands (N2)", c.total_operands.to_string()),
        metric("Vocabulary", d.vocabulary.to_string()),
        metric("Length", d.length.to_string()),
        metric("Volume", fmt_f(d.volume)),
        metric("Difficulty", fmt_f(d.difficulty)),
        metric("Effort", fmt_f(d.effort)),
        metric("Est. Bugs", fmt_f(d.delivered_bugs)),
        metric("Mean Effort", fmt_f(s.mean_effort)),
        metric("Max Effort", fmt_f(s.max_effort)),
        metric("High-Effort Functions", s.high_effort_count.to_string()),
    ]
}

/// The volume-band distribution items (empty for a set without functions).
#[allow(clippy::cast_precision_loss)] // function counts are far below 2^52
fn distribution_items(summary: &Summary) -> Vec<GoValue> {
    if summary.total_functions == 0 {
        return Vec::new();
    }
    let total = summary.total_functions as f64;
    VolumeLevel::ALL
        .iter()
        .map(|level| {
            let count = summary.distribution[level.index()];
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("label", GoValue::Str(level.bucket_label().to_string()));
            m.push("percent", GoValue::Float(count as f64 / total));
            m.push("count", GoValue::Int(count));
            GoValue::Map(m)
        })
        .collect()
}

/// `effort=… | vol=… | bugs=…` with one decimal.
fn format_issue_value(f: &FunctionMetrics) -> String {
    let d = &f.derived;
    format!(
        "effort={} | vol={} | bugs={}",
        cf_reportutil::format_float(d.effort),
        cf_reportutil::format_float(d.volume),
        cf_reportutil::format_float(d.delivered_bugs)
    )
}

/// Every function as an issue, in the total order.
fn issue_items(records: &[FunctionRecord]) -> Vec<GoValue> {
    sorted(records)
        .into_iter()
        .map(|r| {
            let d = &r.metrics.derived;
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(r.metrics.name.clone()));
            m.push("location", GoValue::Str(r.source_file.clone()));
            m.push("value", GoValue::Str(format_issue_value(&r.metrics)));
            m.push(
                "severity",
                GoValue::Str(severity(d.effort, d.delivered_bugs).to_string()),
            );
            GoValue::Map(m)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// yaml / bin: ComputedMetrics
// ---------------------------------------------------------------------------

/// One `function_halstead` entry (empty location fields omitted).
fn function_halstead_entry(r: &FunctionRecord) -> GoValue {
    let d = &r.metrics.derived;
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("name", GoValue::Str(r.metrics.name.clone()));
    if !r.source_file.is_empty() {
        m.push("source_file", GoValue::Str(r.source_file.clone()));
    }
    if !r.language.is_empty() {
        m.push("language", GoValue::Str(r.language.clone()));
    }
    if !r.directory.is_empty() {
        m.push("directory", GoValue::Str(r.directory.clone()));
    }
    m.push("volume", GoValue::Float(d.volume));
    m.push("difficulty", GoValue::Float(d.difficulty));
    m.push("effort", GoValue::Float(d.effort));
    m.push("time_to_program", GoValue::Float(d.time_to_program));
    m.push("delivered_bugs", GoValue::Float(d.delivered_bugs));
    m.push(
        "complexity_level",
        GoValue::Str(VolumeLevel::of(d.volume).name().to_string()),
    );
    GoValue::Map(m)
}

/// One `high_effort_functions` entry.
fn high_effort_entry(r: &FunctionRecord) -> GoValue {
    let d = &r.metrics.derived;
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("name", GoValue::Str(r.metrics.name.clone()));
    if !r.source_file.is_empty() {
        m.push("source_file", GoValue::Str(r.source_file.clone()));
    }
    m.push("volume", GoValue::Float(d.volume));
    m.push("effort", GoValue::Float(d.effort));
    m.push("time_to_program", GoValue::Float(d.time_to_program));
    m.push("delivered_bugs", GoValue::Float(d.delivered_bugs));
    m.push("risk_level", GoValue::Str(risk_level(d.effort).to_string()));
    GoValue::Map(m)
}

/// The `ComputedMetrics` value shared by the yaml and bin formats.
fn computed_metrics(agg: &Aggregate) -> GoValue {
    let ordered = sorted(&agg.functions);
    let function_halstead: Vec<GoValue> =
        ordered.iter().map(|r| function_halstead_entry(r)).collect();
    let high_effort_functions: Vec<GoValue> = ordered
        .iter()
        .filter(|r| is_high_effort(r.metrics.derived.effort))
        .map(|r| high_effort_entry(r))
        .collect();

    let mut dist = GoMap::new(MapOrigin::Struct);
    for level in VolumeLevel::ALL {
        dist.push(
            level.key(),
            GoValue::Int(agg.summary.distribution[level.index()]),
        );
    }

    let mut aggregate = GoMap::new(MapOrigin::Struct);
    aggregate.push("total_functions", GoValue::Int(agg.summary.total_functions));
    push_derived(&mut aggregate, &agg.summary);
    push_counts(&mut aggregate, &agg.summary);
    push_stats(&mut aggregate, &agg.summary);
    aggregate.push("health_score", GoValue::Float(agg.summary.health_score));
    aggregate.push("message", GoValue::Str(agg.summary.message.to_string()));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("function_halstead", GoValue::Array(function_halstead));
    root.push("distribution", GoValue::Map(dist));
    root.push(
        "high_effort_functions",
        GoValue::Array(high_effort_functions),
    );
    root.push("aggregate", GoValue::Map(aggregate));
    GoValue::Map(root)
}

/// Builds the `static/halstead --format bin` CFB1 envelope, or `None` when
/// the path does not exist.
#[must_use]
pub fn halstead_bin_report(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let agg = aggregate_opts(root_path, filter)?;
    cf_reportutil::encode_binary_envelope(&computed_metrics(&agg)).ok()
}

/// Builds the `static/halstead --format yaml` bytes (the same
/// `ComputedMetrics` value as bin, marshaled as YAML), or `None` when the
/// path does not exist.
#[must_use]
pub fn halstead_yaml_report(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let agg = aggregate_opts(root_path, filter)?;
    Some(cf_goyaml::marshal(&computed_metrics(&agg)))
}

// ---------------------------------------------------------------------------
// plot: the raw aggregated report
// ---------------------------------------------------------------------------

/// One raw `functions` item: every per-function number, the three
/// assessments, the operator / operand maps, and the location stamps.
fn raw_function_item(r: &FunctionRecord) -> GoValue {
    let f = &r.metrics;
    let (c, d) = (&f.counts, &f.derived);
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("name", GoValue::Str(f.name.clone()));
    m.push("volume", GoValue::Float(d.volume));
    m.push("difficulty", GoValue::Float(d.difficulty));
    m.push("effort", GoValue::Float(d.effort));
    m.push("time_to_program", GoValue::Float(d.time_to_program));
    m.push("delivered_bugs", GoValue::Float(d.delivered_bugs));
    m.push("distinct_operators", GoValue::Int(c.distinct_operators));
    m.push("distinct_operands", GoValue::Int(c.distinct_operands));
    m.push("total_operators", GoValue::Int(c.total_operators));
    m.push("total_operands", GoValue::Int(c.total_operands));
    m.push("vocabulary", GoValue::Int(d.vocabulary));
    m.push("length", GoValue::Int(d.length));
    m.push("estimated_length", GoValue::Float(d.estimated_length));
    m.push(
        "volume_assessment",
        GoValue::Str(volume_assessment(d.volume).to_string()),
    );
    m.push(
        "difficulty_assessment",
        GoValue::Str(difficulty_assessment(d.difficulty).to_string()),
    );
    m.push(
        "effort_assessment",
        GoValue::Str(effort_assessment(d.effort).to_string()),
    );
    let mut operators = GoMap::new(MapOrigin::Map);
    for (k, v) in &f.operators {
        operators.push(k, GoValue::Int(*v));
    }
    m.push("operators", GoValue::Map(operators));
    let mut operands = GoMap::new(MapOrigin::Map);
    for (k, v) in &f.operands {
        operands.push(k, GoValue::Int(*v));
    }
    m.push("operands", GoValue::Map(operands));
    m.push("_source_file", GoValue::Str(r.source_file.clone()));
    if !r.language.is_empty() {
        m.push("_language", GoValue::Str(r.language.clone()));
    }
    if !r.directory.is_empty() {
        m.push("_directory", GoValue::Str(r.directory.clone()));
    }
    GoValue::Map(m)
}

/// Builds the raw aggregated report consumed by `--format plot`
/// (`report.json`): `analyzer_name`, `total_functions`, `functions` (walk
/// order, each with its operator / operand maps), `message`, the union
/// counts and measures, the per-function statistics and `health_score`.
/// `None` when the path does not exist.
#[must_use]
pub fn halstead_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let agg = aggregate_opts(root_path, filter)?;
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str(ANALYZER_NAME.to_string()));
    m.push("total_functions", GoValue::Int(agg.summary.total_functions));
    m.push(
        "functions",
        GoValue::Array(agg.functions.iter().map(raw_function_item).collect()),
    );
    m.push("message", GoValue::Str(agg.summary.message.to_string()));
    push_derived(&mut m, &agg.summary);
    push_counts(&mut m, &agg.summary);
    push_stats(&mut m, &agg.summary);
    m.push("health_score", GoValue::Float(agg.summary.health_score));
    Some(GoValue::Map(m))
}

// ---------------------------------------------------------------------------
// json: the structured `run` report
// ---------------------------------------------------------------------------

/// One `--per-file` entry: the file's union metrics, its own distribution,
/// its functions as issues, and a score from its own high-effort share.
fn build_file_entry(file: &FileReport, functions: &[FunctionRecord]) -> GoValue {
    let (score, score_label) = score_of(&file.summary);
    let distribution = distribution_items(&file.summary);
    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(file.rel.clone()));
    entry.push("score_label", GoValue::Str(score_label));
    entry.push("status", GoValue::Str(file.summary.message.to_string()));
    entry.push("metrics", GoValue::Array(metrics_list(&file.summary)));
    if !distribution.is_empty() {
        entry.push("distribution", GoValue::Array(distribution));
    }
    entry.push(
        "issues",
        GoValue::Array(issue_items(&functions[file.start..file.end])),
    );
    entry.push("score", GoValue::Float(score));
    GoValue::Map(entry)
}

/// Builds the `static/halstead --format json` bytes for `root_path`, or
/// `None` when the path does not exist.
#[must_use]
pub fn halstead_json_report(root_path: &str) -> Option<Vec<u8>> {
    halstead_json_report_flags(root_path, &StaticFilter::default(), false)
}

/// [`halstead_json_report`] with the policy filter and the `--per-file`
/// enrichment (one entry per analyzed file, function-free files included).
#[must_use]
pub fn halstead_json_report_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let root = halstead_report_value_flags(root_path, filter, per_file)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the single-section structured report value, shared by the json
/// byte path and the multi-analyzer static-JSON merge.
#[must_use]
pub fn halstead_report_value(root_path: &str) -> Option<GoValue> {
    halstead_report_value_mode_flags(root_path, &StaticFilter::default(), false, false)
}

/// [`halstead_report_value`] with the policy filter and `--per-file`.
#[must_use]
pub fn halstead_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    halstead_report_value_mode_flags(root_path, filter, false, per_file)
}

/// The summary-only section (`text` / `compact`): key metrics and status,
/// without the per-function distribution and issues.
#[must_use]
pub fn halstead_report_value_summary(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    halstead_report_value_mode_flags(root_path, filter, true, false)
}

fn halstead_report_value_mode_flags(
    root_path: &str,
    filter: &StaticFilter,
    summary_only: bool,
    per_file: bool,
) -> Option<GoValue> {
    let agg = aggregate_opts(root_path, filter)?;
    let (score, score_label) = score_of(&agg.summary);

    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(score_label.clone()));
    section.push("status", GoValue::Str(agg.summary.message.to_string()));
    section.push("metrics", GoValue::Array(metrics_list(&agg.summary)));
    if !summary_only {
        let distribution = distribution_items(&agg.summary);
        if !distribution.is_empty() {
            section.push("distribution", GoValue::Array(distribution));
        }
        section.push("issues", GoValue::Array(issue_items(&agg.functions)));
    }
    if per_file {
        let entries: Vec<GoValue> = agg
            .files
            .iter()
            .map(|f| build_file_entry(f, &agg.functions))
            .collect();
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(score));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(score_label));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(score));
    Some(GoValue::Map(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_halstead::thresholds::{MSG_LOW, MSG_NO_FUNCTIONS};

    /// `hal.go` (`c := a + b; return c * 2`) and a function-free file.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("hal.go"),
            "package main\n\nfunc hal(a int, b int) int {\n\tc := a + b\n\treturn c * 2\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("types.go"),
            "package main\n\ntype Config struct {\n\tName string\n}\n",
        )
        .unwrap();
        dir
    }

    fn json_of(dir: &tempfile::TempDir, per_file: bool) -> String {
        let bytes = halstead_json_report_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            per_file,
        )
        .expect("report");
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn per_file_flag_emits_one_entry_per_analyzed_file() {
        let dir = fixture();
        let json = json_of(&dir, true);
        assert!(json.contains("\"files\""), "files key missing:\n{json}");
        assert!(json.contains("\"file_path\": \"hal.go\""), "{json}");
        assert!(json.contains("\"file_path\": \"types.go\""), "{json}");
        // The function-free file carries the empty status.
        assert!(
            json.contains(&format!("\"status\": \"{MSG_NO_FUNCTIONS}\"")),
            "{json}"
        );
        // The function file carries the low-complexity status.
        assert!(
            json.contains(&format!("\"status\": \"{MSG_LOW}\"")),
            "{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let json = json_of(&dir, false);
        assert!(!json.contains("\"files\""), "{json}");
        assert!(!json.contains("\"file_path\""));
    }

    /// `hal`: n1 = 4, n2 = 4, N1 = 4, N2 = 5 → V = 27, D = 2.5, E = 67.5.
    #[test]
    fn json_metrics_are_the_union_counts_of_the_functions() {
        let dir = fixture();
        let json = json_of(&dir, false);
        for needle in [
            "\"label\": \"Total Functions\",\n          \"value\": \"1\"",
            "\"label\": \"Distinct Operators (n1)\",\n          \"value\": \"4\"",
            "\"label\": \"Distinct Operands (n2)\",\n          \"value\": \"4\"",
            "\"label\": \"Total Operators (N1)\",\n          \"value\": \"4\"",
            "\"label\": \"Total Operands (N2)\",\n          \"value\": \"5\"",
            "\"label\": \"Vocabulary\",\n          \"value\": \"8\"",
            "\"label\": \"Length\",\n          \"value\": \"9\"",
            "\"label\": \"Volume\",\n          \"value\": \"27.0\"",
            "\"label\": \"Difficulty\",\n          \"value\": \"2.5\"",
            "\"label\": \"Effort\",\n          \"value\": \"67.5\"",
            "\"value\": \"effort=67.5 | vol=27.0 | bugs=0.0\"",
        ] {
            assert!(json.contains(needle), "missing {needle:?} in:\n{json}");
        }
    }

    #[test]
    fn empty_and_unsupported_folders_yield_the_empty_report_not_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("notes.txt"), "this is not source\n").unwrap();
        let path = dir.path().to_str().unwrap();
        let filter = StaticFilter::default();

        let json =
            String::from_utf8(halstead_json_report_flags(path, &filter, true).unwrap()).unwrap();
        assert!(
            json.contains(&format!("\"status\": \"{MSG_NO_FUNCTIONS}\"")),
            "{json}"
        );
        assert!(json.contains("\"label\": \"Total Functions\",\n          \"value\": \"0\""));
        assert!(
            !json.contains("\"distribution\""),
            "no buckets without functions"
        );
        assert!(json.contains("\"files\": []"), "{json}");

        let yaml = String::from_utf8(halstead_yaml_report(path, &filter).unwrap()).unwrap();
        assert!(yaml.contains("total_functions: 0"), "{yaml}");
        assert!(
            yaml.contains(&format!("message: {MSG_NO_FUNCTIONS}")),
            "{yaml}"
        );
        assert!(yaml.contains("function_halstead: []"), "{yaml}");

        assert!(halstead_bin_report(path, &filter).is_some());
        let raw = halstead_raw_report_value(path, &filter).expect("raw");
        let raw = raw.as_map().expect("map");
        assert!(matches!(raw.get("total_functions"), Some(GoValue::Int(0))));
        assert!(matches!(raw.get("functions"), Some(GoValue::Array(v)) if v.is_empty()));

        let text = halstead_report_value_summary(path, &filter).expect("summary");
        assert!(text.as_map().is_some());
    }

    #[test]
    fn missing_path_is_none() {
        assert!(halstead_json_report("/nonexistent/halstead/path").is_none());
        assert!(
            halstead_yaml_report("/nonexistent/halstead/path", &StaticFilter::default()).is_none()
        );
    }

    /// yaml and json classify functions into the same volume buckets, and
    /// the yaml aggregate carries the real union ints (never zeros).
    #[test]
    fn yaml_aggregate_carries_union_ints_and_the_same_distribution() {
        let dir = fixture();
        let yaml = String::from_utf8(
            halstead_yaml_report(dir.path().to_str().unwrap(), &StaticFilter::default()).unwrap(),
        )
        .unwrap();
        for needle in [
            "distinct_operators: 4",
            "distinct_operands: 4",
            "total_operators: 4",
            "total_operands: 5",
            "vocabulary: 8",
            "length: 9",
            "volume: 27",
            "difficulty: 2.5",
            "effort: 67.5",
            "high_effort_functions: 0",
            "health_score: 100",
            &format!("message: {MSG_LOW}"),
            "low: 1",
            "medium: 0",
            "complexity_level: Low",
        ] {
            assert!(yaml.contains(needle), "missing {needle:?} in:\n{yaml}");
        }
        let json = json_of(&dir, false);
        assert!(json.contains("\"label\": \"Low (\\u003c100)\",\n          \"percent\": 1,\n          \"count\": 1"), "{json}");
    }

    /// Issues and `function_halstead` share the total order: effort desc,
    /// then volume desc, then name, then file.
    #[test]
    fn function_lists_follow_the_total_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nfunc small() int { return 1 }\n\nfunc big(a, b, c int) int {\n\td := a + b*c - a/b\n\treturn d * d\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.go"),
            "package main\n\nfunc small() int { return 2 }\n",
        )
        .unwrap();
        let path = dir.path().to_str().unwrap();
        let json = String::from_utf8(
            halstead_json_report_flags(path, &StaticFilter::default(), false).unwrap(),
        )
        .unwrap();
        let big = json.find("\"name\": \"big\"").expect("big");
        let small_a = json.find("\"location\": \"a.go\"").expect("a.go issue");
        let small_b = json.find("\"location\": \"b.go\"").expect("b.go issue");
        assert!(big < small_a, "highest effort first:\n{json}");
        assert!(small_a < small_b, "ties break by name then file:\n{json}");

        let yaml = String::from_utf8(halstead_yaml_report(path, &StaticFilter::default()).unwrap())
            .unwrap();
        let big_y = yaml.find("name: big").expect("big");
        let a_y = yaml.find("source_file: a.go").expect("a.go");
        let b_y = yaml.find("source_file: b.go").expect("b.go");
        assert!(big_y < a_y && a_y < b_y, "{yaml}");
    }
}
