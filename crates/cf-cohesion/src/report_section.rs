//! Report-section data: the distribution buckets and the issues list the
//! terminal and JSON section renderers display.

use crate::metrics::{
    COHESION_THRESHOLD_EXCELLENT, COHESION_THRESHOLD_FAIR, COHESION_THRESHOLD_GOOD,
};

/// Section title.
pub const SECTION_TITLE: &str = "COHESION";

/// Distribution label: cohesion `>= 0.6`.
pub const DIST_LABEL_EXCELLENT: &str = "Excellent (>=0.6)";
/// Distribution label: cohesion in `[0.4, 0.6)`.
pub const DIST_LABEL_GOOD: &str = "Good (0.4-0.6)";
/// Distribution label: cohesion in `[0.3, 0.4)`.
pub const DIST_LABEL_FAIR: &str = "Fair (0.3-0.4)";
/// Distribution label: cohesion `< 0.3`.
pub const DIST_LABEL_POOR: &str = "Poor (<0.3)";

/// Issue severity for cohesion in `[0.4, 0.6)`.
pub const SEVERITY_GOOD: &str = "good";
/// Issue severity for cohesion in `[0.3, 0.4)`.
pub const SEVERITY_FAIR: &str = "fair";
/// Issue severity for cohesion `< 0.3`.
pub const SEVERITY_POOR: &str = "poor";

/// Functions with cohesion below this are listed as issues (everything that
/// is not Excellent).
pub const ISSUE_THRESHOLD: f64 = COHESION_THRESHOLD_EXCELLENT;

/// Decimal places of the rendered metrics and issue values.
const METRIC_DECIMALS: usize = 2;
/// The `N/10` score label scale.
const SCORE_LABEL_SCALE: f64 = 10.0;

/// One distribution bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    /// Bucket label (see the `DIST_LABEL_*` constants).
    pub label: &'static str,
    /// Functions in the bucket.
    pub count: usize,
    /// Fraction of all functions in `[0, 1]`.
    pub percent: f64,
}

/// One issue row: a function below [`ISSUE_THRESHOLD`].
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Function name.
    pub name: String,
    /// Where it lives (the source file).
    pub location: String,
    /// Its cohesion.
    pub cohesion: f64,
    /// Its severity (see the `SEVERITY_*` constants).
    pub severity: &'static str,
}

/// The four buckets over `cohesions`, in display order; empty when there are
/// no functions.
#[must_use]
pub fn distribution(cohesions: &[f64]) -> Vec<Bucket> {
    if cohesions.is_empty() {
        return Vec::new();
    }
    let mut counts = [0usize; 4];
    for &c in cohesions {
        let bucket = if c >= COHESION_THRESHOLD_EXCELLENT {
            0
        } else if c >= COHESION_THRESHOLD_GOOD {
            1
        } else if c >= COHESION_THRESHOLD_FAIR {
            2
        } else {
            3
        };
        counts[bucket] += 1;
    }
    let total = cohesions.len() as f64;
    [
        DIST_LABEL_EXCELLENT,
        DIST_LABEL_GOOD,
        DIST_LABEL_FAIR,
        DIST_LABEL_POOR,
    ]
    .iter()
    .zip(counts)
    .map(|(&label, count)| Bucket {
        label,
        count,
        percent: count as f64 / total,
    })
    .collect()
}

/// The issues among `(name, location, cohesion)` rows: those below
/// [`ISSUE_THRESHOLD`], ordered by cohesion ascending, then name, then
/// location.
#[must_use]
pub fn issues<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str, f64)>) -> Vec<Issue> {
    let mut out: Vec<Issue> = rows
        .into_iter()
        .filter(|&(_, _, cohesion)| cohesion < ISSUE_THRESHOLD)
        .map(|(name, location, cohesion)| Issue {
            name: name.to_string(),
            location: location.to_string(),
            cohesion,
            severity: severity(cohesion),
        })
        .collect();
    out.sort_by(|a, b| {
        a.cohesion
            .total_cmp(&b.cohesion)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.location.cmp(&b.location))
    });
    out
}

/// Issue severity of a cohesion value.
#[must_use]
pub fn severity(cohesion: f64) -> &'static str {
    if cohesion < COHESION_THRESHOLD_FAIR {
        SEVERITY_POOR
    } else if cohesion < COHESION_THRESHOLD_GOOD {
        SEVERITY_FAIR
    } else {
        SEVERITY_GOOD
    }
}

/// Renders a metric or issue value with [`METRIC_DECIMALS`] decimals.
#[must_use]
pub fn format_metric(v: f64) -> String {
    format!("{v:.METRIC_DECIMALS$}")
}

/// Renders a `[0, 1]` score as the `N/10` label.
#[must_use]
pub fn score_label(score: f64) -> String {
    let n = (score * SCORE_LABEL_SCALE).round() as i64;
    format!("{n}/10")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_is_empty_without_functions_and_sums_to_total() {
        assert!(distribution(&[]).is_empty());
        let buckets = distribution(&[1.0, 0.5, 0.35, 0.0, 0.0]);
        let counts: Vec<usize> = buckets.iter().map(|b| b.count).collect();
        assert_eq!(counts, [1, 1, 1, 2]);
        assert_eq!(counts.iter().sum::<usize>(), 5);
        assert!((buckets[3].percent - 0.4).abs() < 1e-9);
        assert_eq!(buckets[0].label, DIST_LABEL_EXCELLENT);
    }

    #[test]
    fn issues_hold_only_non_excellent_functions_worst_first() {
        let rows = [
            ("h", "a.go", 0.9),
            ("f", "a.go", 0.5),
            ("z", "b.go", 0.0),
            ("a", "b.go", 0.0),
        ];
        let got = issues(rows.iter().map(|&(n, l, c)| (n, l, c)));
        let names: Vec<&str> = got.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a", "z", "f"]);
        assert_eq!(got[0].severity, SEVERITY_POOR);
        assert_eq!(got[2].severity, SEVERITY_GOOD);
    }

    #[test]
    fn severity_thresholds() {
        assert_eq!(severity(0.29), SEVERITY_POOR);
        assert_eq!(severity(0.3), SEVERITY_FAIR);
        assert_eq!(severity(0.4), SEVERITY_GOOD);
    }

    #[test]
    fn metrics_render_two_decimals_and_scores_round_to_tenths() {
        assert_eq!(format_metric(1.0 / 3.0), "0.33");
        assert_eq!(format_metric(0.0), "0.00");
        assert_eq!(score_label(0.6), "6/10");
        assert_eq!(score_label(0.0), "0/10");
        assert_eq!(score_label(1.0), "10/10");
    }
}
