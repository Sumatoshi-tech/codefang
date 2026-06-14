//! Default static renderer.
//!
//! Provides the high-level text/compact/JSON rendering entry points used by the
//! `run` command for static-analysis output. The methods are provided as
//! inherent methods (rather than via the `cf-analyze` static-renderer trait)
//! with the same shapes.

use std::fmt::Write as _;

use crate::analyze::ReportSection;
use crate::json::{sections_to_json, JsonReport};
use crate::section_renderer::SectionRenderer;
use crate::summary::{ExecutiveSummary, MIN_SECTIONS_FOR_SUMMARY};
use crate::terminal;

/// Default implementation of static rendering using the renderer and terminal
/// helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultStaticRenderer;

impl DefaultStaticRenderer {
    /// Creates a `DefaultStaticRenderer`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Converts report sections to the JSON-serializable [`JsonReport`].
    /// Returns the owned report; callers enrich per-file data by mutating
    /// `report.sections[_].files` directly.
    #[must_use]
    pub fn sections_to_json(&self, sections: &[&dyn ReportSection]) -> JsonReport {
        sections_to_json(sections)
    }

    /// Renders human-readable text output for the given sections.
    #[must_use]
    pub fn render_text(&self, sections: &[&dyn ReportSection], verbose: bool, no_color: bool) -> String {
        let mut config = terminal::Config::new();
        if no_color {
            config.no_color = true;
        }
        let section_renderer = SectionRenderer::new(config.width, verbose, config.no_color);

        let mut out = String::new();

        if sections.len() >= MIN_SECTIONS_FOR_SUMMARY {
            let summary = ExecutiveSummary::new(sections);
            let _ = writeln!(out, "{}", section_renderer.render_summary(&summary));
        }

        for section in sections {
            out.push('\n');
            let _ = writeln!(out, "{}", section_renderer.render(*section));
        }

        out
    }

    /// Renders single-line-per-section compact output.
    #[must_use]
    pub fn render_compact(&self, sections: &[&dyn ReportSection], no_color: bool) -> String {
        let mut config = terminal::Config::new();
        if no_color {
            config.no_color = true;
        }
        let section_renderer = SectionRenderer::new(config.width, false, config.no_color);

        let mut out = String::new();
        for section in sections {
            let _ = writeln!(out, "{}", section_renderer.render_compact(*section));
        }
        out
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

    #[test]
    fn sections_to_json_delegates() {
        let a = mock("COMPLEXITY", 0.8, "Good");
        let sections: Vec<&dyn ReportSection> = vec![&a];
        let report = DefaultStaticRenderer::new().sections_to_json(&sections);
        assert_eq!(report.sections.len(), 1);
        assert_eq!(report.sections[0].title, "COMPLEXITY");
    }

    #[test]
    fn render_text_includes_summary_when_multiple() {
        let a = mock("COMPLEXITY", 0.8, "Good");
        let b = mock("COMMENTS", 0.6, "Fair");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let out = DefaultStaticRenderer::new().render_text(&sections, false, true);
        assert!(out.contains("CODE ANALYSIS REPORT"));
        assert!(out.contains("COMPLEXITY"));
        assert!(out.contains("COMMENTS"));
    }

    #[test]
    fn render_text_single_section_no_summary() {
        let a = mock("COMPLEXITY", 0.8, "Good");
        let sections: Vec<&dyn ReportSection> = vec![&a];
        let out = DefaultStaticRenderer::new().render_text(&sections, false, true);
        assert!(!out.contains("CODE ANALYSIS REPORT"));
        assert!(out.contains("COMPLEXITY"));
    }

    #[test]
    fn render_compact_one_line_per_section() {
        let a = mock("COMPLEXITY", 0.8, "Good");
        let b = mock("COMMENTS", 0.6, "Fair");
        let sections: Vec<&dyn ReportSection> = vec![&a, &b];
        let out = DefaultStaticRenderer::new().render_compact(&sections, true);
        assert_eq!(out.lines().count(), 2);
        assert!(out.contains("COMPLEXITY"));
        assert!(out.contains("COMMENTS"));
    }
}
