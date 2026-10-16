//! Composition report section: the human-facing section model (title,
//! status, key metrics, distribution, issues), its `renderer.JSONReport`
//! value (`--format json`, the source of the `text` / `compact` renders) and
//! the `--per-file` entries.

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::aggregator::CompositionReport;
use crate::category::{Category, FileClass, ALL_FILE_CLASSES};

/// Section title shown in rendered output.
pub const SECTION_TITLE: &str = "COMPOSITION";
/// Metric label: total files.
pub const METRIC_TOTAL_FILES: &str = "Total Files";
/// Metric label: source files.
pub const METRIC_SOURCE: &str = "Source Files";
/// Metric label: source share.
pub const METRIC_SOURCE_PCT: &str = "Source %";
/// Per-file metric label: the file's enry language.
pub const METRIC_LANGUAGE: &str = "Language";
/// Per-file language value when enry recognizes none.
pub const LANGUAGE_UNKNOWN: &str = "unknown";

/// Status message when at least one file was analyzed.
pub const STATUS_DEFAULT: &str = "File composition analysis completed";
/// Status message when no files were analyzed.
pub const STATUS_EMPTY: &str = "No files analyzed";
/// Per-file status prefix: `Classified as <class>`.
pub const STATUS_CLASSIFIED_PREFIX: &str = "Classified as ";

/// Score sentinel meaning "informational only" (no pass/fail grade).
pub const SCORE_INFO_ONLY: f64 = -1.0;
/// Score label of an info-only section.
pub const SCORE_LABEL_INFO: &str = "Info";

/// Severity string for informational issues.
pub const SEVERITY_INFO: &str = "info";
/// Severity for categories considered problematic (binary files).
pub const SEVERITY_POOR: &str = "poor";

/// A single key/value metric line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metric {
    /// Metric label.
    pub label: String,
    /// Rendered metric value.
    pub value: String,
}

/// A class distribution item.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionItem {
    /// Class label (wire string).
    pub label: String,
    /// Share of total files as a fraction in `[0, 1]` (the renderer
    /// contract shared by every analyzer; the formatted strings carry `%`).
    pub percent: f64,
    /// Absolute file count.
    pub count: i64,
}

/// A reported issue (non-source class breakdown line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Class name (wire string).
    pub name: String,
    /// Human-formatted `<n> files (<pct>%)` value.
    pub value: String,
    /// Severity string.
    pub severity: String,
}

/// One classified file (the `--per-file` entry source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    /// Path relative to the analyzed root.
    pub path: String,
    /// The file's class.
    pub class: FileClass,
    /// The file's enry language, if recognized.
    pub language: Option<String>,
}

/// The composition report section.
#[derive(Debug, Clone, Copy)]
pub struct ReportSection {
    report: CompositionReport,
}

impl ReportSection {
    /// Builds a section from an aggregated composition report.
    #[must_use]
    pub fn new(report: CompositionReport) -> Self {
        Self { report }
    }

