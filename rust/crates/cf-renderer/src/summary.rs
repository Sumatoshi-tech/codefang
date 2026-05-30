//! Executive summary model and rendering. Port of the Go `renderer/summary.go`.

use crate::analyze::{ReportSection, SCORE_INFO_ONLY, SCORE_LABEL_INFO};
use crate::section_renderer::{SectionRenderer, INDENT_WIDTH, SEPARATOR_WIDTH_VALUE};
use crate::terminal::{self, Color};

/// The minimum number of sections before an executive summary is rendered.
/// Mirrors Go's `MinSectionsForSummary`.
pub const MIN_SECTIONS_FOR_SUMMARY: usize = 2;

/// Summary layout/label constants (mirror Go's `summary.go`).
pub const SUMMARY_TITLE: &str = "CODE ANALYSIS REPORT";
/// Prefix for the overall-score header text.
pub const SUMMARY_OVERALL_PREFIX: &str = "Overall: ";
/// Analyzer column header.
pub const SUMMARY_ANALYZER_COL: &str = "Analyzer";
/// Score column header.
pub const SUMMARY_SCORE_COL: &str = "Score";
/// Status column header.
pub const SUMMARY_STATUS_COL: &str = "Status";
/// Analyzer column width.
pub const SUMMARY_ANALYZER_WIDTH: usize = 16;
/// Score column width.
pub const SUMMARY_SCORE_WIDTH: usize = 7;

/// Holds data for the executive summary report. Mirrors Go's `ExecutiveSummary`.
///
/// Borrows the sections for the lifetime of the summary, matching the Go code
/// which stores `[]analyze.ReportSection`.
pub struct ExecutiveSummary<'a> {
    /// The report sections included in the summary.
    pub sections: Vec<&'a dyn ReportSection>,
}

impl<'a> ExecutiveSummary<'a> {
    /// Creates an executive summary from report sections. Mirrors
    /// `NewExecutiveSummary` (a nil slice becomes an empty one).
    pub fn new(sections: &[&'a dyn ReportSection]) -> Self {
        ExecutiveSummary {
            sections: sections.to_vec(),
        }
    }

    /// Returns the average score of all scored sections, excluding info-only
    /// sections. Returns [`SCORE_INFO_ONLY`] when there are no scored sections.
    /// Mirrors `(ExecutiveSummary).OverallScore`.
    pub fn overall_score(&self) -> f64 {
        let mut total = 0.0;
        let mut count = 0usize;
        for section in &self.sections {
            let score = section.score();
            if score >= 0.0 {
                total += score;
                count += 1;
            }
        }
        if count == 0 {
            return SCORE_INFO_ONLY;
        }
        total / count as f64
    }

    /// Returns the formatted overall score ("N/10" or "Info"). Mirrors
    /// `(ExecutiveSummary).OverallScoreLabel`.
    pub fn overall_score_label(&self) -> String {
        let score = self.overall_score();
        if score < 0.0 {
            return SCORE_LABEL_INFO.to_string();
        }
        terminal::format_score(score)
    }
}

impl SectionRenderer {
    /// Produces the executive summary output. Port of
    /// `(SectionRenderer).RenderSummary`.
    pub fn render_summary(&self, summary: &ExecutiveSummary) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(4 + summary.sections.len());

        // Header with title and overall score.
        let title = self.config.colorize(SUMMARY_TITLE, Color::Blue);
        let overall_score = summary.overall_score();
        let mut overall_label = summary.overall_score_label();
        if overall_score >= 0.0 {
            overall_label = self
                .config
                .colorize(&overall_label, terminal::color_for_score(overall_score));
        }
        let right_text = format!("{SUMMARY_OVERALL_PREFIX}{overall_label}");
        parts.push(terminal::draw_header(&title, &right_text, self.config.width));

        // Column headers.
        let indent = " ".repeat(INDENT_WIDTH);
        let header_row = format!(
            "{}{}{}{}",
            indent,
            terminal::pad_right(SUMMARY_ANALYZER_COL, SUMMARY_ANALYZER_WIDTH),
            terminal::pad_right(SUMMARY_SCORE_COL, SUMMARY_SCORE_WIDTH),
            SUMMARY_STATUS_COL,
        );
        let header_row = self.config.colorize(&header_row, Color::Gray);
        parts.push(String::new());
        parts.push(header_row);

        // Separator.
        let separator_width = self
            .config
            .width
            .saturating_sub(INDENT_WIDTH * SEPARATOR_WIDTH_VALUE);
        parts.push(format!("{}{}", indent, terminal::draw_separator(separator_width)));

