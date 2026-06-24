//! Structured JSON output model.
//!
//! These structs are the machine model for the static-analysis report. Two
//! report-format behaviors are load-bearing here (pinned by
//! `tests/compat`):
//!
//! 1. **Score-last field ordering.** `score` / `overall_score` is always the
//!    *last* field emitted in JSON. The
//!    [`to_go_value`](JsonSection::to_go_value)-family methods preserve that
//!    declaration order via [`GoValue::Object`](crate::gocompat::GoValue::Object).
//! 2. **Initialized-empty `[]` vs `omitempty`.** `metrics` and `issues` are
//!    always-present arrays (empty rather than `null`), while `distribution`
//!    and `files` are omitted when empty/absent. This is reproduced by always
//!    pushing `metrics`/`issues` entries and conditionally pushing
//!    `distribution`/`files`.
//!
//! Serialization routes through the byte-compatible
//! [`gocompat`](crate::gocompat) encoder, never `serde_json`, so the bytes
//! match the report-format contract.

use crate::analyze::ReportSection;
use crate::gocompat::{Encoder, GoValue};
use crate::summary::ExecutiveSummary;

/// One key-value metric in JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JsonMetric {
    /// Metric label.
    pub label: String,
    /// Metric value (pre-formatted).
    pub value: String,
}

impl JsonMetric {
    fn to_go_value(&self) -> GoValue {
        GoValue::Object(vec![
            ("label".to_string(), GoValue::Str(self.label.clone())),
            ("value".to_string(), GoValue::Str(self.value.clone())),
        ])
    }
}

/// One distribution category in JSON output.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonDistribution {
    /// Category label.
    pub label: String,
    /// Percentage as `0..1`.
    pub percent: f64,
    /// Absolute count.
    pub count: i64,
}

impl JsonDistribution {
    fn to_go_value(&self) -> GoValue {
        GoValue::Object(vec![
            ("label".to_string(), GoValue::Str(self.label.clone())),
            ("percent".to_string(), GoValue::Float(self.percent)),
            ("count".to_string(), GoValue::Int(self.count)),
        ])
    }
}

/// One issue in JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JsonIssue {
    /// Item name.
    pub name: String,
    /// File location.
    pub location: String,
    /// Metric value.
    pub value: String,
    /// Severity string.
    pub severity: String,
}

impl JsonIssue {
    fn to_go_value(&self) -> GoValue {
        GoValue::Object(vec![
            ("name".to_string(), GoValue::Str(self.name.clone())),
            ("location".to_string(), GoValue::Str(self.location.clone())),
            ("value".to_string(), GoValue::Str(self.value.clone())),
            ("severity".to_string(), GoValue::Str(self.severity.clone())),
        ])
    }
}

fn metrics_array(metrics: &[JsonMetric]) -> GoValue {
    GoValue::Array(metrics.iter().map(JsonMetric::to_go_value).collect())
}

fn issues_array(issues: &[JsonIssue]) -> GoValue {
    GoValue::Array(issues.iter().map(JsonIssue::to_go_value).collect())
}

fn distribution_array(dist: &[JsonDistribution]) -> GoValue {
    GoValue::Array(dist.iter().map(JsonDistribution::to_go_value).collect())
}

/// One file's analysis results within a section.
///
/// Field order (and thus JSON order): `file_path`, `score_label`, `status`,
/// `metrics`, `distribution` (omitempty), `issues`, `score` (last).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonFileEntry {
    /// Relative file path.
    pub file_path: String,
    /// Formatted score label.
    pub score_label: String,
    /// Status message.
    pub status: String,
    /// Always-present metrics array.
    pub metrics: Vec<JsonMetric>,
    /// Distribution categories (omitted from JSON when empty).
    pub distribution: Vec<JsonDistribution>,
    /// Always-present issues array.
    pub issues: Vec<JsonIssue>,
    /// Numeric score (emitted last).
    pub score: f64,
}

