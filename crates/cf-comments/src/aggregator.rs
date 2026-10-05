//! Cross-file aggregation of per-file comment metrics.
//!
//! Counters are summed and every ratio is computed from the sums: coverage is
//! Σdocumented / Σfunctions and the good-comment ratio is Σgood / Σcomments.
//! A file without declarations (or without comments) therefore never dilutes
//! the respective ratio — there is no per-file mean anywhere.

use crate::types::{CommentCounts, CommentDetail, CommentMetrics, FunctionInfo};

/// Where a file's rows come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    /// Path relative to the analyzed root.
    pub source_file: String,
    /// Language name (empty when unknown).
    pub language: String,
    /// Directory part of `source_file` (`.` at the root).
    pub directory: String,
}

/// One file's metrics with its stamp.
#[derive(Debug, Clone, PartialEq)]
pub struct FileReport {
    /// Where the rows come from.
    pub stamp: FileStamp,
    /// The file's metrics.
    pub metrics: CommentMetrics,
}

/// Accumulates per-file metrics in insertion order.
#[derive(Debug, Default)]
pub struct Aggregator {
    files: Vec<FileReport>,
    counts: CommentCounts,
}

impl Aggregator {
    /// Creates an empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Aggregator::default()
    }

    /// Adds one file's metrics.
    pub fn add(&mut self, stamp: FileStamp, metrics: CommentMetrics) {
        self.counts.add(&metrics.counts);
        self.files.push(FileReport { stamp, metrics });
    }

    /// Adds a file that produced no metrics (an unparsable file): it is
    /// listed, with all counters at zero.
    pub fn add_empty(&mut self, stamp: FileStamp) {
        self.add(stamp, CommentMetrics::default());
    }

    /// The files added so far, in insertion order.
    #[must_use]
    pub fn files(&self) -> &[FileReport] {
        &self.files
    }

    /// Number of files added.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The summed counters; their ratios and message are the aggregate's.
    #[must_use]
    pub fn counts(&self) -> CommentCounts {
        self.counts
    }

    /// Every comment row with its file stamp, in file then line order.
    pub fn comments(&self) -> impl Iterator<Item = (&FileStamp, &CommentDetail)> {
        self.files
            .iter()
            .flat_map(|f| f.metrics.comments.iter().map(move |c| (&f.stamp, c)))
    }

    /// Every declaration row with its file stamp, in file then document
    /// order.
    pub fn functions(&self) -> impl Iterator<Item = (&FileStamp, &FunctionInfo)> {
        self.files
            .iter()
            .flat_map(|f| f.metrics.functions.iter().map(move |fun| (&f.stamp, fun)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NO_COMMENTS_MESSAGE;

    fn stamp(name: &str) -> FileStamp {
        FileStamp {
            source_file: name.to_string(),
            language: "go".to_string(),
            directory: ".".to_string(),
        }
    }

    fn metrics(counts: CommentCounts) -> CommentMetrics {
        CommentMetrics {
            counts,
            ..CommentMetrics::default()
        }
    }

    #[test]
    fn ratios_are_computed_from_the_sums() {
        let mut agg = Aggregator::new();
        agg.add(
            stamp("a.go"),
            metrics(CommentCounts {
                total_comments: 1,
                good_comments: 1,
                bad_comments: 0,
                total_functions: 1,
                documented_functions: 1,
            }),
        );
        agg.add(
            stamp("b.go"),
            metrics(CommentCounts {
                total_comments: 1,
                good_comments: 1,
                bad_comments: 0,
                total_functions: 4,
                documented_functions: 1,
            }),
        );
        let counts = agg.counts();
        assert_eq!(counts.total_functions, 5);
        assert_eq!(counts.documented_functions, 2);
        // 2/5, not the mean of 1/1 and 1/4 (0.625).
        assert!((counts.documentation_coverage() - 0.4).abs() < 1e-9);
        assert!((counts.overall_score() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_file_without_declarations_does_not_dilute_coverage() {
        let mut agg = Aggregator::new();
        agg.add(
            stamp("a.go"),
            metrics(CommentCounts {
                total_comments: 1,
                good_comments: 1,
                bad_comments: 0,
                total_functions: 1,
                documented_functions: 1,
            }),
        );
        agg.add_empty(stamp("README.md"));
        assert_eq!(agg.file_count(), 2);
        assert!((agg.counts().documentation_coverage() - 1.0).abs() < 1e-9);
        assert_eq!(
            agg.counts().message(),
            "Excellent comment quality and placement"
        );
    }

    #[test]
    fn empty_aggregation_is_the_empty_report() {
        let agg = Aggregator::new();
        assert_eq!(agg.file_count(), 0);
        assert_eq!(agg.counts(), CommentCounts::default());
        assert_eq!(agg.counts().overall_score(), 0.0);
        assert_eq!(agg.counts().documentation_coverage(), 0.0);
        assert_eq!(agg.counts().message(), NO_COMMENTS_MESSAGE);
        assert_eq!(agg.comments().count(), 0);
        assert_eq!(agg.functions().count(), 0);
    }
}
