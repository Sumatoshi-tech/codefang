//! The clones report section: the human-facing section model (title,
//! status, key metrics, clone-type distribution, pair issues), its
//! `renderer.JSONReport` value (`--format json`, the source of the `text`
//! render), the `--per-file` entries, and the single-line `compact` render.

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::report::{ClonePair, CloneReport, CloneType, FileSummary};
use crate::MSG_NO_FUNCTIONS;

/// Section title.
pub const SECTION_TITLE: &str = "CLONE DETECTION";
/// Similarity at or above which a pair's issue severity is `poor`.
pub const SEVERITY_THRESH_HIGH: f64 = 0.8;
/// Issue severity for a pair at/above [`SEVERITY_THRESH_HIGH`].
pub const SEVERITY_POOR: &str = "poor";
/// Issue severity for a pair below [`SEVERITY_THRESH_HIGH`].
pub const SEVERITY_FAIR: &str = "fair";

/// Metric label: total functions.
pub const METRIC_TOTAL_FUNCTIONS: &str = "Total Functions";
/// Metric label: clone pairs.
pub const METRIC_CLONE_PAIRS: &str = "Clone Pairs";
/// Metric label: clone ratio.
pub const METRIC_CLONE_RATIO: &str = "Clone Ratio";
/// Per-file metric label: functions of the file in at least one pair.
pub const METRIC_CLONED_FUNCTIONS: &str = "Cloned Functions";

/// Per-file status when none of the file's functions is cloned.
pub const FILE_STATUS_NONE: &str = "No functions in clone pairs";
/// Decimals shown for a pair's similarity in issue values.
pub const SIMILARITY_DECIMALS: usize = 2;

/// Score label of a negative (info-only) score.
const SCORE_LABEL_INFO: &str = "Info";
/// Compact title column width.
const COMPACT_TITLE_WIDTH: usize = 12;
/// Compact score-bar width.
const COMPACT_BAR_WIDTH: usize = 10;
/// Filled progress-bar cell.
const PROGRESS_FILLED: &str = "\u{2588}"; // █
/// Empty progress-bar cell.
const PROGRESS_EMPTY: &str = "\u{2591}"; // ░

/// A key/value metric for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metric {
    /// Metric label.
    pub label: String,
    /// Formatted metric value.
    pub value: String,
}

/// A distribution row for display.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionItem {
    /// Row label.
    pub label: String,
    /// Share of the total as a fraction in `[0, 1]` (the renderer contract
    /// shared by every analyzer; the formatted strings carry the `%`).
    pub percent: f64,
    /// Raw count.
    pub count: i64,
}

/// A reported issue (one clone pair) for display.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// `func_a <-> func_b`.
    pub name: String,
    /// The clone type (shown in the location column).
    pub location: String,
    /// Formatted similarity.
    pub value: String,
    /// `poor` / `fair`.
    pub severity: String,
}

/// The clones report section.
#[derive(Debug, Clone)]
pub struct ReportSection<'a> {
    report: &'a CloneReport,
}

impl<'a> ReportSection<'a> {
    /// Wraps a report.
    #[must_use]
    pub fn new(report: &'a CloneReport) -> Self {
        Self { report }
    }

    /// The section title.
    #[must_use]
    pub fn title(&self) -> &'static str {
        SECTION_TITLE
    }

    /// The status message (keyed off the clone ratio).
    #[must_use]
    pub fn message(&self) -> &str {
        &self.report.message
    }

    /// The `0..1` score (higher is better): `1 - clone_ratio`.
    #[must_use]
    pub fn score(&self) -> f64 {
        compute_score(self.report.clone_ratio)
    }

    /// Ordered key metrics.
    #[must_use]
    pub fn key_metrics(&self) -> Vec<Metric> {
        vec![
            metric(
                METRIC_TOTAL_FUNCTIONS,
                cf_reportutil::format_int(self.report.total_functions),
            ),
            metric(
                METRIC_CLONE_PAIRS,
                cf_reportutil::format_int(self.report.total_clone_pairs),
            ),
            metric(
                METRIC_CLONE_RATIO,
                cf_reportutil::format_float(self.report.clone_ratio),
            ),
        ]
    }

    /// Clone-type distribution rows over every pair found (empty when there
    /// are no pairs).
    #[must_use]
    pub fn distribution(&self) -> Vec<DistributionItem> {
        let counts = self.report.distribution;
        let total = counts.total();
        if total == 0 {
            return Vec::new();
        }
        [CloneType::Type1, CloneType::Type2, CloneType::Type3]
            .into_iter()
            .map(|t| DistributionItem {
                label: t.label().to_string(),
                percent: cf_reportutil::pct(counts.get(t), total),
                count: counts.get(t),
            })
            .collect()
    }

    /// Every stored pair as an issue, in report order.
    #[must_use]
    pub fn issues(&self) -> Vec<Issue> {
        self.report.pairs.iter().map(issue_of).collect()
    }
}

