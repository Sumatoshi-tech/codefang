//! The per-tick quality sample container.
//!
//! [`TickQuality`] holds the per-file scalar samples
//! ([`crate::analyzer::FileSample`]) pushed while consuming commits; summary
//! statistics are computed at output time ([`crate::metrics`]). Samples are
//! concatenated, so a tick's statistics do not depend on the order its
//! commits were consumed in.

use crate::analyzer::FileSample;

/// Per-file quality metric values for a single tick (or one commit).
///
/// One value per analyzed file in every vector except `cohesion_scores`,
/// which holds one value per file WITH functions (a function-free file has no
/// cohesion data). Statistics are computed at output time.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TickQuality {
    /// Cyclomatic complexity per file (`total_complexity`).
    pub complexities: Vec<f64>,
    /// Cognitive complexity per file (`cognitive_complexity`).
    pub cognitives: Vec<f64>,
    /// Max single-function complexity per file (`max_complexity`).
    pub max_complexities: Vec<i64>,
    /// Function count per file (`total_functions`).
    pub functions: Vec<i64>,

    /// Halstead volume per file (`volume`).
    pub halstead_volumes: Vec<f64>,
    /// Halstead effort per file (`effort`).
    pub halstead_efforts: Vec<f64>,
    /// Halstead delivered-bugs per file (`delivered_bugs`).
    pub delivered_bugs: Vec<f64>,

    /// Comment overall score per file (`overall_score`).
    pub comment_scores: Vec<f64>,
    /// Documentation coverage per file (`documentation_coverage`).
    pub doc_coverages: Vec<f64>,

    /// Cohesion score per file (`cohesion_score`).
    pub cohesion_scores: Vec<f64>,
}

impl TickQuality {
    /// Creates an empty [`TickQuality`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one analyzed file's sample.
    ///
    /// ```
    /// use cf_quality::{FileSample, TickQuality};
    ///
    /// let mut tq = TickQuality::new();
    /// tq.push(&FileSample {
    ///     complexity: 3.0, cognitive: 1.0, max_complexity: 2, functions: 2,
    ///     halstead_volume: 69.2, halstead_effort: 317.1, delivered_bugs: 0.023,
    ///     comment_score: 1.0, doc_coverage: 0.5, cohesion: None,
    /// });
    /// assert_eq!(tq.files_analyzed(), 1);
    /// assert!(tq.cohesion_scores.is_empty(), "no cohesion data for a function-free sample");
    /// ```
    pub fn push(&mut self, sample: &FileSample) {
        self.complexities.push(sample.complexity);
        self.cognitives.push(sample.cognitive);
        self.max_complexities.push(sample.max_complexity);
        self.functions.push(sample.functions);
        self.halstead_volumes.push(sample.halstead_volume);
        self.halstead_efforts.push(sample.halstead_effort);
        self.delivered_bugs.push(sample.delivered_bugs);
        self.comment_scores.push(sample.comment_score);
        self.doc_coverages.push(sample.doc_coverage);
        if let Some(cohesion) = sample.cohesion {
            self.cohesion_scores.push(cohesion);
        }
    }

    /// Incorporates values from `other` into `self` by concatenation.
    ///
    /// Appends each per-file slice in declaration order. An empty `other` is a
    /// no-op.
    ///
    /// ```
    /// use cf_quality::data::TickQuality;
    ///
    /// let mut a = TickQuality { complexities: vec![5.0, 10.0], ..TickQuality::default() };
    /// let b = TickQuality { complexities: vec![15.0], ..TickQuality::default() };
    /// a.merge(&b);
    /// assert_eq!(a.complexities, vec![5.0, 10.0, 15.0]);
    ///
    /// // Merging an empty tick leaves the receiver unchanged.
    /// a.merge(&TickQuality::default());
    /// assert_eq!(a.files_analyzed(), 3);
    /// ```
    pub fn merge(&mut self, other: &TickQuality) {
        self.complexities.extend_from_slice(&other.complexities);
        self.cognitives.extend_from_slice(&other.cognitives);
        self.max_complexities
            .extend_from_slice(&other.max_complexities);
        self.functions.extend_from_slice(&other.functions);

        self.halstead_volumes
            .extend_from_slice(&other.halstead_volumes);
        self.halstead_efforts
            .extend_from_slice(&other.halstead_efforts);
        self.delivered_bugs.extend_from_slice(&other.delivered_bugs);

        self.comment_scores.extend_from_slice(&other.comment_scores);
        self.doc_coverages.extend_from_slice(&other.doc_coverages);

        self.cohesion_scores
            .extend_from_slice(&other.cohesion_scores);
    }

    /// Returns the number of files analyzed in this tick.
    ///
    /// The length of [`complexities`](TickQuality::complexities) (complexity
    /// is appended for every analyzed file).
    #[must_use]
    pub fn files_analyzed(&self) -> usize {
        self.complexities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_appends_one_value_per_metric() {
        let mut tq = TickQuality::new();
        let with_functions = FileSample {
            complexity: 2.0,
            cognitive: 1.0,
            max_complexity: 2,
            functions: 1,
            halstead_volume: 25.0,
            halstead_effort: 129.0,
            delivered_bugs: 0.008,
            comment_score: 1.0,
            doc_coverage: 1.0,
            cohesion: Some(1.0),
        };
        let function_free = FileSample {
            functions: 0,
            cohesion: None,
            ..with_functions.clone()
        };
        tq.push(&with_functions);
        tq.push(&function_free);
        assert_eq!(tq.files_analyzed(), 2);
        assert_eq!(tq.functions, vec![1, 0]);
        assert_eq!(tq.cohesion_scores, vec![1.0]);
        assert_eq!(tq.halstead_volumes.len(), 2);
    }

    #[test]
    fn merge_concatenates_per_field() {
        let mut tq1 = TickQuality {
            complexities: vec![5.0, 10.0],
            halstead_volumes: vec![100.0],
            comment_scores: vec![0.8],
            cohesion_scores: vec![0.9],
            ..TickQuality::default()
        };
        let tq2 = TickQuality {
            complexities: vec![15.0],
            halstead_volumes: vec![200.0, 300.0],
            comment_scores: vec![0.6],
            cohesion_scores: vec![0.7],
            ..TickQuality::default()
        };

        tq1.merge(&tq2);

        assert_eq!(tq1.complexities.len(), 3);
        assert_eq!(tq1.halstead_volumes.len(), 3);
        assert_eq!(tq1.comment_scores.len(), 2);
        assert_eq!(tq1.cohesion_scores.len(), 2);
    }

    #[test]
    fn files_analyzed_counts_complexities() {
        let tq = TickQuality {
            complexities: vec![1.0, 2.0, 3.0],
            ..TickQuality::default()
        };
        assert_eq!(tq.files_analyzed(), 3);
        assert_eq!(TickQuality::default().files_analyzed(), 0);
    }
}
