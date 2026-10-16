//! Static-analysis report path for the raw-file `static/composition`
//! analyzer: `codefang run --analyzers static/composition --format
//! {json,yaml,bin,compact,text}`.
//!
//!  1. The shared all-files walk ([`super::walk_all_files`]) offers EVERY
//!     regular file under `root_path` (lexical order, `.git` and cache
//!     directories skipped) to the analyzer. Composition exists to report
//!     vendored / generated shares, so the vendor/generated path policy
//!     (`--include-vendored` / `--include-generated`) does not apply to it;
//!     only `--languages` and `--extra-excluded-prefixes` narrow the walk.
//!  2. Each file is classified from its root-relative path and its first
//!     [`super::ALL_FILES_HEADER_BYTES`] bytes by
//!     [`cf_composition::Classifier`] and counted by the
//!     [`cf_composition::Aggregator`]; with `--per-file` the file's class
//!     and language are retained as a [`cf_composition::FileRecord`].
//!  3. The [`cf_composition::CompositionReport`] drives every format through
//!     the crate's builders: `json` / `text` / `compact` ←
//!     [`cf_composition::report_section::json_value`], `yaml` / `bin` and the
//!     plot `report.json` ← [`cf_composition::CompositionReport::to_raw_report`].
//!
//! The analyzer math lives in `cf-composition`; this module owns only the
//! per-file classification call and the serializer routing.

use super::StaticFilter;
use cf_composition::{Aggregator, Classifier, CompositionReport, FileRecord};
use cf_gojson::{Encoder, GoValue};

