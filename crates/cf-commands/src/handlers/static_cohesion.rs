//! Static-analysis report paths for the UAST `static/cohesion` analyzer.
//!
//! Reproduces the reference static pipeline for the single-analyzer
//! `codefang run --analyzers static/cohesion --format {json,yaml,bin}` captures.
//!
//! Pipeline (mirrors `complexity` and the reference `StaticService.uastPhase`):
//!
//!  1. `filepath.WalkDir` in lexical order — directories recursed (except `.git`),
//!     every regular file that is UAST-supported, matches `--languages` (none →
//!     all), and is not `pathpolicy.Exclude`d is parsed.
//!  2. Cohesion is a `VisitorProvider`, so the static factory runs it through its
//!     `cohesion.Visitor` over a preorder DFS (`MultiAnalyzerTraverser`). This is
//!     reproduced by [`cf_cohesion::Analyzer::analyze_visitor`], NOT the
//!     `findFunctions` path. Each parsed file yields one per-file `analyze.Report`
//!     (an empty-result report when the file has no functions).
//!  3. `StampSourceFile` / `StampLanguage` stamp `_source_file` (path relative to
//!     the analyzed root), `_directory` (`filepath.Dir` of it), and `_language`
//!     onto the report and every collection item.
//!  4. The cohesion `Aggregator` (`common.Aggregator`) then:
//!     * SUMS the count key `total_functions`,
//!     * AVERAGES the numeric keys `lcom` / `cohesion_score` / `function_cohesion`
//!       over `reportCount` (= number of parsed files, including empty-result ones),
//!     * collects the `functions` items deduplicated by the composite key
//!       (`_source_file`, `name`) — last write wins — then sorted by `name`
//!       (`GetSortedData` sorts on the last identifier key),
//!     * builds the `message` from the first numeric average (reference map-iteration
//!       order; canonicalized by the harness when it varies).
//!  5. JSON goes through the renderer section (`FormatJSON` → `SectionsToJSON`);
//!     YAML/bin go through `FormatPerAnalyzer` → `cohesion.FormatReportYAML` /
//!     `FormatReportBinary`, which marshal `ComputedMetrics`.
//!
//! The per-function `functions` table order is nondeterministic in the reference implementation (map
//! dedup + unstable `sort.Slice` on `name`), and the issues list is re-sorted by
//! the FormatFloat string of cohesion (again unstable). The compat harness
//! measures this variance and compares those lists as sorted multisets, so only
//! the report VALUE (the set of functions, their cohesion, the scalars) must
//! match — which is what this handler reproduces.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use cf_cohesion::report_value::{Report, ReportValue};
use cf_cohesion::Analyzer;
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};
use cf_uast::Parser;

// --- Section rendering constants ---

const SECTION_TITLE: &str = "COHESION";

const METRIC_TOTAL_FUNCTIONS: &str = "Total Functions";
const METRIC_LCOM: &str = "LCOM Score";
const METRIC_COHESION_SCORE: &str = "Cohesion Score";
const METRIC_FUNCTION_COHESION: &str = "Avg Cohesion";

const DIST_EXCELLENT_MIN: f64 = 0.6;
const DIST_GOOD_MIN: f64 = 0.4;
const DIST_FAIR_MIN: f64 = 0.3;
const DIST_LABEL_EXCELLENT: &str = "Excellent (>0.6)";
const DIST_LABEL_GOOD: &str = "Good (0.4-0.6)";
const DIST_LABEL_FAIR: &str = "Fair (0.3-0.4)";
const DIST_LABEL_POOR: &str = "Poor (<0.3)";

const ISSUE_SEVERITY_FAIR_MAX: f64 = 0.4;
const ISSUE_SEVERITY_POOR_MAX: f64 = 0.3;

const SEVERITY_GOOD: &str = "good";
const SEVERITY_FAIR: &str = "fair";
const SEVERITY_POOR: &str = "poor";

const DEFAULT_STATUS_MESSAGE: &str = "No cohesion data available";

