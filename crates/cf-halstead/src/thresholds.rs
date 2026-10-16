//! Every Halstead threshold, label, and score formula — defined once and
//! used by every report format.
//!
//! All bands are lower-inclusive: a value *at* a bound belongs to the higher
//! band.
//!
//! | Measure (per function) | Low / Simple | Medium / Moderate | High / Complex | Very High |
//! |------------------------|--------------|-------------------|----------------|-----------|
//! | Volume V               | V < 100      | 100 ≤ V < 1000    | 1000 ≤ V < 5000| V ≥ 5000  |
//! | Difficulty D           | D < 5        | 5 ≤ D < 15        | D ≥ 15         | —         |
//! | Effort E               | E < 1000     | 1000 ≤ E < 10000  | E ≥ 10000      | —         |
//!
//! A function is **high-effort** when `E ≥ 10000`; its risk is `HIGH` from
//! `E ≥ 50000`, else `MEDIUM`. Issue severity is `poor` when `E ≥ 50000` or
//! `B ≥ 1.0`, `fair` when `E ≥ 10000` or `B ≥ 0.3`, else `good`.
//!
//! The codebase (or file) **health** is keyed off the share `h` of its
//! functions that are high-effort: `h < 5%` → low complexity, `< 15%` →
//! moderate, `< 30%` → high, else very high. The health score interpolates
//! linearly through `(0, 100)`, `(5%, 90)`, `(15%, 70)`, `(30%, 40)`,
//! `(100%, 0)`; a set with no functions scores 100 with the
//! [`MSG_NO_FUNCTIONS`] message.

/// Volume at which a function leaves the `Low` band.
pub const VOLUME_MEDIUM: f64 = 100.0;
/// Volume at which a function enters the `High` band.
pub const VOLUME_HIGH: f64 = 1000.0;
/// Volume at which a function enters the `Very High` band.
pub const VOLUME_VERY_HIGH: f64 = 5000.0;

/// Difficulty at which a function leaves `Simple`.
pub const DIFFICULTY_MODERATE: f64 = 5.0;
/// Difficulty at which a function becomes `Complex`.
pub const DIFFICULTY_COMPLEX: f64 = 15.0;

/// Effort at which a function leaves `Low`.
pub const EFFORT_MEDIUM: f64 = 1000.0;
/// Effort at which a function is `High` — the high-effort bound.
pub const EFFORT_HIGH: f64 = 10_000.0;
/// Effort at which a high-effort function's risk is `HIGH`.
pub const EFFORT_VERY_HIGH: f64 = 50_000.0;

/// Delivered-bugs estimate at which an issue is at least `fair`.
pub const BUGS_FAIR: f64 = 0.3;
/// Delivered-bugs estimate at which an issue is `poor`.
pub const BUGS_POOR: f64 = 1.0;

/// High-effort share below which the codebase has low complexity.
pub const SHARE_LOW: f64 = 0.05;
/// High-effort share below which the codebase has moderate complexity.
pub const SHARE_MODERATE: f64 = 0.15;
/// High-effort share below which the codebase has high complexity.
pub const SHARE_HIGH: f64 = 0.30;

/// Health score of a codebase with no high-effort functions.
pub const HEALTH_MAX: f64 = 100.0;
/// Health score at the low / moderate boundary ([`SHARE_LOW`]).
pub const HEALTH_LOW_EDGE: f64 = 90.0;
/// Health score at the moderate / high boundary ([`SHARE_MODERATE`]).
pub const HEALTH_MODERATE_EDGE: f64 = 70.0;
/// Health score at the high / very-high boundary ([`SHARE_HIGH`]).
pub const HEALTH_HIGH_EDGE: f64 = 40.0;
/// Health score when every function is high-effort.
pub const HEALTH_MIN: f64 = 0.0;

/// Aggregate message when the analyzed set has no functions.
pub const MSG_NO_FUNCTIONS: &str = "No functions found";
/// Aggregate message for a high-effort share below [`SHARE_LOW`].
pub const MSG_LOW: &str = "Low Halstead complexity - well-structured code";
/// Aggregate message for a high-effort share below [`SHARE_MODERATE`].
pub const MSG_MODERATE: &str = "Moderate Halstead complexity - acceptable";
/// Aggregate message for a high-effort share below [`SHARE_HIGH`].
pub const MSG_HIGH: &str = "High Halstead complexity - consider refactoring";
/// Aggregate message for a high-effort share at or above [`SHARE_HIGH`].
pub const MSG_VERY_HIGH: &str =
    "Very high Halstead complexity - significant refactoring recommended";

