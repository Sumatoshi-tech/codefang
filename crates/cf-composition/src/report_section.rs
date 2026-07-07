//! Composition report section.
//!
//! Builds the human-facing section model (title, status, key metrics,
//! distribution, issues) from an aggregated composition report.
//!
//! This section feeds terminal/HTML rendering, which is **non-binding,
//! cosmetic** output — not part of the byte-identity contract. The logic is
//! nonetheless unit-tested so the section behaviour stays stable.

use std::collections::HashMap;

use crate::category::{Category, ALL_CATEGORIES};

/// Section title shown in rendered output.
pub const SECTION_TITLE: &str = "COMPOSITION";
const METRIC_TOTAL_FILES: &str = "Total Files";
const METRIC_SOURCE: &str = "Source Files";
const METRIC_SOURCE_PCT: &str = "Source %";

/// Status message when at least one file was analyzed.
pub const STATUS_DEFAULT: &str = "File composition analysis completed";
/// Status message when no files were analyzed.
pub const STATUS_EMPTY: &str = "No files analyzed";

/// Score sentinel meaning "informational only" (no pass/fail grade).
///
/// Kept here as the local constant the section reports until
/// `cf-analyzers-common` exposes the shared value.
pub const SCORE_INFO_ONLY: f64 = -1.0;

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

/// A category distribution item.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionItem {
    /// Category label.
    pub label: String,
    /// Share of total files, as a percentage.
    pub percent: f64,
    /// Absolute file count.
    pub count: i64,
}

/// A reported issue (non-source category breakdown line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Category name.
    pub name: String,
    /// Human-formatted "<n> files (<pct>%)" value.
    pub value: String,
    /// Severity string.
    pub severity: String,
}

/// Aggregated composition report in the shape the section consumes.
///
/// This is the decoded form of the aggregator report: a total count plus the
/// per-category breakdown. It deliberately does not depend on `cf-gojson`
/// because the section is cosmetic and is built directly from typed data.
#[derive(Debug, Clone, Default)]
pub struct CompositionReport {
    /// Total files analyzed (`total_files`).
    pub total_files: i64,
    /// Per-category counts (`breakdown`).
    pub breakdown: HashMap<String, i64>,
}

/// Implements the composition report section.
#[derive(Debug, Clone)]
pub struct ReportSection {
    title: String,
    message: String,
    score: f64,
    report: CompositionReport,
}

const PERCENT_MULTIPLIER: f64 = 100.0;

impl ReportSection {
    /// Builds a section from an aggregated composition report.
    ///
    /// The status message is [`STATUS_EMPTY`] when `total_files == 0`,
    /// otherwise [`STATUS_DEFAULT`]. A missing report maps to
    /// [`CompositionReport::default`], which yields the empty status.
    #[must_use]
    pub fn new(report: CompositionReport) -> Self {
        let message = if report.total_files == 0 {
            STATUS_EMPTY
        } else {
            STATUS_DEFAULT
        };

        Self {
            title: SECTION_TITLE.to_string(),
            message: message.to_string(),
            score: SCORE_INFO_ONLY,
            report,
        }
    }

    /// Returns the section title.
    #[must_use]
    pub fn section_title(&self) -> &str {
        &self.title
    }

    /// Returns the status message.
    #[must_use]
    pub fn status_message(&self) -> &str {
        &self.message
    }

    /// Returns the section score (always [`SCORE_INFO_ONLY`]).
    #[must_use]
    pub fn score(&self) -> f64 {
        self.score
    }

    /// Returns the ordered key metrics for display: total files, source-file
    /// count, and source percentage (formatted to one decimal place with a
    /// trailing `%`).
    #[must_use]
    pub fn key_metrics(&self) -> Vec<Metric> {
        let total = self.report.total_files;
        let source_count = self.breakdown_count(Category::Source);

        vec![
            Metric {
                label: METRIC_TOTAL_FILES.to_string(),
                value: format_int(total),
            },
            Metric {
                label: METRIC_SOURCE.to_string(),
                value: format_int(source_count),
            },
            Metric {
                label: METRIC_SOURCE_PCT.to_string(),
                value: format_percent(pct(source_count, total)),
            },
        ]
    }

    /// Returns the category breakdown as distribution items, in
    /// [`ALL_CATEGORIES`] order, omitting zero-count categories.
    ///
    /// Returns an empty vector when no files were analyzed.
    #[must_use]
    pub fn distribution(&self) -> Vec<DistributionItem> {
        let total = self.report.total_files;
        if total == 0 {
            return Vec::new();
        }

        let mut items = Vec::with_capacity(ALL_CATEGORIES.len());
        for cat in ALL_CATEGORIES {
            let count = self.breakdown_count(cat);
            if count == 0 {
                continue;
            }
            items.push(DistributionItem {
                label: cat.as_str().to_string(),
                percent: pct(count, total),
                count,
            });
        }
        items
    }

    /// Returns up to `n` non-source categories as issues (`n == 0` -> all).
    #[must_use]
    pub fn top_issues(&self, n: usize) -> Vec<Issue> {
        self.build_issues(n)
    }

    /// Returns all non-source categories as issues.
    #[must_use]
    pub fn all_issues(&self) -> Vec<Issue> {
        self.build_issues(0)
    }

    /// Builds issues for non-source categories with non-zero counts: skips
    /// `source`, skips zero counts, formats the value as
    /// `"<count> files (<pct>%)"` with one decimal place, and truncates to
    /// `limit` when `limit > 0`.
    fn build_issues(&self, limit: usize) -> Vec<Issue> {
        let total = self.report.total_files;
        if total == 0 {
            return Vec::new();
        }

        let mut issues = Vec::with_capacity(ALL_CATEGORIES.len());
        for cat in ALL_CATEGORIES {
            if cat == Category::Source {
                continue;
            }
            let count = self.breakdown_count(cat);
            if count == 0 {
                continue;
            }
            let percent = (count as f64) / (total as f64) * PERCENT_MULTIPLIER;
            issues.push(Issue {
                name: cat.as_str().to_string(),
                value: format!("{count} files ({percent:.1}%)"),
                severity: severity_for_category(cat).to_string(),
            });
        }

        if limit > 0 && issues.len() > limit {
            issues.truncate(limit);
        }
        issues
    }

    /// Looks up a category count in the breakdown (zero if absent).
    fn breakdown_count(&self, cat: Category) -> i64 {
        self.report
            .breakdown
            .get(cat.as_str())
            .copied()
            .unwrap_or(0)
    }
}

/// Returns the severity for a file category: binary is [`SEVERITY_POOR`],
/// everything else is [`SEVERITY_INFO`].
#[must_use]
pub fn severity_for_category(cat: Category) -> &'static str {
    match cat {
        Category::Binary => SEVERITY_POOR,
        _ => SEVERITY_INFO,
    }
}

/// Percentage of `count` out of `total` (0.0 when `total == 0`).
fn pct(count: i64, total: i64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (count as f64) / (total as f64) * PERCENT_MULTIPLIER
}

/// Formats an integer for display.
fn format_int(value: i64) -> String {
    value.to_string()
}

/// Formats a percentage to one decimal place with a trailing `%`.
fn format_percent(value: f64) -> String {
    format!("{value:.1}%")
}
