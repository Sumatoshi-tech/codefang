//! Static-analysis `--format bin` report path for the UAST `static/complexity`
//! analyzer.
//!
//! Reproduces the `codefang run --analyzers static/complexity --format bin`
//! capture (`static/static_complexity.bin`). The reference static pipeline
//! (reference flow: `StaticService.AnalyzeFolder` → UAST phase → per-file
//! `parser.Parse` → `complexity.Analyzer.Analyze` → `StampSourceFile` /
//! `StampLanguage` → `complexity.Aggregator` → `ParseReportData` →
//! `ComputeAllMetrics` → `complexity.FormatReportBinary` =
//! `reportutil.EncodeBinaryEnvelope(json.Marshal(ComputedMetrics))`) reduces, for
//! this single UAST analyzer over a Go source tree with `--static-workers 1`, to:
//!
//!  1. a lexical directory walk (`filepath.WalkDir` / `streamFiles`), keeping the
//!     files the parser supports and that survive `pathpolicy.Exclude`;
//!  2. per file: parse to UAST, run [`cf_complexity::Analyzer::function_metrics`]
//!     (the analyzer's deterministic per-file function order), stamp the relative
//!     `source_file` / `directory` and detected `language`;
//!  3. concatenate the per-file function lists in walk order (the detailed
//!     collector's single-worker append order) — the exact input permutation
//!     the reference implementation's `sort.Slice` (pdqsort) operates on;
//!  4. [`cf_complexity::report::computed_metrics`] builds the identical
//!     `ComputedMetrics` value the YAML/JSON-sections siblings derive, applying
//!     the reference implementation's pdqsort for the two sorted collections;
//!  5. the compact cf-gojson payload is wrapped in the CFB1 envelope
//!     (cf-reportutil: `CFB1` magic + LE u32 length + compact payload), exactly
//!     as `FormatReportBinary` does.

use std::fs;
use std::path::Path;

use cf_complexity::node::{Node as CxNode, Positions as CxPos};
use cf_complexity::report::{computed_metrics, FunctionInput, ReportScalars};
use cf_complexity::Analyzer;
use cf_pathpolicy::Options as PathPolicyOptions;
use cf_uast::Parser;
use cf_uast_node::Node as UNode;

/// Builds the `static/complexity --format bin` report bytes for `root_path`, or
/// `None` when the path does not exist (the caller falls through to the
/// blocked-dependency sentinel).
#[must_use]
pub fn complexity_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let analyzer = Analyzer;
    let opts = PathPolicyOptions::default();

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    walk(root, &parser, &opts, &mut files);

    let mut inputs: Vec<FunctionInput> = Vec::new();
    let mut total_functions = 0i64;
    let mut total_complexity = 0i64;
    let mut decision_points = 0i64;
    let mut max_complexity = 0i64;

    for path in &files {
        let Some(content) = super::read_source_capped(path) else {
            continue;
        };
        let path_str = path.to_string_lossy();
        let Ok(tree) = parser.parse(&path_str, &content) else {
            super::note_skipped_file();
            continue;
        };
        let cx = convert(&tree);
        let metrics = analyzer.function_metrics(Some(&cx));
        if metrics.is_empty() {
            continue;
        }

        let language = parser.get_language(&path_str);
        let (source_file, directory) = relative_parts(path, root);

        // The aggregator tracks the max over each per-file report's max_complexity.
        let file_max = metrics
            .iter()
            .map(|m| m.cyclomatic_complexity)
            .max()
            .unwrap_or(0);
        if file_max > max_complexity {
            max_complexity = file_max;
        }

        for m in metrics {
            total_functions += 1;
            total_complexity += m.cyclomatic_complexity;
            decision_points += m.decision_points;
            inputs.push(FunctionInput {
                name: m.name,
                source_file: source_file.clone(),
                language: language.clone(),
                directory: directory.clone(),
                cyclomatic_complexity: m.cyclomatic_complexity,
                cognitive_complexity: m.cognitive_complexity,
                nesting_depth: m.nesting_depth,
                lines_of_code: m.lines_of_code,
            });
        }
    }

    let average_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };
    let scalars = ReportScalars {
        total_functions,
        average_complexity,
        max_complexity,
        total_complexity,
        decision_points,
    };

    let value = computed_metrics(&inputs, &scalars);
    Some(
        cf_reportutil::encode_binary_envelope(&value)
            .expect("complexity payload within CFB1 limit"),
    )
}

/// Recursively walks `dir` in lexical order (mirroring `filepath.WalkDir` +
/// `streamFiles`): directories are recursed (skipping `.git`); files are kept
/// when the parser supports them and they survive `pathpolicy.Exclude`.
fn walk(dir: &Path, parser: &Parser, opts: &PathPolicyOptions, out: &mut Vec<std::path::PathBuf>) {
    // Go parity: filepath.WalkDir visits a FILE root as a single entry, so
    // `codefang run <analyzer> path/to/file.c` analyzes that one file.
    if dir.is_file() {
        visit_file(dir, parser, opts, out);
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        // SIGINT/SIGTERM: bail out of the walk promptly (durability, not
        // output-affecting: the run exits 130 before any report is written).
        if super::run_cancelled() {
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
                continue;
            }
            walk(&path, parser, opts, out);
            continue;
        }
        visit_file(&path, parser, opts, out);
    }
}

/// Collects one file entry of the walk (the non-directory branch of the
/// `filepath.WalkDir` callback): parser support + path policy filters.
fn visit_file(
    path: &Path,
    parser: &Parser,
    opts: &PathPolicyOptions,
    out: &mut Vec<std::path::PathBuf>,
) {
    let path_str = path.to_string_lossy();
    if !parser.is_supported(&path_str) {
        return;
    }
    if cf_pathpolicy::exclude(&path_str, None, opts) {
        return;
    }
    out.push(path.to_path_buf());
}

/// Computes the stamped `source_file` (relative to `root`) and its `directory`
/// (`filepath.Dir(rel)`), mirroring `MakeRelativePath` + `filepath.Dir`.
fn relative_parts(path: &Path, root: &Path) -> (String, String) {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let source_file = if rel.as_os_str().is_empty() {
        // filepath.Rel(root, root) == "." (a FILE root stamps ".").
        ".".to_string()
    } else {
        rel.to_string_lossy().to_string()
    };
    let directory = rel.parent().map_or_else(
        || ".".to_string(),
        |p| {
            let s = p.to_string_lossy();
            if s.is_empty() {
                ".".to_string()
            } else {
                s.to_string()
            }
        },
    );
    (source_file, directory)
}

/// Converts a parsed [`cf_uast_node::Node`] into the complexity analyzer's
/// minimal [`cf_complexity::node::Node`] (the two models are field-compatible;
/// types and roles are plain strings).
fn convert(u: &UNode) -> CxNode {
    let mut n = CxNode::new(u.node_type.clone());
    n.token = u.token.clone();
    n.roles = u.roles.clone();
    for (k, v) in &u.props {
        n.props.insert(k.clone(), v.clone());
    }
    if let Some(p) = &u.pos {
        n.pos = Some(CxPos {
            start_line: p.start_line as u32,
            start_col: p.start_col as u32,
            start_offset: p.start_offset as u32,
            end_line: p.end_line as u32,
            end_col: p.end_col as u32,
            end_offset: p.end_offset as u32,
        });
    }
    for c in &u.children {
        n.add_child(convert(c));
    }
    n
}