// Aggregator message thresholds (reference cohesionMessageLabeler).
const MSG_SCORE_HIGH: f64 = 0.7;
const MSG_SCORE_MEDIUM: f64 = 0.4;
const MSG_SCORE_LOW: f64 = 0.3;
const MSG_EXCELLENT: &str = "Excellent overall cohesion across all analyzed code";
const MSG_GOOD: &str = "Good overall cohesion with room for improvement";
const MSG_FAIR: &str = "Fair overall cohesion - consider refactoring some functions";
const MSG_POOR: &str = "Poor overall cohesion - significant refactoring recommended";

/// One collected per-function item, carrying the full the reference implementation
/// `convertCohesionFunctionItems` map the raw aggregated
/// report serializes, plus the stamped `_source_file`.
#[derive(Clone)]
struct FnItem {
    name: String,
    source_file: String,
    cohesion: f64,
    line_count: i64,
    variable_count: i64,
    cohesion_assessment: String,
    variable_assessment: String,
    size_assessment: String,
}

/// The cross-file aggregated state.
#[derive(Default)]
struct Aggregated {
    report_count: i64,
    total_functions: i64,
    lcom_sum: f64,
    cohesion_score_sum: f64,
    function_cohesion_sum: f64,
    /// Functions keyed by the composite `(_source_file, name)` dedup key; last
    /// write wins (reference: buffer overwrite).
    functions: BTreeMap<String, FnItem>,
    /// Per-file snapshots (the reference `PerFileRetainer` clones of each
    /// visitor report) consumed by the `--per-file` section enrichment; walk
    /// order.
    per_file: Vec<PerFileCohesion>,
}

/// One retained per-file cohesion report: the visitor result's scalars,
/// per-file `message`, and this file's `(name, cohesion)` function list.
struct PerFileCohesion {
    /// Path relative to the analyzed root (the `StampSourceFile` stamp).
    rel: String,
    total_functions: i64,
    lcom: f64,
    cohesion_score: f64,
    function_cohesion: f64,
    message: String,
    functions: Vec<(String, f64)>,
}

impl Aggregated {
    fn lcom_avg(&self) -> f64 {
        if self.report_count > 0 {
            self.lcom_sum / self.report_count as f64
        } else {
            0.0
        }
    }
    fn cohesion_score_avg(&self) -> f64 {
        if self.report_count > 0 {
            self.cohesion_score_sum / self.report_count as f64
        } else {
            0.0
        }
    }
    fn function_cohesion_avg(&self) -> f64 {
        if self.report_count > 0 {
            self.function_cohesion_sum / self.report_count as f64
        } else {
            0.0
        }
    }
}

/// Walks `root_path`, runs the cohesion visitor per file, and aggregates, or
/// `None` when the path cannot be read.
fn aggregate(root_path: &str) -> Option<Aggregated> {
    aggregate_opts(root_path, &Options::default())
}

/// [`aggregate`] with explicit path-policy options (the plot path passes the
/// run flags; the stdout formats keep the defaults).
fn aggregate_opts(root_path: &str, opts: &Options) -> Option<Aggregated> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }
    let parser = Parser::new();
    let analyzer = Analyzer::new();
    let mut agg = Aggregated::default();
    walk(root, root_path, &parser, opts, &analyzer, &mut agg);
    Some(agg)
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root_path: &str,
    parser: &Parser,
    opts: &Options,
    analyzer: &Analyzer,
    agg: &mut Aggregated,
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
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
                continue;
            }
            walk(&path, root_path, parser, opts, analyzer, agg);
            continue;
        }

        let path_str = path.to_string_lossy();
        if !parser.is_supported(&path_str) {
            continue;
        }
        if exclude(&path_str, None, opts) {
            continue;
        }
        let Ok(content) = fs::read(&path) else {
            continue;
        };
        let Ok(uast_root) = parser.parse(&path_str, &content) else {
            continue;
        };

        // Per-file report via the visitor path (what the static factory uses).
        let report = analyzer.analyze_visitor(&uast_root);

        // Every parsed file contributes one report to the averages divisor.
        agg.report_count += 1;

        let stamped = make_relative_path(&path_str, root_path);
        accumulate(agg, &report, &stamped);
        retain_per_file(agg, &report, &stamped);
    }
}

