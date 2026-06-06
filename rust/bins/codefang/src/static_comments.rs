//! Static-analysis YAML report path for the UAST `static/comments` analyzer.
//!
//! Reproduces the Go static pipeline for the single-analyzer
//! `codefang run --analyzers static/comments --format yaml` capture
//! (`StaticService.streamFiles` → `analyzeFile` → `comments.Analyzer.Analyze` →
//! `StampSourceFile`/`StampLanguage` → `comments.Aggregator` (base metrics
//! processor + `DetailedDataCollector`) → `comments.ComputeAllMetrics` →
//! `yaml.Marshal(ComputedMetrics)`).
//!
//! # Pipeline
//!
//!  1. Walk `root_path` lexically (`filepath.WalkDir` order), keeping every
//!     regular file the UAST parser supports (`Parser::is_supported`) that is not
//!     excluded by the path policy. `--languages` is empty here, so the language
//!     glob filter is a no-op.
//!  2. Parse each file → UAST `Node`; run [`cf_comments::Analyzer::analyze`] to
//!     get the per-file report map (`comments`/`functions` collections + scalar
//!     metrics).
//!  3. Stamp `_source_file` (path relative to `root_path`), `_directory`
//!     (`filepath.Dir` of the relative path), and `_language`
//!     (`Parser::get_language`) onto every collected comment/function item.
//!  4. Aggregate across files exactly as `common.Aggregator`:
//!     - `comments` / `functions` are **concatenated** in file order
//!       (`DetailedDataCollector`, no dedup);
//!     - count keys (`total_comments`, `good_comments`, `bad_comments`,
//!       `total_functions`, `documented_functions`, `total_comment_details`) are
//!       summed;
//!     - numeric keys (`overall_score`, `good_comments_ratio`,
//!       `documentation_coverage`) are summed and divided by the processed-report
//!       count (the mean over all parsed files).
//!  5. Feed the aggregated report to the `comments.ComputeAllMetrics` closed form
//!     (`ParseReportData` reads only `line` + the stamped source metadata off each
//!     collection item; the convert maps carry neither `quality`/`type`/`score`
//!     for comments nor `name`/`has_comment`/… for functions, so those per-item
//!     fields stay at their zero values, exactly as in the Go golden), and emit
//!     `ComputedMetrics` through cf-goyaml (gopkg.in/yaml.v3 parity).

use std::fs;
use std::path::Path;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
use cf_uast::Parser;

/// One collected comment item (post-stamp): only the fields `ParseReportData`
/// reads off a comment map survive into the YAML.
struct CommentItem {
    line: i64,
    source_file: String,
    language: String,
    directory: String,
}

/// One collected function item (post-stamp): `ParseReportData` reads no payload
/// keys off the function convert map (`function`/`type`/… are not the keys it
/// looks for), so only the stamped source metadata is meaningful.
struct FunctionItem {
    source_file: String,
    language: String,
    directory: String,
}

/// Cross-file accumulator mirroring `comments.Aggregator`.
#[derive(Default)]
struct Aggregated {
    comments: Vec<CommentItem>,
    functions: Vec<FunctionItem>,
    total_comments: i64,
    good_comments: i64,
    bad_comments: i64,
    total_functions: i64,
    documented_functions: i64,
    // Numeric-key running sums + processed-report count (for the mean).
    sum_overall_score: f64,
    sum_good_comments_ratio: f64,
    sum_documentation_coverage: f64,
    report_count: i64,
}

/// Builds the `static/comments --format yaml` report bytes for `root_path`, or
/// `None` when the path cannot be read (the caller then falls through to the
/// blocked-dependency sentinel).
#[must_use]
pub fn comments_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let metrics = comments_metrics(root_path)?;
    Some(cf_goyaml::marshal(&metrics))
}

