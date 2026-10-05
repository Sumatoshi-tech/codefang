//! The single definition of every complexity threshold, label, and message.
//!
//! Every report format (the single-file analyzer value, the aggregated
//! `function_complexity` / `high_risk_functions` view, the JSON sections
//! report, and the terminal renderings) classifies through these functions,
//! so a number means the same thing everywhere. The values are documented in
//! `site/analyzers/complexity.md`.

/// Cyclomatic complexity at or above which a function is "high" risk and is
/// listed in `high_risk_functions`.
pub const CYCLOMATIC_HIGH: i64 = 10;
/// Cyclomatic complexity at or above which a function is "moderate" risk.
pub const CYCLOMATIC_MODERATE: i64 = 5;
/// Cognitive complexity at or above which a function is "high" risk.
pub const COGNITIVE_HIGH: i64 = 15;
/// Cognitive complexity at or above which a function is "moderate" risk.
pub const COGNITIVE_MODERATE: i64 = 7;
/// Nesting depth at or above which a function is "high" risk.
pub const NESTING_HIGH: i64 = 5;
/// Nesting depth at or above which a function is "moderate" risk.
pub const NESTING_MODERATE: i64 = 3;

/// Risk score (sum of the three dimension scores) for `CRITICAL`.
const RISK_SCORE_CRITICAL: i64 = 5;
/// Risk score for `HIGH`.
const RISK_SCORE_HIGH: i64 = 3;
/// Dimension score for crossing a "high" threshold.
const DIMENSION_SCORE_HIGH: i64 = 2;
/// Dimension score for crossing a "moderate" threshold.
const DIMENSION_SCORE_MODERATE: i64 = 1;

/// Per-function assessment: cyclomatic `<= 1` is simple.
const ASSESS_CYCLOMATIC_SIMPLE: i64 = 1;
/// Per-function assessment: cyclomatic `<= 5` is moderate, above is complex.
const ASSESS_CYCLOMATIC_MODERATE: i64 = 5;
/// Per-function assessment: cognitive `<= 5` is low.
const ASSESS_COGNITIVE_LOW: i64 = 5;
/// Per-function assessment: cognitive `<= 10` is medium, above is high.
const ASSESS_COGNITIVE_MEDIUM: i64 = 10;
/// Per-function assessment: nesting `<= 3` is shallow.
const ASSESS_NESTING_SHALLOW: i64 = 3;
/// Per-function assessment: nesting `<= 5` is moderate, above is deep.
const ASSESS_NESTING_MODERATE: i64 = 5;

/// Average-complexity band: `<= 1.0` is excellent.
const AVERAGE_EXCELLENT: f64 = 1.0;
/// Average-complexity band: `<= 3.0` is good.
const AVERAGE_GOOD: f64 = 3.0;
/// Average-complexity band: `<= 5.0` is fair (section score only).
const AVERAGE_FAIR: f64 = 5.0;
/// Average-complexity band: `<= 7.0` is moderate (fair message band).
const AVERAGE_MODERATE: f64 = 7.0;
/// Average-complexity band: `<= 10.0` is poor (section score only).
const AVERAGE_POOR: f64 = 10.0;

/// Section scores (0..=1) per average-complexity band.
const SCORE_EXCELLENT: f64 = 1.0;
const SCORE_GOOD: f64 = 0.8;
const SCORE_FAIR: f64 = 0.6;
const SCORE_MODERATE: f64 = 0.4;
const SCORE_POOR: f64 = 0.2;
const SCORE_CRITICAL: f64 = 0.1;

/// Health score (0..=100) anchors per average-complexity band.
const HEALTH_PERFECT: f64 = 100.0;
const HEALTH_GOOD_BASE: f64 = 80.0;
const HEALTH_GOOD_SLOPE: f64 = 10.0;
const HEALTH_MODERATE_BASE: f64 = 50.0;
const HEALTH_MODERATE_SLOPE: f64 = 7.5;
const HEALTH_HIGH_SLOPE: f64 = 5.0;

/// Issue severity: cyclomatic `>= 11` is poor.
const SEVERITY_POOR_MIN: i64 = 11;
/// Issue severity: cyclomatic `>= 6` is fair, below is good.
const SEVERITY_FAIR_MIN: i64 = 6;

/// Terminal distribution buckets (cyclomatic upper bounds).
const DIST_SIMPLE_MAX: i64 = 5;
const DIST_MODERATE_MAX: i64 = 10;
const DIST_COMPLEX_MAX: i64 = 20;

/// The aggregate `message` texts, keyed by average cyclomatic complexity.
pub const MSG_EXCELLENT: &str = "Excellent complexity - functions are simple and maintainable";
/// See [`MSG_EXCELLENT`].
pub const MSG_GOOD: &str = "Good complexity - functions have reasonable complexity";
/// See [`MSG_EXCELLENT`].
pub const MSG_FAIR: &str = "Fair complexity - some functions could be simplified";
/// See [`MSG_EXCELLENT`].
pub const MSG_HIGH: &str = "High complexity - functions are complex and should be refactored";