    /// The section title.
    #[must_use]
    pub fn title(&self) -> &'static str {
        SECTION_TITLE
    }

    /// [`STATUS_EMPTY`] when no file was analyzed, else [`STATUS_DEFAULT`].
    #[must_use]
    pub fn status_message(&self) -> &'static str {
        if self.report.total_files == 0 {
            STATUS_EMPTY
        } else {
            STATUS_DEFAULT
        }
    }

    /// Always [`SCORE_INFO_ONLY`]: composition is informational.
    #[must_use]
    pub fn score(&self) -> f64 {
        SCORE_INFO_ONLY
    }

    /// Ordered key metrics: total files, source-file count, source share.
    #[must_use]
    pub fn key_metrics(&self) -> Vec<Metric> {
        let source = FileClass::Category(Category::Source);
        vec![
            metric(METRIC_TOTAL_FILES, self.report.total_files.to_string()),
            metric(METRIC_SOURCE, self.report.count(source).to_string()),
            metric(
                METRIC_SOURCE_PCT,
                format_percent(self.report.percent(source)),
            ),
        ]
    }

    /// The class breakdown in [`ALL_FILE_CLASSES`] order, omitting
    /// zero-count classes (empty when no file was analyzed).
    #[must_use]
    pub fn distribution(&self) -> Vec<DistributionItem> {
        ALL_FILE_CLASSES
            .into_iter()
            .filter(|c| self.report.count(*c) > 0)
            .map(|c| DistributionItem {
                label: c.as_str().to_string(),
                percent: self.report.fraction(c),
                count: self.report.count(c),
            })
            .collect()
    }

    /// Every non-source class with files, as `<count> files (<pct>%)`
    /// issues in [`ALL_FILE_CLASSES`] order.
    #[must_use]
    pub fn issues(&self) -> Vec<Issue> {
        ALL_FILE_CLASSES
            .into_iter()
            .filter(|c| *c != FileClass::Category(Category::Source) && self.report.count(*c) > 0)
            .map(|c| Issue {
                name: c.as_str().to_string(),
                value: format!(
                    "{} files ({})",
                    self.report.count(c),
                    format_percent(self.report.percent(c))
                ),
                severity: severity_for(c).to_string(),
            })
            .collect()
    }
}

/// Severity of a class: binary files are [`SEVERITY_POOR`], everything else
/// [`SEVERITY_INFO`].
#[must_use]
pub fn severity_for(class: FileClass) -> &'static str {
    match class {
        FileClass::Category(Category::Binary) => SEVERITY_POOR,
        _ => SEVERITY_INFO,
    }
}

fn metric(label: &str, value: String) -> Metric {
    Metric {
        label: label.to_string(),
        value,
    }
}

/// Formats a `0..100` percentage to one decimal place with a trailing `%`.
fn format_percent(value: f64) -> String {
    format!("{value:.1}%")
}

/// Builds the `renderer.JSONReport` value for the single composition
/// section: `{overall_score_label, sections: [section], overall_score}`,
/// where the section is `{title, score_label, status, metrics,
/// distribution?, issues, files?, score}`. `files` adds one [`file_entry`]
/// per record (the `--per-file` enrichment).
#[must_use]
pub fn json_value(report: CompositionReport, files: Option<&[FileRecord]>) -> GoValue {
    let section = ReportSection::new(report);

    let mut sect = GoMap::new(MapOrigin::Struct);
    sect.push("title", GoValue::Str(section.title().to_string()));
    sect.push("score_label", GoValue::Str(SCORE_LABEL_INFO.to_string()));
    sect.push("status", GoValue::Str(section.status_message().to_string()));
    sect.push("metrics", metrics_value(&section.key_metrics()));
    let distribution = section.distribution();
    if !distribution.is_empty() {
        sect.push("distribution", distribution_value(&distribution));
    }
    sect.push("issues", issues_value(&section.issues()));
    if let Some(records) = files {
        sect.push(
            "files",
            GoValue::Array(records.iter().map(file_entry).collect()),
        );
    }
    sect.push("score", GoValue::Float(section.score()));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "overall_score_label",
        GoValue::Str(SCORE_LABEL_INFO.to_string()),
    );
    root.push("sections", GoValue::Array(vec![GoValue::Map(sect)]));
    root.push("overall_score", GoValue::Float(SCORE_INFO_ONLY));
    GoValue::Map(root)
}