/// Folds one per-file report into the aggregate (the reference `MetricsProcessor.ProcessReport`
/// + `SpillableDataCollector.CollectFromReport`).
fn accumulate(agg: &mut Aggregated, report: &Report, source_file: &str) {
    if let Some(v) = report.get("total_functions").and_then(ReportValue::as_int) {
        agg.total_functions += v;
    }
    if let Some(v) = report.get("lcom").and_then(ReportValue::as_float) {
        agg.lcom_sum += v;
    }
    if let Some(v) = report.get("cohesion_score").and_then(ReportValue::as_float) {
        agg.cohesion_score_sum += v;
    }
    if let Some(v) = report
        .get("function_cohesion")
        .and_then(ReportValue::as_float)
    {
        agg.function_cohesion_sum += v;
    }

    let Some(functions) = report.get("functions").and_then(ReportValue::as_functions) else {
        return;
    };
    for fn_map in functions {
        let get_str = |key: &str| {
            fn_map
                .get(key)
                .and_then(ReportValue::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let get_int = |key: &str| fn_map.get(key).and_then(ReportValue::as_int).unwrap_or(0);
        let name = get_str("name");
        let cohesion = fn_map
            .get("cohesion")
            .and_then(ReportValue::as_float)
            .unwrap_or(0.0);
        // Composite dedup key (_source_file, name); the per-file report items do
        // not yet carry the stamped source file, so we stamp it here.
        let key = format!("{source_file}\u{0}{name}");
        agg.functions.insert(
            key,
            FnItem {
                name,
                source_file: source_file.to_string(),
                cohesion,
                line_count: get_int("line_count"),
                variable_count: get_int("variable_count"),
                cohesion_assessment: get_str("cohesion_assessment"),
                variable_assessment: get_str("variable_assessment"),
                size_assessment: get_str("size_assessment"),
            },
        );
    }
}

/// Retains the per-file visitor report for `--per-file` (the reference
/// `PerFileRetainer.Retain` shallow clone, keyed by the stamped path): the
/// scalar metrics, the ANALYZER's per-file `message` (`No functions found`
/// for function-free files), and this file's `(name, cohesion)` functions.
fn retain_per_file(agg: &mut Aggregated, report: &Report, source_file: &str) {
    let get_f = |key: &str| {
        report
            .get(key)
            .and_then(ReportValue::as_float)
            .unwrap_or(0.0)
    };
    let functions: Vec<(String, f64)> = report
        .get("functions")
        .and_then(ReportValue::as_functions)
        .map(|fns| {
            fns.iter()
                .map(|f| {
                    (
                        f.get("name")
                            .and_then(ReportValue::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        f.get("cohesion")
                            .and_then(ReportValue::as_float)
                            .unwrap_or(0.0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    agg.per_file.push(PerFileCohesion {
        rel: source_file.to_string(),
        total_functions: report
            .get("total_functions")
            .and_then(ReportValue::as_int)
            .unwrap_or(0),
        lcom: get_f("lcom"),
        cohesion_score: get_f("cohesion_score"),
        function_cohesion: get_f("function_cohesion"),
        message: report
            .get("message")
            .and_then(ReportValue::as_str)
            .unwrap_or_default()
            .to_string(),
        functions,
    });
}

/// The collected functions sorted by `name` (the reference `GetSortedData`, last identifier
/// key). Equal-name order is nondeterministic in the reference implementation and canonicalized by the
/// harness; a stable secondary order keeps our output deterministic.
fn sorted_functions(agg: &Aggregated) -> Vec<FnItem> {
    let mut out: Vec<FnItem> = agg.functions.values().cloned().collect();
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.source_file.cmp(&b.source_file))
    });
    out
}

/// The reference `filepath.Rel(rootPath, filePath)` (flat repos → path under the root).
fn make_relative_path(file_path: &str, root_path: &str) -> String {
    if root_path.is_empty() {
        return file_path.to_string();
    }
    match Path::new(file_path).strip_prefix(Path::new(root_path)) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => file_path.to_string(),
    }
}

// === JSON (renderer section) ===

/// Builds the `static/cohesion --format json` report bytes, or `None` when the
/// path cannot be read.
#[must_use]
pub fn cohesion_report_json(root_path: &str) -> Option<Vec<u8>> {
    cohesion_report_json_flags(root_path, false)
}

/// [`cohesion_report_json`] with the `--per-file` flag applied: `per_file`
/// enables the reference implementation's section enrichment
/// (`StaticService.enrichWithPerFileData` → `JSONReport.EnrichWithPerFileData`
/// over the `PerFileRetainer` snapshots — one `JSONFileEntry` per ANALYZED
/// file, function-free files included, keyed into the section's `files` array).
#[must_use]
pub fn cohesion_report_json_flags(root_path: &str, per_file: bool) -> Option<Vec<u8>> {
    let report = cohesion_report_value_flags(root_path, per_file)?;
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&report);
    Some(bytes)
}

/// Builds the `static/cohesion` `renderer.JSONReport` GoValue (single scored
/// section), shared by the single-analyzer byte path and the multi-analyzer
/// static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn cohesion_report_value(root_path: &str) -> Option<GoValue> {
    cohesion_report_value_flags(root_path, false)
}

/// [`cohesion_report_value`] with the `--per-file` section enrichment.
#[must_use]
pub fn cohesion_report_value_flags(root_path: &str, per_file: bool) -> Option<GoValue> {
    let agg = aggregate(root_path)?;
    // --per-file: one JSONFileEntry per ANALYZED file, in walk order (the
    // reference implementation ranges the retainer map here — run-to-run
    // random; the oracle's measured-variance canonicalization compares the set).
    let file_entries: Option<Vec<GoValue>> = if per_file {
        Some(agg.per_file.iter().map(build_file_entry).collect())
    } else {
        None
    };
    Some(build_json_report(&agg, file_entries))
}

/// Builds the `static/cohesion` section tree in the reference implementation's `AggregationModeSummaryOnly`
/// shape (`text` / `compact`): the detailed `functions` collection is a no-op, so
/// the distribution + issues sections (both derived from the collected functions)
/// are absent while the averaged scalar metrics are unchanged.
#[must_use]
pub fn cohesion_report_value_summary(root_path: &str) -> Option<GoValue> {
    let mut agg = aggregate(root_path)?;
    agg.functions.clear();
    Some(build_json_report(&agg, None))
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/cohesion` —
/// the value the reference implementation's `cohesion.Aggregator` (a plain `common.Aggregator`) returns
/// from `GetResult()` (`ResultBuilder.BuildCollectionResult`), which is what
/// `--format plot` consumes and what `writeReportJSON` serializes into
/// `report.json`:
///
/// * `analyzer_name`, `message` (reference keys it off a RANDOM numeric average —
///   measured-nondeterministic; we use `cohesion_score` like the section),
/// * count: `total_functions` (summed; overwrites the collection length),
/// * averages: `lcom` / `cohesion_score` / `function_cohesion`,
/// * `functions`: the dedup-by-`(_source_file, name)` collection sorted by
///   `name` (`GetSortedData`; the equal-name tie order is nondeterministic in the reference binary
///   and canonicalized by the harness), each item the
///   `convertCohesionFunctionItems` map + the `_source_file` stamp only (the
///   base collector never stamps `_language`/`_directory`).
///
/// With no parsed files the reference implementation returns `createEmptyResult` instead (5 keys, no
/// `analyzer_name`/`functions`).
#[must_use]
pub fn cohesion_raw_report_value(root_path: &str, opts: &Options) -> Option<GoValue> {
    let agg = aggregate_opts(root_path, opts)?;

    if agg.report_count == 0 {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("total_functions", GoValue::Int(0));
        m.push("lcom", GoValue::Float(0.0));
        m.push("cohesion_score", GoValue::Float(1.0));
        m.push("function_cohesion", GoValue::Float(1.0));
        m.push("message", GoValue::Str("No functions found".to_string()));
        return Some(GoValue::Map(m));
    }

    let functions: Vec<GoValue> = sorted_functions(&agg)
        .into_iter()
        .map(|f| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("name", GoValue::Str(f.name));
            m.push("line_count", GoValue::Int(f.line_count));
            m.push("variable_count", GoValue::Int(f.variable_count));
            m.push("cohesion", GoValue::Float(f.cohesion));
            m.push("cohesion_assessment", GoValue::Str(f.cohesion_assessment));
            m.push("variable_assessment", GoValue::Str(f.variable_assessment));
            m.push("size_assessment", GoValue::Str(f.size_assessment));
            m.push("_source_file", GoValue::Str(f.source_file));
            GoValue::Map(m)
        })
        .collect();

    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str("cohesion".to_string()));
    m.push("total_functions", GoValue::Int(agg.total_functions));
    m.push("functions", GoValue::Array(functions));
    m.push(
        "message",
        GoValue::Str(cohesion_message(agg.cohesion_score_avg()).to_string()),
    );
    m.push("lcom", GoValue::Float(agg.lcom_avg()));
    m.push("cohesion_score", GoValue::Float(agg.cohesion_score_avg()));
    m.push(
        "function_cohesion",
        GoValue::Float(agg.function_cohesion_avg()),
    );
    Some(GoValue::Map(m))
}

/// Aggregator message keyed by the first numeric average.
fn cohesion_message(score: f64) -> &'static str {
    if score >= MSG_SCORE_HIGH {
        MSG_EXCELLENT
    } else if score >= MSG_SCORE_MEDIUM {
        MSG_GOOD
    } else if score >= MSG_SCORE_LOW {
        MSG_FAIR
    } else {
        MSG_POOR
    }
}

/// Builds one `renderer.JSONFileEntry` for `--per-file` (the reference
/// `SectionToJSONFileEntry` over `cohesion.NewReportSection(perFileReport)`):
/// status/score come from the PER-FILE report (`message` / `cohesion_score`),
/// the metrics are the per-file scalars, and the distribution and
/// value-string-ascending issues cover this file's functions only.
fn build_file_entry(pf: &PerFileCohesion) -> GoValue {
    let score = pf.cohesion_score;
    let status = if pf.message.is_empty() {
        DEFAULT_STATUS_MESSAGE.to_string()
    } else {
        pf.message.clone()
    };

    let metrics = GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &pf.total_functions.to_string()),
        metric(METRIC_LCOM, &format_float(pf.lcom)),
        metric(METRIC_COHESION_SCORE, &format_float(pf.cohesion_score)),
        metric(
            METRIC_FUNCTION_COHESION,
            &format_float(pf.function_cohesion),
        ),
    ]);

    // Distribution over THIS file's functions (absent for function-free files
    // — Distribution() returns nil ⇒ omitempty).
    let total = pf.functions.len() as i64;
    let mut dist_items = Vec::new();
    if total != 0 {
        let mut excellent = 0i64;
        let mut good = 0i64;
        let mut fair = 0i64;
        let mut poor = 0i64;
        for (_, coh) in &pf.functions {
            if *coh >= DIST_EXCELLENT_MIN {
                excellent += 1;
            } else if *coh >= DIST_GOOD_MIN {
                good += 1;
            } else if *coh >= DIST_FAIR_MIN {
                fair += 1;
            } else {
                poor += 1;
            }
        }
        dist_items.push(dist_item(
            DIST_LABEL_EXCELLENT,
            pct(excellent, total),
            excellent,
        ));
        dist_items.push(dist_item(DIST_LABEL_GOOD, pct(good, total), good));
        dist_items.push(dist_item(DIST_LABEL_FAIR, pct(fair, total), fair));
        dist_items.push(dist_item(DIST_LABEL_POOR, pct(poor, total), poor));
    }

    // Issues: this file's functions sorted by the FormatFloat string ascending
    // (the section comparator; name tie-break keeps our output deterministic
    // where the reference unstable sort varies).
    let mut issues: Vec<&(String, f64)> = pf.functions.iter().collect();
    issues.sort_by(|a, b| {
        format_float(a.1)
            .cmp(&format_float(b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    let issue_items: Vec<GoValue> = issues
        .iter()
        .map(|(name, coh)| {
            let mut iss = GoMap::new(MapOrigin::Struct);
            iss.push("name", GoValue::Str(name.clone()));
            iss.push("location", GoValue::Str(pf.rel.clone()));
            iss.push("value", GoValue::Str(format_float(*coh)));
            iss.push(
                "severity",
                GoValue::Str(severity_for_cohesion(*coh).to_string()),
            );
            GoValue::Map(iss)
        })
        .collect();

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(pf.rel.clone()));
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

fn build_json_report(agg: &Aggregated, file_entries: Option<Vec<GoValue>>) -> GoValue {
    let cohesion_score = agg.cohesion_score_avg();
    let functions = sorted_functions(agg);

    // status: the cohesion section message. the reference implementation's aggregator picks the message
    // from the FIRST numeric average in map-iteration order; the numeric keys are
    // {lcom, cohesion_score, function_cohesion} and the harness canonicalizes the
    // status when the reference implementation's choice varies run-to-run. We key it on cohesion_score, the
    // score the section's NewReportSection also reads.
    let status = if agg.report_count == 0 {
        DEFAULT_STATUS_MESSAGE.to_string()
    } else {
        cohesion_message(cohesion_score).to_string()
    };

    // ---- metrics ----
    let metrics = GoValue::Array(vec![
        metric(METRIC_TOTAL_FUNCTIONS, &agg.total_functions.to_string()),
        metric(METRIC_LCOM, &format!("{:.1}", agg.lcom_avg())),
        metric(METRIC_COHESION_SCORE, &format!("{cohesion_score:.1}")),
        metric(
            METRIC_FUNCTION_COHESION,
            &format!("{:.1}", agg.function_cohesion_avg()),
        ),
    ]);

    // ---- distribution (over the collected functions) ----
    let total = functions.len() as i64;
    let mut dist_items = Vec::new();
    if total != 0 {
        let mut excellent = 0i64;
        let mut good = 0i64;
        let mut fair = 0i64;
        let mut poor = 0i64;
        for f in &functions {
            if f.cohesion >= DIST_EXCELLENT_MIN {
                excellent += 1;
            } else if f.cohesion >= DIST_GOOD_MIN {
                good += 1;
            } else if f.cohesion >= DIST_FAIR_MIN {
                fair += 1;
            } else {
                poor += 1;
            }
        }
        dist_items.push(dist_item(
            DIST_LABEL_EXCELLENT,
            pct(excellent, total),
            excellent,
        ));
        dist_items.push(dist_item(DIST_LABEL_GOOD, pct(good, total), good));
        dist_items.push(dist_item(DIST_LABEL_FAIR, pct(fair, total), fair));
        dist_items.push(dist_item(DIST_LABEL_POOR, pct(poor, total), poor));
    }

    // ---- issues: ALL functions sorted by FormatFloat(cohesion) string ascending ----
    let mut issues: Vec<FnItem> = functions.clone();
    issues.sort_by(|a, b| {
        format_float(a.cohesion)
            .cmp(&format_float(b.cohesion))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.source_file.cmp(&b.source_file))
    });
    let issue_items: Vec<GoValue> = issues
        .iter()
        .map(|f| {
            let mut iss = GoMap::new(MapOrigin::Struct);
            iss.push("name", GoValue::Str(f.name.clone()));
            iss.push("location", GoValue::Str(f.source_file.clone()));
            iss.push("value", GoValue::Str(format_float(f.cohesion)));
            iss.push(
                "severity",
                GoValue::Str(severity_for_cohesion(f.cohesion).to_string()),
            );
            GoValue::Map(iss)
        })
        .collect();

    // ---- section ----
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(score_label(cohesion_score)));
    section.push("status", GoValue::Str(status));
    section.push("metrics", metrics);
    if !dist_items.is_empty() {
        section.push("distribution", GoValue::Array(dist_items));
    }
    section.push("issues", GoValue::Array(issue_items));
    // --per-file enrichment: `files` sits between `issues` and `score`
    // (renderer.JSONSection field order; omitempty without the flag).
    if let Some(entries) = file_entries {
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(cohesion_score));

    // ---- report ----
    let mut report = GoMap::new(MapOrigin::Struct);
    report.push(
        "overall_score_label",
        GoValue::Str(score_label(cohesion_score)),
    );
    report.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    report.push("overall_score", GoValue::Float(cohesion_score));
    GoValue::Map(report)
}

// === YAML / binary (ComputedMetrics machine format) ===

/// Builds the cross-file aggregated [`Report`] that the machine formats marshal.
fn aggregated_report(agg: &Aggregated) -> Report {
    let mut r = Report::new();
    r.insert(
        "total_functions".into(),
        ReportValue::Int(agg.total_functions),
    );
    r.insert("lcom".into(), ReportValue::Float(agg.lcom_avg()));
    r.insert(
        "cohesion_score".into(),
        ReportValue::Float(agg.cohesion_score_avg()),
    );
    r.insert(
        "function_cohesion".into(),
        ReportValue::Float(agg.function_cohesion_avg()),
    );
    r.insert(
        "message".into(),
        ReportValue::Str(cohesion_message(agg.cohesion_score_avg()).to_string()),
    );

    let functions: Vec<BTreeMap<String, ReportValue>> = sorted_functions(agg)
        .into_iter()
        .map(|f| {
            // Only `_source_file` is propagated into the collected map items: the reference implementation's
            // `convertCohesionFunctionItems` (the `TypedCollection.ToMaps` callback)
            // stamps ONLY `analyze.SourceFileKey`, NOT `_directory`/`_language`,
            // even though `StampSourceFile`/`StampLanguage` set those on the
            // TypedCollection wrapper. So `directory`/`language` stay empty in the
            // machine-format output (omitempty drops them).
            let mut m: BTreeMap<String, ReportValue> = BTreeMap::new();
            m.insert("name".into(), ReportValue::Str(f.name));
            m.insert("cohesion".into(), ReportValue::Float(f.cohesion));
            m.insert("_source_file".into(), ReportValue::Str(f.source_file));
            m
        })
        .collect();
    r.insert("functions".into(), ReportValue::Functions(functions));
    r
}

/// Builds the `static/cohesion --format yaml` report bytes, or `None`.
///
/// The reference `cohesion.FormatReportYAML` = `yaml.Marshal(*ComputedMetrics)`. Routed
/// through the go-compatible `cf-goyaml` serializer over a `cf-gojson::GoValue`
/// tree of the struct (declaration-order fields, `omitempty`, byte-sorted map
/// keys for `distribution`).
#[must_use]
pub fn cohesion_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let agg = aggregate(root_path)?;
    let report = aggregated_report(&agg);
    let metrics = cf_cohesion::metrics::compute_all_metrics(&report);
    Some(cf_goyaml::marshal(&computed_metrics_go_value(&metrics)))
}

/// Builds the `static/cohesion --format bin` report bytes, or `None`.
///
/// The reference `cohesion.FormatReportBinary` = `reportutil.EncodeBinaryEnvelope(metrics)`
/// = `"CFB1"` + u32-LE len + compact `json.Marshal(*ComputedMetrics)`. Routed
/// through `cf-reportutil` over the same `cf-gojson::GoValue` tree.
#[must_use]
pub fn cohesion_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let agg = aggregate(root_path)?;
    let report = aggregated_report(&agg);
    let metrics = cf_cohesion::metrics::compute_all_metrics(&report);
    cf_reportutil::encode_binary_envelope(&computed_metrics_go_value(&metrics)).ok()
}

/// Builds the `cf-gojson::GoValue` tree for [`ComputedMetrics`] (reference struct field
/// order + `omitempty`), the single value all machine formats encode.
fn computed_metrics_go_value(m: &cf_cohesion::ComputedMetrics) -> GoValue {
    let mut root = GoMap::new(MapOrigin::Struct);

    // function_cohesion []FunctionCohesionData
    let fc: Vec<GoValue> = m
        .function_cohesion
        .iter()
        .map(|f| {
            let mut o = GoMap::new(MapOrigin::Struct);
            o.push("name", GoValue::Str(f.name.clone()));
            if !f.source_file.is_empty() {
                o.push("source_file", GoValue::Str(f.source_file.clone()));
            }
            if !f.language.is_empty() {
                o.push("language", GoValue::Str(f.language.clone()));
            }
            if !f.directory.is_empty() {
                o.push("directory", GoValue::Str(f.directory.clone()));
            }
            o.push("cohesion", GoValue::Float(f.cohesion));
            o.push("quality_level", GoValue::Str(f.quality_level.clone()));
            GoValue::Map(o)
        })
        .collect();
    root.push("function_cohesion", GoValue::Array(fc));

    // distribution map[string]int (byte-sorted keys via Map origin)
    let mut dist = GoMap::new(MapOrigin::Map);
    for (k, v) in &m.distribution {
        dist.push(k, GoValue::Int(*v));
    }
    root.push("distribution", GoValue::Map(dist));

    // low_cohesion_functions []LowCohesionFunctionData
    let low: Vec<GoValue> = m
        .low_cohesion_functions
        .iter()
        .map(|f| {
            let mut o = GoMap::new(MapOrigin::Struct);
            o.push("name", GoValue::Str(f.name.clone()));
            if !f.source_file.is_empty() {
                o.push("source_file", GoValue::Str(f.source_file.clone()));
            }
            if !f.language.is_empty() {
                o.push("language", GoValue::Str(f.language.clone()));
            }
            if !f.directory.is_empty() {
                o.push("directory", GoValue::Str(f.directory.clone()));
            }
            o.push("cohesion", GoValue::Float(f.cohesion));
            o.push("risk_level", GoValue::Str(f.risk_level.clone()));
            o.push("recommendation", GoValue::Str(f.recommendation.clone()));
            GoValue::Map(o)
        })
        .collect();
    root.push("low_cohesion_functions", GoValue::Array(low));

    // aggregate AggregateData
    let a = &m.aggregate;
    let mut agg_o = GoMap::new(MapOrigin::Struct);
    agg_o.push("total_functions", GoValue::Int(a.total_functions));
    agg_o.push("lcom", GoValue::Float(a.lcom));
    agg_o.push("lcom_variant", GoValue::Str(a.lcom_variant.clone()));
    agg_o.push("cohesion_score", GoValue::Float(a.cohesion_score));
    agg_o.push("function_cohesion", GoValue::Float(a.function_cohesion));
    agg_o.push("health_score", GoValue::Float(a.health_score));
    agg_o.push("message", GoValue::Str(a.message.clone()));
    root.push("aggregate", GoValue::Map(agg_o));

    GoValue::Map(root)
}

// === Helpers ===

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

/// The reference `reportutil.Pct`: fraction in [0,1].
fn pct(count: i64, total: i64) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

/// The reference `reportutil.FormatFloat`: `%.1f`.
fn format_float(v: f64) -> String {
    format!("{v:.1}")
}

fn severity_for_cohesion(coh: f64) -> &'static str {
    if coh < ISSUE_SEVERITY_POOR_MAX {
        SEVERITY_POOR
    } else if coh < ISSUE_SEVERITY_FAIR_MAX {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// Renders a score as the `N/10` label.
fn score_label(score: f64) -> String {
    let n = (score * 10.0).round() as i64;
    format!("{n}/10")
}

#[cfg(test)]
mod per_file_tests {
    use super::*;

    /// Fixture: one file with a function, one function-free file.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nfunc add(a, b int) int {\n\tc := a + b\n\treturn c\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("types.go"),
            "package main\n\ntype Config struct {\n\tName string\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn per_file_flag_emits_files_entries() {
        let dir = fixture();
        let bytes = cohesion_report_json_flags(dir.path().to_str().unwrap(), true).unwrap();
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
        // Per-file status is the ANALYZER's per-file message; a function-free
        // file reports the empty-result message.
        assert!(
            json.contains("\"status\": \"No functions found\""),
            "per-file empty-result status missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let bytes = cohesion_report_json_flags(dir.path().to_str().unwrap(), false).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
        assert!(!json.contains("\"file_path\""));
    }
}