        // Analyzer rows.
        for section in &summary.sections {
            let name = terminal::pad_right(&section.section_title(), SUMMARY_ANALYZER_WIDTH);
            let mut score = section.score_label();
            let section_score = section.score();
            if section_score >= 0.0 {
                score = self
                    .config
                    .colorize(&score, terminal::color_for_score(section_score));
            }
            let score = terminal::pad_right(&score, SUMMARY_SCORE_WIDTH);
            let message = section.status_message();
            parts.push(format!("{indent}{name}{score}{message}"));
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::BaseReportSection;

    fn mock(title: &str, score: f64, msg: &str) -> BaseReportSection {
        BaseReportSection {
            title: title.into(),
            message: msg.into(),
            score_value: score,
        }
    }

    /// Port of `TestNewExecutiveSummary_StoresSections`.
    #[test]
    fn new_stores_sections() {
        let a = mock("COMPLEXITY", 0.8, "Good");
        let b = mock("COMMENTS", 0.6, "Fair");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let summary = ExecutiveSummary::new(&sections);
        assert_eq!(summary.sections.len(), 2);
        assert_eq!(summary.sections[0].section_title(), "COMPLEXITY");
        assert_eq!(summary.sections[1].section_title(), "COMMENTS");
    }

    /// Port of `TestNewExecutiveSummary_Empty`.
    #[test]
    fn new_empty() {
        let summary = ExecutiveSummary::new(&[]);
        assert!(summary.sections.is_empty());
    }

    /// Port of `TestOverallScore_*`.
    #[test]
    fn overall_score_variants() {
        let a = mock("C", 0.8, "Good");
        let s1: Vec<&dyn ReportSection> = vec![&a];
        assert!((ExecutiveSummary::new(&s1).overall_score() - 0.8).abs() < 0.001);

        let b = mock("D", 0.6, "Fair");
        let s2: Vec<&dyn ReportSection> = vec![&a, &b];
        assert!((ExecutiveSummary::new(&s2).overall_score() - 0.7).abs() < 0.001);

        let info = mock("I", SCORE_INFO_ONLY, "i");
        let s3: Vec<&dyn ReportSection> = vec![&a, &info];
        assert!((ExecutiveSummary::new(&s3).overall_score() - 0.8).abs() < 0.001);

        let s4: Vec<&dyn ReportSection> = vec![&info];
        assert!((ExecutiveSummary::new(&s4).overall_score() - SCORE_INFO_ONLY).abs() < 0.001);

        assert!((ExecutiveSummary::new(&[]).overall_score() - SCORE_INFO_ONLY).abs() < 0.001);
    }

    /// Port of `TestOverallScoreLabel_*`.
    #[test]
    fn overall_score_label_variants() {
        let a = mock("C", 0.8, "Good");
        let s1: Vec<&dyn ReportSection> = vec![&a];
        assert_eq!(ExecutiveSummary::new(&s1).overall_score_label(), "8/10");

        let info = mock("I", SCORE_INFO_ONLY, "i");
        let s2: Vec<&dyn ReportSection> = vec![&info];
        assert_eq!(ExecutiveSummary::new(&s2).overall_score_label(), "Info");
    }

    /// Port of `TestRenderSummary_*` (no-color path).
    #[test]
    fn render_summary_contents() {
        let a = mock("COMPLEXITY", 0.8, "Good - reasonable complexity");
        let b = mock("COMMENTS", 0.6, "Fair");
        let info = mock("IMPORTS", SCORE_INFO_ONLY, "5 imports");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b, &info];
        let summary = ExecutiveSummary::new(&sections);
        let r = SectionRenderer::new(80, false, true);
        let out = r.render_summary(&summary);
        assert!(out.contains(SUMMARY_TITLE));
        assert!(out.contains("Overall: 7/10"));
        assert!(out.contains(SUMMARY_ANALYZER_COL));
        assert!(out.contains(SUMMARY_SCORE_COL));
        assert!(out.contains(SUMMARY_STATUS_COL));
        assert!(out.contains("COMPLEXITY"));
        assert!(out.contains("COMMENTS"));
        assert!(out.contains("IMPORTS"));
        assert!(out.contains("8/10"));
        assert!(out.contains("6/10"));
        assert!(out.contains("Good - reasonable complexity"));
        assert!(out.contains("Info"));
        assert!(out.contains("5 imports"));
    }

    /// Port of `TestRenderSummary_EmptySections`.
    #[test]
    fn render_summary_empty() {
        let summary = ExecutiveSummary::new(&[]);
        let r = SectionRenderer::new(80, false, true);
        assert!(r.render_summary(&summary).contains(SUMMARY_TITLE));
    }

    /// Port of `TestRenderSummary_ColorEnabled/Disabled` and
    /// `_ScoreRowsColored`.
    #[test]
    fn render_summary_colors() {
        let good = mock("GOOD", 0.9, "Good");
        let poor = mock("POOR", 0.3, "Poor");
        let sections: Vec<&dyn ReportSection> = vec![&good, &poor];
        let summary = ExecutiveSummary::new(&sections);

        let colored = SectionRenderer::new(80, false, false).render_summary(&summary);
        assert!(colored.contains("\u{001b}["));
        assert!(colored.contains("\u{001b}[32m")); // green for good
        assert!(colored.contains("\u{001b}[31m")); // red for poor

        let plain = SectionRenderer::new(80, false, true).render_summary(&summary);
        assert!(!plain.contains("\u{001b}["));
    }
}