/// Builds one `--per-file` entry: the file's class as its status
/// (`Classified as vendor`), a one-file breakdown (`Total Files 1`,
/// `<Class> Files 1`, `<Class> % 100.0%`), the enry language, and a
/// single-row distribution.
#[must_use]
pub fn file_entry(record: &FileRecord) -> GoValue {
    let class = record.class;
    let metrics = vec![
        metric(METRIC_TOTAL_FILES, "1".to_string()),
        metric(&format!("{} Files", class.display_name()), "1".to_string()),
        metric(
            &format!("{} %", class.display_name()),
            format_percent(100.0),
        ),
        metric(
            METRIC_LANGUAGE,
            record
                .language
                .clone()
                .unwrap_or_else(|| LANGUAGE_UNKNOWN.to_string()),
        ),
    ];
    let distribution = vec![DistributionItem {
        label: class.as_str().to_string(),
        percent: 1.0,
        count: 1,
    }];

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(record.path.clone()));
    entry.push("score_label", GoValue::Str(SCORE_LABEL_INFO.to_string()));
    entry.push(
        "status",
        GoValue::Str(format!("{STATUS_CLASSIFIED_PREFIX}{}", class.as_str())),
    );
    entry.push("metrics", metrics_value(&metrics));
    entry.push("distribution", distribution_value(&distribution));
    entry.push("issues", GoValue::Array(Vec::new()));
    entry.push("score", GoValue::Float(SCORE_INFO_ONLY));
    GoValue::Map(entry)
}

fn metrics_value(metrics: &[Metric]) -> GoValue {
    GoValue::Array(
        metrics
            .iter()
            .map(|m| {
                let mut sm = GoMap::new(MapOrigin::Struct);
                sm.push("label", GoValue::Str(m.label.clone()));
                sm.push("value", GoValue::Str(m.value.clone()));
                GoValue::Map(sm)
            })
            .collect(),
    )
}

fn distribution_value(rows: &[DistributionItem]) -> GoValue {
    GoValue::Array(
        rows.iter()
            .map(|d| {
                let mut sd = GoMap::new(MapOrigin::Struct);
                sd.push("label", GoValue::Str(d.label.clone()));
                sd.push("percent", GoValue::Float(d.percent));
                sd.push("count", GoValue::Int(d.count));
                GoValue::Map(sd)
            })
            .collect(),
    )
}

