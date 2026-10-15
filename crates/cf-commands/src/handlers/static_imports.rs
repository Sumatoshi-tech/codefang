//! Static-analysis report path for the UAST `static/imports` analyzer.
//!
//! Builds `codefang run --analyzers static/imports` across its machine
//! formats (yaml, bin, json) and the section value the text/compact
//! renderers and the multi-analyzer static JSON merge consume.
//!
//! Pipeline:
//!
//!  1. The shared static walk ([`super::walk_source_files`]) admits every
//!     UAST-supported file that passes the run filter (`--languages`, the
//!     vendor/generated path policy, the generated-content marker, the read
//!     cap). A FILE root is analyzed as that single file.
//!  2. Parse each file and run [`cf_imports::extract_imports`] with the
//!     parser's language name and the file bytes: one entry per imported
//!     module/package/file, deduplicated within the file in first-seen order
//!     (aliases, specifier lists and keywords never count — see the crate's
//!     `extract` module for the per-language rules).
//!  3. Fold into [`cf_imports::Aggregate`]: `total_files` counts EVERY
//!     analyzed file (import-free files included; a supported file that
//!     fails to parse folds as import-free), and each import's value is the
//!     number of files importing it.
//!  4. Serialize: yaml/bin carry [`cf_imports::ComputedMetrics`] (classified
//!     per the importing file's language); json carries the info-only IMPORTS
//!     section with issues ordered count-desc, then name.
//!
//! A root with no analyzable files is the EMPTY report (status
//! `No import data available`, zero metrics), never an error; only a missing
//! path yields `None`.

use super::StaticFilter;
use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_imports::section::{
    status_message, KEY_COUNT, KEY_IMPORTS, KEY_IMPORT_COUNTS, KEY_TOTAL_FILES, METRIC_TOTAL_FILES,
    METRIC_UNIQUE_IMPORTS, SCORE_INFO, SCORE_LABEL_INFO, SECTION_TITLE, SEVERITY_INFO,
};
use cf_imports::{extract_imports, Aggregate};
use cf_uast::Parser;

/// Number of files a `--per-file` entry stands for.
const PER_FILE_TOTAL_FILES: i64 = 1;
/// Issue value of an import inside a `--per-file` entry (one file).
const PER_FILE_ISSUE_VALUE: &str = "1";
/// Location of an aggregate issue (imports are not located in one file).
const AGGREGATE_LOCATION: &str = "";

/// Builds the `static/imports --format yaml` report bytes for `root_path`.
/// Returns `None` only when the path does not exist.
pub fn imports_report_yaml(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let scan = scan(root_path, filter, false)?;
    let metrics = cf_imports::compute_metrics(&scan.aggregate.report_data());
    Some(cf_goyaml::marshal(&metrics.to_go_value_yaml()))
}

/// Builds the `static/imports --format bin` report bytes for `root_path`:
/// the same metrics as the yaml sibling in a CFB1 envelope.
pub fn imports_report_bin(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let scan = scan(root_path, filter, false)?;
    let metrics = cf_imports::compute_metrics(&scan.aggregate.report_data());
    Some(
        cf_reportutil::encode_binary_envelope(&metrics.to_go_value())
            .expect("imports metrics never exceed the CFB1 length cap"),
    )
}

/// Builds the `static/imports --format json` structured-report bytes with
/// the default filter and no per-file enrichment.
pub fn imports_report_json(root_path: &str) -> Option<Vec<u8>> {
    imports_report_json_flags(root_path, &StaticFilter::default(), false)
}

/// [`imports_report_json`] with the run filter and the `--per-file` flag:
/// `per_file` adds one entry per ANALYZED file (import-free files included)
/// to the section's `files` array.
#[must_use]
pub fn imports_report_json_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let root = imports_report_value_flags(root_path, filter, per_file)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the `static/imports` JSON report value (single info-only section)
/// with the default filter, shared by the single-analyzer path and the
/// multi-analyzer static-JSON merge.
#[must_use]
pub fn imports_report_value(root_path: &str) -> Option<GoValue> {
    imports_report_value_flags(root_path, &StaticFilter::default(), false)
}

/// [`imports_report_value`] with the run filter and `--per-file` enrichment.
#[must_use]
pub fn imports_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    let scan = scan(root_path, filter, per_file)?;
    let count = scan.aggregate.unique_count();

    let issues: Vec<GoValue> = scan
        .aggregate
        .ranked()
        .into_iter()
        .map(|(name, c)| issue(name, AGGREGATE_LOCATION, &cf_reportutil::format_int(c)))
        .collect();

    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(SCORE_LABEL_INFO.to_string()));
    section.push("status", GoValue::Str(status_message(count)));
    section.push(
        "metrics",
        GoValue::Array(metrics(count, scan.aggregate.total_files())),
    );
    section.push("issues", GoValue::Array(issues));
    if per_file {
        let files = scan
            .per_file
            .iter()
            .map(|f| build_file_entry(&f.rel_path, &f.imports))
            .collect();
        section.push("files", GoValue::Array(files));
    }
    section.push("score", GoValue::Float(SCORE_INFO));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "overall_score_label",
        GoValue::Str(SCORE_LABEL_INFO.to_string()),
    );
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(SCORE_INFO));
    Some(GoValue::Map(root))
}

