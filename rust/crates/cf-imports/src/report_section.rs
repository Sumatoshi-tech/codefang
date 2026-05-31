//! Terminal report section for static import analysis.
//!
//! Port of `internal/analyzers/imports/report_section.go`. The [`ReportSection`]
//! is an info-only section (no score): it surfaces a status message, two key
//! metrics (unique imports, total files), and a list of import "issues" ordered
//! by usage count when `import_counts` is present, else alphabetically.
//!
//! The Go section implements the framework `analyze.ReportSection` interface and
//! renders through `renderer.SectionRenderer` (terminal output, which DESIGN §2.7
//! marks non-binding/cosmetic). This module ports the data the section exposes;
//! the actual terminal rendering belongs to the not-yet-ported renderer crate.

use crate::report::ReportValue;

/// Section title. Mirrors Go `SectionTitle`.
pub const SECTION_TITLE: &str = "IMPORTS";
/// Label for the unique-imports metric. Mirrors Go `MetricUniqueImports`.
pub const METRIC_UNIQUE_IMPORTS: &str = "Unique Imports";
/// Label for the total-files metric. Mirrors Go `MetricTotalFiles`.
pub const METRIC_TOTAL_FILES: &str = "Total Files";

/// Report key for the imports list. Mirrors Go `KeyImports`.
pub const KEY_IMPORTS: &str = "imports";
/// Report key for the import count. Mirrors Go `KeyCount`.
pub const KEY_COUNT: &str = "count";
/// Report key for the total file count. Mirrors Go `KeyTotalFiles`.
pub const KEY_TOTAL_FILES: &str = "total_files";
/// Report key for the per-import counts map. Mirrors Go `KeyImportCounts`.
pub const KEY_IMPORT_COUNTS: &str = "import_counts";
/// Report key for the per-file source path. Mirrors Go `analyze.SourceFileKey`.
pub const SOURCE_FILE_KEY: &str = "_source_file";

/// Fallback message when no import data is available. Mirrors Go
/// `DefaultStatusMessage`.
pub const DEFAULT_STATUS_MESSAGE: &str = "No import data available";
const STATUS_MESSAGE_PREFIX: &str = "Found ";
const STATUS_MESSAGE_SUFFIX: &str = " unique imports";

/// Severity label for info issues. Mirrors Go `analyze.SeverityInfo`.
pub const SEVERITY_INFO: &str = "info";

/// A key metric label/value pair. Mirrors Go `analyze.Metric`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metric {
    /// Metric label.
    pub label: String,
    /// Metric value (already formatted as a string, like the Go section).
    pub value: String,
}

/// A reported import "issue" (info item). Mirrors Go `analyze.Issue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The import name.
    pub name: String,
    /// The source file location (empty when unknown).
    pub location: String,
    /// The displayed value (usage count, or `"1"` for list fallback).
    pub value: String,
    /// Severity (always [`SEVERITY_INFO`] here).
    pub severity: String,
}

/// Info-only report section for import analysis.
///
/// Mirrors Go `ReportSection`.
#[derive(Debug, Clone)]
pub struct ReportSection {
    report: ReportValue,
    status_message: String,
}

impl ReportSection {
    /// Builds a section from a report (a `None`/empty report is treated as `{}`).
    ///
    /// Mirrors Go `NewReportSection`.
    pub fn new(report: Option<ReportValue>) -> Self {
        let report = report.unwrap_or_else(ReportValue::map);
        let count = get_int(&report, KEY_COUNT);
        let status_message = build_status_message(count);
        ReportSection {
            report,
            status_message,
        }
    }