/// Per-function volume band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeLevel {
    /// `V < 100`.
    Low,
    /// `100 ≤ V < 1000`.
    Medium,
    /// `1000 ≤ V < 5000`.
    High,
    /// `V ≥ 5000`.
    VeryHigh,
}

impl VolumeLevel {
    /// Every band, lowest first (the distribution order).
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::VeryHigh];

    /// The band of a volume.
    #[must_use]
    pub fn of(volume: f64) -> Self {
        if volume >= VOLUME_VERY_HIGH {
            Self::VeryHigh
        } else if volume >= VOLUME_HIGH {
            Self::High
        } else if volume >= VOLUME_MEDIUM {
            Self::Medium
        } else {
            Self::Low
        }
    }

    /// Position in [`Self::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
            Self::VeryHigh => 3,
        }
    }

    /// Human label (`complexity_level`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::VeryHigh => "Very High",
        }
    }

    /// Machine key (yaml / bin `distribution`).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::VeryHigh => "very_high",
        }
    }

    /// Distribution bucket label with its bounds (json `distribution`).
    #[must_use]
    pub fn bucket_label(self) -> &'static str {
        match self {
            Self::Low => "Low (<100)",
            Self::Medium => "Medium (100-1000)",
            Self::High => "High (1000-5000)",
            Self::VeryHigh => "Very High (>=5000)",
        }
    }
}

/// Per-function volume assessment: 🟢 below 100, 🟡 below 1000, 🔴 above.
#[must_use]
pub fn volume_assessment(volume: f64) -> &'static str {
    match VolumeLevel::of(volume) {
        VolumeLevel::Low => "🟢 Low",
        VolumeLevel::Medium => "🟡 Medium",
        VolumeLevel::High | VolumeLevel::VeryHigh => "🔴 High",
    }
}

/// Per-function difficulty assessment.
#[must_use]
pub fn difficulty_assessment(difficulty: f64) -> &'static str {
    if difficulty >= DIFFICULTY_COMPLEX {
        "🔴 Complex"
    } else if difficulty >= DIFFICULTY_MODERATE {
        "🟡 Moderate"
    } else {
        "🟢 Simple"
    }
}

/// Per-function effort assessment.
#[must_use]
pub fn effort_assessment(effort: f64) -> &'static str {
    if effort >= EFFORT_HIGH {
        "🔴 High"
    } else if effort >= EFFORT_MEDIUM {
        "🟡 Medium"
    } else {
        "🟢 Low"
    }
}

/// True for a high-effort function (`E ≥ 10000`).
#[must_use]
pub fn is_high_effort(effort: f64) -> bool {
    effort >= EFFORT_HIGH
}

/// Risk level of a high-effort function.
#[must_use]
pub fn risk_level(effort: f64) -> &'static str {
    if effort >= EFFORT_VERY_HIGH {
        "HIGH"
    } else {
        "MEDIUM"
    }
}

/// Issue severity of a function from its effort and bug estimate.
#[must_use]
pub fn severity(effort: f64, bugs: f64) -> &'static str {
    if effort >= EFFORT_VERY_HIGH || bugs >= BUGS_POOR {
        "poor"
    } else if effort >= EFFORT_HIGH || bugs >= BUGS_FAIR {
        "fair"
    } else {
        "good"
    }
}

/// The aggregate message for `total_functions` functions of which the share
/// `high_effort_share` (0..=1) is high-effort.
#[must_use]
pub fn message(total_functions: i64, high_effort_share: f64) -> &'static str {
    if total_functions == 0 {
        MSG_NO_FUNCTIONS
    } else if high_effort_share < SHARE_LOW {
        MSG_LOW
    } else if high_effort_share < SHARE_MODERATE {
        MSG_MODERATE
    } else if high_effort_share < SHARE_HIGH {
        MSG_HIGH
    } else {
        MSG_VERY_HIGH
    }
}

