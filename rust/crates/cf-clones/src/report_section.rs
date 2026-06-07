//! The terminal report section for clone detection.
//!
//! Port of `internal/analyzers/clones/report_section.go`. This drives the
//! human-readable `text` output (key metrics, the clone-type distribution, and
//! the top clone-pair issues). Terminal output is a **non-binding** format
//! (DESIGN §2.7), so byte-identity is not required here; the score/percent
//! computations are nonetheless ported faithfully for behavioral parity.

use cf_analyze::{GoMap, GoValue, MapOrigin, Report};

use crate::report::{categorize_clone_pairs, CloneTypeCounts, ClonePair, CLONE_TYPE1, CLONE_TYPE2, CLONE_TYPE3};
use crate::{KEY_CLONE_PAIRS, KEY_CLONE_RATIO, KEY_CLONE_TYPE_DISTRIBUTION, KEY_MESSAGE, KEY_TOTAL_CLONE_PAIRS, KEY_TOTAL_FUNCTIONS};

/// Machine-output severity for a clone pair below [`SEVERITY_THRESH_HIGH`].
/// Mirrors Go `analyze.SeverityFair` (the lowercase JSON form, distinct from the
/// capitalized terminal label produced by [`ReportSection::clone_issues`]).
pub const JSON_SEVERITY_FAIR: &str = "fair";
/// Machine-output severity for a clone pair at/above [`SEVERITY_THRESH_HIGH`].
/// Mirrors Go `analyze.SeverityPoor`.
pub const JSON_SEVERITY_POOR: &str = "poor";

/// Section title. Mirrors Go `sectionTitle`.
pub const SECTION_TITLE: &str = "CLONE DETECTION";
/// Default status message. Mirrors Go `defaultStatusMsg`.
pub const DEFAULT_STATUS_MSG: &str = "Clone analysis completed";
/// Severity boundary above which a clone pair is "poor". Mirrors Go
/// `severityThreshHigh`.
pub const SEVERITY_THRESH_HIGH: f64 = 0.8;

/// Display label for Type-1 clones. Mirrors Go `distLabelType1`.
pub const DIST_LABEL_TYPE1: &str = "Type-1 (Exact)";
/// Display label for Type-2 clones. Mirrors Go `distLabelType2`.
pub const DIST_LABEL_TYPE2: &str = "Type-2 (Renamed)";
/// Display label for Type-3 clones. Mirrors Go `distLabelType3`.
pub const DIST_LABEL_TYPE3: &str = "Type-3 (Near-miss)";

/// A key/value metric for terminal display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metric {
    /// Metric label.
    pub label: String,
    /// Formatted metric value.
    pub value: String,
}

/// A distribution row for terminal display.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionItem {
    /// Row label.
    pub label: String,
    /// Percentage of the total.
    pub percent: f64,
    /// Raw count.
    pub count: i64,
}

/// A reported issue (clone pair) for terminal display.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// `func_a <-> func_b`.
    pub name: String,
    /// The clone type (used as the location field).
    pub location: String,
    /// Formatted similarity value.
    pub value: String,
    /// Severity label (`"Poor"` / `"Fair"`).
    pub severity: String,
}

/// Terminal report section for clone detection. Mirrors Go `ReportSection`.
pub struct ReportSection {
    /// Section title.
    pub title: String,
    /// Status message.
    pub message: String,
    /// 0..1 score (higher is better).
    pub score: f64,
    report: Report,
}

impl ReportSection {
    /// Builds a section from a clone-detection report. Mirrors Go
    /// `NewReportSection`.
    #[must_use]
    pub fn new(report: Report) -> Self {
        let clone_ratio = cf_reportutil::get_float64(&report, KEY_CLONE_RATIO);
        let mut msg = cf_reportutil::get_string(&report, KEY_MESSAGE);
        if msg.is_empty() {
            msg = DEFAULT_STATUS_MSG.to_string();
        }
        let score = compute_score(clone_ratio);
        Self {
            title: SECTION_TITLE.to_string(),
            message: msg,
            score,
            report,
        }
    }

    /// Ordered key metrics for display. Mirrors Go `KeyMetrics`.
    #[must_use]
    pub fn key_metrics(&self) -> Vec<Metric> {
        vec![
            Metric {
                label: "Total Functions".to_string(),
                value: cf_reportutil::format_int(cf_reportutil::get_int(&self.report, KEY_TOTAL_FUNCTIONS)),
            },
            Metric {
                label: "Clone Pairs".to_string(),
                value: cf_reportutil::format_int(cf_reportutil::get_int(&self.report, KEY_TOTAL_CLONE_PAIRS)),
            },
            Metric {
                label: "Clone Ratio".to_string(),
                value: cf_reportutil::format_float(cf_reportutil::get_float64(&self.report, KEY_CLONE_RATIO)),
            },
        ]
    }