/// Builds the `static/comments --format bin` report bytes for `root_path`, or
/// `None` when the path cannot be read.
///
/// The CFB1 bin payload is the **same** `ComputedMetrics` value as the yaml
/// sibling (`comments.FormatReportBinary` and `FormatReportYAML` both call
/// `ComputeAllMetrics(report)`), so it reuses [`comments_metrics`] verbatim and
/// only swaps the encoder: cf-reportutil's CFB1 envelope
/// (`reportutil.EncodeBinaryEnvelope`: magic `CFB1` + little-endian u32 payload
/// length + compact `encoding/json` payload) instead of cf-goyaml.
#[must_use]
pub fn comments_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let metrics = comments_metrics(root_path)?;
    Some(
        cf_reportutil::encode_binary_envelope(&metrics)
            .expect("comments metrics never exceed the CFB1 length cap"),
    )
}

/// Runs the static pipeline over `root_path` and returns the assembled
/// `ComputedMetrics` go-value (the shared report value behind every machine
/// encoding). Returns `None` when the path does not exist.
fn comments_metrics(root_path: &str) -> Option<GoValue> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let opts = PathPolicyOptions::default();
    let analyzer = cf_comments::Analyzer::new();

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_files(root, &parser, &opts, &mut files);
    // filepath.WalkDir visits entries in lexical (byte-sorted) path order.
    files.sort();

    let mut agg = Aggregated::default();

    for path in &files {
        let Ok(content) = fs::read(path) else {
            continue;
        };
        let path_str = path.to_string_lossy();
        let Ok(node) = parser.parse(&path_str, &content) else {
            continue;
        };
        let Ok(report) = analyzer.analyze(Some(&node)) else {
            continue;
        };

        // Stamp metadata exactly as StampSourceFile / StampLanguage.
        let stamped = make_relative_path(&path_str, root_path);
        let directory = parent_dir(&stamped);
        let language = parser.get_language(&path_str);

        aggregate_report(&mut agg, &report, &stamped, &directory, &language);
    }

    Some(compute_metrics(&agg))
}

/// Recursively collects UAST-supported, non-excluded regular files under `dir`.
fn collect_files(
    dir: &Path,
    parser: &Parser,
    opts: &PathPolicyOptions,
    out: &mut Vec<std::path::PathBuf>,
) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if entry.file_name() == ".git" {
                continue; // filepath.SkipDir on .git
            }
            collect_files(&path, parser, opts, out);
            continue;
        }
        let path_str = path.to_string_lossy();
        // ShouldSkipFolderNode: skip files the parser does not support.
        if !parser.is_supported(&path_str) {
            continue;
        }
        // matchesLanguageGlobs: --languages empty → all match (no-op).
        // pathpolicy.Exclude(path, nil, opts).
        if exclude(&path_str, None, opts) {
            continue;
        }
        out.push(path);
    }
}

/// Folds one per-file report into the cross-file accumulator.
fn aggregate_report(
    agg: &mut Aggregated,
    report: &GoValue,
    source_file: &str,
    directory: &str,
    language: &str,
) {
    agg.report_count += 1;

    // DetailedDataCollector: append the file's comment/function items (stamped).
    if let Some(items) = map_get(report, "comments").and_then(as_array) {
        for item in items {
            let line = map_get(item, "line").and_then(as_int).unwrap_or(0);
            agg.comments.push(CommentItem {
                line,
                source_file: source_file.to_string(),
                language: language.to_string(),
                directory: directory.to_string(),
            });
        }
    }
    if let Some(items) = map_get(report, "functions").and_then(as_array) {
        for _ in items {
            agg.functions.push(FunctionItem {
                source_file: source_file.to_string(),
                language: language.to_string(),
                directory: directory.to_string(),
            });
        }
    }

    // MetricsProcessor: sum count keys, sum numeric keys (mean computed later).
    agg.total_comments += scalar_int(report, "total_comments");
    agg.good_comments += scalar_int(report, "good_comments");
    agg.bad_comments += scalar_int(report, "bad_comments");
    agg.total_functions += scalar_int(report, "total_functions");
    agg.documented_functions += scalar_int(report, "documented_functions");

    agg.sum_overall_score += scalar_float(report, "overall_score");
    agg.sum_good_comments_ratio += scalar_float(report, "good_comments_ratio");
    agg.sum_documentation_coverage += scalar_float(report, "documentation_coverage");
}