impl JsonFileEntry {
    fn to_go_value(&self) -> GoValue {
        let mut fields: Vec<(String, GoValue)> = Vec::with_capacity(7);
        fields.push((
            "file_path".to_string(),
            GoValue::Str(self.file_path.clone()),
        ));
        fields.push((
            "score_label".to_string(),
            GoValue::Str(self.score_label.clone()),
        ));
        fields.push(("status".to_string(), GoValue::Str(self.status.clone())));
        fields.push(("metrics".to_string(), metrics_array(&self.metrics)));
        if !self.distribution.is_empty() {
            fields.push((
                "distribution".to_string(),
                distribution_array(&self.distribution),
            ));
        }
        fields.push(("issues".to_string(), issues_array(&self.issues)));
        fields.push(("score".to_string(), GoValue::Float(self.score)));
        GoValue::Object(fields)
    }
}

/// One analyzer's output in JSON.
///
/// Field order: `title`, `score_label`, `status`, `metrics`,
/// `distribution` (omitempty), `issues`, `files` (omitempty), `score` (last).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonSection {
    /// Section title.
    pub title: String,
    /// Formatted score label.
    pub score_label: String,
    /// Status message.
    pub status: String,
    /// Always-present metrics array.
    pub metrics: Vec<JsonMetric>,
    /// Distribution categories (omitted from JSON when empty).
    pub distribution: Vec<JsonDistribution>,
    /// Always-present issues array.
    pub issues: Vec<JsonIssue>,
    /// Optional per-file entries. `None` => the `files` key is omitted;
    /// `Some(_)` (even empty) => the key is present (report-format
    /// optional-pointer semantics).
    pub files: Option<Vec<JsonFileEntry>>,
    /// Numeric score (emitted last).
    pub score: f64,
}

impl JsonSection {
    fn to_go_value(&self) -> GoValue {
        let mut fields: Vec<(String, GoValue)> = Vec::with_capacity(8);
        fields.push(("title".to_string(), GoValue::Str(self.title.clone())));
        fields.push((
            "score_label".to_string(),
            GoValue::Str(self.score_label.clone()),
        ));
        fields.push(("status".to_string(), GoValue::Str(self.status.clone())));
        fields.push(("metrics".to_string(), metrics_array(&self.metrics)));
        if !self.distribution.is_empty() {
            fields.push((
                "distribution".to_string(),
                distribution_array(&self.distribution),
            ));
        }
        fields.push(("issues".to_string(), issues_array(&self.issues)));
        if let Some(files) = &self.files {
            fields.push((
                "files".to_string(),
                GoValue::Array(files.iter().map(JsonFileEntry::to_go_value).collect()),
            ));
        }
        fields.push(("score".to_string(), GoValue::Float(self.score)));
        GoValue::Object(fields)
    }
}

/// The top-level structured JSON output.
///
/// Field order: `overall_score_label`, `sections`, `overall_score` (last).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonReport {
    /// Formatted overall score label.
    pub overall_score_label: String,
    /// Per-analyzer sections.
    pub sections: Vec<JsonSection>,
    /// Overall numeric score (emitted last).
    pub overall_score: f64,
}

impl JsonReport {
    /// Converts the report into a [`GoValue`] preserving score-last field order.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        GoValue::Object(vec![
            (
                "overall_score_label".to_string(),
                GoValue::Str(self.overall_score_label.clone()),
            ),
            (
                "sections".to_string(),
                GoValue::Array(self.sections.iter().map(JsonSection::to_go_value).collect()),
            ),
            (
                "overall_score".to_string(),
                GoValue::Float(self.overall_score),
            ),
        ])
    }

    /// Serializes the report to compact report-contract JSON bytes.
    ///
    /// Routes through [`gocompat::Encoder`](crate::gocompat::Encoder) (HTML
    /// escape on, no trailing newline) — the report-format marshal defaults.
    /// `metrics`/`issues` always serialize as `[]` (never `null`), an empty
    /// `distribution` is omitted, and `overall_score` is emitted last:
    ///
    /// ```
    /// use cf_renderer::{JsonReport, JsonSection};
    ///
    /// let report = JsonReport {
    ///     overall_score_label: "8/10".to_string(),
    ///     sections: vec![JsonSection {
    ///         title: "COMPLEXITY".to_string(),
    ///         score_label: "8/10".to_string(),
    ///         status: "Good".to_string(),
    ///         score: 0.8,
    ///         ..JsonSection::default()
    ///     }],
    ///     overall_score: 0.8,
    /// };
    ///
    /// let json = report.to_json();
    /// assert!(json.contains(r#""metrics":[]"#));
    /// assert!(json.contains(r#""issues":[]"#));
    /// assert!(!json.contains("distribution"));
    /// // overall_score is the last field.
    /// assert!(json.ends_with(r#""overall_score":0.8}"#));
    /// ```
    #[must_use]
    pub fn to_json(&self) -> String {
        Encoder::default().encode(&self.to_go_value())
    }
}