/// Builds the AGGREGATED RAW report value for `static/imports` — what
/// `--format plot` consumes and what `report.json` stores: `imports` (unique
/// paths, byte-sorted), `import_counts` (path -> number of importing files),
/// `count` (unique paths) and `total_files`.
#[must_use]
pub fn imports_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let scan = scan(root_path, filter, false)?;

    let imports: Vec<GoValue> = scan
        .aggregate
        .entries()
        .map(|(path, _)| GoValue::Str(path.to_string()))
        .collect();
    let mut import_counts = GoMap::new(MapOrigin::Map);
    for (path, entry) in scan.aggregate.entries() {
        import_counts.push(path, GoValue::Int(entry.count));
    }

    let mut m = GoMap::new(MapOrigin::Map);
    m.push(KEY_IMPORTS, GoValue::Array(imports));
    m.push(KEY_IMPORT_COUNTS, GoValue::Map(import_counts));
    m.push(KEY_COUNT, GoValue::Int(scan.aggregate.unique_count()));
    m.push(KEY_TOTAL_FILES, GoValue::Int(scan.aggregate.total_files()));
    Some(GoValue::Map(m))
}

/// One analyzed file's deduplicated import paths, for `--per-file`.
struct FileImports {
    /// Path relative to the walk root (a file root stamps the file name).
    rel_path: String,
    /// Import paths in first-seen order.
    imports: Vec<String>,
}

/// The result of walking a root.
#[derive(Default)]
struct Scan {
    aggregate: Aggregate,
    per_file: Vec<FileImports>,
}

/// Walks `root_path` (a directory or a single file) through the shared
/// static walk, parses each admitted file, extracts its imports and folds
/// them into the aggregate. Returns `None` only when the path does not
/// exist; a root with no analyzable files yields an empty [`Scan`].
fn scan(root_path: &str, filter: &StaticFilter, keep_per_file: bool) -> Option<Scan> {
    let parser = Parser::new();
    let walk = super::walk_source_files(root_path, &parser, filter).ok()?;

    let mut scan = Scan::default();
    for file in walk {
        let imports: Vec<String> = match super::parse_for_imports(&parser, &file) {
            Ok(node) => {
                let found = extract_imports(&node, &file.language, &file.bytes);
                scan.aggregate.add_file(&file.language, &found);
                found.into_iter().map(|i| i.path).collect()
            }
            Err(_) => {
                scan.aggregate.add_file(&file.language, &[]);
                Vec::new()
            }
        };
        if keep_per_file {
            scan.per_file.push(FileImports {
                rel_path: file.rel_path,
                imports,
            });
        }
    }
    Some(scan)
}

/// The section's key metrics: unique imports and analyzed files.
fn metrics(unique_imports: i64, total_files: i64) -> Vec<GoValue> {
    vec![
        metric(
            METRIC_UNIQUE_IMPORTS,
            cf_reportutil::format_int(unique_imports),
        ),
        metric(METRIC_TOTAL_FILES, cf_reportutil::format_int(total_files)),
    ]
}

/// One `{label, value}` metric.
fn metric(label: &str, value: String) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("label", GoValue::Str(label.to_string()));
    m.push("value", GoValue::Str(value));
    GoValue::Map(m)
}

/// One info issue `{name, location, value, severity}`.
fn issue(name: &str, location: &str, value: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("name", GoValue::Str(name.to_string()));
    m.push("location", GoValue::Str(location.to_string()));
    m.push("value", GoValue::Str(value.to_string()));
    m.push("severity", GoValue::Str(SEVERITY_INFO.to_string()));
    GoValue::Map(m)
}