/// Builds the `comments.ComputedMetrics` GoValue (struct-origin: declaration
/// key order) from the aggregated report, applying the `ComputeAllMetrics` /
/// metric `Compute` transforms and sorts.
fn compute_metrics(agg: &Aggregated) -> GoValue {
    let report_count = agg.report_count.max(0);
    let mean = |sum: f64| {
        if report_count > 0 {
            sum / report_count as f64
        } else {
            0.0
        }
    };
    let overall_score = mean(agg.sum_overall_score);
    let good_comments_ratio = mean(agg.sum_good_comments_ratio);
    let documentation_coverage = mean(agg.sum_documentation_coverage);

    // --- comment_quality: sorted by line_number ascending via Go sort.Slice ---
    // CommentQualityMetric.Compute calls sort.Slice (unstable pdqsort); the
    // equal-line ties must follow pdqsort's permutation, not a stable order.
    let mut comment_order: Vec<&CommentItem> = agg.comments.iter().collect();
    crate::go_sort::slice(&mut comment_order, |a, b| a.line < b.line);
    let comment_quality: Vec<GoValue> = comment_order
        .iter()
        .map(|&c| {
            // CommentQualityData declaration order; quality/type/target_name
            // empty + score 0 (not present on the convert map) — recommendation
            // omitempty (empty → omitted).
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("line_number", GoValue::Int(c.line));
            m.push("source_file", GoValue::Str(c.source_file.clone()));
            m.push("language", GoValue::Str(c.language.clone()));
            m.push("directory", GoValue::Str(c.directory.clone()));
            m.push("quality", GoValue::Str(String::new()));
            m.push("type", GoValue::Str(String::new()));
            m.push("target_name", GoValue::Str(String::new()));
            m.push("score", GoValue::Int(0));
            GoValue::Map(m)
        })
        .collect();

    // --- function_documentation: sorted by documentation_score asc (sort.Slice) ---
    // documentation_score is 0 for every function (the convert map carries no
    // `comment_score`), so every key is equal; the emitted order is pdqsort's
    // permutation of the equal-key input, reproduced via go_sort.
    let mut doc_order: Vec<&FunctionItem> = agg.functions.iter().collect();
    crate::go_sort::slice(&mut doc_order, |_a, _b| false);
    let function_documentation: Vec<GoValue> = doc_order
        .iter()
        .map(|&f| {
            // !HasComment → status "Undocumented", is_documented false, score 0.
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(String::new()));
            m.push("source_file", GoValue::Str(f.source_file.clone()));
            m.push("language", GoValue::Str(f.language.clone()));
            m.push("directory", GoValue::Str(f.directory.clone()));
            m.push("is_documented", GoValue::Bool(false));
            m.push("documentation_score", GoValue::Int(0));
            m.push("status", GoValue::Str("Undocumented".to_string()));
            GoValue::Map(m)
        })
        .collect();

    // --- undocumented_functions: every function (none documented), risk MEDIUM ---
    // needs_comment false for all → all MEDIUM → all equal RiskPriority; the
    // emitted order is pdqsort's permutation of the equal-key input (sort.Slice).
    let mut risk_order: Vec<&FunctionItem> = agg.functions.iter().collect();
    crate::go_sort::slice(&mut risk_order, |_a, _b| false);
    let undocumented_functions: Vec<GoValue> = risk_order
        .iter()
        .map(|&f| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(String::new()));
            m.push("source_file", GoValue::Str(f.source_file.clone()));
            m.push("language", GoValue::Str(f.language.clone()));
            m.push("directory", GoValue::Str(f.directory.clone()));
            m.push("needs_comment", GoValue::Bool(false));
            m.push("risk_level", GoValue::Str("MEDIUM".to_string()));
            GoValue::Map(m)
        })
        .collect();

    // --- aggregate ---
    let mut aggregate = GoMap::new(MapOrigin::Struct);
    aggregate.push("total_comments", GoValue::Int(agg.total_comments));
    aggregate.push("good_comments", GoValue::Int(agg.good_comments));
    aggregate.push("bad_comments", GoValue::Int(agg.bad_comments));
    aggregate.push("overall_score", GoValue::Float(overall_score));
    aggregate.push("total_functions", GoValue::Int(agg.total_functions));
    aggregate.push("documented_functions", GoValue::Int(agg.documented_functions));
    aggregate.push("good_comments_ratio", GoValue::Float(good_comments_ratio));
    aggregate.push("documentation_coverage", GoValue::Float(documentation_coverage));
    aggregate.push("health_score", GoValue::Float(overall_score * 100.0));
    // buildMessage over the overall_score average (the value captured in the
    // golden); ThresholdLabeler: ≥0.8 Excellent, ≥0.6 Good, ≥0.4 Fair, else Poor.
    aggregate.push("message", GoValue::Str(comment_message(overall_score)));

    // ComputedMetrics struct order: comment_quality, function_documentation,
    // undocumented_functions, aggregate.
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("comment_quality", GoValue::Array(comment_quality));
    root.push("function_documentation", GoValue::Array(function_documentation));
    root.push("undocumented_functions", GoValue::Array(undocumented_functions));
    root.push("aggregate", GoValue::Map(aggregate));
    GoValue::Map(root)
}

