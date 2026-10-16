//! Report paths for the `static/comments` analyzer: the folder walk, the
//! per-file stamping and every serialization (`json` section report, `yaml`
//! / `bin` computed metrics, the raw report behind `plot`, the `text` /
//! `compact` summary).
//!
//! # Pipeline
//!
//!  1. The shared static walk ([`super::walk_source_files`]) admits every
//!     UAST-supported file that passes the run filter (`--languages`, path
//!     policy, generated-content marker, read cap). A file root analyzes
//!     that one file.
//!  2. Parse each file and run [`cf_comments::Analyzer::analyze_metrics`].
//!     A supported file the parser cannot handle is kept as an empty file
//!     (all counters zero) so it still appears under `--per-file`.
//!  3. Stamp every file with its root-relative path, directory and language.
//!  4. Aggregate with [`cf_comments::Aggregator`]: counters are summed and
//!     every ratio is Σ/Σ (coverage = Σdocumented / Σfunctions, good ratio =
//!     Σgood / Σcomments); the section score and status follow the weighted
//!     good ratio.
//!
//! All machine formats share one aggregation, so `json`, `yaml`, `bin`,
//! `text` and `plot` report the same counts.

use super::{SourceFile, StaticFilter};
use cf_comments::{
    comment_preview, Aggregator, CommentCounts, CommentDetail, FileReport, FileStamp, FunctionInfo,
};
use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_uast::Parser;

/// An undocumented declaration spanning at least this many lines is a
/// `HIGH` documentation risk.
const RISK_HIGH_MIN_LINES: i64 = 50;
/// ... at least this many lines is a `MEDIUM` risk; shorter ones are `LOW`.
const RISK_MEDIUM_MIN_LINES: i64 = 10;

/// The section title.
const SECTION_TITLE: &str = "COMMENTS";
/// Issue value / severity for an undocumented declaration.
const ISSUE_VALUE: &str = "undocumented";
const ISSUE_SEVERITY: &str = "poor";

/// Builds the `static/comments --format yaml` report bytes for `root_path`.
#[must_use]
pub fn comments_report_yaml(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let agg = aggregate(root_path, filter);
    Some(cf_goyaml::marshal(&computed_metrics(&agg)))
}

/// Builds the `static/comments --format bin` report bytes for `root_path`:
/// the same computed-metrics value as the yaml sibling inside the CFB1
/// envelope. `None` when the payload exceeds the envelope's length cap.
#[must_use]
pub fn comments_report_bin(root_path: &str, filter: &StaticFilter) -> Option<Vec<u8>> {
    let agg = aggregate(root_path, filter);
    cf_reportutil::encode_binary_envelope(&computed_metrics(&agg)).ok()
}

/// Builds the `static/comments --format json` report bytes for `root_path`;
/// `per_file` adds one `files` entry per analyzed file.
#[must_use]
pub fn comments_report_json_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<Vec<u8>> {
    let root = comments_report_value_flags(root_path, filter, per_file)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the single-section `static/comments` JSON report value, shared by
/// the single-analyzer path and the multi-analyzer static-JSON merge.
#[must_use]
pub fn comments_report_value_flags(
    root_path: &str,
    filter: &StaticFilter,
    per_file: bool,
) -> Option<GoValue> {
    Some(section_report(
        &aggregate(root_path, filter),
        per_file,
        false,
    ))
}

/// The `text` / `compact` summary: the section without its issue list.
#[must_use]
pub fn comments_report_value_summary(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    Some(section_report(&aggregate(root_path, filter), false, true))
}

// --- section report (json / text / compact) -----------------------------------

fn section_report(agg: &Aggregator, per_file: bool, summary_only: bool) -> GoValue {
    let counts = agg.counts();
    let score = counts.overall_score();
    let score_label = format_score(score);

    let issues = if summary_only {
        Vec::new()
    } else {
        undocumented_issues(agg.functions())
    };

    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str(SECTION_TITLE.to_string()));
    section.push("score_label", GoValue::Str(score_label.clone()));
    section.push("status", GoValue::Str(counts.message().to_string()));
    section.push("metrics", GoValue::Array(key_metrics(&counts)));
    push_distribution(&mut section, &counts);
    section.push("issues", GoValue::Array(issues));
    if per_file {
        let entries = agg.files().iter().map(file_entry).collect();
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(score));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(score_label));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(score));
    GoValue::Map(root)
}