fn issues_value(issues: &[Issue]) -> GoValue {
    GoValue::Array(
        issues
            .iter()
            .map(|i| {
                let mut si = GoMap::new(MapOrigin::Struct);
                si.push("name", GoValue::Str(i.name.clone()));
                si.push("location", GoValue::Str(String::new()));
                si.push("value", GoValue::Str(i.value.clone()));
                si.push("severity", GoValue::Str(i.severity.clone()));
                GoValue::Map(si)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator::Aggregator;
    use cf_gojson::Encoder;

    fn sample() -> CompositionReport {
        let mut agg = Aggregator::new();
        for class in [
            Category::Source.into(),
            Category::Source.into(),
            Category::Source.into(),
            Category::Source.into(),
            Category::Source.into(),
            Category::Source.into(),
            Category::Vendor.into(),
            Category::Vendor.into(),
            Category::Documentation.into(),
            Category::Binary.into(),
        ] {
            agg.add(class);
        }
        agg.report()
    }

    #[test]
    fn section_title_status_and_score() {
        let s = ReportSection::new(sample());
        assert_eq!(s.title(), SECTION_TITLE);
        assert_eq!(s.status_message(), STATUS_DEFAULT);
        assert!((s.score() - SCORE_INFO_ONLY).abs() < 1e-12);
        let empty = ReportSection::new(CompositionReport::default());
        assert_eq!(empty.status_message(), STATUS_EMPTY);
        assert!(empty.distribution().is_empty());
        assert!(empty.issues().is_empty());
    }

    #[test]
    fn key_metrics() {
        let m = ReportSection::new(sample()).key_metrics();
        assert_eq!(m[0], metric(METRIC_TOTAL_FILES, "10".into()));
        assert_eq!(m[1], metric(METRIC_SOURCE, "6".into()));
        assert_eq!(m[2], metric(METRIC_SOURCE_PCT, "60.0%".into()));
    }

    #[test]
    fn distribution_follows_class_order_and_skips_zero() {
        let d = ReportSection::new(sample()).distribution();
        let labels: Vec<&str> = d.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["source", "documentation", "vendor", "binary"]);
        assert!((d[0].percent - 0.6).abs() < 1e-12);
        assert_eq!(d[2].count, 2);
    }

    #[test]
    fn issues_list_non_source_classes_with_severity() {
        let issues = ReportSection::new(sample()).issues();
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].name, "documentation");
        assert_eq!(issues[0].value, "1 files (10.0%)");
        assert_eq!(issues[0].severity, SEVERITY_INFO);
        assert_eq!(issues[2].name, "binary");
        assert_eq!(issues[2].severity, SEVERITY_POOR);
    }

    #[test]
    fn json_value_shape() {
        let json = Encoder::marshal().encode_to_string(&json_value(sample(), None));
        assert!(json.starts_with(r#"{"overall_score_label":"Info","sections":[{"title":"COMPOSITION","score_label":"Info","status":"File composition analysis completed","metrics":[{"label":"Total Files","value":"10"},{"label":"Source Files","value":"6"},{"label":"Source %","value":"60.0%"}],"distribution":[{"label":"source","percent":0.6,"count":6}"#), "{json}");
        assert!(json.contains(r#""issues":[{"name":"documentation","location":"","value":"1 files (10.0%)","severity":"info"}"#), "{json}");
        assert!(!json.contains("\"files\""));
        assert!(
            json.ends_with(r#""score":-1}],"overall_score":-1}"#),
            "{json}"
        );
    }

    #[test]
    fn empty_json_omits_distribution() {
        let json =
            Encoder::marshal().encode_to_string(&json_value(CompositionReport::default(), None));
        assert!(!json.contains("distribution"));
        assert!(json.contains(r#""status":"No files analyzed""#));
        assert!(json.contains(r#""issues":[]"#));
    }

    #[test]
    fn per_file_entry_carries_class_and_language() {
        let record = FileRecord {
            path: "vendor/x/y.go".into(),
            class: Category::Vendor.into(),
            language: Some("Go".into()),
        };
        let json = Encoder::marshal().encode_to_string(&file_entry(&record));
        assert_eq!(
            json,
            r#"{"file_path":"vendor/x/y.go","score_label":"Info","status":"Classified as vendor","metrics":[{"label":"Total Files","value":"1"},{"label":"Vendor Files","value":"1"},{"label":"Vendor %","value":"100.0%"},{"label":"Language","value":"Go"}],"distribution":[{"label":"vendor","percent":1,"count":1}],"issues":[],"score":-1}"#
        );
        let other = FileRecord {
            path: "data.xyz".into(),
            class: FileClass::Other,
            language: None,
        };
        let json = Encoder::marshal().encode_to_string(&file_entry(&other));
        assert!(json.contains(r#""status":"Classified as other""#));
        assert!(json.contains(r#"{"label":"Other Files","value":"1"}"#));
        assert!(json.contains(r#"{"label":"Language","value":"unknown"}"#));
    }

    #[test]
    fn json_value_with_files_lists_every_record() {
        let records = vec![
            FileRecord {
                path: "a.go".into(),
                class: Category::Source.into(),
                language: Some("Go".into()),
            },
            FileRecord {
                path: "b.xyz".into(),
                class: FileClass::Other,
                language: None,
            },
        ];
        let json = Encoder::marshal().encode_to_string(&json_value(sample(), Some(&records)));
        assert!(json.contains(r#""files":[{"file_path":"a.go""#), "{json}");
        assert!(json.contains(r#"{"file_path":"b.xyz""#), "{json}");
        // `files` sits between `issues` and `score`.
        let files_at = json.find("\"files\"").unwrap();
        let issues_at = json.find("\"issues\"").unwrap();
        assert!(issues_at < files_at);
        assert!(json.ends_with(r#""score":-1}],"overall_score":-1}"#));
    }
}