/// `common.ThresholdLabeler` for comments (aggregator.go thresholds).
fn comment_message(score: f64) -> String {
    if score >= 0.8 {
        "Excellent comment quality and placement".to_string()
    } else if score >= 0.6 {
        "Good comment quality with room for improvement".to_string()
    } else if score >= 0.4 {
        "Fair comment quality - consider improving placement".to_string()
    } else {
        "Poor comment quality - significant improvement needed".to_string()
    }
}

// --- report-map accessors over cf-gojson GoValue ----------------------------

fn map_get<'a>(v: &'a GoValue, key: &str) -> Option<&'a GoValue> {
    match v {
        GoValue::Map(m) => m.get(key),
        _ => None,
    }
}

fn as_array(v: &GoValue) -> Option<&Vec<GoValue>> {
    match v {
        GoValue::Array(a) => Some(a),
        _ => None,
    }
}

fn as_int(v: &GoValue) -> Option<i64> {
    match v {
        GoValue::Int(n) => Some(*n),
        _ => None,
    }
}

/// `ParseReportData` reads `total_comments` etc. as Go `int`; the analyzer emits
/// them as `GoValue::Int`.
fn scalar_int(report: &GoValue, key: &str) -> i64 {
    map_get(report, key).and_then(as_int).unwrap_or(0)
}

/// Reads a numeric metric key as a float; absent (e.g. the empty result lacks
/// `good_comments_ratio`/`documentation_coverage`) contributes 0 to the sum but
/// the report still counts toward the mean denominator.
fn scalar_float(report: &GoValue, key: &str) -> f64 {
    match map_get(report, key) {
        Some(GoValue::Float(f)) => *f,
        Some(GoValue::Int(n)) => *n as f64,
        _ => 0.0,
    }
}

// --- path helpers (filepath.Rel / filepath.Dir parity for this flat case) ----

/// `analyze.MakeRelativePath`: the path made relative to `root` (slash-joined).
/// For files directly under `root` this is the bare basename.
fn make_relative_path(path: &str, root: &str) -> String {
    let root_trimmed = root.trim_end_matches('/');
    if let Some(rest) = path.strip_prefix(root_trimmed) {
        let rest = rest.trim_start_matches('/');
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    path.to_string()
}

/// `filepath.Dir(stamped)`: the directory portion, or "." when none.
fn parent_dir(stamped: &str) -> String {
    match stamped.rfind('/') {
        Some(0) => "/".to_string(),
        Some(idx) => stamped[..idx].to_string(),
        None => ".".to_string(),
    }
}