    /// Clone-type distribution rows. Mirrors Go `Distribution`.
    #[must_use]
    pub fn distribution(&self) -> Vec<DistributionItem> {
        let (counts, total) = self.extract_distribution();
        if total == 0 {
            return Vec::new();
        }
        vec![
            DistributionItem {
                label: DIST_LABEL_TYPE1.to_string(),
                percent: cf_reportutil::pct(counts.type1, total),
                count: counts.type1,
            },
            DistributionItem {
                label: DIST_LABEL_TYPE2.to_string(),
                percent: cf_reportutil::pct(counts.type2, total),
                count: counts.type2,
            },
            DistributionItem {
                label: DIST_LABEL_TYPE3.to_string(),
                percent: cf_reportutil::pct(counts.type3, total),
                count: counts.type3,
            },
        ]
    }

    /// Returns the distribution counts and total. Mirrors Go
    /// `extractDistribution` (prefer the stored full-population distribution,
    /// falling back to the capped pairs array).
    fn extract_distribution(&self) -> (CloneTypeCounts, i64) {
        if let Some(counts) = stored_distribution(&self.report) {
            let total = counts.total();
            return (counts, total);
        }
        let pairs = extract_clone_pairs(&self.report);
        let counts = categorize_clone_pairs(&pairs);
        (counts, i64::try_from(pairs.len()).unwrap_or(i64::MAX))
    }

    /// All clone pairs as issues, sorted by similarity descending. Mirrors Go
    /// `AllIssues`.
    #[must_use]
    pub fn all_issues(&self) -> Vec<Issue> {
        self.clone_issues(0)
    }

    /// The top `n` clone pairs as issues. Mirrors Go `TopIssues`.
    #[must_use]
    pub fn top_issues(&self, n: usize) -> Vec<Issue> {
        self.clone_issues(n)
    }

    fn clone_issues(&self, limit: usize) -> Vec<Issue> {
        let mut pairs = extract_clone_pairs(&self.report);
        if pairs.is_empty() {
            return Vec::new();
        }
        // Go: mapx.SortAndLimit with clonePairLess (similarity descending).
        pairs.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
        if limit > 0 && pairs.len() > limit {
            pairs.truncate(limit);
        }

        pairs
            .into_iter()
            .map(|p| {
                let severity = if p.similarity >= SEVERITY_THRESH_HIGH {
                    "Poor"
                } else {
                    "Fair"
                };
                Issue {
                    name: format!("{} <-> {}", p.func_a, p.func_b),
                    location: p.clone_type.clone(),
                    value: cf_reportutil::format_float(p.similarity),
                    severity: severity.to_string(),
                }
            })
            .collect()
    }
}

/// Formats a 0..1 score as the Go `N/10` label (`terminal.FormatScore`:
/// `round(score*10)` then `"%d/10"`). A negative score renders `"Info"`, but the
/// clone score is always in `[0, 1]`.
#[must_use]
pub fn score_label(score: f64) -> String {
    if score < 0.0 {
        return "Info".to_string();
    }
    let scaled = (score * 10.0).round() as i64;
    format!("{scaled}/10")
}