/// Risk level names, in priority order.
pub const RISK_CRITICAL: &str = "CRITICAL";
/// See [`RISK_CRITICAL`].
pub const RISK_HIGH: &str = "HIGH";
/// See [`RISK_CRITICAL`].
pub const RISK_MEDIUM: &str = "MEDIUM";
/// See [`RISK_CRITICAL`].
pub const RISK_LOW: &str = "LOW";

/// Issue texts attached to `high_risk_functions` entries.
pub const ISSUE_HIGH_CYCLOMATIC: &str = "High cyclomatic complexity";
/// See [`ISSUE_HIGH_CYCLOMATIC`].
pub const ISSUE_HIGH_COGNITIVE: &str = "High cognitive complexity";
/// See [`ISSUE_HIGH_CYCLOMATIC`].
pub const ISSUE_DEEP_NESTING: &str = "Deep nesting";

/// Terminal distribution bucket labels.
pub const DIST_LABEL_SIMPLE: &str = "Simple (1-5)";
/// See [`DIST_LABEL_SIMPLE`].
pub const DIST_LABEL_MODERATE: &str = "Moderate (6-10)";
/// See [`DIST_LABEL_SIMPLE`].
pub const DIST_LABEL_COMPLEX: &str = "Complex (11-20)";
/// See [`DIST_LABEL_SIMPLE`].
pub const DIST_LABEL_VERY_COMPLEX: &str = "Very Complex (>20)";

/// Issue severity names.
pub const SEVERITY_GOOD: &str = "good";
/// See [`SEVERITY_GOOD`].
pub const SEVERITY_FAIR: &str = "fair";
/// See [`SEVERITY_GOOD`].
pub const SEVERITY_POOR: &str = "poor";

/// Cyclomatic-complexity assessment label (`<= 1` simple, `<= 5` moderate,
/// otherwise complex):
///
/// ```
/// use cf_complexity::thresholds::complexity_assessment;
/// assert_eq!(complexity_assessment(1), "🟢 Simple");
/// assert_eq!(complexity_assessment(5), "🟡 Moderate");
/// assert_eq!(complexity_assessment(6), "🔴 Complex");
/// ```
#[must_use]
pub fn complexity_assessment(cyclomatic: i64) -> &'static str {
    if cyclomatic <= ASSESS_CYCLOMATIC_SIMPLE {
        "🟢 Simple"
    } else if cyclomatic <= ASSESS_CYCLOMATIC_MODERATE {
        "🟡 Moderate"
    } else {
        "🔴 Complex"
    }
}

/// Cognitive-complexity assessment label (`<= 5` low, `<= 10` medium,
/// otherwise high):
///
/// ```
/// use cf_complexity::thresholds::cognitive_assessment;
/// assert_eq!(cognitive_assessment(5), "🟢 Low");
/// assert_eq!(cognitive_assessment(10), "🟡 Medium");
/// assert_eq!(cognitive_assessment(11), "🔴 High");
/// ```
#[must_use]
pub fn cognitive_assessment(cognitive: i64) -> &'static str {
    if cognitive <= ASSESS_COGNITIVE_LOW {
        "🟢 Low"
    } else if cognitive <= ASSESS_COGNITIVE_MEDIUM {
        "🟡 Medium"
    } else {
        "🔴 High"
    }
}

/// Nesting-depth assessment label (`<= 3` shallow, `<= 5` moderate,
/// otherwise deep):
///
/// ```
/// use cf_complexity::thresholds::nesting_assessment;
/// assert_eq!(nesting_assessment(3), "🟢 Shallow");
/// assert_eq!(nesting_assessment(5), "🟡 Moderate");
/// assert_eq!(nesting_assessment(6), "🔴 Deep");
/// ```
#[must_use]
pub fn nesting_assessment(depth: i64) -> &'static str {
    if depth <= ASSESS_NESTING_SHALLOW {
        "🟢 Shallow"
    } else if depth <= ASSESS_NESTING_MODERATE {
        "🟡 Moderate"
    } else {
        "🔴 Deep"
    }
}

/// The dimension score of one metric against its moderate/high thresholds.
fn dimension_score(value: i64, moderate: i64, high: i64) -> i64 {
    if value >= high {
        DIMENSION_SCORE_HIGH
    } else if value >= moderate {
        DIMENSION_SCORE_MODERATE
    } else {
        0
    }
}

