//! Per-tick / per-commit quality data containers.
//!
//! [`TickQuality`] and [`TickData`] hold per-file scalar samples gathered while
//! consuming commits; summary statistics are computed at output time
//! ([`crate::metrics`]).
//!
//! Order-independence: the composite quality analyzer keys per-commit
//! [`TickQuality`] by commit hash and merges by concatenation, so the result is
//! independent of the order commits are consumed.

use std::collections::BTreeMap;

/// Per-file quality metric values for a single tick.
///
/// Values are appended per file while consuming a commit; statistics are
/// computed at output time. The vector field order is the declaration order,
/// which is the order [`merge`](TickQuality::merge) concatenates in.
#[derive(Debug, Clone, Default, PartialEq)]
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

    /// Incorporates values from `other` into `self` by concatenation.
    ///
    /// Appends each per-file slice in declaration order. An empty `other` is a
    /// no-op.
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

/// Per-tick aggregated payload.
///
/// `commit_quality` maps commit hash (lowercase hex) to that commit's
/// [`TickQuality`]. A [`BTreeMap`] is used so that any direct iteration is
/// deterministic; the canonical machine output sorts keys regardless.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TickData {
    /// Maps commit hash (hex) to per-commit [`TickQuality`].
    pub commit_quality: BTreeMap<String, TickQuality>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors the reference suite's TestTickQuality_Merge.
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
