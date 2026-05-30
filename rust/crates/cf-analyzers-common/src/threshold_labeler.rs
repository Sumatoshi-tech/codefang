//! Float-score-to-label mapping (`threshold_labeler.go`).
//!
//! A thin wrapper over an ordered list of [`Threshold<f64>`] values. Unlike
//! [`crate::classify::Classifier`], the labeler does **not** sort its
//! thresholds: callers must supply them in descending order by limit, and the
//! first threshold where `score >= limit` wins. An empty labeler, or a score
//! below every limit, yields the empty string.

use crate::classify::Threshold;

/// Maps a `f64` score to a string label using a caller-ordered threshold list.
///
/// Mirrors `common.ThresholdLabeler`. Thresholds must be sorted descending by
/// limit (highest first); a catch-all can be added as a final entry with the
/// minimum limit.
#[derive(Debug, Clone, Default)]
pub struct ThresholdLabeler {
    thresholds: Vec<Threshold<f64>>,
}

impl ThresholdLabeler {
    /// Creates a labeler from an already-descending threshold list.
    #[must_use]
    pub fn new(thresholds: Vec<Threshold<f64>>) -> Self {
        ThresholdLabeler { thresholds }
    }

    /// Returns the label of the first threshold where `score >= limit`.
    ///
    /// Returns `""` if the labeler is empty or no threshold matches. Mirrors
    /// `common.ThresholdLabeler.Label`.
    #[must_use]
    pub fn label(&self, score: f64) -> &str {
        for t in &self.thresholds {
            if score >= t.limit {
                return &t.label;
            }
        }
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labeler() -> ThresholdLabeler {
        ThresholdLabeler::new(vec![
            Threshold::new(0.8, "Excellent"),
            Threshold::new(0.6, "Good"),
            Threshold::new(0.4, "Fair"),
            Threshold::new(0.0, "Poor"),
        ])
    }

    #[test]
    fn label_boundaries() {
        let l = labeler();
        for (score, want) in [
            (0.9, "Excellent"),
            (0.8, "Excellent"),
            (0.7, "Good"),
            (0.6, "Good"),
            (0.5, "Fair"),
            (0.4, "Fair"),
            (0.1, "Poor"),
            (0.0, "Poor"),
        ] {
            assert_eq!(l.label(score), want, "score {score}");
        }
    }

    #[test]
    fn empty_labeler_returns_empty() {
        let l = ThresholdLabeler::default();
        assert_eq!(l.label(0.5), "");
    }

    #[test]
    fn no_match_returns_empty() {
        let l = ThresholdLabeler::new(vec![Threshold::new(0.8, "high")]);
        assert_eq!(l.label(0.5), "");
    }
}
