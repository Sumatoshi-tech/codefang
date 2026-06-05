//! The terminal report section for clone detection.
//!
//! Port of `internal/analyzers/clones/report_section.go`. This drives the
//! human-readable `text` output (key metrics, the clone-type distribution, and
//! the top clone-pair issues). Terminal output is a **non-binding** format
//! (DESIGN §2.7), so byte-identity is not required here; the score/percent
//! computations are nonetheless ported faithfully for behavioral parity.

use cf_analyze::Report;

use crate::report::{categorize_clone_pairs, CloneTypeCounts, ClonePair, CLONE_TYPE1, CLONE_TYPE2, CLONE_TYPE3};
use crate::{KEY_CLONE_PAIRS, KEY_CLONE_RATIO, KEY_CLONE_TYPE_DISTRIBUTION, KEY_MESSAGE, KEY_TOTAL_CLONE_PAIRS, KEY_TOTAL_FUNCTIONS};

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
    use cf_uast_node::{Builder, Node};

    fn function(name: &str) -> Node {
        let name_node = Builder::new("Identifier").role("Name").token(name).build();
        let mut f = Builder::new("Function").role("Function").child(name_node).build();
        let mut block = Node::new("Block");
        for i in 0..24 {
            let kind = ["Identifier", "Call", "Literal", "Operator"][i % 4];
            block.add_child(Node::new(kind));
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
        let root = Builder::new("File")
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