    /// Returns the section title. Mirrors Go `(*ReportSection).SectionTitle`.
    pub fn section_title(&self) -> &'static str {
        SECTION_TITLE
    }

    /// Returns the status message. Mirrors Go `StatusMessage`.
    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    /// Returns the key metrics (unique imports, total files).
    ///
    /// Mirrors Go `(*ReportSection).KeyMetrics`.
    pub fn key_metrics(&self) -> Vec<Metric> {
        vec![
            Metric {
                label: METRIC_UNIQUE_IMPORTS.to_string(),
                value: get_int(&self.report, KEY_COUNT).to_string(),
            },
            Metric {
                label: METRIC_TOTAL_FILES.to_string(),
                value: get_int(&self.report, KEY_TOTAL_FILES).to_string(),
            },
        ]
    }

    /// Returns the top `n` most-used imports as info issues.
    ///
    /// Mirrors Go `(*ReportSection).TopIssues`.
    pub fn top_issues(&self, n: usize) -> Vec<Issue> {
        self.import_issues(n)
    }

    /// Returns all imports as info issues.
    ///
    /// Mirrors Go `(*ReportSection).AllIssues`.
    pub fn all_issues(&self) -> Vec<Issue> {
        self.import_issues(0)
    }

    /// Builds import issues sorted by frequency (or name), limited to `limit`
    /// (0 = all). Mirrors Go `(*ReportSection).importIssues`.
    fn import_issues(&self, limit: usize) -> Vec<Issue> {
        let location = get_string(&self.report, SOURCE_FILE_KEY);

        let counts = get_string_int_map(&self.report, KEY_IMPORT_COUNTS);
        if !counts.is_empty() {
            return build_issues_from_counts(&counts, limit, &location);
        }

        let imports = get_string_slice(&self.report, KEY_IMPORTS);
        if imports.is_empty() {
            return Vec::new();
        }
        build_issues_from_list(&imports, limit, &location)
    }
}

/// Builds issues from an `import_counts` map, sorted by count descending.
///
/// Mirrors Go `buildIssuesFromCounts`. Go's `mapx.SortAndLimit` sorts by count
/// desc; ties are resolved here by name ascending for determinism.
fn build_issues_from_counts(
    counts: &std::collections::BTreeMap<String, i64>,
    limit: usize,
    location: &str,
) -> Vec<Issue> {
    let mut entries: Vec<(&String, &i64)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    if limit > 0 && entries.len() > limit {
        entries.truncate(limit);
    }
    entries
        .into_iter()
        .map(|(name, count)| Issue {
            name: name.clone(),
            location: location.to_string(),
            value: count.to_string(),
            severity: SEVERITY_INFO.to_string(),
        })
        .collect()
}

/// Builds issues from a simple imports list, sorted alphabetically.
///
/// Mirrors Go `buildIssuesFromList`.
fn build_issues_from_list(imports: &[String], limit: usize, location: &str) -> Vec<Issue> {
    let mut sorted: Vec<String> = imports.to_vec();
    sorted.sort();
    if limit > 0 && sorted.len() > limit {
        sorted.truncate(limit);
    }
    sorted
        .into_iter()
        .map(|imp| Issue {
            name: imp,
            location: location.to_string(),
            value: "1".to_string(),
            severity: SEVERITY_INFO.to_string(),
        })
        .collect()
}

/// Builds the status message from the import count. Mirrors Go
/// `buildStatusMessage`.
fn build_status_message(count: i64) -> String {
    if count == 0 {
        return DEFAULT_STATUS_MESSAGE.to_string();
    }
    format!("{STATUS_MESSAGE_PREFIX}{count}{STATUS_MESSAGE_SUFFIX}")
}

// --- small reportutil-style accessors (Go internal/.../reportutil) ---

fn get_int(report: &ReportValue, key: &str) -> i64 {
    match report.as_map().and_then(|m| m.get(key)) {
        Some(ReportValue::Int(n)) => *n,
        Some(ReportValue::Float(f)) => *f as i64,
        _ => 0,
    }
}

