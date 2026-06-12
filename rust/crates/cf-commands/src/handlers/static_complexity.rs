//! Static-analysis JSON report path for the UAST `static/complexity` analyzer.
//!
//! Reproduces the reference static pipeline for the single-analyzer
//! `codefang run --analyzers static/complexity --format json` capture:
//!
//!  1. the reference `StaticService.uastPhase`
//!     walks `rootPath` with `filepath.WalkDir` in lexical order — directories
//!     are recursed (except `.git`), and every regular file that (a) is
//!     UAST-supported (`parser.IsSupported`), (b) matches the `--languages`
//!     globs (none here → all), and (c) is **not** excluded by
//!     `pathpolicy.Exclude(path, nil, opts)` is streamed for analysis.
//!  2. Each surviving file is parsed into a UAST and the complexity analyzer
//!     computes per-function metrics (`(*Analyzer).Analyze` →
//!     `calculateAllFunctionMetrics`): the per-file `functions` table is sorted
//!     by cyclomatic desc, cognitive desc, name asc (`sort.Slice`, pdqsort).
//!     Each item is stamped with `_source_file` (path made relative to the
//!     analyzed root, here the file basename).
//!  3. The complexity `Aggregator`
//!     sums the count/numeric totals across files, tracks the true max
//!     complexity, concatenates the per-file `functions` tables in file-walk
//!     order (`DetailedDataCollector`, append, no dedup), and computes the
//!     derived `average_complexity` / `max_complexity` / `message`.
//!  4. The aggregated report becomes one `complexity.ReportSection`
//!     (`renderer.SectionsToJSON`): the section status is the aggregator
//!     message, the 6 key metrics, the 4-bucket distribution, and the issues
//!     list = **all** functions sorted by cyclomatic desc, cognitive desc,
//!     nesting desc, name asc (`mapx.SortAndLimit`, `sort.Slice`/pdqsort), each
//!     rendered `name`/`location`/`value`/`severity`.
//!  5. Serialized via `json.NewEncoder(w).SetIndent("", "  ").Encode(report)` —
//!     two-space indent, one trailing newline — routed through cf-gojson.
//!
//! The two `sort.Slice` calls are the reference implementation's **unstable** pdqsort; we reproduce its
//! exact element movement with [`go_pdqsort`] so the tie order (functions equal
//! on every sort key) matches the reference output byte-for-byte.

use std::fs;
use std::path::Path;

use cf_complexity::node::{Node as CxNode, Positions as CxPositions};
use cf_complexity::{Analyzer, FunctionMetrics};
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};
use cf_uast::Parser;
use cf_uast_node::Node as UastNode;

// --- Section rendering constants ---

const SECTION_TITLE: &str = "COMPLEXITY";

const SCORE_EXCELLENT_THRESHOLD: f64 = 1.0;
const SCORE_GOOD_THRESHOLD: f64 = 3.0;
const SCORE_FAIR_THRESHOLD: f64 = 5.0;
const SCORE_MODERATE_THRESHOLD: f64 = 7.0;
const SCORE_POOR_THRESHOLD: f64 = 10.0;

const SCORE_EXCELLENT: f64 = 1.0;
const SCORE_GOOD: f64 = 0.8;
const SCORE_FAIR: f64 = 0.6;
const SCORE_MODERATE: f64 = 0.4;
const SCORE_POOR: f64 = 0.2;
const SCORE_CRITICAL: f64 = 0.1;

const DIST_SIMPLE_MAX: i64 = 5;
const DIST_MODERATE_MAX: i64 = 10;
const DIST_COMPLEX_MAX: i64 = 20;
const DIST_LABEL_SIMPLE: &str = "Simple (1-5)";
const DIST_LABEL_MOD: &str = "Moderate (6-10)";
const DIST_LABEL_COMPLEX: &str = "Complex (11-20)";
const DIST_LABEL_VERYC: &str = "Very Complex (>20)";

const ISSUE_SEVERITY_FAIR_MIN: i64 = 6;
const ISSUE_SEVERITY_POOR_MIN: i64 = 11;

const SEVERITY_GOOD: &str = "good";
const SEVERITY_FAIR: &str = "fair";
const SEVERITY_POOR: &str = "poor";

const METRIC_TOTAL_FUNCTIONS: &str = "Total Functions";
const METRIC_AVG_COMPLEXITY: &str = "Avg Complexity";
const METRIC_MAX_COMPLEXITY: &str = "Max Complexity";
const METRIC_TOTAL_COMPLEXITY: &str = "Total Complexity";
const METRIC_COGNITIVE_TOTAL: &str = "Cognitive Total";
const METRIC_DECISION_POINTS: &str = "Decision Points";

const DEFAULT_STATUS_MESSAGE: &str = "No complexity data available";

// Aggregator messages (reference buildComplexityMessage).
const MSG_EXCELLENT: &str = "Excellent complexity - functions are simple and maintainable";
const MSG_GOOD: &str = "Good complexity - functions have reasonable complexity";
const MSG_FAIR: &str = "Fair complexity - some functions could be simplified";
const MSG_HIGH: &str = "High complexity - functions are complex and should be refactored";

