//! Terminal report section.
//!
//! This is **non-binding / cosmetic** output (DESIGN §2.7): byte-identity is
//! not required for the terminal renderer. The data extraction (distribution
//! buckets, top issues, severities) lives here so `cf-renderer` can produce
//! the section; the actual table/ANSI rendering is delegated to the renderer
//! crate.

use crate::report_value::{Report, ReportValue};

/// Section title.
pub const SECTION_TITLE: &str = "COHESION";
/// Default status message.
pub const DEFAULT_STATUS_MESSAGE: &str = "No cohesion data available";

/// Distribution thresholds.
const DIST_EXCELLENT_MIN: f64 = 0.6;
const DIST_GOOD_MIN: f64 = 0.4;
const DIST_FAIR_MIN: f64 = 0.3;

/// Distribution labels.
pub const DIST_LABEL_EXCELLENT: &str = "Excellent (>0.6)";
/// See [`DIST_LABEL_EXCELLENT`].
pub const DIST_LABEL_GOOD: &str = "Good (0.4-0.6)";
/// See [`DIST_LABEL_EXCELLENT`].
pub const DIST_LABEL_FAIR: &str = "Fair (0.3-0.4)";
/// See [`DIST_LABEL_EXCELLENT`].
pub const DIST_LABEL_POOR: &str = "Poor (<0.3)";

/// Issue severity thresholds.
const ISSUE_SEVERITY_FAIR_MAX: f64 = 0.4;
const ISSUE_SEVERITY_POOR_MAX: f64 = 0.3;

/// Severity labels.
pub const SEVERITY_POOR: &str = "poor";
/// See [`SEVERITY_POOR`].
pub const SEVERITY_FAIR: &str = "fair";
/// See [`SEVERITY_POOR`].
pub const SEVERITY_GOOD: &str = "good";

/// A distribution bucket as rendered in the section.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionItem {
    /// Bucket label.
    pub label: String,
    /// Percentage of functions in the bucket (0-100).
    pub percent: f64,
    /// Count of functions.
    pub count: usize,
}

/// A single issue row.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Function name.
    pub name: String,
    /// Source location.
    pub location: String,
    /// Formatted cohesion value (a fixed two-decimal string; the *string* is
    /// also the sort key, see [`ReportSection::all_issues`]).
    pub value: String,
    /// Severity label.
    pub severity: String,
}

/// The cohesion terminal section.
#[derive(Debug, Clone)]
pub struct ReportSection {
    /// Section title.
    pub title: String,
    /// Status message.
    pub message: String,
    /// Cohesion score used for the section header.
    pub score_value: f64,
    report: Report,
}

impl ReportSection {
    /// Builds a section from a cohesion report.
    #[must_use]
    pub fn new(report: Report) -> Self {
        let score = report
            .get("cohesion_score")
            .and_then(ReportValue::as_float)
            .unwrap_or(0.0);
        let msg = report
            .get("message")
            .and_then(ReportValue::as_str)
            .filter(|s| !s.is_empty())
            .map_or_else(|| DEFAULT_STATUS_MESSAGE.to_string(), ToString::to_string);
        ReportSection {
            title: SECTION_TITLE.to_string(),
            message: msg,
            score_value: score,
            report,
        }
    }

    fn functions(&self) -> &[std::collections::BTreeMap<String, ReportValue>] {
        self.report
            .get("functions")
            .and_then(ReportValue::as_functions)
            .unwrap_or(&[])
    }

    /// Distribution buckets. Returns empty when no functions.
    #[must_use]
    pub fn distribution(&self) -> Vec<DistributionItem> {
        let functions = self.functions();
        if functions.is_empty() {
            return Vec::new();
        }
        let mut excellent = 0usize;
        let mut good = 0usize;
        let mut fair = 0usize;
        let mut poor = 0usize;
        for f in functions {
            let coh = f
                .get("cohesion")
                .and_then(ReportValue::as_float)
                .unwrap_or(0.0);
            if coh >= DIST_EXCELLENT_MIN {
                excellent += 1;
            } else if coh >= DIST_GOOD_MIN {
                good += 1;
            } else if coh >= DIST_FAIR_MIN {
                fair += 1;
            } else {
                poor += 1;
            }
        }
        let total = functions.len();
        let pct = |c: usize| (c as f64) * 100.0 / (total as f64);
        vec![
            DistributionItem {
                label: DIST_LABEL_EXCELLENT.into(),
                percent: pct(excellent),
                count: excellent,
            },
            DistributionItem {
                label: DIST_LABEL_GOOD.into(),
                percent: pct(good),
                count: good,
            },
            DistributionItem {
                label: DIST_LABEL_FAIR.into(),
                percent: pct(fair),
                count: fair,
            },
            DistributionItem {
                label: DIST_LABEL_POOR.into(),
                percent: pct(poor),
                count: poor,
            },
        ]
    }

