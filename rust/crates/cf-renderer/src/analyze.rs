//! Minimal port of the `internal/analyzers/analyze` types the renderer
//! depends on: the [`ReportSection`] trait, its data records ([`Metric`],
//! [`DistributionItem`], [`Issue`]), the severity/score constants, and the
//! [`BaseReportSection`] default implementation.
//!
//! The full analyze crate (`cf-analyze`) is still a scaffold in this
//! workspace, so the renderer's dependency surface is reproduced here. When
//! `cf-analyze` lands, replace this module with a dependency on it (see
//! `Cargo.toml`) and re-export these types.

use crate::terminal;

/// A section has no score (info only). Mirrors Go's `ScoreInfoOnly`.
pub const SCORE_INFO_ONLY: f64 = -1.0;

/// The label shown for info-only sections. Mirrors Go's `ScoreLabelInfo`.
pub const SCORE_LABEL_INFO: &str = "Info";

/// Severity classification for an [`Issue`]. The string values match Go's
/// `Severity*` constants exactly, so they appear unchanged in machine output.
pub mod severity {
    /// Good severity.
    pub const GOOD: &str = "good";
    /// Fair severity.
    pub const FAIR: &str = "fair";
    /// Poor severity.
    pub const POOR: &str = "poor";
    /// Info severity.
    pub const INFO: &str = "info";
}

/// A key-value metric for display. Mirrors Go's `analyze.Metric`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metric {
    /// Display label (e.g. "Total Functions").
    pub label: String,
    /// Pre-formatted value (e.g. "156").
    pub value: String,
}

/// A category in a distribution chart. Mirrors Go's `analyze.DistributionItem`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DistributionItem {
    /// Category label (e.g. "Simple (1-5)").
    pub label: String,
    /// Percentage as `0..1`.
    pub percent: f64,
    /// Absolute count.
    pub count: i64,
}

/// A problem or item to highlight. Mirrors Go's `analyze.Issue`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Issue {
    /// Item name (e.g. a function name).
    pub name: String,
    /// File location (e.g. "pkg/foo/bar.go:42").
    pub location: String,
    /// Metric value (e.g. "12").
    pub value: String,
    /// One of [`severity`]'s constants.
    pub severity: String,
}

/// A standardized structure for analyzer reports. Analyzers implement this to
/// enable unified rendering. Mirrors Go's `analyze.ReportSection` interface.
pub trait ReportSection {
    /// The display title (e.g. "COMPLEXITY").
    fn section_title(&self) -> String;

    /// A `0..1` score, or [`SCORE_INFO_ONLY`] for info-only sections.
    fn score(&self) -> f64;

    /// The formatted score (e.g. "8/10" or "Info").
    fn score_label(&self) -> String {
        if self.score() < 0.0 {
            SCORE_LABEL_INFO.to_string()
        } else {
            terminal::format_score(self.score())
        }
    }

    /// A summary message (e.g. "Good - reasonable complexity").
    fn status_message(&self) -> String;

    /// Ordered key metrics for display. Empty by default.
    fn key_metrics(&self) -> Vec<Metric> {
        Vec::new()
    }

    /// Distribution data for bar charts. Empty by default.
    fn distribution(&self) -> Vec<DistributionItem> {
        Vec::new()
    }

    /// The top `n` issues/items to highlight. Empty by default.
    fn top_issues(&self, _n: usize) -> Vec<Issue> {
        Vec::new()
    }

    /// All issues, for verbose mode. Empty by default.
    fn all_issues(&self) -> Vec<Issue> {
        Vec::new()
    }
}

/// Default field-bearing implementation analyzers can build on. Mirrors Go's
/// `analyze.BaseReportSection`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BaseReportSection {
    /// Display title.
    pub title: String,
    /// Summary message.
    pub message: String,
    /// `0..1` score, or [`SCORE_INFO_ONLY`].
    pub score_value: f64,
}

impl ReportSection for BaseReportSection {
    fn section_title(&self) -> String {
        self.title.clone()
    }

    fn score(&self) -> f64 {
        self.score_value
    }

    fn status_message(&self) -> String {
        self.message.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_score_label_formats_and_info() {
        let good = BaseReportSection {
            title: "C".into(),
            message: "m".into(),
            score_value: 0.8,
        };
        assert_eq!(good.score_label(), "8/10");

        let info = BaseReportSection {
            title: "I".into(),
            message: "m".into(),
            score_value: SCORE_INFO_ONLY,
        };
        assert_eq!(info.score_label(), "Info");
    }

    #[test]
    fn base_defaults_are_empty() {
        let s = BaseReportSection::default();
        assert!(s.key_metrics().is_empty());
        assert!(s.distribution().is_empty());
        assert!(s.top_issues(5).is_empty());
        assert!(s.all_issues().is_empty());
    }
}