/// Health score (0..=100) from the high-effort share, interpolated linearly
/// through the documented anchors.
#[must_use]
pub fn health_score(high_effort_share: f64) -> f64 {
    const ANCHORS: [(f64, f64); 5] = [
        (0.0, HEALTH_MAX),
        (SHARE_LOW, HEALTH_LOW_EDGE),
        (SHARE_MODERATE, HEALTH_MODERATE_EDGE),
        (SHARE_HIGH, HEALTH_HIGH_EDGE),
        (1.0, HEALTH_MIN),
    ];
    let share = high_effort_share.clamp(0.0, 1.0);
    for pair in ANCHORS.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if share <= x1 {
            return y0 + (share - x0) / (x1 - x0) * (y1 - y0);
        }
    }
    HEALTH_MIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_bands_are_lower_inclusive() {
        assert_eq!(VolumeLevel::of(99.999), VolumeLevel::Low);
        assert_eq!(VolumeLevel::of(100.0), VolumeLevel::Medium);
        assert_eq!(VolumeLevel::of(999.999), VolumeLevel::Medium);
        assert_eq!(VolumeLevel::of(1000.0), VolumeLevel::High);
        assert_eq!(VolumeLevel::of(5000.0), VolumeLevel::VeryHigh);
        assert_eq!(VolumeLevel::of(0.0), VolumeLevel::Low);
    }

    #[test]
    fn every_band_has_a_distinct_index_name_key_and_label() {
        for (i, level) in VolumeLevel::ALL.iter().enumerate() {
            assert_eq!(level.index(), i);
        }
        assert_eq!(VolumeLevel::VeryHigh.name(), "Very High");
        assert_eq!(VolumeLevel::VeryHigh.key(), "very_high");
        assert_eq!(VolumeLevel::Medium.bucket_label(), "Medium (100-1000)");
    }

    #[test]
    fn per_function_assessments_follow_the_table() {
        assert_eq!(volume_assessment(99.0), "🟢 Low");
        assert_eq!(volume_assessment(100.0), "🟡 Medium");
        assert_eq!(volume_assessment(1000.0), "🔴 High");
        assert_eq!(difficulty_assessment(4.99), "🟢 Simple");
        assert_eq!(difficulty_assessment(5.0), "🟡 Moderate");
        assert_eq!(difficulty_assessment(15.0), "🔴 Complex");
        assert_eq!(effort_assessment(999.0), "🟢 Low");
        assert_eq!(effort_assessment(1000.0), "🟡 Medium");
        assert_eq!(effort_assessment(10_000.0), "🔴 High");
    }

    #[test]
    fn high_effort_risk_and_severity() {
        assert!(!is_high_effort(9999.0));
        assert!(is_high_effort(10_000.0));
        assert_eq!(risk_level(10_000.0), "MEDIUM");
        assert_eq!(risk_level(50_000.0), "HIGH");
        assert_eq!(severity(0.0, 0.0), "good");
        assert_eq!(severity(10_000.0, 0.0), "fair");
        assert_eq!(severity(0.0, 0.3), "fair");
        assert_eq!(severity(50_000.0, 0.0), "poor");
        assert_eq!(severity(0.0, 1.0), "poor");
    }

    #[test]
    fn message_bands_on_the_high_effort_share() {
        assert_eq!(message(0, 0.0), MSG_NO_FUNCTIONS);
        assert_eq!(message(10, 0.0), MSG_LOW);
        assert_eq!(message(100, 0.04), MSG_LOW);
        assert_eq!(message(100, 0.05), MSG_MODERATE);
        assert_eq!(message(100, 0.15), MSG_HIGH);
        assert_eq!(message(100, 0.30), MSG_VERY_HIGH);
        assert_eq!(message(1, 1.0), MSG_VERY_HIGH);
    }

    #[test]
    fn health_score_interpolates_through_the_anchors() {
        assert_eq!(health_score(0.0), HEALTH_MAX);
        assert_eq!(health_score(SHARE_LOW), HEALTH_LOW_EDGE);
        assert_eq!(health_score(SHARE_MODERATE), HEALTH_MODERATE_EDGE);
        assert_eq!(health_score(SHARE_HIGH), HEALTH_HIGH_EDGE);
        assert_eq!(health_score(1.0), HEALTH_MIN);
        // Midpoint of the first segment.
        assert!((health_score(0.025) - 95.0).abs() < 1e-9);
        // Monotone non-increasing.
        let mut last = HEALTH_MAX;
        for i in 0..=100 {
            let h = health_score(f64::from(i) / 100.0);
            assert!(h <= last + 1e-9, "health must not increase with the share");
            last = h;
        }
        assert_eq!(health_score(-1.0), HEALTH_MAX);
        assert_eq!(health_score(2.0), HEALTH_MIN);
    }
}
