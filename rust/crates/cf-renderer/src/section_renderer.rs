//! Terminal section rendering. Port of the Go `renderer/renderer.go`.
//!
//! Renders an [`analyze::ReportSection`](crate::analyze::ReportSection) into
//! human-readable terminal output (header box, summary line, key metrics,
//! distribution bars, and issues). Terminal output is non-binding/cosmetic per
//! DESIGN.md §2.7, but this port reproduces the Go layout faithfully so the
//! ported tests are meaningful.

use crate::analyze::{self, ReportSection};
use crate::terminal::{self, Color, Config};

// --- Magic-number constants mirroring renderer.go ---

const LINES_VALUE: usize = 3;
const MAGIC2: usize = 2;
const MAGIC2_1: usize = 2;
const MAKE_ARG3: usize = 3;
/// Multiplier applied to the indent width when computing separator widths.
pub(crate) const SEPARATOR_WIDTH_VALUE: usize = 2;

/// Compact-mode bar width. Mirrors Go's `CompactBarWidth`.
pub const COMPACT_BAR_WIDTH: usize = 10;
/// Compact-mode title width. Mirrors Go's `CompactTitleWidth`.
pub const COMPACT_TITLE_WIDTH: usize = 12;

// --- Render layout constants (mirror renderer.go) ---

/// Indentation width used throughout terminal output.
pub const INDENT_WIDTH: usize = 2;
/// Prefix for the section summary line.
pub const SUMMARY_PREFIX: &str = "Summary: ";
/// "Key Metrics" section label.
pub const METRICS_LABEL: &str = "Key Metrics";
/// Number of metrics per row in the 2-column layout.
pub const METRICS_PER_ROW: usize = 2;
/// Width of the metric label column.
pub const METRIC_LABEL_WIDTH: usize = 20;
/// Width of the metric value column.
pub const METRIC_VALUE_WIDTH: usize = 12;
/// "Distribution" section label.
pub const DISTRIBUTION_LABEL: &str = "Distribution";
/// Width of distribution bars.
pub const DISTRIBUTION_BAR_WIDTH: usize = 40;
/// Width of the distribution label column.
pub const DIST_LABEL_WIDTH: usize = 18;
/// "Top Issues" section label.
pub const ISSUES_LABEL: &str = "Top Issues";
/// "All Issues" section label (verbose).
pub const ALL_ISSUES_LABEL: &str = "All Issues";
/// Number of issues shown in non-verbose mode.
pub const DEFAULT_TOP_ISSUES: usize = 5;
/// Width of the issue name column.
pub const ISSUE_NAME_WIDTH: usize = 25;
/// Width of the issue location column.
pub const ISSUE_LOCATION_WIDTH: usize = 35;

/// Maps an issue severity string to a terminal color. Port of
/// `ColorForSeverity`.
pub fn color_for_severity(severity: &str) -> Color {
    match severity {
        analyze::severity::GOOD => Color::Green,
        analyze::severity::FAIR => Color::Yellow,
        analyze::severity::POOR => Color::Red,
        _ => Color::Blue,
    }
}

/// Renders [`ReportSection`]s to formatted terminal output. Port of Go's
/// `SectionRenderer`.
#[derive(Debug, Clone, Copy)]
pub struct SectionRenderer {
    pub(crate) config: Config,
    pub(crate) verbose: bool,
}

impl SectionRenderer {
    /// Creates a renderer with the given configuration. Port of
    /// `NewSectionRenderer`.
    pub fn new(width: usize, verbose: bool, no_color: bool) -> Self {
        SectionRenderer {
            config: Config { width, no_color },
            verbose,
        }
    }

    /// Produces single-line compact output for narrow terminals. Port of
    /// `(SectionRenderer).RenderCompact`. Format: "Title [bar] N/10  Message".
    pub fn render_compact(&self, section: &dyn ReportSection) -> String {
        let title = terminal::pad_right(&section.section_title(), COMPACT_TITLE_WIDTH);
        let score_bar = terminal::format_score_bar(section.score(), COMPACT_BAR_WIDTH);
        let score_color = terminal::color_for_score(section.score());
        let score_bar = self.config.colorize(&score_bar, score_color);
        let message = section.status_message();
        format!("{title} {score_bar}  {message}")
    }

