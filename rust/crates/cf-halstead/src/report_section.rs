//! Terminal report section (`report_section.go`).
//!
//! Produces the human-facing HALSTEAD section: key metrics, a volume
//! distribution, and the top issues sorted by effort descending. Terminal output
//! is non-binding/cosmetic (DESIGN §2.7), but the *scoring*, *distribution
//! bucketing*, and *severity* logic is shared with the rest of the analyzer and
//! is ported exactly.
//!
//! The section reads from a report map. Here a report is modeled as a
//! [`cf_gojson::GoValue::Object`] and functions as `GoValue::Array` of objects,
//! matching how [`crate::report::build_result`] shapes the data.

use cf_gojson::GoValue;

/// Section title.
pub const SECTION_TITLE: &str = "HALSTEAD";

/// Default status message when no Halstead data is present.
pub const DEFAULT_STATUS_MESSAGE: &str = "No Halstead data available";

// --- Score thresholds (report_section.go) ---
const SCORE_EXCELLENT_MAX: f64 = 5.0;
const SCORE_GOOD_MAX: f64 = 15.0;
const SCORE_FAIR_MAX: f64 = 30.0;

/// Score for excellent (difficulty <= 5).
pub const SCORE_EXCELLENT: f64 = 1.0;
/// Score for good (difficulty <= 15).
pub const SCORE_GOOD: f64 = 0.8;
/// Score for fair (difficulty <= 30).
pub const SCORE_FAIR: f64 = 0.6;
/// Score for poor (difficulty > 30).
pub const SCORE_POOR: f64 = 0.3;

// --- Distribution buckets (report_section.go) ---
const DIST_LOW_MAX: f64 = 100.0;
const DIST_MED_MAX: f64 = 1000.0;
const DIST_HIGH_MAX: f64 = 5000.0;

/// Label for the low volume bucket.
pub const DIST_LABEL_LOW: &str = "Low (<=100)";
/// Label for the medium volume bucket.
pub const DIST_LABEL_MED: &str = "Medium (101-1000)";
/// Label for the high volume bucket.
pub const DIST_LABEL_HIGH: &str = "High (1001-5000)";
/// Label for the very-high volume bucket.
pub const DIST_LABEL_VHIGH: &str = "Very High (>5000)";

// --- Severity thresholds (report_section.go) ---
const ISSUE_SEVERITY_FAIR_MIN: f64 = 10000.0;
const ISSUE_SEVERITY_POOR_MIN: f64 = 50000.0;

/// Severity label for a healthy function.
pub const SEVERITY_GOOD: &str = "good";
/// Severity label for a function needing attention.
pub const SEVERITY_FAIR: &str = "fair";
/// Severity label for a high-risk function.
pub const SEVERITY_POOR: &str = "poor";

/// Volume distribution counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VolumeDistCounts {
    /// Volume <= 100.
    pub low: i64,
    /// 100 < volume <= 1000.
    pub medium: i64,
    /// 1000 < volume <= 5000.
    pub high: i64,
    /// Volume > 5000.
    pub very_high: i64,
}

/// One issue row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Function name.
    pub name: String,
    /// Source location (from `_source_file`).
    pub location: String,
    /// Formatted value string (`effort=… | vol=… | bugs=…`).
    pub value: String,
    /// Severity (`good`/`fair`/`poor`).
    pub severity: String,
}

/// Maps a difficulty to a section score (`calculateScore`).
#[must_use]
pub fn calculate_score(difficulty: f64) -> f64 {
    if difficulty <= SCORE_EXCELLENT_MAX {
        SCORE_EXCELLENT
    } else if difficulty <= SCORE_GOOD_MAX {
        SCORE_GOOD
    } else if difficulty <= SCORE_FAIR_MAX {
        SCORE_FAIR
    } else {
        SCORE_POOR
    }
}

/// Buckets a slice of function objects by volume (`categorizeVolume`).
#[must_use]
pub fn categorize_volume(functions: &[GoValue]) -> VolumeDistCounts {
    let mut counts = VolumeDistCounts::default();
    for fnv in functions {
        let vol = get_float(fnv, "volume");
        if vol <= DIST_LOW_MAX {
            counts.low += 1;
        } else if vol <= DIST_MED_MAX {
            counts.medium += 1;
        } else if vol <= DIST_HIGH_MAX {
            counts.high += 1;
        } else {
            counts.very_high += 1;
        }
    }
    counts
}