/// Classifies a function's risk level from its three metrics.
///
/// Each dimension scores 2 at or above its high threshold (cyclomatic 10,
/// cognitive 15, nesting 5), 1 at or above its moderate threshold (5, 7, 3),
/// else 0; the sum maps to `CRITICAL` (>= 5), `HIGH` (>= 3), `MEDIUM` (>= 1)
/// or `LOW`:
///
/// ```
/// use cf_complexity::thresholds::risk_level;
/// assert_eq!(risk_level(2, 1, 1), "LOW");
/// assert_eq!(risk_level(8, 12, 3), "HIGH");      // 1 + 1 + 1
/// assert_eq!(risk_level(15, 22, 5), "CRITICAL"); // 2 + 2 + 2
/// assert_eq!(risk_level(5, 1, 1), "MEDIUM");
/// ```
#[must_use]
pub fn risk_level(cyclomatic: i64, cognitive: i64, nesting: i64) -> &'static str {
    let score = dimension_score(cyclomatic, CYCLOMATIC_MODERATE, CYCLOMATIC_HIGH)
        + dimension_score(cognitive, COGNITIVE_MODERATE, COGNITIVE_HIGH)
        + dimension_score(nesting, NESTING_MODERATE, NESTING_HIGH);
    if score >= RISK_SCORE_CRITICAL {
        RISK_CRITICAL
    } else if score >= RISK_SCORE_HIGH {
        RISK_HIGH
    } else if score >= DIMENSION_SCORE_MODERATE {
        RISK_MEDIUM
    } else {
        RISK_LOW
    }
}

/// Sort priority of a risk level (`CRITICAL` first).
#[must_use]
pub fn risk_priority(level: &str) -> u8 {
    match level {
        RISK_CRITICAL => 0,
        RISK_HIGH => 1,
        RISK_MEDIUM => 2,
        _ => 3,
    }
}

/// The issues a function raises for `high_risk_functions`; empty when it
/// crosses none of the high thresholds.
#[must_use]
pub fn risk_issues(cyclomatic: i64, cognitive: i64, nesting: i64) -> Vec<&'static str> {
    let mut issues = Vec::new();
    if cyclomatic >= CYCLOMATIC_HIGH {
        issues.push(ISSUE_HIGH_CYCLOMATIC);
    }
    if cognitive >= COGNITIVE_HIGH {
        issues.push(ISSUE_HIGH_COGNITIVE);
    }
    if nesting >= NESTING_HIGH {
        issues.push(ISSUE_DEEP_NESTING);
    }
    issues
}

/// The three-bucket distribution key of the aggregated report
/// (`simple` <= 5, `moderate` <= 10, `complex` above).
#[must_use]
pub fn distribution_bucket(cyclomatic: i64) -> &'static str {
    if cyclomatic <= CYCLOMATIC_MODERATE {
        "simple"
    } else if cyclomatic <= CYCLOMATIC_HIGH {
        "moderate"
    } else {
        "complex"
    }
}

/// The four terminal distribution buckets, in display order.
pub const TERMINAL_BUCKETS: [&str; 4] = [
    DIST_LABEL_SIMPLE,
    DIST_LABEL_MODERATE,
    DIST_LABEL_COMPLEX,
    DIST_LABEL_VERY_COMPLEX,
];

/// Index into [`TERMINAL_BUCKETS`] for a cyclomatic value.
#[must_use]
pub fn terminal_bucket(cyclomatic: i64) -> usize {
    if cyclomatic <= DIST_SIMPLE_MAX {
        0
    } else if cyclomatic <= DIST_MODERATE_MAX {
        1
    } else if cyclomatic <= DIST_COMPLEX_MAX {
        2
    } else {
        3
    }
}

/// Issue severity of a function row (`poor` >= 11, `fair` >= 6, else `good`).
#[must_use]
pub fn severity(cyclomatic: i64) -> &'static str {
    if cyclomatic >= SEVERITY_POOR_MIN {
        SEVERITY_POOR
    } else if cyclomatic >= SEVERITY_FAIR_MIN {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// The aggregate message for an average cyclomatic complexity
/// (`<= 1` excellent, `<= 3` good, `<= 7` fair, else high):
///
/// ```
/// use cf_complexity::thresholds::{complexity_message, MSG_GOOD};
/// assert_eq!(complexity_message(2.5), MSG_GOOD);
/// ```
#[must_use]
pub fn complexity_message(average: f64) -> &'static str {
    if average <= AVERAGE_EXCELLENT {
        MSG_EXCELLENT
    } else if average <= AVERAGE_GOOD {
        MSG_GOOD
    } else if average <= AVERAGE_MODERATE {
        MSG_FAIR
    } else {
        MSG_HIGH
    }
}