fn metric(label: &str, value: String) -> Metric {
    Metric {
        label: label.to_string(),
        value,
    }
}

fn issue_of(p: &ClonePair) -> Issue {
    let severity = if p.similarity >= SEVERITY_THRESH_HIGH {
        SEVERITY_POOR
    } else {
        SEVERITY_FAIR
    };
    Issue {
        name: p.display_name(),
        location: p.clone_type.as_str().to_string(),
        value: format_similarity(p.similarity),
        severity: severity.to_string(),
    }
}

/// Formats a similarity with [`SIMILARITY_DECIMALS`] decimals (`0.64`,
/// `1.00`): one decimal would collapse every Type-3 pair onto `0.5`–`0.9`.
fn format_similarity(similarity: f64) -> String {
    format!("{similarity:.SIMILARITY_DECIMALS$}")
}

/// Converts a clone ratio to a `0..1` score (lower ratio = higher score):
/// clamps to `[0, 1]` then inverts.
#[must_use]
pub fn compute_score(clone_ratio: f64) -> f64 {
    1.0 - clone_ratio.clamp(0.0, 1.0)
}

/// Formats a `0..1` score as the `N/10` label (`round(score*10)`); a
/// negative score renders `Info`.
#[must_use]
pub fn score_label(score: f64) -> String {
    if score < 0.0 {
        return SCORE_LABEL_INFO.to_string();
    }
    format!("{}/10", (score * 10.0).round() as i64)
}

/// Builds the `renderer.JSONReport` value for the single clones section:
/// `{overall_score_label, sections: [section], overall_score}`, where the
/// section is `{title, score_label, status, metrics, distribution?, issues,
/// files?, score}`. `per_file` adds one entry per analyzed file
/// ([`file_entry`]).
#[must_use]
pub fn json_value(report: &CloneReport, per_file: bool) -> GoValue {
    let section = ReportSection::new(report);
    let score = section.score();

    let mut sect = GoMap::new(MapOrigin::Struct);
    sect.push("title", GoValue::Str(section.title().to_string()));
    sect.push("score_label", GoValue::Str(score_label(score)));
    sect.push("status", GoValue::Str(section.message().to_string()));
    sect.push("metrics", metrics_value(&section.key_metrics()));
    let distribution = section.distribution();
    if !distribution.is_empty() {
        sect.push("distribution", distribution_value(&distribution));
    }
    sect.push("issues", issues_value(&section.issues()));
    if per_file {
        let files = report.files.iter().map(|f| file_entry(report, f)).collect();
        sect.push("files", GoValue::Array(files));
    }
    sect.push("score", GoValue::Float(score));

    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str(score_label(score)));
    root.push("sections", GoValue::Array(vec![GoValue::Map(sect)]));
    root.push("overall_score", GoValue::Float(score));
    GoValue::Map(root)
}