/// Severity for a function given effort and bugs (`severityForFunction`).
#[must_use]
pub fn severity_for_function(effort: f64, bugs: f64) -> &'static str {
    if effort >= ISSUE_SEVERITY_POOR_MIN || bugs >= 1.0 {
        SEVERITY_POOR
    } else if effort >= ISSUE_SEVERITY_FAIR_MIN || bugs >= 0.3 {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// Formats the issue value string (`formatIssueValue`). Uses [`format_float`] to
/// match the Go `reportutil.FormatFloat` rendering.
#[must_use]
pub fn format_issue_value(effort: f64, volume: f64, bugs: f64) -> String {
    format!(
        "effort={} | vol={} | bugs={}",
        format_float(effort),
        format_float(volume),
        format_float(bugs)
    )
}

/// Builds issues from functions sorted by effort descending, limited to `limit`
/// (`limit == 0` => all) (`halsteadIssues`).
#[must_use]
pub fn halstead_issues(functions: &[GoValue], limit: usize) -> Vec<Issue> {
    if functions.is_empty() {
        return Vec::new();
    }

    // Sort by effort descending (mapx.SortAndLimit with effort-desc comparator).
    let mut sorted: Vec<&GoValue> = functions.iter().collect();
    sorted.sort_by(|a, b| {
        get_float(b, "effort")
            .partial_cmp(&get_float(a, "effort"))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if limit > 0 && sorted.len() > limit {
        sorted.truncate(limit);
    }

    sorted
        .into_iter()
        .map(|fnv| {
            let effort = get_float(fnv, "effort");
            let volume = get_float(fnv, "volume");
            let bugs = get_float(fnv, "delivered_bugs");
            Issue {
                name: get_string(fnv, "name"),
                location: get_string(fnv, "_source_file"),
                value: format_issue_value(effort, volume, bugs),
                severity: severity_for_function(effort, bugs).to_string(),
            }
        })
        .collect()
}

/// Resolves the section status message from a report (`NewReportSection`):
/// the report `message`, or [`DEFAULT_STATUS_MESSAGE`] when absent/empty.
#[must_use]
pub fn status_message(report: &GoValue) -> String {
    let msg = get_string(report, "message");
    if msg.is_empty() {
        DEFAULT_STATUS_MESSAGE.to_string()
    } else {
        msg
    }
}

// --- report-reading helpers (reportutil.Get*) ---

fn get_field<'a>(value: &'a GoValue, key: &str) -> Option<&'a GoValue> {
    match value {
        GoValue::Object(m) => m.get(key),
        _ => None,
    }
}

fn get_float(value: &GoValue, key: &str) -> f64 {
    match get_field(value, key) {
        Some(GoValue::Float(f)) => *f,
        Some(GoValue::Int(i)) => *i as f64,
        Some(GoValue::Uint(u)) => *u as f64,
        _ => 0.0,
    }
}

fn get_string(value: &GoValue, key: &str) -> String {
    match get_field(value, key) {
        Some(GoValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Reads the `functions` array out of a report, if present.
#[must_use]
pub fn report_functions(report: &GoValue) -> &[GoValue] {
    match get_field(report, "functions") {
        Some(GoValue::Array(a)) => a.as_slice(),
        _ => &[],
    }
}

/// Renders a float the way Go's `reportutil.FormatFloat` does: integral values
/// drop the decimal point, otherwise two decimal places.
///
/// NOTE: This matches the observed `FormatFloat` behavior used in the terminal
/// section (cosmetic output). It is NOT the byte-identity `cf_gojson` float path.
#[must_use]
pub fn format_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, GoValue};

    fn obj(pairs: &[(&str, GoValue)]) -> GoValue {
        let mut m = GoMap::new_map();
        for (k, v) in pairs {
            m.push(k, v.clone());
        }
        GoValue::Object(m)
    }

    fn test_report() -> GoValue {
        obj(&[
            ("difficulty", GoValue::Float(12.3)),
            ("message", GoValue::Str("Good complexity - code is reasonably complex".into())),
            (
                "functions",
                GoValue::Array(vec![
                    obj(&[("name", GoValue::Str("ProcessData".into())), ("volume", GoValue::Float(800.0)), ("effort", GoValue::Float(25000.0))]),
                    obj(&[("name", GoValue::Str("HandleRequest".into())), ("volume", GoValue::Float(200.0)), ("effort", GoValue::Float(5000.0))]),
                    obj(&[("name", GoValue::Str("ParseConfig".into())), ("volume", GoValue::Float(50.0)), ("effort", GoValue::Float(1500.0))]),
                    obj(&[("name", GoValue::Str("GetName".into())), ("volume", GoValue::Float(20.0)), ("effort", GoValue::Float(200.0))]),
                ]),
            ),
        ])
    }

    /// Ported from `TestHalsteadScore_*`.
    #[test]
    fn score_tiers() {
        assert_eq!(calculate_score(3.0), SCORE_EXCELLENT);
        assert_eq!(calculate_score(12.3), SCORE_GOOD);
        assert_eq!(calculate_score(25.0), SCORE_FAIR);
        assert_eq!(calculate_score(50.0), SCORE_POOR);
        assert_eq!(calculate_score(0.0), SCORE_EXCELLENT);
    }

    /// Ported from `TestHalsteadStatusMessage` / `_Empty`.
    #[test]
    fn status_message_default_and_present() {
        assert_eq!(status_message(&test_report()), "Good complexity - code is reasonably complex");
        assert_eq!(status_message(&obj(&[])), DEFAULT_STATUS_MESSAGE);
    }

    /// Ported from `TestHalsteadDistribution`.
    #[test]
    fn distribution_counts() {
        let counts = categorize_volume(report_functions(&test_report()));
        // Low(<=100): ParseConfig(50), GetName(20)=2; Medium: ProcessData(800), HandleRequest(200)=2.
        assert_eq!(counts.low, 2);
        assert_eq!(counts.medium, 2);
        assert_eq!(counts.high, 0);
        assert_eq!(counts.very_high, 0);
    }

    /// Ported from `TestHalsteadDistribution_HighVolume`.
    #[test]
    fn distribution_high_volume() {
        let report = obj(&[(
            "functions",
            GoValue::Array(vec![
                obj(&[("name", GoValue::Str("Big".into())), ("volume", GoValue::Float(3000.0))]),
                obj(&[("name", GoValue::Str("Huge".into())), ("volume", GoValue::Float(8000.0))]),
            ]),
        )]);
        let counts = categorize_volume(report_functions(&report));
        assert_eq!(counts.high, 1);
        assert_eq!(counts.very_high, 1);
    }

    /// Ported from `TestHalsteadTopIssues_SortedByEffort` and `_Severity`.
    #[test]
    fn top_issues_sorted_and_severity() {
        let funcs = report_functions(&test_report());
        let issues = halstead_issues(funcs, 3);
        assert_eq!(issues.len(), 3);
        assert_eq!(issues[0].name, "ProcessData"); // highest effort 25000
        // ProcessData(25000) -> fair, HandleRequest(5000) -> good.
        assert_eq!(issues[0].severity, SEVERITY_FAIR);
        assert_eq!(issues[1].severity, SEVERITY_GOOD);
    }

    /// Ported from `TestHalsteadTopIssues_SeverityPoor`.
    #[test]
    fn severity_poor() {
        let report = obj(&[(
            "functions",
            GoValue::Array(vec![obj(&[("name", GoValue::Str("Monster".into())), ("effort", GoValue::Float(60000.0))])]),
        )]);
        let issues = halstead_issues(report_functions(&report), 1);
        assert_eq!(issues[0].severity, SEVERITY_POOR);
    }

    /// Ported from `TestHalsteadAllIssues`.
    #[test]
    fn all_issues_returns_every_function() {
        let issues = halstead_issues(report_functions(&test_report()), 0);
        assert_eq!(issues.len(), 4);
    }

    /// Ported from `TestHalsteadTopIssues_Empty`.
    #[test]
    fn empty_report_no_issues() {
        assert!(halstead_issues(report_functions(&obj(&[])), 5).is_empty());
    }
}