/// One `--per-file` entry: this file's score, status, metrics, distribution
/// and undocumented declarations.
fn file_entry(file: &FileReport) -> GoValue {
    let counts = file.metrics.counts;
    let score = counts.overall_score();
    let issues = undocumented_issues(file.metrics.functions.iter().map(|f| (&file.stamp, f)));

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(file.stamp.source_file.clone()));
    entry.push("score_label", GoValue::Str(format_score(score)));
    entry.push("status", GoValue::Str(counts.message().to_string()));
    entry.push("metrics", GoValue::Array(key_metrics(&counts)));
    push_distribution(&mut entry, &counts);
    entry.push("issues", GoValue::Array(issues));
    entry.push("score", GoValue::Float(score));
    GoValue::Map(entry)
}

fn key_metrics(counts: &CommentCounts) -> Vec<GoValue> {
    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    vec![
        metric(
            "Total Comments",
            cf_reportutil::format_int(counts.total_comments),
        ),
        metric(
            "Good Comments",
            cf_reportutil::format_int(counts.good_comments),
        ),
        metric(
            "Bad Comments",
            cf_reportutil::format_int(counts.bad_comments),
        ),
        metric(
            "Doc Coverage",
            format_percent(counts.documentation_coverage()),
        ),
        metric("Good Ratio", format_percent(counts.overall_score())),
        metric(
            "Total Functions",
            cf_reportutil::format_int(counts.total_functions),
        ),
    ]
}

/// The Documented / Undocumented distribution; absent without declarations.
fn push_distribution(section: &mut GoMap, counts: &CommentCounts) {
    let total = counts.total_functions;
    if total == 0 {
        return;
    }
    let dist = |label: &str, count: i64| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("percent", GoValue::Float(count as f64 / total as f64));
        m.push("count", GoValue::Int(count));
        GoValue::Map(m)
    };
    let documented = counts.documented_functions;
    section.push(
        "distribution",
        GoValue::Array(vec![
            dist("Documented", documented),
            dist("Undocumented", total - documented),
        ]),
    );
}

/// One issue per undocumented declaration, sorted by name, then file, then
/// line.
fn undocumented_issues<'a>(
    functions: impl Iterator<Item = (&'a FileStamp, &'a FunctionInfo)>,
) -> Vec<GoValue> {
    let mut undocumented: Vec<(&FileStamp, &FunctionInfo)> =
        functions.filter(|(_, f)| !f.is_documented()).collect();
    undocumented.sort_by_key(|(stamp, f)| (f.name.clone(), stamp.source_file.clone(), f.line));
    undocumented
        .into_iter()
        .map(|(stamp, f)| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(f.name.clone()));
            m.push("location", GoValue::Str(stamp.source_file.clone()));
            m.push("value", GoValue::Str(ISSUE_VALUE.to_string()));
            m.push("severity", GoValue::Str(ISSUE_SEVERITY.to_string()));
            GoValue::Map(m)
        })
        .collect()
}

// --- raw report (plot / report.json) -------------------------------------------

/// Builds the aggregated raw report value for `static/comments` — what
/// `--format plot` consumes: the counters and ratios, the quality message,
/// and the stamped `comments` / `functions` rows of every file (`functions`
/// only when there is at least one declaration).
#[must_use]
pub fn comments_raw_report_value(root_path: &str, filter: &StaticFilter) -> Option<GoValue> {
    let agg = aggregate(root_path, filter);
    let counts = agg.counts();

    let comments: Vec<GoValue> = agg
        .comments()
        .map(|(stamp, c)| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("line", GoValue::Int(c.line));
            m.push("comment", GoValue::Str(comment_preview(&c.text)));
            m.push("placement", GoValue::Str(c.placement.as_str().to_string()));
            m.push("target", GoValue::Str(c.target_name.clone()));
            m.push("assessment", GoValue::Str(c.assessment().to_string()));
            push_stamps(&mut m, stamp);
            GoValue::Map(m)
        })
        .collect();
    let functions: Vec<GoValue> = agg
        .functions()
        .map(|(stamp, f)| {
            let mut m = GoMap::new(MapOrigin::Map);
            m.push("function", GoValue::Str(f.name.clone()));
            m.push("type", GoValue::Str(f.kind.clone()));
            m.push("line", GoValue::Int(f.line));
            m.push("lines", GoValue::Int(f.lines));
            m.push("doc_line", GoValue::Int(f.doc_line.unwrap_or(0)));
            m.push("assessment", GoValue::Str(f.assessment().to_string()));
            push_stamps(&mut m, stamp);
            GoValue::Map(m)
        })
        .collect();

    let mut m = GoMap::new(MapOrigin::Map);
    m.push("analyzer_name", GoValue::Str("comments".to_string()));
    m.push("comments", GoValue::Array(comments));
    if !functions.is_empty() {
        m.push("functions", GoValue::Array(functions));
    }
    m.push("message", GoValue::Str(counts.message().to_string()));
    push_counts(&mut m, &counts);
    Some(GoValue::Map(m))
}