fn get_string(report: &ReportValue, key: &str) -> String {
    match report.as_map().and_then(|m| m.get(key)) {
        Some(ReportValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn get_string_slice(report: &ReportValue, key: &str) -> Vec<String> {
    match report.as_map().and_then(|m| m.get(key)) {
        Some(ReportValue::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                ReportValue::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn get_string_int_map(report: &ReportValue, key: &str) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(ReportValue::Map(m)) = report.as_map().and_then(|m| m.get(key)) {
        for (k, v) in m {
            match v {
                ReportValue::Int(n) => {
                    out.insert(k.clone(), *n);
                }
                ReportValue::Float(f) => {
                    out.insert(k.clone(), *f as i64);
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ported from report_section_test.go ---

    fn test_imports_report() -> ReportValue {
        let mut r = ReportValue::map();
        r.insert(
            KEY_IMPORTS,
            ReportValue::List(
                ["os", "fmt", "strings", "errors"]
                    .iter()
                    .map(|s| ReportValue::Str(s.to_string()))
                    .collect(),
            ),
        );
        r.insert(KEY_COUNT, ReportValue::Int(4));
        r.insert(KEY_TOTAL_FILES, ReportValue::Int(10));
        let mut counts = ReportValue::map();
        counts.insert("os", ReportValue::Int(8));
        counts.insert("fmt", ReportValue::Int(12));
        counts.insert("strings", ReportValue::Int(3));
        counts.insert("errors", ReportValue::Int(5));
        r.insert(KEY_IMPORT_COUNTS, counts);
        r
    }

    fn simple_imports_report() -> ReportValue {
        let mut r = ReportValue::map();
        r.insert(
            KEY_IMPORTS,
            ReportValue::List(vec![
                ReportValue::Str("os".to_string()),
                ReportValue::Str("fmt".to_string()),
            ]),
        );
        r.insert(KEY_COUNT, ReportValue::Int(2));
        r
    }

    #[test]
    fn test_imports_title() {
        let s = ReportSection::new(Some(test_imports_report()));
        assert_eq!(s.section_title(), SECTION_TITLE);
    }

    #[test]
    fn test_imports_nil_report() {
        let s = ReportSection::new(None);
        assert_eq!(s.section_title(), SECTION_TITLE);
    }

    #[test]
    fn test_imports_status_message() {
        let s = ReportSection::new(Some(test_imports_report()));
        assert_eq!(s.status_message(), "Found 4 unique imports");
    }

    #[test]
    fn test_imports_status_message_empty() {
        let s = ReportSection::new(Some(ReportValue::map()));
        assert_eq!(s.status_message(), DEFAULT_STATUS_MESSAGE);
    }

    #[test]
    fn test_imports_key_metrics_count() {
        let s = ReportSection::new(Some(test_imports_report()));
        assert_eq!(s.key_metrics().len(), 2);
    }

    #[test]
    fn test_imports_key_metrics_labels() {
        let s = ReportSection::new(Some(test_imports_report()));
        let m = s.key_metrics();
        assert_eq!(m[0].label, METRIC_UNIQUE_IMPORTS);
        assert_eq!(m[1].label, METRIC_TOTAL_FILES);
    }

    #[test]
    fn test_imports_key_metrics_values() {
        let s = ReportSection::new(Some(test_imports_report()));
        let m = s.key_metrics();
        assert_eq!(m[0].value, "4");
        assert_eq!(m[1].value, "10");
    }

    #[test]
    fn test_imports_top_issues_from_counts() {
        let s = ReportSection::new(Some(test_imports_report()));
        let issues = s.top_issues(2);
        assert_eq!(issues.len(), 2);
        // Sorted by count desc: fmt(12) first.
        assert_eq!(issues[0].name, "fmt");
        assert_eq!(issues[0].severity, SEVERITY_INFO);
    }

    #[test]
    fn test_imports_top_issues_from_list() {
        let s = ReportSection::new(Some(simple_imports_report()));
        let issues = s.top_issues(2);
        assert_eq!(issues.len(), 2);
        // Alphabetical: fmt, os.
        assert_eq!(issues[0].name, "fmt");
    }

    #[test]
    fn test_imports_all_issues() {
        let s = ReportSection::new(Some(test_imports_report()));
        assert_eq!(s.all_issues().len(), 4);
    }

    #[test]
    fn test_imports_top_issues_empty() {
        let s = ReportSection::new(Some(ReportValue::map()));
        assert_eq!(s.top_issues(5).len(), 0);
    }

    #[test]
    fn test_imports_per_file_issues_have_location() {
        let mut r = ReportValue::map();
        r.insert(
            KEY_IMPORTS,
            ReportValue::List(vec![
                ReportValue::Str("fmt".to_string()),
                ReportValue::Str("os".to_string()),
            ]),
        );
        r.insert(KEY_COUNT, ReportValue::Int(2));
        let mut counts = ReportValue::map();
        counts.insert("fmt", ReportValue::Int(1));
        counts.insert("os", ReportValue::Int(1));
        r.insert(KEY_IMPORT_COUNTS, counts);
        r.insert(SOURCE_FILE_KEY, ReportValue::Str("/repo/pkg/foo.go".to_string()));

        let s = ReportSection::new(Some(r));
        let issues = s.all_issues();
        assert!(!issues.is_empty());
        for issue in &issues {
            assert_eq!(issue.location, "/repo/pkg/foo.go");
        }
    }

    #[test]
    fn test_imports_per_file_no_source_file_empty_location() {
        let s = ReportSection::new(Some(test_imports_report()));
        let issues = s.all_issues();
        assert!(!issues.is_empty());
        for issue in &issues {
            assert_eq!(issue.location, "");
        }
    }
}