/// One aggregated function record (a per-file metric stamped with its source
/// file), carrying the numeric sort keys plus the rendered location and the
/// per-file stamps the raw aggregated report serializes (`_language`; the
/// `_directory` stamp derives from `location`).
pub(crate) struct FnRecord {
    pub(crate) name: String,
    pub(crate) cyclomatic: i64,
    pub(crate) cognitive: i64,
    pub(crate) nesting: i64,
    pub(crate) lines_of_code: i64,
    pub(crate) location: String,
    pub(crate) language: String,
}

/// Builds the `static/complexity --format json` report bytes for `root_path`,
/// or `None` when the path cannot be read (the reference implementation would surface a walk error; the
/// caller then falls through to the blocked-dependency sentinel).
#[must_use]
pub fn complexity_report(root_path: &str) -> Option<Vec<u8>> {
    complexity_report_flags(root_path, &Options::default(), false)
}

/// `static/complexity --format json` with the run-level static flags applied:
/// `opts` carries `--include-vendored` / `--include-generated` (the shared
/// path-policy options the reference implementation builds in the reference `pathPolicyFromFlags`), and
/// `per_file` enables the reference implementation's `--per-file` section enrichment
/// (`StaticService.enrichWithPerFileData` → `JSONReport.EnrichWithPerFileData`:
/// one `JSONFileEntry` per ANALYZED file — function-free files included — keyed
/// into the section's `files` array).
#[must_use]
pub fn complexity_report_flags(root_path: &str, opts: &Options, per_file: bool) -> Option<Vec<u8>> {
    let report = complexity_report_value_opts(root_path, false, opts, per_file)?;
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&report);
    Some(bytes)
}

/// Builds the `static/complexity` `renderer.JSONReport` GoValue (single scored
/// section), shared by the single-analyzer byte path and the multi-analyzer
/// static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn complexity_report_value(root_path: &str) -> Option<GoValue> {
    complexity_report_value_opts(root_path, false, &Options::default(), false)
}

/// Builds the `static/complexity` section tree in the reference implementation's `AggregationModeSummaryOnly`
/// shape used for the `text` / `compact` formats: the per-item `functions`
/// detailed collection is a no-op, so the distribution and top-issues sections
/// (both derived from `functions`) are absent while the scalar Key Metrics
/// (computed by the always-on `MetricsProcessor`) are unchanged. mirrors the reference implementation
/// `ResolveAggregationMode(FormatText|FormatCompact) -> SummaryOnly`.
#[must_use]
pub fn complexity_report_value_summary(root_path: &str) -> Option<GoValue> {
    complexity_report_value_opts(root_path, true, &Options::default(), false)
}