/// Pushes the file stamp keys of a row (map-origin: keys byte-sort at encode
/// time).
fn push_stamps(m: &mut GoMap, stamp: &FileStamp) {
    m.push("_source_file", GoValue::Str(stamp.source_file.clone()));
    if !stamp.language.is_empty() {
        m.push("_language", GoValue::Str(stamp.language.clone()));
    }
    if !stamp.directory.is_empty() {
        m.push("_directory", GoValue::Str(stamp.directory.clone()));
    }
}

/// Pushes the counters and their ratios.
fn push_counts(m: &mut GoMap, counts: &CommentCounts) {
    m.push("total_comments", GoValue::Int(counts.total_comments));
    m.push("good_comments", GoValue::Int(counts.good_comments));
    m.push("bad_comments", GoValue::Int(counts.bad_comments));
    m.push("total_functions", GoValue::Int(counts.total_functions));
    m.push(
        "documented_functions",
        GoValue::Int(counts.documented_functions),
    );
    m.push("overall_score", GoValue::Float(counts.overall_score()));
    m.push(
        "good_comments_ratio",
        GoValue::Float(counts.overall_score()),
    );
    m.push(
        "documentation_coverage",
        GoValue::Float(counts.documentation_coverage()),
    );
}

// --- computed metrics (yaml / bin) --------------------------------------------

/// Builds the computed-metrics value (struct-origin: declaration key order):
/// `comment_quality` (one row per comment), `function_documentation` (one
/// row per declaration), `undocumented_functions` (the undocumented subset
/// with a length-based risk level) and the `aggregate` counters. Rows are
/// sorted by file, then line.
fn computed_metrics(agg: &Aggregator) -> GoValue {
    let mut comments: Vec<(&FileStamp, &CommentDetail)> = agg.comments().collect();
    comments.sort_by_key(|(stamp, c)| (stamp.source_file.clone(), c.line));
    let comment_quality: Vec<GoValue> = comments
        .into_iter()
        .map(|(stamp, c)| comment_quality_row(stamp, c))
        .collect();

    let mut functions: Vec<(&FileStamp, &FunctionInfo)> = agg.functions().collect();
    functions.sort_by_key(|(stamp, f)| (stamp.source_file.clone(), f.line));
    let function_documentation: Vec<GoValue> = functions
        .iter()
        .map(|(stamp, f)| function_documentation_row(stamp, f))
        .collect();
    let undocumented_functions: Vec<GoValue> = functions
        .iter()
        .filter(|(_, f)| !f.is_documented())
        .map(|(stamp, f)| undocumented_row(stamp, f))
        .collect();

    let counts = agg.counts();
    let mut aggregate = GoMap::new(MapOrigin::Struct);
    aggregate.push("total_comments", GoValue::Int(counts.total_comments));
    aggregate.push("good_comments", GoValue::Int(counts.good_comments));
    aggregate.push("bad_comments", GoValue::Int(counts.bad_comments));
    aggregate.push("overall_score", GoValue::Float(counts.overall_score()));
    aggregate.push("total_functions", GoValue::Int(counts.total_functions));
    aggregate.push(
        "documented_functions",
        GoValue::Int(counts.documented_functions),
    );
    aggregate.push(
        "good_comments_ratio",
        GoValue::Float(counts.overall_score()),
    );
    aggregate.push(
        "documentation_coverage",
        GoValue::Float(counts.documentation_coverage()),
    );
    aggregate.push(
        "health_score",
        GoValue::Float(counts.overall_score() * 100.0),
    );
    aggregate.push("message", GoValue::Str(counts.message().to_string()));

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

fn comment_quality_row(stamp: &FileStamp, c: &CommentDetail) -> GoValue {
    let quality = if c.is_good() { "good" } else { "bad" };
    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("line_number", GoValue::Int(c.line));
    push_location(&mut m, stamp);
    m.push("quality", GoValue::Str(quality.to_string()));
    m.push("placement", GoValue::Str(c.placement.as_str().to_string()));
    m.push("target_type", GoValue::Str(c.target_type.clone()));
    m.push("target_name", GoValue::Str(c.target_name.clone()));
    m.push("score", GoValue::Float(c.score));
    GoValue::Map(m)
}

fn function_documentation_row(stamp: &FileStamp, f: &FunctionInfo) -> GoValue {
    let status = if f.is_documented() {
        "Documented"
    } else {
        "Undocumented"
    };
    let mut m = GoMap::new(MapOrigin::Struct);
    push_identity(&mut m, f);
    push_location(&mut m, stamp);
    m.push("is_documented", GoValue::Bool(f.is_documented()));
    m.push("doc_line", GoValue::Int(f.doc_line.unwrap_or(0)));
    m.push("status", GoValue::Str(status.to_string()));
    GoValue::Map(m)
}

fn undocumented_row(stamp: &FileStamp, f: &FunctionInfo) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Struct);
    push_identity(&mut m, f);
    push_location(&mut m, stamp);
    m.push("risk_level", GoValue::Str(risk_level(f.lines).to_string()));
    GoValue::Map(m)
}