/// Builds the `renderer.SectionsToJSON` [`GoValue`] tree for the single clones
/// section produced by a `run --analyzers static/clones --format json`.
///
/// Mirrors Go `StaticService.FormatJSON` for one analyzer: one
/// [`ReportSection`] is rendered by `renderer.SectionToJSON`
/// (`title, score_label, status, metrics, distribution (omitempty), issues,
/// score`) and wrapped by `SectionsToJSON` (`overall_score_label, sections,
/// overall_score`). For a single scored section the executive summary's overall
/// score equals the section score. Issue severities use the lowercase machine
/// form (`analyze.SeverityPoor`/`SeverityFair`), NOT the capitalized terminal
/// label. The issues list is a Go-order-nondeterministic VARIANT list (the
/// stored pair multiset is stable; only the tie order varies run-to-run), so it
/// is emitted here sorted by similarity descending as the deterministic
/// representative — the differential oracle canonicalizes this list before
/// comparison.
#[must_use]
pub fn report_section_json_value(report: &Report) -> GoValue {
    let section = ReportSection::new(report.clone());
    let score = section.score;

    // metrics
    let metrics = GoValue::Array(
        section
            .key_metrics()
            .into_iter()
            .map(|m| {
                let mut sm = GoMap::new(MapOrigin::Struct);
                sm.push("label", GoValue::Str(m.label));
                sm.push("value", GoValue::Str(m.value));
                GoValue::Object(sm)
            })
            .collect(),
    );

    // distribution (omitempty: omitted entirely when empty)
    let dist = section.distribution();

    // issues: all pairs sorted by similarity descending, lowercase severity.
    let mut pairs = extract_clone_pairs(report);
    pairs.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
    let issues = GoValue::Array(
        pairs
            .into_iter()
            .map(|p| {
                let severity = if p.similarity >= SEVERITY_THRESH_HIGH {
                    JSON_SEVERITY_POOR
                } else {
                    JSON_SEVERITY_FAIR
                };
                let mut si = GoMap::new(MapOrigin::Struct);
                si.push("name", GoValue::Str(format!("{} <-> {}", p.func_a, p.func_b)));
                si.push("location", GoValue::Str(p.clone_type));
                si.push("value", GoValue::Str(cf_reportutil::format_float(p.similarity)));
                si.push("severity", GoValue::Str(severity.to_string()));
                GoValue::Object(si)
            })
            .collect(),
    );

    let mut sect = GoMap::new(MapOrigin::Struct);
    sect.push("title", GoValue::Str(section.title));
    sect.push("score_label", GoValue::Str(score_label(score)));
    sect.push("status", GoValue::Str(section.message));
    sect.push("metrics", metrics);
    if !dist.is_empty() {
        sect.push(
            "distribution",
            GoValue::Array(
                dist.into_iter()
                    .map(|d| {
                        let mut sd = GoMap::new(MapOrigin::Struct);
                        sd.push("label", GoValue::Str(d.label));
                        sd.push("percent", GoValue::Float(d.percent));
                        sd.push("count", GoValue::Int(d.count));
                        GoValue::Object(sd)
                    })
                    .collect(),
            ),
        );
    }
    sect.push("issues", issues);
    sect.push("score", GoValue::Float(score));

    let mut report_tree = GoMap::new(MapOrigin::Struct);
    report_tree.push("overall_score_label", GoValue::Str(score_label(score)));
    report_tree.push("sections", GoValue::Array(vec![GoValue::Object(sect)]));
    report_tree.push("overall_score", GoValue::Float(score));
    GoValue::Object(report_tree)
}

/// Compact title column width. Mirrors Go `renderer.CompactTitleWidth`.
const COMPACT_TITLE_WIDTH: usize = 12;
/// Compact score-bar width. Mirrors Go `renderer.CompactBarWidth`.
const COMPACT_BAR_WIDTH: usize = 10;
/// Filled progress-bar cell. Mirrors Go `terminal.ProgressFilled`.
const PROGRESS_FILLED: &str = "\u{2588}"; // █
/// Empty progress-bar cell. Mirrors Go `terminal.ProgressEmpty`.
const PROGRESS_EMPTY: &str = "\u{2591}"; // ░

/// Right-pads `s` with spaces to `width`; returns `s` unchanged when already at
/// least `width` (by BYTE length, matching Go `terminal.PadRight`'s `len(s)`,
/// which counts UTF-8 bytes — irrelevant for the ASCII section title but kept
/// faithful).
fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(width);
    out.push_str(s);
    out.extend(std::iter::repeat(' ').take(width - s.len()));
    out
}

/// Go `terminal.DrawProgressBar(value, width)`: `int(value*width)` filled cells
/// (value clamped to `[0,1]`), the remainder empty.
fn draw_progress_bar(value: f64, width: usize) -> String {
    let v = value.clamp(0.0, 1.0);
    // Go: filled := int(value * float64(width)) — truncation toward zero.
    let filled = (v * width as f64) as usize;
    let empty = width.saturating_sub(filled);
    let mut bar = String::with_capacity((filled + empty) * 3);
    for _ in 0..filled {
        bar.push_str(PROGRESS_FILLED);
    }
    for _ in 0..empty {
        bar.push_str(PROGRESS_EMPTY);
    }
    bar
}

/// Builds the single-line `--format compact` bytes for the clones section,
/// mirroring Go `DefaultStaticRenderer.RenderCompact` →
/// `SectionRenderer.RenderCompact`:
///
/// ```text
/// <title padded to 12> [<bar>] <N/10>  <status message>\n
/// ```
///
/// Color is always disabled (the compat env pins `NO_COLOR=1`), so no ANSI
/// codes are emitted. `fmt.Fprintln` appends the trailing newline. This is the
/// only fully Go-deterministic terminal format for clones (it never lists the
/// order-nondeterministic clone pairs), so it must match Go byte-for-byte.
#[must_use]
pub fn report_section_compact(report: &Report) -> Vec<u8> {
    let section = ReportSection::new(report.clone());
    let title = pad_right(&section.title, COMPACT_TITLE_WIDTH);
    let bar = draw_progress_bar(section.score, COMPACT_BAR_WIDTH);
    // Go terminal.FormatScoreBar: "[<bar>] <N/10>". FormatScore uses
    // round(score*10); score_label() reproduces it exactly.
    let score_bar = format!("[{}] {}", bar, score_label(section.score));
    // Go SectionRenderer.RenderCompact: "%s %s  %s" then Fprintln adds '\n'.
    let line = format!("{title} {score_bar}  {}\n", section.message);
    line.into_bytes()
}

