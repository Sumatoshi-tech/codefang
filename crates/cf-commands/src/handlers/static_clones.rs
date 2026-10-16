//! Static-analysis report path for the UAST `static/clones` analyzer:
//! `codefang run --analyzers static/clones --format {json,yaml,bin,compact,text}`.
//!
//!  1. The shared static walk ([`super::walk_source_files`]) admits every
//!     UAST-supported file that passes the run filter (`--languages`, the
//!     vendor / generated path policy, the generated-content marker, the
//!     read cap); each admitted file is parsed.
//!  2. Per file the [`cf_clones::Visitor`] scans the UAST for function-like
//!     nodes (functions, methods, closures) and builds each candidate's
//!     profile + `MinHash` signature.
//!  3. The [`cf_clones::Aggregator`] qualifies every candidate as
//!     `relative/path.go::name`, builds ONE global LSH index, verifies every
//!     candidate pair exactly and emits the [`cf_clones::CloneReport`].
//!  4. That one report drives every format through the crate's own builders:
//!     `json` / `text` ← [`cf_clones::report_section::json_value`],
//!     `compact` ← [`cf_clones::report_section::compact`],
//!     `yaml` / `bin` ← [`cf_clones::CloneReport::to_metrics_value`], and the
//!     plot/`report.json` raw value ← [`cf_clones::CloneReport::to_raw_report`].
//!
//! The analyzer math lives entirely in `cf-clones`; this module owns only the
//! per-file parse and the serializer routing.

use super::{SkipReason, StaticFilter};
use cf_clones::{Aggregator, CloneReport, Visitor};
use cf_gojson::{Encoder, GoValue};
use cf_uast::Parser;

/// Builds the `static/clones --format json` report bytes for `root_path`
/// (two-space indent, trailing newline), or `None` when the path cannot be
/// read. `per_file` adds one `files` entry per analyzed file.
#[must_use]
pub fn clones_report_json_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let value = clones_report_value_flags(root_path, filter, per_file)?;
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode_to_vec(&value);
    Some(bytes)
}

/// Builds the `static/clones` `renderer.JSONReport` GoValue (single scored
/// section), shared by the single-analyzer JSON path, the `text` render and
/// the multi-analyzer static-JSON merge. With `per_file` the section carries
/// one entry per analyzed file: the file's function count, how many take
/// part in a clone pair, and the stored pairs that involve the file. `None`
/// when the path cannot be walked.
#[must_use]
pub fn clones_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let report = aggregate_report_opts(root_path, filter)?;
    Some(cf_clones::report_section::json_value(&report, per_file))
}

/// Builds the `static/clones --format yaml` report bytes for `root_path`
/// (the computed-metrics struct: `total_functions, total_clone_pairs,
/// clone_ratio, clone_type_distribution, clone_pairs, message`), or `None`
/// when the path cannot be read.
#[must_use]
pub fn clones_report_yaml(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let report = aggregate_report_opts(root_path, filter)?;
    Some(cf_goyaml::marshal(&report.to_metrics_value()))
}

/// Builds the `static/clones --format bin` report bytes for `root_path` (the
/// CFB1 envelope of the same computed-metrics struct the YAML format
/// emits), or `None` when the path cannot be read.
#[must_use]
pub fn clones_report_bin(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let report = aggregate_report_opts(root_path, filter)?;
    cf_reportutil::encode_binary_envelope(&report.to_metrics_value()).ok()
}

/// Builds the `static/clones --format compact` report bytes for `root_path`
/// (one single-line section render), or `None` when the path cannot be read.
#[must_use]
pub fn clones_report_compact(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let report = aggregate_report_opts(root_path, filter)?;
    Some(cf_clones::report_section::compact(&report))
}

/// Builds the aggregated RAW `analyze.Report` GoValue for `static/clones` —
/// what `--format plot` consumes and what `writeReportJSON` serializes into
/// `report.json`. `filter` carries the run's path-policy flags.
#[must_use]
pub fn clones_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let report = aggregate_report_opts(root_path, filter)?;
    Some(GoValue::Map(report.to_raw_report()))
}

/// Walks `root_path` through the shared static walk, scans every parsed
/// file, and folds the candidates into the cross-file [`CloneReport`] under
/// their root-relative paths. Returns `None` when the root path does not
/// exist; a file that fails to parse is counted as skipped.
fn aggregate_report_opts(root_path: &str, filter: &StaticFilter) -> Option<CloneReport> {
    let parser = Parser::new();
    let mut agg = Aggregator::new();
    for file in super::walk_source_files(root_path, &parser, filter).ok()? {
        let Ok(uast_root) = super::parse_for_functions(&parser, &file) else {
            super::note_skipped(&file.path, SkipReason::Unparseable);
            continue;
        };
        agg.add_file(&file.rel_path, Visitor::new().scan(&uast_root));
    }
    Some(agg.finish())
}

#[cfg(test)]
mod per_file_tests {
    use super::*;

    const PROCESS_ITEMS: &str = r#"package main

import "fmt"

func ProcessItems(items []int) int {
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}
"#;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.go"), PROCESS_ITEMS).unwrap();
        std::fs::write(dir.path().join("b.go"), PROCESS_ITEMS).unwrap();
        std::fs::write(
            dir.path().join("c.go"),
            "package main\n\nfunc add(a, b int) int {\n\treturn a + b\n}\n",
        )
        .unwrap();
        dir
    }

    fn json(dir: &tempfile::TempDir, per_file: bool) -> String {
        let bytes = clones_report_json_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            per_file,
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn per_file_flag_lists_each_files_cloned_functions() {
        let dir = fixture();
        let json = json(&dir, true);
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "a.go entry missing:\n{json}"
        );
        assert!(
            json.contains("\"status\": \"1 of 1 functions in clone pairs\""),
            "cloned-function status missing:\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"c.go\"")
                && json.contains("\"status\": \"No functions in clone pairs\""),
            "uncloned file entry missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let json = json(&dir, false);
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
        assert!(json.contains("\"location\": \"Type-1\""), "{json}");
    }

    #[test]
    fn file_root_is_analyzed_as_a_single_file() {
        let dir = fixture();
        let file = dir.path().join("a.go");
        let bytes = clones_report_yaml(file.to_str().unwrap(), &StaticFilter::default()).unwrap();
        let yaml = String::from_utf8(bytes).unwrap();
        assert!(yaml.contains("total_functions: 1\n"), "{yaml}");
        // The file root stamps the file's NAME on its per-file entry.
        let json = clones_report_json_flags(file.to_str().unwrap(), &StaticFilter::default(), true)
            .unwrap();
        let json = String::from_utf8(json).unwrap();
        assert!(json.contains("\"file_path\": \"a.go\""), "{json}");
    }

    #[test]
    fn missing_root_yields_none() {
        assert!(
            clones_report_compact("/nonexistent/codefang-clones", &StaticFilter::default())
                .is_none()
        );
    }
}