/// One `--per-file` entry: the file's imports alphabetically, each located
/// at the file and valued `1`, with `Total Files` `1`.
fn build_file_entry(rel_path: &str, imports: &[String]) -> GoValue {
    let count = imports.len() as i64;
    let mut sorted: Vec<&String> = imports.iter().collect();
    sorted.sort();
    let issues: Vec<GoValue> = sorted
        .iter()
        .map(|name| issue(name, rel_path, PER_FILE_ISSUE_VALUE))
        .collect();

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(rel_path.to_string()));
    entry.push("score_label", GoValue::Str(SCORE_LABEL_INFO.to_string()));
    entry.push("status", GoValue::Str(status_message(count)));
    entry.push(
        "metrics",
        GoValue::Array(metrics(count, PER_FILE_TOTAL_FILES)),
    );
    entry.push("issues", GoValue::Array(issues));
    entry.push("score", GoValue::Float(SCORE_INFO));
    GoValue::Map(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Fixture: one file with two imports, one import-free file, one
    /// unsupported file.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("types.go"),
            "package main\n\ntype Config struct {\n\tName string\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "import nothing\n").unwrap();
        dir
    }

    fn json(dir: &Path, per_file: bool) -> String {
        let bytes =
            imports_report_json_flags(dir.to_str().unwrap(), &StaticFilter::default(), per_file)
                .expect("existing path");
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn per_file_flag_emits_one_entry_per_analyzed_file() {
        let dir = fixture();
        let json = json(dir.path(), true);
        assert!(json.contains("\"files\""), "files key missing:\n{json}");
        assert!(json.contains("\"file_path\": \"a.go\""), "{json}");
        assert!(json.contains("\"file_path\": \"types.go\""), "{json}");
        assert!(
            !json.contains("notes.txt"),
            "unsupported file listed:\n{json}"
        );
        assert!(
            json.contains("\"status\": \"No import data available\""),
            "import-free per-file status missing:\n{json}"
        );
        assert!(json.contains("\"location\": \"a.go\""), "{json}");
        // Every per-file entry stands for exactly one file.
        let value = imports_report_value_flags(
            dir.path().to_str().unwrap(),
            &StaticFilter::default(),
            true,
        )
        .expect("existing path");
        let files = section_files(&value);
        assert_eq!(files.len(), 2);
        for entry in files {
            assert_eq!(
                metric_value(entry, METRIC_TOTAL_FILES),
                Some("1"),
                "{entry:?}"
            );
        }
    }

    /// The `files` array of the single IMPORTS section.
    fn section_files(report: &GoValue) -> &[GoValue] {
        let Some(GoValue::Array(sections)) = report.as_map().and_then(|m| m.get("sections")) else {
            panic!("sections array missing: {report:?}");
        };
        match sections[0].as_map().and_then(|m| m.get("files")) {
            Some(GoValue::Array(files)) => files,
            other => panic!("files array missing: {other:?}"),
        }
    }

    /// The value of the metric labelled `label` in a section or file entry.
    fn metric_value<'a>(entry: &'a GoValue, label: &str) -> Option<&'a str> {
        let GoValue::Array(metrics) = entry.as_map()?.get("metrics")? else {
            return None;
        };
        metrics
            .iter()
            .filter_map(GoValue::as_map)
            .find(|m| m.get("label") == Some(&GoValue::Str(label.to_string())))
            .and_then(|m| match m.get("value") {
                Some(GoValue::Str(v)) => Some(v.as_str()),
                _ => None,
            })
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let json = json(dir.path(), false);
        assert!(!json.contains("\"files\""), "{json}");
        assert!(!json.contains("\"file_path\""));
    }

    #[test]
    fn empty_root_is_the_empty_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let json = json(dir.path(), false);
        assert!(
            json.contains("\"status\": \"No import data available\""),
            "{json}"
        );
        assert!(json.contains("\"issues\": []"), "{json}");
        let yaml = imports_report_yaml(dir.path().to_str().unwrap(), &StaticFilter::default())
            .expect("existing path");
        assert!(String::from_utf8(yaml)
            .unwrap()
            .contains("total_imports: 0"));
    }

    #[test]
    fn missing_root_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope");
        assert!(imports_report_value(missing.to_str().unwrap()).is_none());
    }

    #[test]
    fn file_root_stamps_the_file_name() {
        let dir = fixture();
        let file = dir.path().join("a.go");
        let json = json(&file, true);
        assert!(json.contains("\"file_path\": \"a.go\""), "{json}");
        assert!(
            json.contains("\"label\": \"Total Files\",\n          \"value\": \"1\""),
            "{json}"
        );
        assert!(!json.contains("\"file_path\": \".\""), "{json}");
    }

    /// A nested file is stamped root-relative (slash-joined).
    #[test]
    fn nested_files_are_stamped_root_relative() {
        let dir = fixture();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(
            dir.path().join("sub/c.go"),
            "package sub\n\nimport \"io\"\n\nvar _ = io.EOF\n",
        )
        .unwrap();
        let json = json(dir.path(), true);
        assert!(json.contains("\"file_path\": \"sub/c.go\""), "{json}");
        assert!(json.contains("\"location\": \"sub/c.go\""), "{json}");
    }

    #[test]
    fn raw_report_counts_files_per_import() {
        let dir = fixture();
        let raw = imports_raw_report_value(dir.path().to_str().unwrap(), &StaticFilter::default())
            .expect("existing path");
        let top = raw.as_map().expect("map");
        assert_eq!(top.get(KEY_COUNT), Some(&GoValue::Int(2)));
        assert_eq!(top.get(KEY_TOTAL_FILES), Some(&GoValue::Int(2)));
        let counts = top
            .get(KEY_IMPORT_COUNTS)
            .and_then(GoValue::as_map)
            .expect("counts");
        assert_eq!(counts.get("fmt"), Some(&GoValue::Int(1)));
    }
}
