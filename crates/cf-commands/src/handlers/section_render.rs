//! Terminal `text` / `compact` rendering over a section report tree.
//!
//! Rather than re-derive each analyzer's section data, this adapter reads the
//! **same** JSON section tree every static handler already builds for
//! `--format json` (`{overall_score_label, sections:[{title, score_label,
//! status, metrics, distribution?, issues, score}], overall_score}`) and feeds
//! it to [`cf_renderer::DefaultStaticRenderer`], so the terminal formats show
//! exactly the metrics, distribution and issues the JSON report carries, in
//! the same order. The history `--format compact` lines
//! ([`super::history_formats::history_compact`]) are rendered through the same
//! tree.

use cf_gojson::{GoMap, GoValue};
use cf_renderer::analyze::{DistributionItem, Issue, Metric, ReportSection};
use cf_renderer::DefaultStaticRenderer;

/// A [`ReportSection`] backed by an analyzer's already-built JSON section
/// `GoValue`. The issue list is stored in render order, so
/// [`Self::top_issues`] is a prefix slice and [`Self::all_issues`] the whole
/// list.
struct JsonSection {
    title: String,
    score: f64,
    status: String,
    metrics: Vec<Metric>,
    distribution: Vec<DistributionItem>,
    issues: Vec<Issue>,
}

impl ReportSection for JsonSection {
    fn section_title(&self) -> String {
        self.title.clone()
    }

    fn score(&self) -> f64 {
        self.score
    }

    fn status_message(&self) -> String {
        self.status.clone()
    }

    fn key_metrics(&self) -> Vec<Metric> {
        self.metrics.clone()
    }

    fn distribution(&self) -> Vec<DistributionItem> {
        self.distribution.clone()
    }

    fn top_issues(&self, n: usize) -> Vec<Issue> {
        if n > 0 && self.issues.len() > n {
            self.issues[..n].to_vec()
        } else {
            self.issues.clone()
        }
    }

    fn all_issues(&self) -> Vec<Issue> {
        self.issues.clone()
    }
}

fn map_str(m: &GoMap, key: &str) -> String {
    match m.get(key) {
        Some(GoValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn map_f64(m: &GoMap, key: &str) -> f64 {
    match m.get(key) {
        Some(GoValue::Float(v)) => *v,
        Some(GoValue::Int(v)) => *v as f64,
        Some(GoValue::Uint(v)) => *v as f64,
        _ => 0.0,
    }
}

fn map_i64(m: &GoMap, key: &str) -> i64 {
    match m.get(key) {
        Some(GoValue::Int(v)) => *v,
        Some(GoValue::Uint(v)) => *v as i64,
        Some(GoValue::Float(v)) => *v as i64,
        _ => 0,
    }
}

fn array<'a>(m: &'a GoMap, key: &str) -> &'a [GoValue] {
    match m.get(key) {
        Some(GoValue::Array(a)) => a.as_slice(),
        _ => &[],
    }
}

fn parse_section(section: &GoValue) -> Option<JsonSection> {
    let GoValue::Map(sm) = section else {
        return None;
    };
    let metrics = array(sm, "metrics")
        .iter()
        .filter_map(|it| {
            let GoValue::Map(m) = it else { return None };
            Some(Metric {
                label: map_str(m, "label"),
                value: map_str(m, "value"),
            })
        })
        .collect();
    let distribution = array(sm, "distribution")
        .iter()
        .filter_map(|it| {
            let GoValue::Map(m) = it else { return None };
            Some(DistributionItem {
                label: map_str(m, "label"),
                percent: map_f64(m, "percent"),
                count: map_i64(m, "count"),
            })
        })
        .collect();
    let issues = array(sm, "issues")
        .iter()
        .filter_map(|it| {
            let GoValue::Map(m) = it else { return None };
            Some(Issue {
                name: map_str(m, "name"),
                location: map_str(m, "location"),
                value: map_str(m, "value"),
                severity: map_str(m, "severity"),
            })
        })
        .collect();
    Some(JsonSection {
        title: map_str(sm, "title"),
        score: map_f64(sm, "score"),
        status: map_str(sm, "status"),
        metrics,
        distribution,
        issues,
    })
}

/// Reads the `sections` array of a report tree into renderer sections,
/// preserving order (the executive summary + multi-analyzer text/compact
/// depend on it).
fn sections_from_report(report: &GoValue) -> Vec<JsonSection> {
    let GoValue::Map(rm) = report else {
        return Vec::new();
    };
    array(rm, "sections")
        .iter()
        .filter_map(parse_section)
        .collect()
}

/// Whether `NO_COLOR` is set (`--no-color` sets it for the run).
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

/// `--format compact` bytes for a report tree: one line per section.
#[must_use]
pub fn render_compact_report(report: &GoValue) -> Vec<u8> {
    let sections = sections_from_report(report);
    let refs: Vec<&dyn ReportSection> = sections.iter().map(|s| s as &dyn ReportSection).collect();
    DefaultStaticRenderer::new()
        .render_compact(&refs, no_color())
        .into_bytes()
}

/// `--format text` bytes for a report tree: an executive summary when there
/// are two or more sections, then a blank line + the full render of each
/// section.
#[must_use]
pub fn render_text_report(report: &GoValue) -> Vec<u8> {
    let sections = sections_from_report(report);
    let refs: Vec<&dyn ReportSection> = sections.iter().map(|s| s as &dyn ReportSection).collect();
    DefaultStaticRenderer::new()
        .render_text(&refs, false, no_color())
        .into_bytes()
}
