//! Static-analysis report path for the UAST `static/clones` analyzer.
//!
//! Reproduces the reference static folder pipeline for
//! `codefang run --analyzers static/clones --format {json,yaml,bin}`:
//!
//!  1. `StaticService.uastPhase` walks `rootPath` with `filepath.WalkDir` in
//!     lexical order (directories recursed except `.git`); every regular file
//!     that is UAST-supported (`parser.IsSupported`), matches `--languages`
//!     (none here -> all), and is not excluded by `pathpolicy.Exclude` is
//!     streamed for analysis.
//!  2. clones is a `VisitorProvider`, so per file the framework runs the
//!     single-pass [`cf_clones::Visitor`] (full pre-order traversal): it counts
//!     function nodes and exports each surviving function's MinHash signature in
//!     the `_func_signatures` report key. The framework then stamps every
//!     signature item with `_source_file` (the repo-relative path).
//!  3. The clones [`cf_clones::Aggregator`] folds every per-file report: it sums
//!     `total_functions`, qualifies each function name as `sourceFile::name`,
//!     builds ONE global LSH index over all signatures, finds cross-file clone
//!     pairs (`findClonePairs`, dedup by canonical name key, cap stored pairs at
//!     `DefaultMaxClonePairs = 1000` while keeping the exact total count), and
//!     emits the aggregate report value.
//!  4. That single report value drives every format:
//!       * `json` -> the report becomes ONE `clones.ReportSection`
//!         (`renderer.SectionsToJSON`): `overall_score_label` / `sections`
//!         (title, score_label, status, metrics, distribution, issues, score) /
//!         `overall_score`. Issues are the stored clone pairs sorted by
//!         similarity descending; the issue list order is nondeterministic in the reference binary
//!         (the stored pair multiset is stable, only the tie order varies),
//!         which the differential oracle canonicalizes.
//!       * `yaml` / `bin` -> `FormatPerAnalyzer` -> `computeMetricsFromReport`
//!         (`ComputedMetrics`: total_functions, total_clone_pairs, clone_ratio,
//!         clone_type_distribution, clone_pairs (the stored <=1000), message),
//!         marshaled through cf-goyaml / the CFB1 binary envelope.
//!
//! The analyzer MATH (signatures, LSH, pair finding, classification, ratio,
//! section/metrics projection) lives entirely in the cf-clones crate; this
//! module owns only the pipeline-tier folder walk + the serializer routing,
//! exactly as the reference `internal/framework` + `internal/analyzers/analyze` do.

use std::fs;
use std::path::Path;

use cf_analyze::Report;
use cf_clones::aggregator::Aggregator;
use cf_clones::report_section::report_section_json_value;
use cf_clones::Visitor;
use cf_gojson::Encoder;
use cf_pathpolicy::{exclude, Options};
use cf_uast::Parser;
use cf_uast_node::Node as UastNode;

/// Builds the `static/clones --format json` report bytes for `root_path`
/// (`renderer.SectionsToJSON` of the single aggregated clones section), or
/// `None` when the path cannot be read.
#[must_use]
pub fn clones_report_json(root_path: &str) -> Option<Vec<u8>> {
    clones_report_json_flags(root_path, false)
}

/// [`clones_report_json`] with the `--per-file` flag applied. The clones
/// aggregator retains NO per-file snapshots (it is not `PerFileModeEnabled` in
/// the reference implementation), but `EnrichWithPerFileData` still initializes
/// EVERY section with an EMPTY `files` array under `--per-file` — so the flag
/// adds `files: []` to the clones section, exactly as the reference emits.
#[must_use]
pub fn clones_report_json_flags(root_path: &str, per_file: bool) -> Option<Vec<u8>> {
    let value = clones_report_value_flags(root_path, per_file)?;
    // Reference: json.NewEncoder(w).SetIndent("", "  ").Encode(report) -> two-space
    // indent + one trailing newline.
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&value);
    Some(bytes)
}

/// Builds the `static/clones` `renderer.JSONReport` GoValue (single scored
/// section), shared by the single-analyzer byte path and the multi-analyzer
/// static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn clones_report_value(root_path: &str) -> Option<cf_gojson::GoValue> {
    clones_report_value_flags(root_path, false)
}

/// [`clones_report_value`] with the `--per-file` section enrichment (an empty
/// `files` array — see [`clones_report_json_flags`]).
#[must_use]
pub fn clones_report_value_flags(root_path: &str, per_file: bool) -> Option<cf_gojson::GoValue> {
    let report = aggregate_report(root_path)?;
    let value = report_section_json_value(&report);
    if per_file {
        Some(super::ensure_sections_files_key(value))
    } else {
        Some(value)
    }
}

/// Builds the `static/clones --format yaml` report bytes for `root_path`
/// (`FormatPerAnalyzer(YAML)` -> `yaml.Marshal(computeMetricsFromReport)`), or
/// `None` when the path cannot be read.
#[must_use]
pub fn clones_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report(root_path)?;
    let metrics = cf_clones::Analyzer::new().computed_metrics(&report);
    Some(cf_goyaml::marshal(&metrics.to_go_value()))
}