/// The 0..=1 section score for an average cyclomatic complexity
/// (`<= 1` 1.0, `<= 3` 0.8, `<= 5` 0.6, `<= 7` 0.4, `<= 10` 0.2, else 0.1).
#[must_use]
pub fn section_score(average: f64) -> f64 {
    if average <= AVERAGE_EXCELLENT {
        SCORE_EXCELLENT
    } else if average <= AVERAGE_GOOD {
        SCORE_GOOD
    } else if average <= AVERAGE_FAIR {
        SCORE_FAIR
    } else if average <= AVERAGE_MODERATE {
        SCORE_MODERATE
    } else if average <= AVERAGE_POOR {
        SCORE_POOR
    } else {
        SCORE_CRITICAL
    }
}

/// The 0..=100 health score for an average cyclomatic complexity: 100 at or
/// below 1.0, a linear 80..100 band to 3.0, a linear 50..80 band to 7.0, then
/// 5 points off per unit above 7.0 (floored at 0).
#[must_use]
pub fn health_score(average: f64) -> f64 {
    if average <= AVERAGE_EXCELLENT {
        HEALTH_PERFECT
    } else if average <= AVERAGE_GOOD {
        HEALTH_GOOD_BASE + (AVERAGE_GOOD - average) * HEALTH_GOOD_SLOPE
    } else if average <= AVERAGE_MODERATE {
        HEALTH_MODERATE_BASE + (AVERAGE_MODERATE - average) * HEALTH_MODERATE_SLOPE
    } else {
        (HEALTH_MODERATE_BASE - (average - AVERAGE_MODERATE) * HEALTH_HIGH_SLOPE).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        distribution_bucket, health_score, risk_issues, risk_level, risk_priority, section_score,
        severity, terminal_bucket, ISSUE_DEEP_NESTING, ISSUE_HIGH_COGNITIVE, ISSUE_HIGH_CYCLOMATIC,
        RISK_CRITICAL, RISK_HIGH, RISK_LOW, RISK_MEDIUM,
    };

    #[test]
    fn risk_levels_follow_the_documented_score_bands() {
        assert_eq!(risk_level(1, 0, 0), RISK_LOW);
        assert_eq!(risk_level(5, 0, 0), RISK_MEDIUM);
        assert_eq!(risk_level(10, 0, 0), RISK_MEDIUM);
        assert_eq!(risk_level(10, 7, 0), RISK_HIGH);
        assert_eq!(risk_level(10, 15, 0), RISK_HIGH);
        assert_eq!(risk_level(10, 15, 3), RISK_CRITICAL);
        assert_eq!(risk_level(8, 12, 3), RISK_HIGH);
        assert_eq!(risk_level(15, 22, 5), RISK_CRITICAL);
    }

    #[test]
    fn risk_priority_orders_critical_first() {
        assert!(risk_priority(RISK_CRITICAL) < risk_priority(RISK_HIGH));
        assert!(risk_priority(RISK_HIGH) < risk_priority(RISK_MEDIUM));
        assert!(risk_priority(RISK_MEDIUM) < risk_priority(RISK_LOW));
        assert_eq!(risk_priority("unknown"), risk_priority(RISK_LOW));
    }

    #[test]
    fn risk_issues_list_each_crossed_high_threshold() {
        assert!(risk_issues(9, 14, 4).is_empty());
        assert_eq!(risk_issues(10, 0, 0), vec![ISSUE_HIGH_CYCLOMATIC]);
        assert_eq!(
            risk_issues(10, 15, 5),
            vec![
                ISSUE_HIGH_CYCLOMATIC,
                ISSUE_HIGH_COGNITIVE,
                ISSUE_DEEP_NESTING
            ]
        );
    }

    #[test]
    fn buckets_and_severity_share_the_cyclomatic_bands() {
        assert_eq!(distribution_bucket(5), "simple");
        assert_eq!(distribution_bucket(6), "moderate");
        assert_eq!(distribution_bucket(11), "complex");
        assert_eq!(terminal_bucket(5), 0);
        assert_eq!(terminal_bucket(10), 1);
        assert_eq!(terminal_bucket(20), 2);
        assert_eq!(terminal_bucket(21), 3);
        assert_eq!(severity(5), "good");
        assert_eq!(severity(6), "fair");
        assert_eq!(severity(11), "poor");
    }

    #[test]
    fn scores_are_monotone_in_the_average() {
        let averages = [
            0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 12.0, 20.0,
        ];
        for pair in averages.windows(2) {
            assert!(section_score(pair[0]) >= section_score(pair[1]));
            assert!(health_score(pair[0]) >= health_score(pair[1]));
        }
        assert!((health_score(1.0) - 100.0).abs() < f64::EPSILON);
        assert!((health_score(3.0) - 80.0).abs() < f64::EPSILON);
        assert!((health_score(7.0) - 50.0).abs() < f64::EPSILON);
        assert!(health_score(100.0).abs() < f64::EPSILON);
    }
}