    /// Produces formatted output for a [`ReportSection`]. Port of
    /// `(SectionRenderer).Render`.
    pub fn render(&self, section: &dyn ReportSection) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Header with title and score.
        let title = self.config.colorize(&section.section_title(), Color::Blue);
        let mut score_text = format!("Score: {}", section.score_label());
        let score_color = terminal::color_for_score(section.score());
        score_text = self.config.colorize(&score_text, score_color);
        parts.push(terminal::draw_header(&title, &score_text, self.config.width));

        // Summary line.
        let indent = " ".repeat(INDENT_WIDTH);
        parts.push(format!(
            "\n{}{}{}",
            indent,
            SUMMARY_PREFIX,
            section.status_message()
        ));

        // Key Metrics section.
        let metrics = section.key_metrics();
        if !metrics.is_empty() {
            parts.push(self.render_metrics(&metrics, &indent));
        }

        // Distribution section.
        let distribution = section.distribution();
        if !distribution.is_empty() {
            parts.push(self.render_distribution(&distribution, &indent));
        }

        // Issues section.
        let (issues, issues_label) = if self.verbose {
            (section.all_issues(), ALL_ISSUES_LABEL)
        } else {
            (section.top_issues(DEFAULT_TOP_ISSUES), ISSUES_LABEL)
        };
        if !issues.is_empty() {
            parts.push(self.render_issues(&issues, issues_label, &indent));
        }