fn complexity_report_value_opts(
    root_path: &str,
    summary_only: bool,
    opts: &Options,
    per_file: bool,
) -> Option<GoValue> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let analyzer = Analyzer;

    // Aggregated totals (aggregator count/numeric keys).
    let mut total_functions: i64 = 0;
    let mut total_complexity: i64 = 0; // sum of cyclomatic
    let mut cognitive_total: i64 = 0;
    let mut nesting_total: i64 = 0;
    let mut decision_points: i64 = 0;
    let mut max_complexity: i64 = 0;
    let mut report_count: usize = 0;

    // Concatenated per-file function tables (file-walk order).
    let mut records: Vec<FnRecord> = Vec::new();
    // Per-analyzed-file boundaries into `records` (reference per-file aggregator
    // snapshots; one entry per parsed file, function-free included).
    let mut files: Vec<FileBoundary> = Vec::new();

    walk(
        root,
        root_path,
        &parser,
        opts,
        &analyzer,
        &mut total_functions,
        &mut total_complexity,
        &mut cognitive_total,
        &mut nesting_total,
        &mut decision_points,
        &mut max_complexity,
        &mut report_count,
        &mut records,
        &mut files,
    );

    // average_complexity = total_complexity / total_functions (aggregator).
    let avg_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };

    // cognitive_complexity / nesting_depth are NUMERIC (averaged) keys in the
    // base aggregator: the per-file sums are accumulated, then divided by the
    // number of aggregated reports (reportCount = every parsed file, including
    // empty-result files) in CalculateAverages. The section's "Cognitive Total"
    // metric reads this averaged value back through GetInt → safeconv.ToInt,
    // which truncates the float toward zero.
    let cognitive_metric: i64 = if report_count > 0 {
        (cognitive_total as f64 / report_count as f64) as i64
    } else {
        0
    };
    let _ = nesting_total; // numeric key, averaged but not surfaced in the section.

    let message = if report_count > 0 {
        build_complexity_message(avg_complexity)
    } else {
        // Empty result: aggregator never adds a message; the section falls back
        // to DefaultStatusMessage. (Not reachable for the sets fixture.)
        DEFAULT_STATUS_MESSAGE.to_string()
    };

    // SummaryOnly: the detailed `functions` collection is a no-op, so the
    // section's distribution + issues (both read `functions`) are empty while the
    // scalar metrics above are untouched.
    let records_ref: &[FnRecord] = if summary_only { &[] } else { &records };

    // --per-file: one JSONFileEntry per analyzed file, in walk order (the reference implementation ranges
    // a map here — run-to-run random; the oracle's measured-variance
    // canonicalization compares the set).
    let file_entries: Option<Vec<GoValue>> = if per_file {
        Some(files.iter().map(|f| build_file_entry(f, &records)).collect())
    } else {
        None
    };

    let report = build_json_report(
        total_functions,
        avg_complexity,
        max_complexity,
        total_complexity,
        cognitive_metric,
        decision_points,
        &message,
        records_ref,
        file_entries,
    );

    Some(report)
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/complexity`
/// — the value the reference implementation's complexity `Aggregator.GetResult()` returns after the
/// folder walk, which is what `--format plot` consumes
/// (`PlotSectionsFor("static/complexity")` over the raw report) and what
/// `writeReportJSON` serializes into `report.json`.
///
/// Shape (a reference-style `map[string]any`, keys byte-sorted at encode time):
///
/// * `analyzer_name`, `message` (derived via `buildComplexityMessage(avg)`),
/// * counts: `total_functions`, `total_complexity`, `decision_points`
///   (summed ints — `MetricsProcessor.GetCounts`),
/// * averages: `cognitive_complexity`, `nesting_depth` (per-file sums divided
///   by the parsed-file count — `MetricsProcessor.CalculateAverages`),
/// * derived: `average_complexity` (float), `max_complexity` (int),
/// * `functions`: the per-file tables concatenated in walk order
///   (`DetailedDataCollector`), each item the reference `convertFunctionReportItems`
///   map + the `_source_file`/`_language`/`_directory` stamps
///   (`StampSourceFile`/`StampLanguage` + `stampCollectionMetadata`).
///
/// With no parsed files the reference base aggregator returns
/// `buildEmptyComplexityResult` instead (5 keys, no `analyzer_name`).
#[must_use]
pub fn complexity_raw_report_value(root_path: &str, opts: &Options) -> Option<GoValue> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let analyzer = Analyzer;

    let mut total_functions: i64 = 0;
    let mut total_complexity: i64 = 0;
    let mut cognitive_total: i64 = 0;
    let mut nesting_total: i64 = 0;
    let mut decision_points: i64 = 0;
    let mut max_complexity: i64 = 0;
    let mut report_count: usize = 0;
    let mut records: Vec<FnRecord> = Vec::new();
    let mut files: Vec<FileBoundary> = Vec::new();

    walk(
        root,
        root_path,
        &parser,
        opts,
        &analyzer,
        &mut total_functions,
        &mut total_complexity,
        &mut cognitive_total,
        &mut nesting_total,
        &mut decision_points,
        &mut max_complexity,
        &mut report_count,
        &mut records,
        &mut files,
    );

    if report_count == 0 {
        // The reference base `Aggregator.GetResult` with reportCount==0 returns the
        // analyzer's emptyResultBuilder (buildEmptyComplexityResult).
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("total_functions", GoValue::Int(0));
        m.push("average_complexity", GoValue::Float(0.0));
        m.push("max_complexity", GoValue::Int(0));
        m.push("total_complexity", GoValue::Int(0));
        m.push("message", GoValue::Str("No functions found".to_string()));
        return Some(GoValue::Map(m));
    }

    let avg_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };

    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str("complexity".to_string()));
    // counts (MetricsProcessor.GetCounts): present whenever any parsed file
    // carried the key — total_functions/total_complexity always do (the empty
    // per-file result sets them to 0); decision_points only when some file had
    // functions (the empty result omits it).
    m.push("total_functions", GoValue::Int(total_functions));
    m.push("total_complexity", GoValue::Int(total_complexity));
    if !records.is_empty() {
        m.push("decision_points", GoValue::Int(decision_points));
        // averages (CalculateAverages): per-file sums / parsed-file count;
        // the keys appear only when some per-file report carried them.
        m.push(
            "cognitive_complexity",
            GoValue::Float(cognitive_total as f64 / report_count as f64),
        );
        m.push(
            "nesting_depth",
            GoValue::Float(nesting_total as f64 / report_count as f64),
        );
    }
    // functions: the detailed collection in walk order; with zero functions
    // the base collector's empty (non-nil) slice survives — `[]`.
    m.push(
        "functions",
        GoValue::Array(records.iter().map(raw_function_item).collect()),
    );
    // derived metrics (addDerivedMetrics).
    m.push("average_complexity", GoValue::Float(avg_complexity));
    m.push("max_complexity", GoValue::Int(max_complexity));
    m.push(
        "message",
        GoValue::Str(build_complexity_message(avg_complexity)),
    );
    Some(GoValue::Map(m))
}

/// One raw `functions` item: the reference `convertFunctionReportItems` map
/// plus the `_source_file`/`_language`/`_directory` stamps.
/// Map-origin, so JSON keys byte-sort at encode time.
fn raw_function_item(r: &FnRecord) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("name", GoValue::Str(r.name.clone()));
    m.push("cyclomatic_complexity", GoValue::Int(r.cyclomatic));
    m.push("cognitive_complexity", GoValue::Int(r.cognitive));
    m.push("nesting_depth", GoValue::Int(r.nesting));
    m.push("lines_of_code", GoValue::Int(r.lines_of_code));
    m.push(
        "complexity_assessment",
        GoValue::Str(cf_complexity::get_complexity_assessment(r.cyclomatic).to_string()),
    );
    m.push(
        "cognitive_assessment",
        GoValue::Str(cf_complexity::get_cognitive_assessment(r.cognitive).to_string()),
    );
    m.push(
        "nesting_assessment",
        GoValue::Str(cf_complexity::get_nesting_assessment(r.nesting).to_string()),
    );
    m.push("_source_file", GoValue::Str(r.location.clone()));
    if !r.language.is_empty() {
        m.push("_language", GoValue::Str(r.language.clone()));
    }
    let dir = go_filepath_dir(&r.location);
    if !dir.is_empty() {
        m.push("_directory", GoValue::Str(dir));
    }
    GoValue::Map(m)
}

/// The reference `filepath.Dir` over the clean relative paths `MakeRelativePath` yields:
/// everything before the final separator, or `"."` when there is none.
fn go_filepath_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => ".".to_string(),
    }
}

/// One analyzed file's slice of the walk products (the Rust shape of the reference implementation's
/// per-file aggregator snapshot).
struct FileBoundary {
    /// Path relative to the analyzed root.
    location: String,
    /// Range into the concatenated `records` (empty for function-free files).
    start: usize,
    end: usize,
    /// Per-file totals (the analyzer's single-file report values).
    total_complexity: i64,
    cognitive_total: i64,
    decision_points: i64,
    max_complexity: i64,
}

/// Builds one `renderer.JSONFileEntry` (the reference `SectionToJSONFileEntry` over the
/// per-file report's section): `file_path`, `score_label`, `status`, `metrics`,
/// `distribution` (omitempty — absent for function-free files), `issues`,
/// `score`.
fn build_file_entry(f: &FileBoundary, records: &[FnRecord]) -> GoValue {
    let frecs = &records[f.start..f.end];
    let n_fns = frecs.len() as i64;
    let avg = if n_fns > 0 { f.total_complexity as f64 / n_fns as f64 } else { 0.0 };
    let score = calculate_score(avg);
    // Per-file status: the analyzer's single-file report message — the empty
    // result carries "No functions found" (reference buildEmptyResult).
    let status = if n_fns > 0 {
        build_complexity_message(avg)
    } else {
        "No functions found".to_string()
    };

    let metrics = GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &n_fns.to_string()),
        metric(METRIC_AVG_COMPLEXITY, &format!("{avg:.1}")),
        metric(METRIC_MAX_COMPLEXITY, &f.max_complexity.to_string()),
        metric(METRIC_TOTAL_COMPLEXITY, &f.total_complexity.to_string()),
        metric(METRIC_COGNITIVE_TOTAL, &f.cognitive_total.to_string()),
        metric(METRIC_DECISION_POINTS, &f.decision_points.to_string()),
    ]);

    // Distribution over THIS file's functions (4 buckets; absent when empty).
    let mut simple = 0i64;
    let mut moderate = 0i64;
    let mut complex = 0i64;
    let mut very_complex = 0i64;
    for r in frecs {
        if r.cyclomatic <= DIST_SIMPLE_MAX {
            simple += 1;
        } else if r.cyclomatic <= DIST_MODERATE_MAX {
            moderate += 1;
        } else if r.cyclomatic <= DIST_COMPLEX_MAX {
            complex += 1;
        } else {
            very_complex += 1;
        }
    }
    let mut dist_items = Vec::new();
    if n_fns != 0 {
        dist_items.push(dist_item(DIST_LABEL_SIMPLE, pct(simple, n_fns), simple));
        dist_items.push(dist_item(DIST_LABEL_MOD, pct(moderate, n_fns), moderate));
        dist_items.push(dist_item(DIST_LABEL_COMPLEX, pct(complex, n_fns), complex));
        dist_items.push(dist_item(DIST_LABEL_VERYC, pct(very_complex, n_fns), very_complex));
    }

    // Issues: this file's functions in the section comparator order.
    let mut order: Vec<usize> = (0..frecs.len()).collect();
    go_pdqsort(&mut order, &|&ia, &ib| {
        let a = &frecs[ia];
        let b = &frecs[ib];
        if a.cyclomatic != b.cyclomatic {
            return a.cyclomatic > b.cyclomatic;
        }
        if a.cognitive != b.cognitive {
            return a.cognitive > b.cognitive;
        }
        if a.nesting != b.nesting {
            return a.nesting > b.nesting;
        }
        a.name < b.name
    });
    let issue_items: Vec<GoValue> = order
        .iter()
        .map(|&i| {
            let r = &frecs[i];
            let mut iss = GoMap::new(MapOrigin::Struct);
            iss.push("name", GoValue::Str(r.name.clone()));
            iss.push("location", GoValue::Str(r.location.clone()));
            iss.push(
                "value",
                GoValue::Str(format!(
                    "CC={} | Cog={} | Nest={}",
                    r.cyclomatic, r.cognitive, r.nesting
                )),
            );
            iss.push("severity", GoValue::Str(severity_for_complexity(r.cyclomatic).to_string()));
            GoValue::Map(iss)
        })
        .collect();

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(f.location.clone()));
    entry.push("score_label", GoValue::Str(score_label(score)));
    entry.push("status", GoValue::Str(status));
    entry.push("metrics", metrics);
    if !dist_items.is_empty() {
        entry.push("distribution", GoValue::Array(dist_items));
    }
    entry.push("issues", GoValue::Array(issue_items));
    entry.push("score", GoValue::Float(score));
    GoValue::Map(entry)
}

/// Recursively walks `dir` in lexical order, mirroring `filepath.WalkDir`:
/// directories are recursed (except `.git`), files are filtered through
/// parser support + path policy, parsed, and analyzed.
#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root_path: &str,
    parser: &Parser,
    opts: &Options,
    analyzer: &Analyzer,
    total_functions: &mut i64,
    total_complexity: &mut i64,
    cognitive_total: &mut i64,
    nesting_total: &mut i64,
    decision_points: &mut i64,
    max_complexity: &mut i64,
    report_count: &mut usize,
    records: &mut Vec<FnRecord>,
    files: &mut Vec<FileBoundary>,
) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk(
                &path,
                root_path,
                parser,
                opts,
                analyzer,
                total_functions,
                total_complexity,
                cognitive_total,
                nesting_total,
                decision_points,
                max_complexity,
                report_count,
                records,
                files,
            );
            continue;
        }

        let path_str = path.to_string_lossy();

        // ShouldSkipFolderNode: must be UAST-supported.
        if !parser.is_supported(&path_str) {
            continue;
        }
        // matchesLanguageGlobs (empty → all match), then pathpolicy.Exclude.
        if exclude(&path_str, None, opts) {
            continue;
        }

        let Ok(content) = fs::read(&path) else {
            continue;
        };
        let Ok(uast_root) = parser.parse(&path_str, &content) else {
            continue;
        };

        // Every successfully parsed & analyzed file produces a report, even one
        // with no functions (`buildEmptyResult`). The base aggregator counts
        // ALL such reports (`reportCount`), which is the divisor for the
        // averaged numeric keys (`cognitive_complexity`, `nesting_depth`).
        *report_count += 1;

        let cx_root = convert_node(&uast_root);
        // Per-file metrics (the crate sorts by cc desc, cog desc, name asc;
        // the reference implementation's per-file sort.Slice over the same comparator yields the same
        // order for the sets fixture — no within-file all-key ties).
        let fns = analyzer.function_metrics(Some(&cx_root));

        // _source_file stamp = path relative to the analyzed root.
        let location = make_relative_path(&path_str, root_path);
        // _language stamp (reference: StampLanguage(parser.GetLanguage(filePath))).
        let language = parser.get_language(&path_str);

        if fns.is_empty() {
            // Empty-result report: no functions, no cognitive/nesting/count
            // contribution; only reportCount (above) and the per-file snapshot
            // (an empty "No functions found" entry) are affected.
            files.push(FileBoundary {
                location,
                start: records.len(),
                end: records.len(),
                total_complexity: 0,
                cognitive_total: 0,
                decision_points: 0,
                max_complexity: 0,
            });
            continue;
        }

        // Aggregate the per-file totals (matches the analyzer's own result
        // computed in buildResult: totals over all functions).
        let start = records.len();
        let mut file_max: i64 = 0;
        let mut file_cx: i64 = 0;
        let mut file_cog: i64 = 0;
        let mut file_dec: i64 = 0;
        for m in &fns {
            *total_functions += 1;
            *total_complexity += m.cyclomatic_complexity;
            *cognitive_total += m.cognitive_complexity;
            *nesting_total += m.nesting_depth;
            *decision_points += m.decision_points;
            file_cx += m.cyclomatic_complexity;
            file_cog += m.cognitive_complexity;
            file_dec += m.decision_points;
            if m.cyclomatic_complexity > file_max {
                file_max = m.cyclomatic_complexity;
            }
            records.push(record_for(m, &location, &language));
        }
        if file_max > *max_complexity {
            *max_complexity = file_max;
        }
        files.push(FileBoundary {
            location,
            start,
            end: records.len(),
            total_complexity: file_cx,
            cognitive_total: file_cog,
            decision_points: file_dec,
            max_complexity: file_max,
        });
    }
}

fn record_for(m: &FunctionMetrics, location: &str, language: &str) -> FnRecord {
    FnRecord {
        name: m.name.clone(),
        cyclomatic: m.cyclomatic_complexity,
        cognitive: m.cognitive_complexity,
        nesting: m.nesting_depth,
        lines_of_code: m.lines_of_code,
        location: location.to_string(),
        language: language.to_string(),
    }
}

/// The reference `MakeRelativePath`: `filepath.Rel(rootPath, filePath)`.
/// For the flat sets fixture this yields the basename.
fn make_relative_path(file_path: &str, root_path: &str) -> String {
    if root_path.is_empty() {
        return file_path.to_string();
    }
    let root = Path::new(root_path);
    let file = Path::new(file_path);
    match file.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => file_path.to_string(),
    }
}

/// Converts a `cf_uast_node::Node` into the `cf_complexity::node::Node` subset
/// the complexity analyzer reads (type, token, roles, props, children, pos).
fn convert_node(n: &UastNode) -> CxNode {
    let mut out = CxNode::new(n.node_type.clone());
    out.token = n.token.clone();
    out.roles = n.roles.clone();
    out.props = n.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    out.pos = n.pos.as_ref().map(|p| CxPositions {
        start_line: p.start_line as u32,
        start_col: p.start_col as u32,
        start_offset: p.start_offset as u32,
        end_line: p.end_line as u32,
        end_col: p.end_col as u32,
        end_offset: p.end_offset as u32,
    });
    out.children = n.children.iter().map(convert_node).collect();
    out
}

// --- Score / message / severity / distribution helpers ---

fn calculate_score(avg: f64) -> f64 {
    if avg <= SCORE_EXCELLENT_THRESHOLD {
        SCORE_EXCELLENT
    } else if avg <= SCORE_GOOD_THRESHOLD {
        SCORE_GOOD
    } else if avg <= SCORE_FAIR_THRESHOLD {
        SCORE_FAIR
    } else if avg <= SCORE_MODERATE_THRESHOLD {
        SCORE_MODERATE
    } else if avg <= SCORE_POOR_THRESHOLD {
        SCORE_POOR
    } else {
        SCORE_CRITICAL
    }
}

fn build_complexity_message(score: f64) -> String {
    if score <= 1.0 {
        MSG_EXCELLENT
    } else if score <= SCORE_GOOD_THRESHOLD {
        MSG_GOOD
    } else if score <= SCORE_MODERATE_THRESHOLD {
        MSG_FAIR
    } else {
        MSG_HIGH
    }
    .to_string()
}

fn severity_for_complexity(cc: i64) -> &'static str {
    if cc >= ISSUE_SEVERITY_POOR_MIN {
        SEVERITY_POOR
    } else if cc >= ISSUE_SEVERITY_FAIR_MIN {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

fn pct(count: i64, total: i64) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

/// Builds the `renderer.JSONReport` GoValue for a single complexity section.
#[allow(clippy::too_many_arguments)]
fn build_json_report(
    total_functions: i64,
    avg_complexity: f64,
    max_complexity: i64,
    total_complexity: i64,
    cognitive_metric: i64,
    decision_points: i64,
    message: &str,
    records: &[FnRecord],
    file_entries: Option<Vec<GoValue>>,
) -> GoValue {
    let score = calculate_score(avg_complexity);

    // ---- metrics (reference KeyMetrics) ----
    let metrics = GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &total_functions.to_string()),
        metric(METRIC_AVG_COMPLEXITY, &format!("{avg_complexity:.1}")),
        metric(METRIC_MAX_COMPLEXITY, &max_complexity.to_string()),
        metric(METRIC_TOTAL_COMPLEXITY, &total_complexity.to_string()),
        metric(METRIC_COGNITIVE_TOTAL, &cognitive_metric.to_string()),
        metric(METRIC_DECISION_POINTS, &decision_points.to_string()),
    ]);

    // ---- distribution (categorize over all functions) ----
    let mut simple = 0i64;
    let mut moderate = 0i64;
    let mut complex = 0i64;
    let mut very_complex = 0i64;
    for r in records {
        if r.cyclomatic <= DIST_SIMPLE_MAX {
            simple += 1;
        } else if r.cyclomatic <= DIST_MODERATE_MAX {
            moderate += 1;
        } else if r.cyclomatic <= DIST_COMPLEX_MAX {
            complex += 1;
        } else {
            very_complex += 1;
        }
    }
    let total = records.len() as i64;
    let mut dist_items = Vec::new();
    if total != 0 {
        dist_items.push(dist_item(DIST_LABEL_SIMPLE, pct(simple, total), simple));
        dist_items.push(dist_item(DIST_LABEL_MOD, pct(moderate, total), moderate));
        dist_items.push(dist_item(DIST_LABEL_COMPLEX, pct(complex, total), complex));
        dist_items.push(dist_item(DIST_LABEL_VERYC, pct(very_complex, total), very_complex));
    }

    // ---- issues: ALL functions sorted by cc desc, cog desc, nest desc, name asc ----
    let mut order: Vec<usize> = (0..records.len()).collect();
    go_pdqsort(&mut order, &|&ia, &ib| {
        let a = &records[ia];
        let b = &records[ib];
        if a.cyclomatic != b.cyclomatic {
            return a.cyclomatic > b.cyclomatic;
        }
        if a.cognitive != b.cognitive {
            return a.cognitive > b.cognitive;
        }
        if a.nesting != b.nesting {
            return a.nesting > b.nesting;
        }
        a.name < b.name
    });

    let issue_items: Vec<GoValue> = order
        .iter()
        .map(|&i| {
            let r = &records[i];
            let mut iss = GoMap::new(MapOrigin::Struct);
            iss.push("name", GoValue::Str(r.name.clone()));
            iss.push("location", GoValue::Str(r.location.clone()));
            iss.push(
                "value",
                GoValue::Str(format!(
                    "CC={} | Cog={} | Nest={}",
                    r.cyclomatic, r.cognitive, r.nesting
                )),
            );
            iss.push("severity", GoValue::Str(severity_for_complexity(r.cyclomatic).to_string()));
            GoValue::Map(iss)
        })
        .collect();

    // ---- section (renderer.SectionToJSON) ----
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(score_label(score)));
    section.push("status", GoValue::Str(message.to_string()));
    section.push("metrics", metrics);
    if !dist_items.is_empty() {
        section.push("distribution", GoValue::Array(dist_items));
    }
    section.push("issues", GoValue::Array(issue_items));
    // --per-file enrichment: `files` sits between `issues` and `score`
    // (renderer.JSONSection field order, `files` is `omitempty` without the
    // flag and an explicit — possibly empty — array with it).
    if let Some(entries) = file_entries {
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(score));

    // ---- report (renderer.SectionsToJSON over one scored section) ----
    let mut report = GoMap::new(MapOrigin::Struct);
    report.push("overall_score_label", GoValue::Str(score_label(score)));
    report.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    report.push("overall_score", GoValue::Float(score));

    GoValue::Map(report)
}

/// Renders a numeric score as the `N/10` label (renderer score formatting).
fn score_label(score: f64) -> String {
    // BaseReportSection.ScoreLabel: fmt.Sprintf("%d/10", int(score*10)) for a
    // score in [0,1]; -1 (info) renders "Info" but complexity is never info.
    let n = (score * 10.0).round() as i64;
    format!("{n}/10")
}

fn metric(label: &str, value: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("label", GoValue::Str(label.to_string()));
    m.push("value", GoValue::Str(value.to_string()));
    GoValue::Map(m)
}

fn dist_item(label: &str, percent: f64, count: i64) -> GoValue {
    let mut d = GoMap::new(MapOrigin::Struct);
    d.push("label", GoValue::Str(label.to_string()));
    d.push("percent", GoValue::Float(percent));
    d.push("count", GoValue::Int(count));
    GoValue::Map(d)
}

// ===========================================================================
// the reference `sort.Slice` (pdqsort) port — exact element-movement parity.
// Operates on an index permutation; `less(&a, &b)` returns true when a < b.
// Mirrors the reference sort sources (limit = bits.Len(len)).
// ===========================================================================

const MAX_INSERTION: usize = 12;

fn bits_len(n: usize) -> u32 {
    usize::BITS - n.leading_zeros()
}

/// Sorts `data` with the reference `sort.Slice` semantics using `less`.
pub(crate) fn go_pdqsort<T, F: Fn(&T, &T) -> bool>(data: &mut [T], less: &F) {
    let n = data.len();
    let limit = bits_len(n);
    pdqsort(data, 0, n, limit, less);
}

fn pdqsort<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    mut a: usize,
    mut b: usize,
    mut limit: u32,
    less: &F,
) {
    let mut was_balanced = true;
    let mut was_partitioned = true;

    loop {
        let length = b - a;

        if length <= MAX_INSERTION {
            insertion_sort(data, a, b, less);
            return;
        }

        if limit == 0 {
            heap_sort(data, a, b, less);
            return;
        }

        if !was_balanced {
            break_patterns(data, a, b);
            limit -= 1;
        }

        let (mut pivot, mut hint) = choose_pivot(data, a, b, less);
        if hint == HINT_DECREASING {
            reverse_range(data, a, b);
            pivot = (b - 1) - (pivot - a);
            hint = HINT_INCREASING;
        }

        if was_balanced
            && was_partitioned
            && hint == HINT_INCREASING
            && partial_insertion_sort(data, a, b, less)
        {
            return;
        }

        // a > 0 && !less(a-1, pivot)
        if a > 0 && !less(&data[a - 1], &data[pivot]) {
            let mid = partition_equal(data, a, b, pivot, less);
            a = mid;
            continue;
        }

        let (mid, already_partitioned) = partition(data, a, b, pivot, less);
        was_partitioned = already_partitioned;

        let left_len = mid - a;
        let right_len = b - mid;
        let balance_threshold = length / 8;
        if left_len < right_len {
            was_balanced = left_len >= balance_threshold;
            pdqsort(data, a, mid, limit, less);
            a = mid + 1;
        } else {
            was_balanced = right_len >= balance_threshold;
            pdqsort(data, mid + 1, b, limit, less);
            b = mid;
        }
    }
}

fn insertion_sort<T, F: Fn(&T, &T) -> bool>(data: &mut [T], a: usize, b: usize, less: &F) {
    for i in (a + 1)..b {
        let mut j = i;
        while j > a && less(&data[j], &data[j - 1]) {
            data.swap(j, j - 1);
            j -= 1;
        }
    }
}

fn sift_down<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    lo: usize,
    hi: usize,
    first: usize,
    less: &F,
) {
    let mut root = lo;
    loop {
        let mut child = 2 * root + 1;
        if child >= hi {
            break;
        }
        if child + 1 < hi && less(&data[first + child], &data[first + child + 1]) {
            child += 1;
        }
        if !less(&data[first + root], &data[first + child]) {
            return;
        }
        data.swap(first + root, first + child);
        root = child;
    }
}

fn heap_sort<T, F: Fn(&T, &T) -> bool>(data: &mut [T], a: usize, b: usize, less: &F) {
    let first = a;
    let lo = 0;
    let hi = b - a;

    let mut i = (hi as isize - 1) / 2;
    while i >= 0 {
        sift_down(data, i as usize, hi, first, less);
        i -= 1;
    }

    let mut i = hi as isize - 1;
    while i >= 0 {
        data.swap(first, first + i as usize);
        sift_down(data, lo, i as usize, first, less);
        i -= 1;
    }
}

fn partition<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    pivot: usize,
    less: &F,
) -> (usize, bool) {
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    while i <= j && less(&data[i], &data[a]) {
        i += 1;
    }
    while i <= j && !less(&data[j], &data[a]) {
        j -= 1;
    }
    if i > j {
        data.swap(j, a);
        return (j, true);
    }
    data.swap(i, j);
    i += 1;
    j -= 1;

    loop {
        while i <= j && less(&data[i], &data[a]) {
            i += 1;
        }
        while i <= j && !less(&data[j], &data[a]) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    data.swap(j, a);
    (j, false)
}

fn partition_equal<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    pivot: usize,
    less: &F,
) -> usize {
    data.swap(a, pivot);
    let mut i = a + 1;
    let mut j = b - 1;

    loop {
        while i <= j && !less(&data[a], &data[i]) {
            i += 1;
        }
        while i <= j && less(&data[a], &data[j]) {
            j -= 1;
        }
        if i > j {
            break;
        }
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
    i
}

fn partial_insertion_sort<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    less: &F,
) -> bool {
    const MAX_STEPS: usize = 5;
    const SHORTEST_SHIFTING: usize = 50;
    let mut i = a + 1;
    for _ in 0..MAX_STEPS {
        while i < b && !less(&data[i], &data[i - 1]) {
            i += 1;
        }

        if i == b {
            return true;
        }

        if b - a < SHORTEST_SHIFTING {
            return false;
        }

        data.swap(i, i - 1);

        // Shift the smaller one to the left.
        if i - a >= 2 {
            let mut j = i - 1;
            while j >= 1 {
                if !less(&data[j], &data[j - 1]) {
                    break;
                }
                data.swap(j, j - 1);
                j -= 1;
            }
        }
        // Shift the greater one to the right.
        if b - i >= 2 {
            let mut j = i + 1;
            while j < b {
                if !less(&data[j], &data[j - 1]) {
                    break;
                }
                data.swap(j, j - 1);
                j += 1;
            }
        }
    }
    false
}

fn break_patterns<T>(data: &mut [T], a: usize, b: usize) {
    let length = b - a;
    if length >= 8 {
        let mut random: u64 = length as u64;
        let modulus = next_power_of_two(length);

        let base = a + (length / 4) * 2;
        for idx in (base - 1)..=(base + 1) {
            random = xorshift_next(&mut random);
            let mut other = (random as usize) & (modulus - 1);
            if other >= length {
                other -= length;
            }
            data.swap(idx, a + other);
        }
    }
}

fn xorshift_next(r: &mut u64) -> u64 {
    *r ^= *r << 13;
    *r ^= *r >> 7;
    *r ^= *r << 17;
    *r
}

fn next_power_of_two(length: usize) -> usize {
    let shift = bits_len(length);
    1usize << shift
}

const HINT_UNKNOWN: u8 = 0;
const HINT_INCREASING: u8 = 1;
const HINT_DECREASING: u8 = 2;

fn choose_pivot<T, F: Fn(&T, &T) -> bool>(
    data: &mut [T],
    a: usize,
    b: usize,
    less: &F,
) -> (usize, u8) {
    const SHORTEST_NINTHER: usize = 50;
    const MAX_SWAPS: i32 = 4 * 3;

    let l = b - a;

    let mut swaps: i32 = 0;
    let mut i = a + l / 4;
    let mut j = a + l / 4 * 2;
    let mut k = a + l / 4 * 3;

    if l >= 8 {
        if l >= SHORTEST_NINTHER {
            i = median_adjacent(data, i, &mut swaps, less);
            j = median_adjacent(data, j, &mut swaps, less);
            k = median_adjacent(data, k, &mut swaps, less);
        }
        j = median(data, i, j, k, &mut swaps, less);
    }

    match swaps {
        0 => (j, HINT_INCREASING),
        MAX_SWAPS => (j, HINT_DECREASING),
        _ => (j, HINT_UNKNOWN),
    }
}

fn order2<T, F: Fn(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    b: usize,
    swaps: &mut i32,
    less: &F,
) -> (usize, usize) {
    if less(&data[b], &data[a]) {
        *swaps += 1;
        (b, a)
    } else {
        (a, b)
    }
}

fn median<T, F: Fn(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    b: usize,
    c: usize,
    swaps: &mut i32,
    less: &F,
) -> usize {
    let (a, b) = order2(data, a, b, swaps, less);
    let (b, c) = order2(data, b, c, swaps, less);
    let (_a, b) = order2(data, a, b, swaps, less);
    let _ = c;
    b
}

fn median_adjacent<T, F: Fn(&T, &T) -> bool>(
    data: &[T],
    a: usize,
    swaps: &mut i32,
    less: &F,
) -> usize {
    median(data, a - 1, a, a + 1, swaps, less)
}

fn reverse_range<T>(data: &mut [T], a: usize, b: usize) {
    let mut i = a;
    let mut j = b - 1;
    while i < j {
        data.swap(i, j);
        i += 1;
        j -= 1;
    }
}