/// Builds the `static/clones --format bin` report bytes for `root_path`
/// (`FormatPerAnalyzer(Binary)` -> CFB1 envelope of `computeMetricsFromReport`),
/// or `None` when the path cannot be read.
#[must_use]
pub fn clones_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report(root_path)?;
    let metrics = cf_clones::Analyzer::new().computed_metrics(&report);
    cf_reportutil::encode_binary_envelope(&metrics.to_go_value()).ok()
}

/// Builds the `static/clones --format compact` report bytes for `root_path`
/// (the reference `StaticService.FormatCompact` -> `DefaultStaticRenderer.RenderCompact`:
/// one single-line section render), or `None` when the path cannot be read.
/// This is the only fully deterministic in the reference binary terminal format for clones — it shows
/// only the title/score-bar/message, never the order-nondeterministic pair list.
#[must_use]
pub fn clones_report_compact(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report(root_path)?;
    Some(cf_clones::report_section::report_section_compact(&report))
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/clones` —
/// the value the reference `clones.Aggregator.GetResult()` returns after the
/// folder walk, which is what `--format plot` consumes and what
/// `writeReportJSON` serializes into `report.json`. `opts` carries the run's
/// path-policy flags. The stored `clone_pairs` ORDER is nondeterministic in the reference binary
/// (LSH candidate iteration); the differential oracle measures and
/// canonicalizes it — the pair multiset and every scalar are deterministic.
#[must_use]
pub fn clones_raw_report_value(root_path: &str, opts: &Options) -> Option<cf_gojson::GoValue> {
    Some(cf_gojson::GoValue::Map(aggregate_report_opts(
        root_path, opts,
    )?))
}

/// Walks `root_path`, runs the clones visitor per file, and folds the per-file
/// signature reports into the cross-file aggregate report value. Returns `None`
/// when the root path does not exist (reference: would surface a walk error).
fn aggregate_report(root_path: &str) -> Option<Report> {
    aggregate_report_opts(root_path, &Options::default())
}

/// [`aggregate_report`] with explicit path-policy options (the plot path passes
/// the run flags; the stdout formats keep the defaults).
fn aggregate_report_opts(root_path: &str, opts: &Options) -> Option<Report> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let mut agg = Aggregator::new();

    walk(root, root_path, &parser, opts, &mut agg);

    Some(agg.get_result())
}

/// Recursively walks `dir` in lexical order, mirroring `filepath.WalkDir`:
/// directories are recursed (except `.git`), files are filtered through parser
/// support + path policy, parsed, run through the clones visitor, and folded
/// into `agg`.
fn walk(dir: &Path, root_path: &str, parser: &Parser, opts: &Options, agg: &mut Aggregator) {
    // Go parity: filepath.WalkDir visits a FILE root as a single entry, so
    // `codefang run <analyzer> path/to/file.c` analyzes that one file.
    if dir.is_file() {
        visit_file(dir, root_path, parser, opts, agg);
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
            walk(&path, root_path, parser, opts, agg);
            continue;
        }

        visit_file(&path, root_path, parser, opts, agg);
    }
}

/// Analyzes one file entry of the walk (the non-directory branch of the
/// `filepath.WalkDir` callback): parser support + path policy filters, parse,
/// clones visitor, and fold into the aggregate.
fn visit_file(path: &Path, root_path: &str, parser: &Parser, opts: &Options, agg: &mut Aggregator) {
    let path_str = path.to_string_lossy();

    // ShouldSkipFolderNode: must be UAST-supported.
    if !parser.is_supported(&path_str) {
        return;
    }
    // matchesLanguageGlobs (empty -> all match), then pathpolicy.Exclude.
    if exclude(&path_str, None, opts) {
        return;
    }

    let Some(content) = super::read_source_capped(path) else {
        return;
    };
    let Ok(uast_root) = parser.parse(&path_str, &content) else {
        super::note_skipped_file();
        return;
    };

    // Single-pass clones visitor: count functions + export signatures.
    let mut visitor = Visitor::new();
    uast_root.visit_pre_order(&mut |n: &UastNode| visitor.on_enter(n));

    // _source_file stamp = path relative to the analyzed root, then fold the
    // stamped per-file signature report into the aggregate.
    let source = make_relative_path(&path_str, root_path);
    let report = visitor.get_report_with_source(&source);
    agg.aggregate(&[(cf_clones::ANALYZER_NAME.to_string(), report)]);
}

/// The reference `MakeRelativePath`: `filepath.Rel(rootPath, filePath)`.
fn make_relative_path(file_path: &str, root_path: &str) -> String {
    if root_path.is_empty() {
        return file_path.to_string();
    }
    let root = Path::new(root_path);
    let file = Path::new(file_path);
    match file.strip_prefix(root) {
        // filepath.Rel(root, root) == "." (a FILE root stamps ".").
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => file_path.to_string(),
    }
}

#[cfg(test)]
mod per_file_tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nfunc add(a, b int) int {\n\treturn a + b\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn per_file_flag_emits_empty_files_array() {
        let dir = fixture();
        let bytes = clones_report_json_flags(dir.path().to_str().unwrap(), true).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        // Clones retains no per-file data, but the enrichment still initializes
        // the section with an EMPTY files array (present, not omitted).
        assert!(
            json.contains("\"files\": []"),
            "empty files array missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let bytes = clones_report_json_flags(dir.path().to_str().unwrap(), false).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
    }
}
