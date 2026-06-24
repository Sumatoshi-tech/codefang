//! Static-analysis YAML report path for the UAST `static/comments` analyzer.
//!
//! Reproduces the reference static pipeline for the single-analyzer
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
//!     fields stay at their zero values, exactly as in the golden capture), and emit
//!     `ComputedMetrics` through cf-goyaml (gopkg.in/yaml.v3 parity).

use std::fs;
use std::path::Path;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
use cf_uast::Parser;

/// One collected comment item (post-stamp): the full the reference implementation
/// `convertCommentReportItems` map — the raw aggregated
/// report (plot path / report.json) serializes every key; the YAML
/// `ParseReportData` reads only `line` + the stamped source metadata.
struct CommentItem {
    line: i64,
    comment: String,
    placement: String,
    target: String,
    assessment: String,
    source_file: String,
    language: String,
    directory: String,
}

/// One collected function item (post-stamp): the full the reference implementation
/// `convertFunctionReportItems` map. The machine
/// `ComputeAllMetrics` payload reads only the stamped source metadata; the raw
/// aggregated report serializes every key.
struct FunctionItem {
    source_file: String,
    language: String,
    directory: String,
    /// The function's name (`function` key).
    function_name: String,
    /// The function's kind (`type` key, e.g. `"Function"`).
    kind: String,
    /// The function's line count (`lines` key).
    lines: i64,
    /// The associated comment type (`comment` key; `"None"` when undocumented).
    comment: String,
    /// The per-function documentation assessment (`assessment` key, e.g.
    /// `"❌ No Comment"`); the JSON section emits an issue for each bad one.
    assessment: String,
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
    total_comment_details: i64,
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

/// Builds the `static/comments --format json` structured-report bytes for
/// `root_path`, or `None` when the folder cannot be walked.
///
/// The reference JSON path (`StaticService.FormatJSON` → `comments.CreateReportSection`
/// → `renderer.SectionToJSON` → `json.Encoder.SetIndent("","  ").Encode`) emits
/// a scored COMMENTS section: key metrics (Total/Good/Bad Comments, Doc
/// Coverage, Good Ratio, Total Functions), a Documented/Undocumented
/// distribution, and one info issue per UNDOCUMENTED function (assessment
/// `"❌ No Comment"`), sorted by name. the reference implementation sorts via an unstable `sort.Slice`
/// over functions collected in (parallel/map) order, so the tie order of
/// same-named functions is intrinsically nondeterministic in the reference binary (the project
/// MANIFEST marks static_comments.json nonBinding). We emit the deterministic
/// computation: the section score/labels/metrics/distribution are exact, and the
/// issue list is sorted by name (file-walk order within equal names).
#[must_use]
pub fn comments_report_json(root_path: &str) -> Option<Vec<u8>> {
    let root = comments_report_value(root_path)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the `static/comments` `renderer.JSONReport` GoValue (single scored
/// section), shared by the single-analyzer byte path and the multi-analyzer
/// static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn comments_report_value(root_path: &str) -> Option<GoValue> {
    comments_report_value_mode(root_path, false)
}

/// Builds the `static/comments` section tree in the reference implementation's `AggregationModeSummaryOnly`
/// shape (`text` / `compact`): the detailed `comments`/`functions` collections are
/// no-ops, so the top-issues list (undocumented functions, read from `functions`)
/// is absent, while the Documented/Undocumented distribution — computed from the
/// always-on scalar `total_functions`/`documented_functions` counts — and the Key
/// Metrics are unchanged.
#[must_use]
pub fn comments_report_value_summary(root_path: &str) -> Option<GoValue> {
    comments_report_value_mode(root_path, true)
}

fn comments_report_value_mode(root_path: &str, summary_only: bool) -> Option<GoValue> {
    let mut agg = comments_aggregate(root_path)?;
    if summary_only {
        agg.functions.clear();
    }
    let report_count = agg.report_count.max(0);
    let mean = |sum: f64| {
        if report_count > 0 {
            sum / report_count as f64
        } else {
            0.0
        }
    };
    let overall_score = mean(agg.sum_overall_score);
    let good_ratio = mean(agg.sum_good_comments_ratio);
    let doc_coverage = mean(agg.sum_documentation_coverage);

    // --- section score / labels ---
    let score = overall_score;
    let score_label = format_score(score);

    // --- status: NewReportSection reads the aggregate `message`; we emit the
    // deterministic ThresholdLabeler value (matches the common reference output). ---
    let status = comment_message(overall_score);

    // --- metrics (reference KeyMetrics order) ---
    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    let metrics = vec![
        metric(
            "Total Comments",
            cf_reportutil::format_int(agg.total_comments),
        ),
        metric(
            "Good Comments",
            cf_reportutil::format_int(agg.good_comments),
        ),
        metric("Bad Comments", cf_reportutil::format_int(agg.bad_comments)),
        metric("Doc Coverage", format_percent(doc_coverage)),
        metric("Good Ratio", format_percent(good_ratio)),
        metric(
            "Total Functions",
            cf_reportutil::format_int(agg.total_functions),
        ),
    ];

    // --- distribution (Documented / Undocumented); nil when no functions ---
    let total_fns = agg.total_functions;
    let distribution: Vec<GoValue> = if total_fns == 0 {
        Vec::new()
    } else {
        let documented = agg.documented_functions;
        let undocumented = total_fns - documented;
        let pct = |c: i64| {
            if total_fns == 0 {
                0.0
            } else {
                c as f64 / total_fns as f64
            }
        };
        let dist = |label: &str, count: i64| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("label", GoValue::Str(label.to_string()));
            m.push("percent", GoValue::Float(pct(count)));
            m.push("count", GoValue::Int(count));
            GoValue::Map(m)
        };
        vec![
            dist("Documented", documented),
            dist("Undocumented", undocumented),
        ]
    };

    // --- issues: undocumented functions (assessment "❌ No Comment"), name asc ---
    let mut issue_fns: Vec<&FunctionItem> = agg
        .functions
        .iter()
        .filter(|f| f.assessment == "❌ No Comment")
        .collect();
    // Reference: mapx.SortAndLimit(buildIssues(), commentNameLess, 0) — unstable
    // sort.Slice by Name ascending. Reproduce the pdqsort permutation.
    crate::handlers::go_sort::slice(&mut issue_fns, |a, b| a.function_name < b.function_name);
    let issues: Vec<GoValue> = issue_fns
        .iter()
        .map(|f| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(f.function_name.clone()));
            m.push("location", GoValue::Str(f.source_file.clone()));
            m.push("value", GoValue::Str("undocumented".to_string()));
            m.push("severity", GoValue::Str("poor".to_string()));
            GoValue::Map(m)
        })
        .collect();

    // --- section (JSONSection field order) ---
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str("COMMENTS".to_string()));
    section.push("score_label", GoValue::Str(score_label.clone()));
    section.push("status", GoValue::Str(status));
    section.push("metrics", GoValue::Array(metrics));
    if !distribution.is_empty() {
        section.push("distribution", GoValue::Array(distribution));
    }
    section.push("issues", GoValue::Array(issues));
    section.push("score", GoValue::Float(score));

    // --- top-level JSONReport: single scored section ⇒ overall == section ---
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(score_label));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(score));

    Some(GoValue::Map(root))
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/comments` —
/// the value the reference implementation's `comments.Aggregator.GetResult()` returns (the base
/// `BuildCollectionResult` + the `DetailedDataCollector.AddToResult`
/// overwrite), which is what `--format plot` consumes and what
/// `writeReportJSON` serializes into `report.json`:
///
/// * `analyzer_name`, `message` (`buildMessage` over a random numeric average;
///   all three averages land in the same threshold bucket on real corpora —
///   we key it off `overall_score` like the section),
/// * counts (summed): `total_comments`, `good_comments`, `bad_comments`,
///   `total_functions`, `documented_functions`, `total_comment_details`,
/// * averages: `overall_score`, `good_comments_ratio`,
///   `documentation_coverage`,
/// * `comments` / `functions`: the per-file convert maps concatenated in
///   walk order, each stamped `_source_file`/`_language`/`_directory`
///   (`stampCollectionMetadata`). The base spillable collector contributes an
///   empty `comments` slice (its `line` identifier is an int, never a string),
///   so with zero comments the key stays `[]`; `functions` appears only when
///   the detailed collection is non-empty.
///
/// With no parsed files the reference implementation returns `buildEmptyResult` instead (8 keys, no
/// `analyzer_name`/collections).
#[must_use]
pub fn comments_raw_report_value(root_path: &str, opts: &PathPolicyOptions) -> Option<GoValue> {
    let agg = comments_aggregate_opts(root_path, opts)?;

    if agg.report_count == 0 {
        let mut m = GoMap::new(MapOrigin::Map);
        m.push("total_comments", GoValue::Int(0));
        m.push("good_comments", GoValue::Int(0));
        m.push("bad_comments", GoValue::Int(0));
        m.push("overall_score", GoValue::Float(0.0));
        m.push("total_functions", GoValue::Int(0));
        m.push("documented_functions", GoValue::Int(0));
        m.push("total_comment_details", GoValue::Int(0));
        m.push("message", GoValue::Str("No comments found".to_string()));
        return Some(GoValue::Map(m));
    }

    let mean = |sum: f64| {
        if agg.report_count > 0 {
            sum / agg.report_count as f64
        } else {
            0.0
        }
    };
    let overall_score = mean(agg.sum_overall_score);

    // Stamped item maps (map-origin: keys byte-sort at encode time).
    let push_stamps = |m: &mut GoMap, sf: &str, lang: &str, dir: &str| {
        m.push("_source_file", GoValue::Str(sf.to_string()));
        if !lang.is_empty() {
            m.push("_language", GoValue::Str(lang.to_string()));
        }
        if !dir.is_empty() {
            m.push("_directory", GoValue::Str(dir.to_string()));
        }
    };
    let comments: Vec<GoValue> = agg
        .comments
        .iter()
        .map(|c| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("line", GoValue::Int(c.line));
            m.push("comment", GoValue::Str(c.comment.clone()));
            m.push("placement", GoValue::Str(c.placement.clone()));
            m.push("target", GoValue::Str(c.target.clone()));
            m.push("assessment", GoValue::Str(c.assessment.clone()));
            push_stamps(&mut m, &c.source_file, &c.language, &c.directory);
            GoValue::Map(m)
        })
        .collect();
    let functions: Vec<GoValue> = agg
        .functions
        .iter()
        .map(|f| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("function", GoValue::Str(f.function_name.clone()));
            m.push("type", GoValue::Str(f.kind.clone()));
            m.push("lines", GoValue::Int(f.lines));
            m.push("comment", GoValue::Str(f.comment.clone()));
            m.push("assessment", GoValue::Str(f.assessment.clone()));
            push_stamps(&mut m, &f.source_file, &f.language, &f.directory);
            GoValue::Map(m)
        })
        .collect();

    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str("comments".to_string()));
    m.push("comments", GoValue::Array(comments));
    if !functions.is_empty() {
        m.push("functions", GoValue::Array(functions));
    }
    m.push("message", GoValue::Str(comment_message(overall_score)));
    m.push("total_comments", GoValue::Int(agg.total_comments));
    m.push("good_comments", GoValue::Int(agg.good_comments));
    m.push("bad_comments", GoValue::Int(agg.bad_comments));
    m.push("total_functions", GoValue::Int(agg.total_functions));
    m.push(
        "documented_functions",
        GoValue::Int(agg.documented_functions),
    );
    m.push(
        "total_comment_details",
        GoValue::Int(agg.total_comment_details),
    );
    m.push("overall_score", GoValue::Float(overall_score));
    m.push(
        "good_comments_ratio",
        GoValue::Float(mean(agg.sum_good_comments_ratio)),
    );
    m.push(
        "documentation_coverage",
        GoValue::Float(mean(agg.sum_documentation_coverage)),
    );
    Some(GoValue::Map(m))
}

/// `terminal.FormatScore`: `round(score*10)/10` → `"N/10"`.
fn format_score(score: f64) -> String {
    let scaled = (score * 10.0).round() as i64;
    format!("{scaled}/10")
}

/// `reportutil.FormatPercent`: `"%.1f%%"` of `v*100`.
fn format_percent(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

/// Runs the static pipeline over `root_path` and returns the assembled
/// `ComputedMetrics` go-value (the shared report value behind every machine
/// encoding). Returns `None` when the path does not exist.
fn comments_metrics(root_path: &str) -> Option<GoValue> {
    let agg = comments_aggregate(root_path)?;
    Some(compute_metrics(&agg))
}

/// Runs the static pipeline over `root_path` and returns the cross-file
/// [`Aggregated`] accumulator (shared by the machine-format `ComputeAllMetrics`
/// path and the JSON-section path). Returns `None` when the path is missing.
fn comments_aggregate(root_path: &str) -> Option<Aggregated> {
    comments_aggregate_opts(root_path, &PathPolicyOptions::default())
}

/// [`comments_aggregate`] with explicit path-policy options (the plot path
/// passes the run flags; the stdout formats keep the defaults).
fn comments_aggregate_opts(root_path: &str, opts: &PathPolicyOptions) -> Option<Aggregated> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let analyzer = cf_comments::Analyzer::new();

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_files(root, &parser, opts, &mut files);
    // filepath.WalkDir visits entries in lexical (byte-sorted) path order.
    files.sort();

    let mut agg = Aggregated::default();

    for path in &files {
        let Ok(content) = fs::read(path) else {
            continue;
        };
        let path_str = path.to_string_lossy();

        // The reference implementation's static pipeline parses EVERY UAST-supported file and runs the
        // analyzer; files whose tree has no functions/comments (markdown
        // READMEs) yield an empty report that still counts as one file in the
        // cross-file averages (`report_count`). Rust may lack a wired grammar
        // for some supported extensions (markdown), so a parse failure on a
        // supported file is folded as that same empty report — keeping the mean
        // denominator (and thus the averaged scalar metrics) byte-identical.
        let Ok(node) = parser.parse(&path_str, &content) else {
            agg.report_count += 1;
            continue;
        };
        let Ok(report) = analyzer.analyze(Some(&node)) else {
            agg.report_count += 1;
            continue;
        };

        // Stamp metadata exactly as StampSourceFile / StampLanguage.
        let stamped = make_relative_path(&path_str, root_path);
        let directory = parent_dir(&stamped);
        let language = parser.get_language(&path_str);

        aggregate_report(&mut agg, &report, &stamped, &directory, &language);
    }

    Some(agg)
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
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
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
    let item_str = |item: &GoValue, key: &str| {
        map_get(item, key)
            .and_then(as_str)
            .unwrap_or_default()
            .to_string()
    };
    if let Some(items) = map_get(report, "comments").and_then(as_array) {
        for item in items {
            let line = map_get(item, "line").and_then(as_int).unwrap_or(0);
            agg.comments.push(CommentItem {
                line,
                comment: item_str(item, "comment"),
                placement: item_str(item, "placement"),
                target: item_str(item, "target"),
                assessment: item_str(item, "assessment"),
                source_file: source_file.to_string(),
                language: language.to_string(),
                directory: directory.to_string(),
            });
        }
    }
    if let Some(items) = map_get(report, "functions").and_then(as_array) {
        for item in items {
            agg.functions.push(FunctionItem {
                source_file: source_file.to_string(),
                language: language.to_string(),
                directory: directory.to_string(),
                function_name: item_str(item, "function"),
                kind: item_str(item, "type"),
                lines: map_get(item, "lines").and_then(as_int).unwrap_or(0),
                comment: item_str(item, "comment"),
                assessment: item_str(item, "assessment"),
            });
        }
    }

    // MetricsProcessor: sum count keys, sum numeric keys (mean computed later).
    agg.total_comments += scalar_int(report, "total_comments");
    agg.good_comments += scalar_int(report, "good_comments");
    agg.bad_comments += scalar_int(report, "bad_comments");
    agg.total_functions += scalar_int(report, "total_functions");
    agg.documented_functions += scalar_int(report, "documented_functions");
    agg.total_comment_details += scalar_int(report, "total_comment_details");

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

    // --- comment_quality: sorted by line_number ascending via the reference `sort.Slice` ---
    // CommentQualityMetric.Compute calls sort.Slice (unstable pdqsort); the
    // equal-line ties must follow pdqsort's permutation, not a stable order.
    let mut comment_order: Vec<&CommentItem> = agg.comments.iter().collect();
    crate::handlers::go_sort::slice(&mut comment_order, |a, b| a.line < b.line);
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
    crate::handlers::go_sort::slice(&mut doc_order, |_a, _b| false);
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
    crate::handlers::go_sort::slice(&mut risk_order, |_a, _b| false);
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
    aggregate.push(
        "documented_functions",
        GoValue::Int(agg.documented_functions),
    );
    aggregate.push("good_comments_ratio", GoValue::Float(good_comments_ratio));
    aggregate.push(
        "documentation_coverage",
        GoValue::Float(documentation_coverage),
    );
    aggregate.push("health_score", GoValue::Float(overall_score * 100.0));
    // buildMessage over the overall_score average (the value captured in the
    // golden); ThresholdLabeler: ≥0.8 Excellent, ≥0.6 Good, ≥0.4 Fair, else Poor.
    aggregate.push("message", GoValue::Str(comment_message(overall_score)));

    // ComputedMetrics struct order: comment_quality, function_documentation,
    // undocumented_functions, aggregate.
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("comment_quality", GoValue::Array(comment_quality));
    root.push(
        "function_documentation",
        GoValue::Array(function_documentation),
    );
    root.push(
        "undocumented_functions",
        GoValue::Array(undocumented_functions),
    );
    root.push("aggregate", GoValue::Map(aggregate));
    GoValue::Map(root)
}

/// `common.ThresholdLabeler` for comments (reference thresholds).
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

fn as_str(v: &GoValue) -> Option<&str> {
    match v {
        GoValue::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

/// `ParseReportData` reads `total_comments` etc. as the reference `int`; the analyzer emits
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