/// Converts a clone ratio to a 0..1 score (lower ratio = higher score).
///
/// Mirrors Go `computeScore`: clamps to `[0, 1]` then inverts.
#[must_use]
pub fn compute_score(clone_ratio: f64) -> f64 {
    if clone_ratio >= 1.0 {
        0.0
    } else if clone_ratio <= 0.0 {
        1.0
    } else {
        1.0 - clone_ratio
    }
}

/// Reads the stored `clone_type_distribution` counts, if present.
fn stored_distribution(report: &Report) -> Option<CloneTypeCounts> {
    use cf_analyze::GoValue;
    let Some(GoValue::Map(m)) =
        cf_reportutil::get(report, KEY_CLONE_TYPE_DISTRIBUTION)
    else {
        return None;
    };
    let mut counts = CloneTypeCounts::default();
    for (k, v) in m.entries() {
        if let GoValue::Int(n) = v {
            match k.as_str() {
                CLONE_TYPE1 => counts.type1 = *n,
                CLONE_TYPE2 => counts.type2 = *n,
                CLONE_TYPE3 => counts.type3 = *n,
                _ => {}
            }
        }
    }
    Some(counts)
}

/// Extracts the clone pairs from a report's `clone_pairs` array.
fn extract_clone_pairs(report: &Report) -> Vec<ClonePair> {
    use cf_analyze::GoValue;
    let Some(GoValue::Array(items)) = cf_reportutil::get(report, KEY_CLONE_PAIRS) else {
        return Vec::new();
    };
    let mut pairs = Vec::with_capacity(items.len());
    for item in items {
        if let GoValue::Map(m) = item {
            pairs.push(pair_from_map(m));
        }
    }
    pairs
}

fn pair_from_map(m: &cf_analyze::GoMap) -> ClonePair {
    use cf_analyze::GoValue;
    let mut pair = ClonePair {
        func_a: String::new(),
        func_b: String::new(),
        similarity: 0.0,
        clone_type: String::new(),
    };
    for (k, v) in m.entries() {
        match (k.as_str(), v) {
            ("func_a", GoValue::Str(s)) => pair.func_a = s.clone(),
            ("func_b", GoValue::Str(s)) => pair.func_b = s.clone(),
            ("similarity", GoValue::Float(f)) => pair.similarity = *f,
            ("clone_type", GoValue::Str(s)) => pair.clone_type = s.clone(),
            _ => {}
        }
    }
    pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::Analyzer;
    use crate::uast::NodeBuilder;
    use cf_uast_node::Node;

    fn function(name: &str) -> Node {
        let name_node = NodeBuilder::new("Identifier").role("Name").token(name).build();
        let mut f = NodeBuilder::new("Function").role("Function").child(name_node).build();
        let mut block = NodeBuilder::new("Block").build();
        for i in 0..24 {
            let kind = ["Identifier", "Call", "Literal", "Operator"][i % 4];
            block.add_child(NodeBuilder::new(kind).build());
        }
        f.add_child(block);
        f
    }

    #[test]
    fn compute_score_clamps_and_inverts() {
        assert_eq!(compute_score(1.5), 0.0);
        assert_eq!(compute_score(1.0), 0.0);
        assert_eq!(compute_score(0.0), 1.0);
        assert_eq!(compute_score(-0.5), 1.0);
        assert!((compute_score(0.25) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn section_from_clone_report_has_metrics_and_issues() {
        let a = Analyzer::new();
        let root = NodeBuilder::new("File")
            .child(function("foo"))
            .child(function("bar"))
            .build();
        let report = a.analyze_node(Some(&root));
        let section = ReportSection::new(report);

        assert_eq!(section.title, SECTION_TITLE);
        assert_eq!(section.key_metrics().len(), 3);
        let issues = section.all_issues();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, "Poor"); // similarity 1.0 >= 0.8
    }

    #[test]
    fn empty_report_uses_default_status_and_full_score() {
        let a = Analyzer::new();
        let report = a.analyze_node(None);
        let section = ReportSection::new(report);
        // clone_ratio 0.0 -> score 1.0; message present so default not used.
        assert_eq!(section.score, 1.0);
        assert!(section.distribution().is_empty());
    }
}