/// What the walk collected: the aggregate plus, under `--per-file`, one
/// record per classified file in walk order.
struct Walked {
    report: CompositionReport,
    files: Option<Vec<FileRecord>>,
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/composition`
/// (`breakdown` / `percentages` / `total_files`) — composition renders no
/// plot page, so this is what `writeReportJSON` serializes into
/// `report.json`.
#[must_use]
pub fn composition_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let walked = walk_root(root_path, filter, false)?;
    Some(GoValue::Map(walked.report.to_raw_report()))
}

/// Builds the `static/composition --format json` report bytes for
/// `root_path` (two-space indent, trailing newline), or `None` when the path
/// cannot be read. `per_file` adds one `files` entry per classified file.
#[must_use]
pub fn composition_report_opts_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let report = composition_report_value_flags(root_path, filter, per_file)?;
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&report);
    Some(bytes)
}

/// Builds the `static/composition` `renderer.JSONReport` GoValue (single
/// info-only section) without per-file entries — the `text` / `compact`
/// render source.
#[must_use]
pub fn composition_report_value_opts(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    composition_report_value_flags(root_path, filter, false)
}

/// Builds the `static/composition` `renderer.JSONReport` GoValue, shared by
/// the single-analyzer JSON path and the multi-analyzer static-JSON merge.
/// With `per_file` the section carries one entry per classified file: its
/// class (`Classified as vendor`), a one-file breakdown and its language.
#[must_use]
pub fn composition_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let walked = walk_root(root_path, filter, per_file)?;
    Some(cf_composition::report_section::json_value(
        walked.report,
        walked.files.as_deref(),
    ))
}

/// Builds the `static/composition --format bin` report bytes for
/// `root_path` (the CFB1 envelope of the raw aggregated report), or `None`
/// when the path cannot be read.
#[must_use]
pub fn composition_bin_opts(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let walked = walk_root(root_path, filter, false)?;
    cf_reportutil::encode_binary_envelope(&GoValue::Map(walked.report.to_raw_report())).ok()
}

/// Builds the `static/composition --format yaml` report bytes for
/// `root_path` (the raw aggregated report as block YAML), or `None` when the
/// path cannot be read.
#[must_use]
pub fn composition_yaml_opts(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let walked = walk_root(root_path, filter, false)?;
    Some(cf_goyaml::marshal(&GoValue::Map(
        walked.report.to_raw_report(),
    )))
}

/// Walks `root_path` through the shared all-files walk and classifies every
/// admitted file against its root-relative path and header bytes, or `None`
/// when the path does not exist.
fn walk_root(root_path: &str, filter: &StaticFilter, per_file: bool) -> Option<Walked> {
    let classifier = Classifier::new();
    let mut agg = Aggregator::new();
    let mut files = per_file.then(Vec::new);
    for file in super::walk_all_files(root_path, filter).ok()? {
        let class = classifier.classify_file(&file.rel_path, &file.bytes);
        agg.add(class);
        if let Some(records) = files.as_mut() {
            records.push(FileRecord {
                language: classifier.language(&file.rel_path, &file.bytes),
                path: file.rel_path,
                class,
            });
        }
    }
    Some(Walked {
        report: agg.report(),
        files,
    })
}

#[cfg(test)]
mod per_file_tests {
    use super::*;
    use crate::handlers::DEFAULT_MAX_PARSE_FILE_BYTES;

    /// Fixture: a source file, a vendored file, a generated file, a doc
    /// under `docs/` and an unknown extension.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let files: &[(&str, &[u8])] = &[
            ("a.go", b"package main\n\nfunc main() {}\n"),
            ("vendor/x/y.go", b"package x\n"),
            (
                "gen/t.pb.go",
                b"// Code generated. DO NOT EDIT.\npackage gen\n",
            ),
            ("docs/guide.md", b"# Guide\n"),
            ("data.xyz", b"hello\n"),
        ];
        for (rel, content) in files {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        dir
    }

    fn json(dir: &tempfile::TempDir, filter: &StaticFilter, per_file: bool) -> String {
        let bytes =
            composition_report_opts_flags(dir.path().to_str().unwrap(), filter, per_file).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn vendor_and_generated_are_counted_under_the_default_policy() {
        let dir = fixture();
        let json = json(&dir, &StaticFilter::default(), false);
        assert!(json.contains("\"label\": \"vendor\""), "{json}");
        assert!(json.contains("\"label\": \"generated\""), "{json}");
        assert!(json.contains("\"label\": \"other\""), "{json}");
        // docs/guide.md is classified against its RELATIVE path.
        assert!(json.contains("\"label\": \"documentation\""), "{json}");
        assert!(
            json.contains("\"label\": \"Total Files\",\n          \"value\": \"5\""),
            "{json}"
        );
    }

    #[test]
    fn per_file_entries_carry_the_class() {
        let dir = fixture();
        let json = json(&dir, &StaticFilter::default(), true);
        assert!(json.contains("\"file_path\": \"vendor/x/y.go\""), "{json}");
        assert!(
            json.contains("\"status\": \"Classified as vendor\""),
            "{json}"
        );
        assert!(
            json.contains("\"status\": \"Classified as generated\""),
            "{json}"
        );
        assert!(
            json.contains("\"status\": \"Classified as other\""),
            "{json}"
        );
        assert!(
            json.contains("\"label\": \"Language\",\n              \"value\": \"Go\""),
            "{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let json = json(&dir, &StaticFilter::default(), false);
        assert!(!json.contains("\"files\""), "{json}");
        assert!(!json.contains("\"file_path\""));
    }

    #[test]
    fn languages_globs_narrow_the_walk() {
        let dir = fixture();
        let filter = StaticFilter {
            policy: cf_pathpolicy::Options::default(),
            language_globs: Some(vec!["*.go".to_string()]),
            max_parse_bytes: DEFAULT_MAX_PARSE_FILE_BYTES,
        };
        let json = json(&dir, &filter, false);
        assert!(
            json.contains("\"label\": \"Total Files\",\n          \"value\": \"3\""),
            "{json}"
        );
        assert!(!json.contains("\"label\": \"other\""));
    }

    #[test]
    fn file_root_is_classified_by_its_name() {
        let dir = fixture();
        let file = dir.path().join("docs").join("guide.md");
        let yaml = composition_yaml_opts(file.to_str().unwrap(), &StaticFilter::default()).unwrap();
        let yaml = String::from_utf8(yaml).unwrap();
        assert!(yaml.contains("total_files: 1\n"), "{yaml}");
        assert!(yaml.contains("    documentation: 1\n"), "{yaml}");
    }

    #[test]
    fn missing_root_yields_none() {
        assert!(composition_bin_opts(
            "/nonexistent/codefang-composition",
            &StaticFilter::default()
        )
        .is_none());
    }
}