fn push_identity(m: &mut GoMap, f: &FunctionInfo) {
    m.push("name", GoValue::Str(f.name.clone()));
    m.push("type", GoValue::Str(f.kind.clone()));
    m.push("line", GoValue::Int(f.line));
    m.push("lines", GoValue::Int(f.lines));
}

fn push_location(m: &mut GoMap, stamp: &FileStamp) {
    m.push("source_file", GoValue::Str(stamp.source_file.clone()));
    m.push("language", GoValue::Str(stamp.language.clone()));
    m.push("directory", GoValue::Str(stamp.directory.clone()));
}

/// Documentation risk of an undocumented declaration, by its length.
fn risk_level(lines: i64) -> &'static str {
    if lines >= RISK_HIGH_MIN_LINES {
        "HIGH"
    } else if lines >= RISK_MEDIUM_MIN_LINES {
        "MEDIUM"
    } else {
        "LOW"
    }
}

/// `round(score*10)/10` → `"N/10"`.
fn format_score(score: f64) -> String {
    let scaled = (score * 10.0).round() as i64;
    format!("{scaled}/10")
}

/// `"%.1f%%"` of `v*100`.
fn format_percent(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

// --- folder walk + per-file analysis ------------------------------------------

/// Runs the static pipeline over `root_path` through the shared static walk
/// and returns the cross-file aggregation. A nonexistent root analyzes zero
/// files (the empty report).
fn aggregate(root_path: &str, filter: &StaticFilter) -> Aggregator {
    let mut agg = Aggregator::new();
    let parser = Parser::new();
    let Ok(walk) = super::walk_source_files(root_path, &parser, filter) else {
        return agg;
    };
    let analyzer = cf_comments::Analyzer::new();
    for file in walk {
        let stamp = file_stamp(&file);
        let Ok(node) = parser.parse(&file.rel_path, &file.bytes) else {
            // A supported extension without a usable grammar (markdown):
            // listed as an empty file, never a skip.
            agg.add_empty(stamp);
            continue;
        };
        match analyzer.analyze_metrics(Some(&node), None) {
            Ok(metrics) => agg.add(stamp, metrics),
            Err(_) => agg.add_empty(stamp),
        }
    }
    agg
}

/// The file's stamp: root-relative path, its directory and its language.
fn file_stamp(file: &SourceFile) -> FileStamp {
    FileStamp {
        directory: file.directory(),
        language: file.language.clone(),
        source_file: file.rel_path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One documented (`add`) and one undocumented (`sub`) function.
    const A_GO: &str = "package main\n\n// add sums two ints.\nfunc add(a, b int) int {\n\treturn a + b\n}\n\nfunc sub(a, b int) int {\n\treturn a - b\n}\n";
    /// One documented function out of four.
    const B_GO: &str = "package main\n\n// g does g.\nfunc g() int {\n\treturn 1\n}\n\nfunc h() int {\n\treturn 2\n}\n\nfunc i() int {\n\treturn 3\n}\n\nfunc j() int {\n\treturn 4\n}\n";
    /// A parser-supported file with neither declarations nor comments.
    const README_MD: &str = "# Title\n\nSome text.\n";
    /// A declaration-free, comment-free Go file.
    const EMPTY_GO: &str = "package main\n\nvar version = \"1\"\n";

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) {
        std::fs::write(dir.path().join(name), content).expect("fixture file");
    }

    /// Fixture: `a.go` (1/2 documented) and a declaration-free `empty.go`.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir, "a.go", A_GO);
        write(&dir, "empty.go", EMPTY_GO);
        dir
    }

    fn root(dir: &tempfile::TempDir) -> &str {
        dir.path().to_str().expect("utf-8 tempdir")
    }

    fn json(dir: &tempfile::TempDir, per_file: bool) -> String {
        let bytes = comments_report_json_flags(root(dir), &StaticFilter::default(), per_file)
            .expect("json report");
        String::from_utf8(bytes).expect("utf-8 json")
    }

    fn yaml(dir: &tempfile::TempDir) -> String {
        let bytes = comments_report_yaml(root(dir), &StaticFilter::default()).expect("yaml");
        String::from_utf8(bytes).expect("utf-8 yaml")
    }

    fn metric_value(json: &str, label: &str) -> String {
        let needle = format!("\"label\": \"{label}\",\n          \"value\": \"");
        let start = json
            .find(&needle)
            .unwrap_or_else(|| panic!("{label} missing:\n{json}"))
            + needle.len();
        json[start..].split('"').next().expect("value").to_string()
    }

    #[test]
    fn per_file_flag_emits_files_entries() {
        let dir = fixture();
        let json = json(&dir, true);
        assert!(json.contains("\"files\""), "files key missing:\n{json}");
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "a.go entry missing:\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"empty.go\""),
            "empty.go entry missing:\n{json}"
        );
        // A file with neither comments nor declarations reports the
        // empty-result message.
        assert!(
            json.contains("\"status\": \"No comments found\""),
            "per-file empty-result status missing:\n{json}"
        );
        // The undocumented function surfaces as a located per-file issue.
        assert!(
            json.contains("\"location\": \"a.go\""),
            "per-file issue location missing:\n{json}"
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
        assert!(!json.contains("\"file_path\""));
    }

    #[test]
    fn coverage_and_good_ratio_are_weighted_over_all_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir, "a.go", A_GO);
        write(&dir, "b.go", B_GO);
        write(&dir, "README.md", README_MD);
        let json = json(&dir, false);
        // 2 documented of 6 functions (1/2 + 1/4), never the mean of the
        // per-file ratios (0.5, 0.25, 0) and never diluted by the README.
        assert_eq!(metric_value(&json, "Total Functions"), "6");
        assert_eq!(metric_value(&json, "Doc Coverage"), "33.3%");
        assert_eq!(metric_value(&json, "Good Ratio"), "100.0%");
        assert_eq!(metric_value(&json, "Total Comments"), "2");
        assert!(
            json.contains("\"overall_score\": 1\n"),
            "score follows the weighted good ratio:\n{json}"
        );
    }

    #[test]
    fn zero_comment_file_counts_its_functions() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            &dir,
            "a.go",
            "package main\n\nfunc add() int {\n\treturn 1\n}\n\nfunc sub() int {\n\treturn 2\n}\n",
        );
        let json = json(&dir, false);
        assert_eq!(metric_value(&json, "Total Functions"), "2");
        assert_eq!(metric_value(&json, "Doc Coverage"), "0.0%");
        assert!(json.contains("\"name\": \"add\""), "{json}");
        assert!(json.contains("\"name\": \"sub\""), "{json}");
    }

    #[test]
    fn yaml_rows_carry_real_names_and_status() {
        let dir = fixture();
        let yaml = yaml(&dir);
        for needle in [
            "name: add\n",
            "is_documented: true",
            "status: Documented",
            "name: sub\n",
            "is_documented: false",
            "status: Undocumented",
            "target_name: add",
            "quality: good",
            "placement: above",
            "total_functions: 2",
            "documented_functions: 1",
            "documentation_coverage: 0.5",
        ] {
            assert!(yaml.contains(needle), "yaml lacks {needle:?}:\n{yaml}");
        }
        let undocumented = yaml
            .split("undocumented_functions:")
            .nth(1)
            .and_then(|rest| rest.split("aggregate:").next())
            .expect("undocumented_functions block");
        assert_eq!(undocumented.matches("- name:").count(), 1, "{undocumented}");
        assert!(undocumented.contains("name: sub"), "{undocumented}");
        assert!(undocumented.contains("risk_level: LOW"), "{undocumented}");
    }

    #[test]
    fn risk_level_follows_the_declaration_length() {
        assert_eq!(risk_level(RISK_MEDIUM_MIN_LINES - 1), "LOW");
        assert_eq!(risk_level(RISK_MEDIUM_MIN_LINES), "MEDIUM");
        assert_eq!(risk_level(RISK_HIGH_MIN_LINES), "HIGH");
    }

    #[test]
    fn bin_payload_is_the_yaml_value_in_a_cfb1_envelope() {
        let dir = fixture();
        let bytes = comments_report_bin(root(&dir), &StaticFilter::default()).expect("bin");
        assert_eq!(&bytes[..4], b"CFB1");
        let payload = String::from_utf8_lossy(&bytes[8..]);
        assert!(payload.contains("\"documented_functions\":1"), "{payload}");
        assert!(payload.contains("\"name\":\"sub\""), "{payload}");
    }

    #[test]
    fn file_root_stamps_the_file_name() {
        let dir = fixture();
        let file = dir.path().join("a.go");
        let bytes =
            comments_report_json_flags(file.to_str().unwrap(), &StaticFilter::default(), true)
                .expect("json report");
        let json = String::from_utf8(bytes).expect("utf-8 json");
        assert!(json.contains("\"file_path\": \"a.go\""), "{json}");
        assert!(json.contains("\"location\": \"a.go\""), "{json}");
        assert!(!json.contains("\"file_path\": \".\""), "{json}");
    }

    #[test]
    fn nested_files_are_stamped_root_relative() {
        let dir = fixture();
        std::fs::create_dir_all(dir.path().join("sub")).expect("sub dir");
        std::fs::write(dir.path().join("sub").join("c.go"), A_GO).expect("sub/c.go");
        let json = json(&dir, true);
        assert!(json.contains("\"file_path\": \"sub/c.go\""), "{json}");
        assert!(json.contains("\"location\": \"sub/c.go\""), "{json}");
        assert!(!json.contains("\"../"), "{json}");
    }

    #[test]
    fn empty_directory_yields_the_empty_report() {
        let dir = tempfile::tempdir().expect("tempdir");
        let json = json(&dir, false);
        assert!(json.contains("\"status\": \"No comments found\""), "{json}");
        assert_eq!(metric_value(&json, "Total Functions"), "0");
        assert!(!json.contains("\"distribution\""), "{json}");
        let raw = comments_raw_report_value(root(&dir), &StaticFilter::default()).expect("raw");
        assert_eq!(
            raw.as_map().and_then(|m| m.get("total_functions")),
            Some(&GoValue::Int(0))
        );
        assert!(raw.as_map().and_then(|m| m.get("functions")).is_none());
    }

    #[test]
    fn unparsable_supported_file_is_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir, "README.md", README_MD);
        let json = json(&dir, true);
        assert!(json.contains("\"file_path\": \"README.md\""), "{json}");
        assert!(json.contains("\"status\": \"No comments found\""), "{json}");
        assert_eq!(metric_value(&json, "Total Functions"), "0");
    }

    #[test]
    fn text_summary_shares_the_weighted_metrics() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir, "a.go", A_GO);
        write(&dir, "b.go", B_GO);
        let summary = comments_report_value_summary(root(&dir), &StaticFilter::default())
            .expect("summary value");
        let encoded = cf_gojson::Encoder::indented("  ").encode(&summary);
        let text = String::from_utf8(encoded).expect("utf-8");
        assert_eq!(metric_value(&text, "Doc Coverage"), "33.3%");
        assert!(
            !text.contains("\"name\""),
            "summary carries no issues:\n{text}"
        );
    }

    #[test]
    fn raw_report_rows_are_stamped() {
        let dir = fixture();
        let raw = comments_raw_report_value(root(&dir), &StaticFilter::default()).expect("raw");
        let functions = match raw.as_map().and_then(|m| m.get("functions")) {
            Some(GoValue::Array(rows)) => rows.clone(),
            other => panic!("functions missing: {other:?}"),
        };
        assert_eq!(functions.len(), 2);
        let first = functions[0].as_map().expect("row map");
        assert_eq!(
            first.get("function"),
            Some(&GoValue::Str("add".to_string()))
        );
        assert_eq!(
            first.get("_source_file"),
            Some(&GoValue::Str("a.go".to_string()))
        );
        assert_eq!(
            first.get("_language"),
            Some(&GoValue::Str("go".to_string()))
        );
        assert_eq!(
            first.get("_directory"),
            Some(&GoValue::Str(".".to_string()))
        );
        assert_eq!(first.get("doc_line"), Some(&GoValue::Int(3)));
    }
}