/// Builds one `--per-file` entry: the file's function count, how many of
/// them take part in a clone pair, the resulting per-file ratio/score, and
/// the stored pairs that involve the file as issues.
#[must_use]
pub fn file_entry(report: &CloneReport, file: &FileSummary) -> GoValue {
    let ratio = crate::compute_clone_ratio(
        file.cloned_functions as usize,
        file.total_functions as usize,
    );
    let score = compute_score(ratio);
    let status = if file.total_functions == 0 {
        MSG_NO_FUNCTIONS.to_string()
    } else if file.cloned_functions == 0 {
        FILE_STATUS_NONE.to_string()
    } else {
        format!(
            "{} of {} functions in clone pairs",
            file.cloned_functions, file.total_functions
        )
    };
    let metrics = vec![
        metric(
            METRIC_TOTAL_FUNCTIONS,
            cf_reportutil::format_int(file.total_functions),
        ),
        metric(
            METRIC_CLONED_FUNCTIONS,
            cf_reportutil::format_int(file.cloned_functions),
        ),
        metric(METRIC_CLONE_RATIO, cf_reportutil::format_float(ratio)),
    ];
    let issues: Vec<Issue> = file
        .pairs
        .iter()
        .filter_map(|&i| report.pairs.get(i))
        .map(issue_of)
        .collect();

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(file.path.clone()));
    entry.push("score_label", GoValue::Str(score_label(score)));
    entry.push("status", GoValue::Str(status));
    entry.push("metrics", metrics_value(&metrics));
    entry.push("issues", issues_value(&issues));
    entry.push("score", GoValue::Float(score));
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
                si.push("location", GoValue::Str(i.location.clone()));
                si.push("value", GoValue::Str(i.value.clone()));
                si.push("severity", GoValue::Str(i.severity.clone()));
                GoValue::Map(si)
            })
            .collect(),
    )
}

/// Right-pads `s` with spaces to `width` characters.
fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(width);
    out.push_str(s);
    out.extend(std::iter::repeat(' ').take(width - len));
    out
}

/// Progress bar: `trunc(value*width)` filled cells (value clamped to
/// `[0,1]`), the remainder empty.
fn draw_progress_bar(value: f64, width: usize) -> String {
    let filled = (value.clamp(0.0, 1.0) * width as f64) as usize;
    let mut bar = String::with_capacity(width * 3);
    bar.extend(std::iter::repeat(PROGRESS_FILLED).take(filled));
    bar.extend(std::iter::repeat(PROGRESS_EMPTY).take(width - filled));
    bar
}

