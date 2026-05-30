//! Threshold-based classification (`classify.go`).
//!
//! Maps ordered values to string labels using descending thresholds: the first
//! threshold whose `limit` is `<=` the value wins. This is the generic engine
//! behind [`crate::threshold_labeler::ThresholdLabeler`].

/// A single classification boundary: values `>= limit` are assigned `label`.
///
/// Mirrors `common.Threshold[T]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Threshold<T> {
    /// Inclusive lower bound for this label.
    pub limit: T,
    /// Label assigned when `value >= limit`.
    pub label: String,
}

impl<T> Threshold<T> {
    /// Creates a threshold from a limit and label.
    pub fn new(limit: T, label: impl Into<String>) -> Self {
        Threshold {
            limit,
            label: label.into(),
        }
    }
}

/// Maps ordered values to string labels using descending thresholds.
///
/// Mirrors `common.Classifier[T]`. Construction copies and sorts the thresholds
/// in descending order by `limit`, leaving the caller's slice untouched, so the
/// classifier is safe to share after construction.
#[derive(Debug, Clone)]
pub struct Classifier<T> {
    thresholds: Vec<Threshold<T>>,
    default_label: String,
}

impl<T: PartialOrd + Clone> Classifier<T> {
    /// Creates a classifier from the given thresholds and default label.
    ///
    /// Thresholds are copied and sorted in descending order by `limit`; the
    /// input slice is not modified. Mirrors `common.NewClassifier`.
    pub fn new(thresholds: &[Threshold<T>], default_label: impl Into<String>) -> Self {
        let mut sorted: Vec<Threshold<T>> = thresholds.to_vec();
        // Descending by limit. partial_cmp matches Go's cmp.Compare for the
        // ordered numeric types used here; equal limits keep a stable order.
        sorted.sort_by(|a, b| {
            b.limit
                .partial_cmp(&a.limit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Classifier {
            thresholds: sorted,
            default_label: default_label.into(),
        }
    }

    /// Returns the label of the first threshold where `value >= limit`.
    ///
    /// Falls back to the default label when no threshold matches. Mirrors
    /// `common.Classifier.Classify`.
    pub fn classify(&self, value: T) -> &str {
        for t in &self.thresholds {
            if value >= t.limit {
                return &t.label;
            }
        }
        &self.default_label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(limit: f64, label: &str) -> Threshold<f64> {
        Threshold::new(limit, label)
    }

    #[test]
    fn classify_float_boundaries() {
        let thresholds = [f(0.8, "high"), f(0.5, "medium"), f(0.2, "low")];
        let c = Classifier::new(&thresholds, "none");

        assert_eq!(c.classify(0.9), "high");
        assert_eq!(c.classify(0.8), "high");
        assert_eq!(c.classify(0.6), "medium");
        assert_eq!(c.classify(0.5), "medium");
        assert_eq!(c.classify(0.3), "low");
        assert_eq!(c.classify(0.2), "low");
        assert_eq!(c.classify(0.1), "none");
    }

    #[test]
    fn classify_int_type() {
        let thresholds = [
            Threshold::new(100, "large"),
            Threshold::new(50, "medium"),
            Threshold::new(10, "small"),
        ];
        let c = Classifier::new(&thresholds, "tiny");

        for (v, want) in [
            (150, "large"),
            (100, "large"),
            (75, "medium"),
            (50, "medium"),
            (25, "small"),
            (10, "small"),
            (5, "tiny"),
        ] {
            assert_eq!(c.classify(v), want, "value {v}");
        }
    }

    #[test]
    fn new_classifier_sorts_unsorted_input() {
        let thresholds = [f(0.2, "low"), f(0.8, "high"), f(0.5, "medium")];
        let c = Classifier::new(&thresholds, "none");
        assert_eq!(c.classify(0.9), "high");
        assert_eq!(c.classify(0.6), "medium");
    }

    #[test]
    fn new_classifier_does_not_modify_input() {
        let thresholds = [f(0.2, "low"), f(0.8, "high")];
        let _ = Classifier::new(&thresholds, "none");
        assert_eq!(thresholds[0].limit, 0.2);
    }
}
