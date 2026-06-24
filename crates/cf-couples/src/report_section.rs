//! Human-facing report-section logic.
//!
//! Computes the section score, status message, coupling-strength distribution,
//! and per-pair severity. These feed the (cosmetic, non-binding) terminal/HTML
//! summary; the numeric values follow the reference implementation so the
//! section behaves identically.

use crate::metrics::{ComputedMetrics, FileCouplingData};

/// Section title.
pub const REPORT_SECTION_TITLE: &str = "COUPLES";

/// Status message when no coupling data is available.
pub const DEFAULT_STATUS_MSG: &str = "No coupling data available";

/// Sentinel score used for info-only sections.
///
/// The canonical constant belongs in `cf-analyze`; the contractual value is
/// `-1.0`. Reconcile when `cf-analyze` lands (see crate TODOs).
pub const SCORE_INFO_ONLY: f64 = -1.0;

// Distribution thresholds.
const DIST_STRONG_MIN: f64 = 0.7;
const DIST_MODERATE_MIN: f64 = 0.4;
const DIST_WEAK_MIN: f64 = 0.1;

/// Distribution bucket labels.
pub const DIST_LABEL_STRONG: &str = "Strong (>70%)";
pub const DIST_LABEL_MOD: &str = "Moderate (40-70%)";
pub const DIST_LABEL_WEAK: &str = "Weak (10-40%)";
pub const DIST_LABEL_NONE: &str = "Minimal (<10%)";

// Issue severity thresholds.
const ISSUE_SEVERITY_HIGH_MIN: f64 = 0.7;
const ISSUE_SEVERITY_MED_MIN: f64 = 0.4;

/// Severity labels. The canonical string values belong in `cf-analyze`; the
/// contractual literals are reproduced here until that crate lands.
pub const SEVERITY_POOR: &str = "poor";
pub const SEVERITY_FAIR: &str = "fair";
pub const SEVERITY_GOOD: &str = "good";

/// Computes the section score and status message.
///
/// Score is `1.0 - avg_coupling_strength` (clamped to `[0, ∞)`); a higher score
/// means lower coupling. Returns [`SCORE_INFO_ONLY`] / [`DEFAULT_STATUS_MSG`]
/// when there are no files.
#[must_use]
pub fn compute_score(m: &ComputedMetrics) -> (f64, String) {
    const GOOD_THRESHOLD: f64 = 0.7;
    const FAIR_THRESHOLD: f64 = 0.4;
    if m.aggregate.total_files == 0 {
        return (SCORE_INFO_ONLY, DEFAULT_STATUS_MSG.to_string());
    }
    let mut score = 1.0 - m.aggregate.avg_coupling_strength;
    if score < 0.0 {
        score = 0.0;
    }
    let msg = if score >= GOOD_THRESHOLD {
        format!(
            "Good - low coupling across {} files",
            m.aggregate.total_files
        )
    } else if score >= FAIR_THRESHOLD {
        format!(
            "Fair - moderate coupling ({} highly coupled pairs)",
            m.aggregate.highly_coupled_pairs
        )
    } else {
        format!(
            "Poor - high coupling ({} highly coupled pairs need attention)",
            m.aggregate.highly_coupled_pairs
        )
    };
    (score, msg)
}

/// Coupling-strength distribution counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StrengthDistCounts {
    pub strong: i32,
    pub moderate: i32,
    pub weak: i32,
    pub minimal: i32,
}

/// Categorizes coupling pairs by strength.
#[must_use]
pub fn categorize_strength(couples: &[FileCouplingData]) -> StrengthDistCounts {
    let mut counts = StrengthDistCounts::default();
    for cp in couples {
        if cp.strength >= DIST_STRONG_MIN {
            counts.strong += 1;
        } else if cp.strength >= DIST_MODERATE_MIN {
            counts.moderate += 1;
        } else if cp.strength >= DIST_WEAK_MIN {
            counts.weak += 1;
        } else {
            counts.minimal += 1;
        }
    }
    counts
}

/// Maps a coupling strength to a severity label.
#[must_use]
pub fn severity_for_strength(strength: f64) -> &'static str {
    if strength >= ISSUE_SEVERITY_HIGH_MIN {
        SEVERITY_POOR
    } else if strength >= ISSUE_SEVERITY_MED_MIN {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// Computes a fraction `count / total`, returning `0.0` for an empty total.
#[must_use]
pub fn pct(count: i32, total: i32) -> f64 {
    if total == 0 {
        0.0
    } else {
        f64::from(count) / f64::from(total)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact contract values (score constants, guards) are the point
mod tests {
    use super::*;
    use crate::metrics::AggregateData;

    fn metrics(total_files: i32, avg: f64, highly: i32) -> ComputedMetrics {
        ComputedMetrics {
            aggregate: AggregateData {
                total_files,
                avg_coupling_strength: avg,
                highly_coupled_pairs: highly,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn score_no_files_is_info_only() {
        let (s, msg) = compute_score(&metrics(0, 0.0, 0));
        assert_eq!(s, SCORE_INFO_ONLY);
        assert_eq!(msg, DEFAULT_STATUS_MSG);
    }

    #[test]
    fn score_good_fair_poor() {
        let (s, msg) = compute_score(&metrics(5, 0.1, 0));
        assert!((s - 0.9).abs() < 1e-12);
        assert!(msg.starts_with("Good"));

        let (_s, msg) = compute_score(&metrics(5, 0.5, 2));
        assert!(msg.starts_with("Fair"));

        let (_s, msg) = compute_score(&metrics(5, 0.9, 3));
        assert!(msg.starts_with("Poor"));
    }

    #[test]
    fn distribution_categorization() {
        let couples = vec![
            FileCouplingData {
                file1: "a".into(),
                file2: "b".into(),
                co_changes: 1,
                strength: 0.8,
            },
            FileCouplingData {
                file1: "c".into(),
                file2: "d".into(),
                co_changes: 1,
                strength: 0.5,
            },
            FileCouplingData {
                file1: "e".into(),
                file2: "f".into(),
                co_changes: 1,
                strength: 0.2,
            },
            FileCouplingData {
                file1: "g".into(),
                file2: "h".into(),
                co_changes: 1,
                strength: 0.05,
            },
        ];
        let c = categorize_strength(&couples);
        assert_eq!(
            c,
            StrengthDistCounts {
                strong: 1,
                moderate: 1,
                weak: 1,
                minimal: 1
            }
        );
    }

    #[test]
    fn severity_mapping() {
        assert_eq!(severity_for_strength(0.8), SEVERITY_POOR);
        assert_eq!(severity_for_strength(0.5), SEVERITY_FAIR);
        assert_eq!(severity_for_strength(0.1), SEVERITY_GOOD);
    }

    #[test]
    fn pct_guards_zero() {
        assert_eq!(pct(0, 0), 0.0);
        assert_eq!(pct(1, 4), 0.25);
    }
}