/// Converts a [`ReportSection`] to a [`JsonSection`].
///
/// `metrics`, `distribution`, and `issues` are always initialized to (possibly
/// empty) vectors so JSON output contains `[]`, never `null` — except
/// `distribution` which is then conditionally omitted by [`JsonSection::to_go_value`]
/// when empty (report-format `omitempty` semantics).
pub fn section_to_json(section: &dyn ReportSection) -> JsonSection {
    let metrics = section
        .key_metrics()
        .into_iter()
        .map(|m| JsonMetric {
            label: m.label,
            value: m.value,
        })
        .collect();

    let distribution = section
        .distribution()
        .into_iter()
        .map(|d| JsonDistribution {
            label: d.label,
            percent: d.percent,
            count: d.count,
        })
        .collect();

    let issues = section
        .all_issues()
        .into_iter()
        .map(|i| JsonIssue {
            name: i.name,
            location: i.location,
            value: i.value,
            severity: i.severity,
        })
        .collect();

    JsonSection {
        title: section.section_title(),
        score: section.score(),
        score_label: section.score_label(),
        status: section.status_message(),
        metrics,
        distribution,
        issues,
        files: None,
    }
}

/// Converts a [`ReportSection`] to a [`JsonFileEntry`] for per-file output.
pub fn section_to_json_file_entry(section: &dyn ReportSection, file_path: &str) -> JsonFileEntry {
    let base = section_to_json(section);
    JsonFileEntry {
        file_path: file_path.to_string(),
        score: base.score,
        score_label: base.score_label,
        status: base.status,
        metrics: base.metrics,
        distribution: base.distribution,
        issues: base.issues,
    }
}