    /// All issues, sorted by formatted cohesion value ascending.
    #[must_use]
    pub fn all_issues(&self) -> Vec<Issue> {
        self.sorted_issues(0)
    }

    /// Top `n` issues; `n == 0` means all.
    #[must_use]
    pub fn top_issues(&self, n: usize) -> Vec<Issue> {
        self.sorted_issues(n)
    }

    fn sorted_issues(&self, limit: usize) -> Vec<Issue> {
        let functions = self.functions();
        if functions.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Issue> = functions
            .iter()
            .map(|f| {
                let coh = f
                    .get("cohesion")
                    .and_then(ReportValue::as_float)
                    .unwrap_or(0.0);
                let name = f
                    .get("name")
                    .and_then(ReportValue::as_str)
                    .unwrap_or("")
                    .to_string();
                let location = f
                    .get(crate::metrics::SOURCE_FILE_KEY)
                    .and_then(ReportValue::as_str)
                    .unwrap_or("")
                    .to_string();
                Issue {
                    name,
                    location,
                    value: format_float(coh),
                    severity: severity_for_cohesion(coh).to_string(),
                }
            })
            .collect();
        // The report contract orders issues by the formatted string `value`
        // ascending via the pinned unstable sort (pdqsort). Replicate both the
        // string key and the exact tie permutation so the emitted byte order
        // matches; numeric sorting or a stable sort diverges on ties.
        cf_gosort::go_sort_slice(&mut out, |a, b| a.value < b.value);
        if limit > 0 && out.len() > limit {
            out.truncate(limit);
        }
        out
    }
}

/// Maps a cohesion value to a severity label.
#[must_use]
pub fn severity_for_cohesion(coh: f64) -> &'static str {
    if coh < ISSUE_SEVERITY_POOR_MAX {
        SEVERITY_POOR
    } else if coh < ISSUE_SEVERITY_FAIR_MAX {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// Formats a float with two decimals for the terminal renderer. Cosmetic only.
#[must_use]
pub fn format_float(v: f64) -> String {
    format!("{v:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report_value::ReportValue;
    use std::collections::BTreeMap;

    fn report(funcs: &[(&str, f64)], score: f64, msg: &str) -> Report {
        let mut r = Report::new();
        r.insert("cohesion_score".into(), ReportValue::Float(score));
        if !msg.is_empty() {
            r.insert("message".into(), ReportValue::Str(msg.into()));
        }
        let functions: Vec<BTreeMap<String, ReportValue>> = funcs
            .iter()
            .map(|(n, c)| {
                let mut m = BTreeMap::new();
                m.insert("name".into(), ReportValue::Str((*n).into()));
                m.insert("cohesion".into(), ReportValue::Float(*c));
                m
            })
            .collect();
        r.insert("functions".into(), ReportValue::Functions(functions));
        r
    }

    #[test]
    fn default_message_when_empty() {
        let s = ReportSection::new(report(&[], 0.0, ""));
        assert_eq!(s.message, DEFAULT_STATUS_MESSAGE);
        assert_eq!(s.title, "COHESION");
    }

    #[test]
    fn distribution_buckets() {
        let s = ReportSection::new(report(
            &[("a", 0.9), ("b", 0.5), ("c", 0.35), ("d", 0.1)],
            0.5,
            "m",
        ));
        let d = s.distribution();
        assert_eq!(d[0].count, 1); // excellent
        assert_eq!(d[1].count, 1); // good
        assert_eq!(d[2].count, 1); // fair
        assert_eq!(d[3].count, 1); // poor
        assert!((d[0].percent - 25.0).abs() < 1e-9);
    }

    #[test]
    fn issues_sorted_worst_first() {
        let s = ReportSection::new(report(&[("a", 0.9), ("b", 0.1)], 0.5, "m"));
        let issues = s.all_issues();
        assert_eq!(issues[0].name, "b");
        assert_eq!(issues[0].severity, SEVERITY_POOR);
        assert_eq!(issues[1].name, "a");
        assert_eq!(issues[1].severity, SEVERITY_GOOD);
    }

    #[test]
    fn top_issues_limits() {
        let s = ReportSection::new(report(&[("a", 0.9), ("b", 0.5), ("c", 0.1)], 0.5, "m"));
        assert_eq!(s.top_issues(2).len(), 2);
        assert_eq!(s.top_issues(0).len(), 3);
    }

    #[test]
    fn severity_thresholds() {
        assert_eq!(severity_for_cohesion(0.29), SEVERITY_POOR);
        assert_eq!(severity_for_cohesion(0.3), SEVERITY_FAIR);
        assert_eq!(severity_for_cohesion(0.39), SEVERITY_FAIR);
        assert_eq!(severity_for_cohesion(0.4), SEVERITY_GOOD);
    }
}