        parts.join("\n")
    }

    /// Renders the key metrics section in a 2-column layout. Port of
    /// `renderMetrics`.
    fn render_metrics(&self, metrics: &[analyze::Metric], indent: &str) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(String::new());
        let metrics_header = self.config.colorize(METRICS_LABEL, Color::Gray);
        lines.push(format!("{indent}{metrics_header}"));
        let separator_width = self
            .config
            .width
            .saturating_sub(INDENT_WIDTH * SEPARATOR_WIDTH_VALUE);
        lines.push(format!("{}{}", indent, terminal::draw_separator(separator_width)));

        let mut i = 0;
        while i < metrics.len() {
            let mut row = String::new();
            let mut j = 0;
            while j < METRICS_PER_ROW && i + j < metrics.len() {
                let m = &metrics[i + j];
                let label = terminal::pad_right(&m.label, METRIC_LABEL_WIDTH);
                let value = terminal::pad_right(&m.value, METRIC_VALUE_WIDTH);
                row.push_str(&label);
                row.push_str(&value);
                j += 1;
            }
            lines.push(format!("{indent}{row}"));
            i += METRICS_PER_ROW;
        }

        lines.join("\n")
    }

    /// Renders the distribution section with percent bars. Port of
    /// `renderDistribution`.
    fn render_distribution(&self, items: &[analyze::DistributionItem], indent: &str) -> String {
        let mut lines: Vec<String> = Vec::with_capacity(LINES_VALUE + items.len());
        lines.push(String::new());
        let dist_header = self.config.colorize(DISTRIBUTION_LABEL, Color::Gray);
        lines.push(format!("{indent}{dist_header}"));
        let separator_width = self.config.width.saturating_sub(INDENT_WIDTH * MAGIC2);
        lines.push(format!("{}{}", indent, terminal::draw_separator(separator_width)));

        for item in items {
            let bar = terminal::draw_percent_bar(
                &item.label,
                item.percent,
                item.count,
                DIST_LABEL_WIDTH,
                DISTRIBUTION_BAR_WIDTH,
            );
            lines.push(format!("{indent}{bar}"));
        }

        lines.join("\n")
    }

    /// Renders the issues section with the given label. Port of `renderIssues`.
    fn render_issues(&self, issues: &[analyze::Issue], label: &str, indent: &str) -> String {
        let mut lines: Vec<String> = Vec::with_capacity(MAKE_ARG3 + issues.len());
        lines.push(String::new());
        let issues_header = self.config.colorize(label, Color::Gray);
        lines.push(format!("{indent}{issues_header}"));
        let separator_width = self.config.width.saturating_sub(INDENT_WIDTH * MAGIC2_1);
        lines.push(format!("{}{}", indent, terminal::draw_separator(separator_width)));

        for issue in issues {
            let name = terminal::truncate_with_ellipsis(&issue.name, ISSUE_NAME_WIDTH);
            let name = terminal::pad_right(&name, ISSUE_NAME_WIDTH);
            let location = terminal::truncate_with_ellipsis(&issue.location, ISSUE_LOCATION_WIDTH);
            let location = terminal::pad_right(&location, ISSUE_LOCATION_WIDTH);
            let value_color = color_for_severity(&issue.severity);
            let colored_value = self.config.colorize(&issue.value, value_color);
            lines.push(format!("{indent}{name} {location} {colored_value}"));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{severity, BaseReportSection, DistributionItem, Issue, Metric};

    struct Mock {
        base: BaseReportSection,
        metrics: Vec<Metric>,
        distribution: Vec<DistributionItem>,
        issues: Vec<Issue>,
    }

    impl Mock {
        fn new(title: &str, score: f64, msg: &str) -> Self {
            Mock {
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

    impl ReportSection for Mock {
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
            if n > self.issues.len() {
                self.issues.clone()
            } else {
                self.issues[..n].to_vec()
            }
        }
        fn all_issues(&self) -> Vec<Issue> {
            self.issues.clone()
        }
    }

    fn complexity_mock() -> Mock {
        Mock::new("COMPLEXITY", 0.8, "Good - reasonable complexity")
    }

    /// Port of `TestNewSectionRenderer_*`.
    #[test]
    fn new_section_renderer_fields() {
        let r = SectionRenderer::new(80, false, false);
        assert_eq!(r.config.width, 80);
        assert!(!r.verbose);
        assert!(!r.config.no_color);

        assert!(SectionRenderer::new(80, true, false).verbose);
        assert!(SectionRenderer::new(80, false, true).config.no_color);
    }

    /// Port of `TestRenderCompact_*`.
    #[test]
    fn render_compact_contents() {
        let r = SectionRenderer::new(80, false, true);
        let out = r.render_compact(&complexity_mock());
        assert!(out.contains("COMPLEXITY"));
        assert!(out.contains('\u{2588}') || out.contains('\u{2591}'));
        assert!(out.contains("8/10"));
        assert!(out.contains("Good - reasonable complexity"));
    }

    /// Port of `TestRender_ContainsTitle/Score/Summary/HeaderBox`.
    #[test]
    fn render_basic_contents() {
        let r = SectionRenderer::new(80, false, true);
        let out = r.render(&complexity_mock());
        assert!(out.contains("COMPLEXITY"));
        assert!(out.contains("8/10"));
        assert!(out.contains("Good - reasonable complexity"));
        assert!(out.contains('\u{250f}'));
        assert!(out.contains('\u{2517}'));
    }

    /// Port of `TestRender_ContainsMetricsSection/Values` + `_EmptyMetrics`.
    #[test]
    fn render_metrics_section() {
        let mut m = complexity_mock();
        m.metrics = vec![
            Metric {
                label: "Total Functions".into(),
                value: "156".into(),
            },
            Metric {
                label: "Avg Complexity".into(),
                value: "3.2".into(),
            },
        ];
        let r = SectionRenderer::new(80, false, true);
        let out = r.render(&m);
        assert!(out.contains("Key Metrics"));
        assert!(out.contains("Total Functions"));
        assert!(out.contains("156"));

        // Empty metrics => no Key Metrics section.
        let out_empty = r.render(&complexity_mock());
        assert!(!out_empty.contains("Key Metrics"));
    }

    /// Port of `TestRender_ContainsDistributionSection/Bars` + `_Empty`.
    #[test]
    fn render_distribution_section() {
        let mut m = complexity_mock();
        m.distribution = vec![
            DistributionItem {
                label: "Simple (1-5)".into(),
                percent: 0.68,
                count: 106,
            },
            DistributionItem {
                label: "Moderate (6-10)".into(),
                percent: 0.28,
                count: 44,
            },
        ];
        let r = SectionRenderer::new(80, false, true);
        let out = r.render(&m);
        assert!(out.contains("Distribution"));
        assert!(out.contains('\u{2588}'));
        assert!(out.contains("68%"));

        let out_empty = r.render(&complexity_mock());
        assert!(!out_empty.contains("Distribution"));
    }

    fn many_issues_mock() -> Mock {
        let mut m = Mock::new("COMPLEXITY", 0.4, "Issues found");
        m.issues = (1..=7)
            .map(|n| Issue {
                name: format!("Func{n}"),
                location: String::new(),
                value: format!("CC={}", 19 - n),
                severity: if n <= 2 {
                    severity::POOR.into()
                } else {
                    severity::FAIR.into()
                },
            })
            .collect();
        m
    }

    /// Port of `TestRender_NonVerboseShowsTopIssues`,
    /// `_VerboseShowsAllIssues`, `_VerboseChangesLabel`, `_EmptyIssues`.
    #[test]
    fn render_issues_section() {
        let m = many_issues_mock();

        let non_verbose = SectionRenderer::new(80, false, true).render(&m);
        assert!(non_verbose.contains("Top Issues"));
        assert!(non_verbose.contains("Func1"));
        assert!(non_verbose.contains("Func5"));
        assert!(!non_verbose.contains("Func6"));

        let verbose = SectionRenderer::new(80, true, true).render(&m);
        assert!(verbose.contains("All Issues"));
        assert!(!verbose.contains("Top Issues"));
        assert!(verbose.contains("Func1"));
        assert!(verbose.contains("Func7"));

        let empty = SectionRenderer::new(80, false, true).render(&complexity_mock());
        assert!(!empty.contains("Top Issues"));
    }

    /// Port of `TestColorForSeverity_*`.
    #[test]
    fn color_for_severity_mapping() {
        assert_eq!(color_for_severity(severity::GOOD), Color::Green);
        assert_eq!(color_for_severity(severity::FAIR), Color::Yellow);
        assert_eq!(color_for_severity(severity::POOR), Color::Red);
        assert_eq!(color_for_severity(severity::INFO), Color::Blue);
        assert_eq!(color_for_severity("unknown"), Color::Blue);
    }

    /// Port of `TestRender_Color*` and `_MetricsHeaderMuted`.
    #[test]
    fn render_colors() {
        // Color enabled => ANSI present; title blue; good score green.
        let colored = SectionRenderer::new(80, false, false).render(&complexity_mock());
        assert!(colored.contains("\u{001b}["));
        assert!(colored.contains("\u{001b}[34m")); // blue title
        assert!(colored.contains("\u{001b}[32m")); // green good score

        // Color disabled => no ANSI.
        let plain = SectionRenderer::new(80, false, true).render(&complexity_mock());
        assert!(!plain.contains("\u{001b}["));

        // Fair/poor coloring.
        let fair = SectionRenderer::new(80, false, false).render(&Mock::new("T", 0.6, "Fair"));
        assert!(fair.contains("\u{001b}[33m"));
        let poor = SectionRenderer::new(80, false, false).render(&Mock::new("T", 0.3, "Poor"));
        assert!(poor.contains("\u{001b}[31m"));

        // Metrics header muted gray.
        let mut m = complexity_mock();
        m.metrics = vec![Metric {
            label: "X".into(),
            value: "1".into(),
        }];
        let muted = SectionRenderer::new(80, false, false).render(&m);
        assert!(muted.contains("\u{001b}[90m"));
    }

    /// Port of `TestRenderCompact_Color*`.
    #[test]
    fn render_compact_colors() {
        let colored = SectionRenderer::new(80, false, false).render_compact(&complexity_mock());
        assert!(colored.contains("\u{001b}["));
        let plain = SectionRenderer::new(80, false, true).render_compact(&complexity_mock());
        assert!(!plain.contains("\u{001b}["));
    }
}