/// Builds the single-line `--format compact` bytes for the clones section:
/// `<title padded to 12> [<bar>] <N/10>  <status message>\n` (no ANSI
/// color).
#[must_use]
pub fn compact(report: &CloneReport) -> Vec<u8> {
    let section = ReportSection::new(report);
    let score = section.score();
    let title = pad_right(section.title(), COMPACT_TITLE_WIDTH);
    let bar = draw_progress_bar(score, COMPACT_BAR_WIDTH);
    format!(
        "{title} [{bar}] {}  {}\n",
        score_label(score),
        section.message()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{CloneTypeCounts, FileSummary};
    use cf_gojson::Encoder;

    fn pair(a: &str, b: &str, similarity: f64, clone_type: CloneType) -> ClonePair {
        ClonePair {
            func_a: a.into(),
            func_b: b.into(),
            similarity,
            clone_type,
        }
    }

    fn sample() -> CloneReport {
        let mut distribution = CloneTypeCounts::default();
        distribution.increment(CloneType::Type1);
        distribution.increment(CloneType::Type3);
        CloneReport {
            total_functions: 4,
            total_clone_pairs: 2,
            clone_ratio: 0.75,
            distribution,
            pairs: vec![
                pair("a.go::f", "b.go::f", 1.0, CloneType::Type1),
                pair("a.go::f", "c.go::g", 0.6, CloneType::Type3),
            ],
            message: crate::MSG_HIGH_CLONES.into(),
            files: vec![
                FileSummary {
                    path: "a.go".into(),
                    total_functions: 1,
                    cloned_functions: 1,
                    pairs: vec![0, 1],
                },
                FileSummary {
                    path: "d.go".into(),
                    total_functions: 1,
                    cloned_functions: 0,
                    pairs: vec![],
                },
            ],
        }
    }

    // Exact comparisons on purpose: the clamp endpoints map exactly.
    #[allow(clippy::float_cmp)]
    #[test]
    fn compute_score_clamps_and_inverts() {
        assert_eq!(compute_score(1.5), 0.0);
        assert_eq!(compute_score(1.0), 0.0);
        assert_eq!(compute_score(0.0), 1.0);
        assert_eq!(compute_score(-0.5), 1.0);
        assert!((compute_score(0.25) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn score_labels() {
        assert_eq!(score_label(1.0), "10/10");
        assert_eq!(score_label(0.25), "3/10");
        assert_eq!(score_label(-1.0), "Info");
    }

    #[test]
    fn section_metrics_distribution_and_issues() {
        let report = sample();
        let s = ReportSection::new(&report);
        assert_eq!(s.title(), SECTION_TITLE);
        let m = s.key_metrics();
        assert_eq!(m[0].value, "4");
        assert_eq!(m[1].value, "2");
        assert_eq!(m[2].value, "0.8");
        let d = s.distribution();
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].count, 1);
        assert!((d[0].percent - 0.5).abs() < 1e-12);
        let i = s.issues();
        assert_eq!(i[0].severity, SEVERITY_POOR);
        assert_eq!(i[1].severity, SEVERITY_FAIR);
        assert_eq!(i[1].location, "Type-3");
        assert_eq!(i[1].value, "0.60");
    }

    #[test]
    fn empty_report_has_no_distribution_and_full_score() {
        let report = CloneReport {
            message: crate::MSG_NO_FUNCTIONS.into(),
            ..CloneReport::default()
        };
        let s = ReportSection::new(&report);
        assert!(s.distribution().is_empty());
        assert!((s.score() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn json_value_shape_and_order() {
        let json = Encoder::marshal().encode_to_string(&json_value(&sample(), false));
        assert!(json.starts_with(r#"{"overall_score_label":"3/10","sections":[{"title":"CLONE DETECTION","score_label":"3/10","status":"High duplication - significant refactoring recommended","metrics":[{"label":"Total Functions","value":"4"}"#), "{json}");
        assert!(
            json.contains(r#""distribution":[{"label":"Type-1 (Exact)","percent":0.5,"count":1}"#)
        );
        // The report encoder HTML-escapes `<` / `>` (`<` / `>`).
        assert!(json.contains(r#""issues":[{"name":"a.go::f \u003c-\u003e b.go::f","location":"Type-1","value":"1.00","severity":"poor"}"#), "{json}");
        assert!(!json.contains("\"files\""));
        assert!(
            json.ends_with(r#""score":0.25}],"overall_score":0.25}"#),
            "{json}"
        );
    }

    #[test]
    fn per_file_entries_follow_the_file_summaries() {
        let json = Encoder::marshal().encode_to_string(&json_value(&sample(), true));
        assert!(json.contains(r#""files":[{"file_path":"a.go","score_label":"0/10","status":"1 of 1 functions in clone pairs","metrics":[{"label":"Total Functions","value":"1"},{"label":"Cloned Functions","value":"1"},{"label":"Clone Ratio","value":"1.0"}],"issues":[{"name":"a.go::f \u003c-\u003e b.go::f""#), "{json}");
        assert!(json.contains(r#"{"file_path":"d.go","score_label":"10/10","status":"No functions in clone pairs","metrics":[{"label":"Total Functions","value":"1"},{"label":"Cloned Functions","value":"0"},{"label":"Clone Ratio","value":"0.0"}],"issues":[],"score":1}"#), "{json}");
    }

    #[test]
    fn file_entry_without_functions_uses_the_no_functions_status() {
        let report = sample();
        let file = FileSummary {
            path: "empty.go".into(),
            ..FileSummary::default()
        };
        let json = Encoder::marshal().encode_to_string(&file_entry(&report, &file));
        assert!(json.contains(r#""status":"No functions found for clone analysis""#));
    }

    #[test]
    fn compact_line() {
        let line = String::from_utf8(compact(&sample())).unwrap();
        assert_eq!(
            line,
            "CLONE DETECTION [██░░░░░░░░] 3/10  High duplication - significant refactoring recommended\n"
        );
    }

    #[test]
    fn pad_and_bar_helpers() {
        assert_eq!(pad_right("ab", 4), "ab  ");
        assert_eq!(pad_right("abcdef", 4), "abcdef");
        assert_eq!(draw_progress_bar(0.0, 3), "░░░");
        assert_eq!(draw_progress_bar(1.0, 3), "███");
        assert_eq!(draw_progress_bar(0.5, 3), "█░░");
    }
}