/// Converts multiple [`ReportSection`]s to a [`JsonReport`] with an overall
/// score.
pub fn sections_to_json(sections: &[&dyn ReportSection]) -> JsonReport {
    let summary = ExecutiveSummary::new(sections);

    let json_sections = sections
        .iter()
        .map(|s| section_to_json(*s))
        .collect::<Vec<_>>();

    JsonReport {
        overall_score: summary.overall_score(),
        overall_score_label: summary.overall_score_label(),
        sections: json_sections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{
        severity, BaseReportSection, DistributionItem, Issue, Metric, ReportSection,
        SCORE_INFO_ONLY,
    };

    /// A ReportSection backed by explicit metric/distribution/issue lists.
    struct MockSection {
        base: BaseReportSection,
        metrics: Vec<Metric>,
        distribution: Vec<DistributionItem>,
        issues: Vec<Issue>,
    }

    impl MockSection {
        fn new(title: &str, score: f64, msg: &str) -> Self {
            Self {
                base: BaseReportSection {
                    title: title.into(),
                    message: msg.into(),
                    score_value: score,
                },
                metrics: Vec::new(),
                distribution: Vec::new(),
                issues: Vec::new(),
            }
        }
    }

    impl ReportSection for MockSection {
        fn section_title(&self) -> String {
            self.base.section_title()
        }
        fn score(&self) -> f64 {
            self.base.score()
        }
        fn status_message(&self) -> String {
            self.base.status_message()
        }
        fn key_metrics(&self) -> Vec<Metric> {
            self.metrics.clone()
        }
        fn distribution(&self) -> Vec<DistributionItem> {
            self.distribution.clone()
        }
        fn top_issues(&self, n: usize) -> Vec<Issue> {
            if n >= self.issues.len() {
                self.issues.clone()
            } else {
                self.issues[..n].to_vec()
            }
        }
        fn all_issues(&self) -> Vec<Issue> {
            self.issues.clone()
        }
    }

    /// Mirrors reference test `TestSectionToJSON_Fields`.
    #[test]
    fn section_to_json_fields() {
        let mut mock = MockSection::new("COMPLEXITY", 0.8, "Good - reasonable complexity");
        mock.metrics = vec![
            Metric {
                label: "Total Functions".into(),
                value: "42".into(),
            },
            Metric {
                label: "Avg Complexity".into(),
                value: "3.2".into(),
            },
        ];
        mock.distribution = vec![
            DistributionItem {
                label: "Simple (1-5)".into(),
                percent: 0.7,
                count: 30,
            },
            DistributionItem {
                label: "Complex (6-10)".into(),
                percent: 0.3,
                count: 12,
            },
        ];
        mock.issues = vec![Issue {
            name: "processData".into(),
            location: "main.go:10".into(),
            value: "15".into(),
            severity: severity::POOR.into(),
        }];

        let result = section_to_json(&mock);
        assert_eq!(result.title, "COMPLEXITY");
        assert!((result.score - 0.8).abs() < 0.001);
        assert_eq!(result.score_label, "8/10");
        assert_eq!(result.status, "Good - reasonable complexity");
        assert_eq!(result.metrics.len(), 2);
        assert_eq!(result.metrics[0].label, "Total Functions");
        assert_eq!(result.metrics[0].value, "42");
        assert_eq!(result.distribution.len(), 2);
        assert_eq!(result.distribution[0].label, "Simple (1-5)");
        assert!((result.distribution[0].percent - 0.7).abs() < 0.001);
        assert_eq!(result.distribution[0].count, 30);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].name, "processData");
        assert_eq!(result.issues[0].location, "main.go:10");
        assert_eq!(result.issues[0].value, "15");
        assert_eq!(result.issues[0].severity, severity::POOR);
    }

    /// Mirrors reference test `TestSectionToJSON_InfoOnly`.
    #[test]
    fn section_to_json_info_only() {
        let mock = MockSection::new("IMPORTS", SCORE_INFO_ONLY, "5 unique imports found");
        let result = section_to_json(&mock);
        assert_eq!(result.title, "IMPORTS");
        assert!((result.score + 1.0).abs() < 0.001);
        assert_eq!(result.score_label, "Info");
        assert_eq!(result.status, "5 unique imports found");
    }

    /// Mirrors reference test `TestSectionToJSON_EmptyIssues` / `_EmptyMetrics`: empty arrays,
    /// not null.
    #[test]
    fn section_to_json_empty_arrays() {
        let mock = MockSection::new("COMMENTS", 0.6, "Fair comment quality");
        let result = section_to_json(&mock);
        assert!(result.issues.is_empty());
        assert!(result.metrics.is_empty());
        // Serialized issues/metrics must be [] (not null); distribution omitted.
        let json = JsonReport {
            overall_score_label: "6/10".into(),
            sections: vec![result],
            overall_score: 0.6,
        }
        .to_json();
        assert!(json.contains(r#""metrics":[]"#));
        assert!(json.contains(r#""issues":[]"#));
        assert!(!json.contains("distribution"));
    }

    /// Mirrors reference test `TestSectionsToJSON_MultipleSections`.
    #[test]
    fn sections_to_json_multiple() {
        let a = MockSection::new("COMPLEXITY", 0.8, "Good");
        let b = MockSection::new("COMMENTS", 0.6, "Fair");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let result = sections_to_json(&sections);
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].title, "COMPLEXITY");
        assert_eq!(result.sections[1].title, "COMMENTS");
    }

    /// Mirrors reference test `TestSectionsToJSON_IncludesOverall`.
    #[test]
    fn sections_to_json_includes_overall() {
        let a = MockSection::new("COMPLEXITY", 0.8, "Good");
        let b = MockSection::new("COMMENTS", 0.6, "Fair");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let result = sections_to_json(&sections);
        assert!((result.overall_score - 0.7).abs() < 0.001);
        assert_eq!(result.overall_score_label, "7/10");
    }

    /// Mirrors reference test `TestSectionsToJSON_OverallExcludesInfoOnly`.
    #[test]
    fn sections_to_json_excludes_info_only() {
        let a = MockSection::new("COMPLEXITY", 0.8, "Good");
        let b = MockSection::new("IMPORTS", SCORE_INFO_ONLY, "Info");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let result = sections_to_json(&sections);
        assert!((result.overall_score - 0.8).abs() < 0.001);
    }

    /// Mirrors reference test `TestSectionsToJSON_AllInfoOnly`.
    #[test]
    fn sections_to_json_all_info_only() {
        let a = MockSection::new("IMPORTS", SCORE_INFO_ONLY, "Info");
        let sections: Vec<&dyn ReportSection> = vec![&a];
        let result = sections_to_json(&sections);
        assert!((result.overall_score - SCORE_INFO_ONLY).abs() < 0.001);
        assert_eq!(result.overall_score_label, "Info");
    }

    /// Mirrors reference test `TestSectionsToJSON_Serializable`: byte-level JSON shape.
    #[test]
    fn sections_to_json_serializable() {
        let a = MockSection::new("COMPLEXITY", 0.8, "Good");
        let sections: Vec<&dyn ReportSection> = vec![&a];
        let json = sections_to_json(&sections).to_json();
        assert!(json.contains(r#""title":"COMPLEXITY""#));
        assert!(json.contains(r#""overall_score":0.8"#));
    }

    /// Mirrors reference test `TestJSONSection_NoFiles_OmittedFromJSON`.
    #[test]
    fn json_section_no_files_omitted() {
        let section = JsonSection {
            title: "COMPLEXITY".into(),
            score: 0.8,
            score_label: "8/10".into(),
            status: "Good".into(),
            metrics: vec![JsonMetric {
                label: "Total Functions".into(),
                value: "42".into(),
            }],
            distribution: Vec::new(),
            issues: Vec::new(),
            files: None,
        };
        let json = Encoder::default().encode(&section.to_go_value());
        assert!(!json.contains(r#""files""#));
    }

    /// Mirrors reference test `TestJSONSection_WithFiles_IncludedInJSON`.
    #[test]
    fn json_section_with_files_included() {
        let section = JsonSection {
            title: "COMPLEXITY".into(),
            score: 0.8,
            score_label: "8/10".into(),
            status: "Good".into(),
            metrics: vec![JsonMetric {
                label: "Total Functions".into(),
                value: "42".into(),
            }],
            distribution: Vec::new(),
            issues: Vec::new(),
            files: Some(vec![JsonFileEntry {
                file_path: "pkg/foo/bar.go".into(),
                score: 0.6,
                score_label: "6/10".into(),
                status: "Fair".into(),
                metrics: vec![JsonMetric {
                    label: "Total Functions".into(),
                    value: "12".into(),
                }],
                distribution: Vec::new(),
                issues: Vec::new(),
            }]),
        };
        let json = Encoder::default().encode(&section.to_go_value());
        assert!(json.contains(r#""files""#));
        assert!(json.contains(r#""file_path":"pkg/foo/bar.go""#));
        assert!(json.contains(r#""score":0.6"#));
    }

    /// Score is emitted last (score-last field ordering) in section JSON.
    #[test]
    fn score_is_last_field() {
        let section = JsonSection {
            title: "X".into(),
            score: 0.5,
            score_label: "5/10".into(),
            status: "S".into(),
            metrics: Vec::new(),
            distribution: Vec::new(),
            issues: Vec::new(),
            files: None,
        };
        let json = Encoder::default().encode(&section.to_go_value());
        let score_pos = json.find(r#""score""#).unwrap();
        let title_pos = json.find(r#""title""#).unwrap();
        let issues_pos = json.find(r#""issues""#).unwrap();
        assert!(title_pos < score_pos);
        assert!(
            issues_pos < score_pos,
            "score must come after issues: {json}"
        );
    }
}
