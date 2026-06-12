//! Sentinel errors for the analyze crate.
//!
//! The error *strings* are part of the log/CLI contract and are reproduced
//! verbatim (pinned by the differential gate); sentinel matching is modelled
//! by enum variants.

use thiserror::Error;

/// Errors produced by the analyze crate.
///
/// Each variant is a sentinel error or a wrapped-detail family. Detail
/// strings reproduce the contract's `%s` / `%q`-style formatting.
#[derive(Debug, Error)]
pub enum AnalyzeError {
    /// `no registered analyzer with name`.
    #[error("no registered analyzer with name: {0}")]
    UnregisteredAnalyzer(String),

    /// `analysis failed`.
    #[error("analysis failed: {0}")]
    AnalysisFailed(String),

    /// `root node is nil`.
    #[error("root node is nil")]
    NilRootNode,

    /// `not implemented`.
    #[error("not implemented")]
    NotImplemented,

    /// `missing ComputeMetricsFn hook`.
    #[error("missing ComputeMetricsFn hook")]
    MissingComputeMetrics,

    /// `unknown analyzer id`.
    #[error("unknown analyzer id: {0}")]
    UnknownAnalyzerId(String),

    /// `duplicate analyzer id`.
    #[error("duplicate analyzer id: {0}")]
    DuplicateAnalyzerId(String),

    /// `invalid analyzer mode`.
    #[error("invalid analyzer mode for {id}: expected {expected}, got {got}")]
    InvalidAnalyzerMode {
        /// Analyzer ID whose mode mismatched.
        id: String,
        /// The mode expected from the registration category.
        expected: String,
        /// The mode declared by the descriptor.
        got: String,
    },

    /// `invalid analyzer glob`.
    #[error("invalid analyzer glob {pattern:?}: {cause}")]
    InvalidAnalyzerGlob {
        /// The malformed glob pattern.
        pattern: String,
        /// Underlying match error.
        cause: String,
    },

    /// `unsupported format`.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    /// `invalid unified model`.
    #[error("invalid unified model: {0}")]
    InvalidUnifiedModel(String),

    /// `invalid mixed format`.
    #[error("invalid mixed format: {0}")]
    InvalidMixedFormat(String),

    /// `invalid static format`.
    #[error("invalid static format: {0}")]
    InvalidStaticFormat(String),

    /// `invalid history format`.
    #[error("invalid history format: {0}")]
    InvalidHistoryFormat(String),

    /// `invalid input format`.
    #[error("invalid input format: {0}")]
    InvalidInputFormat(String),

    /// `unexpected binary envelope count`.
    #[error("unexpected binary envelope count: {0}")]
    BinaryEnvelopeCount(String),

    /// A wrapped conversion/encode error preserving the "{step}: {cause}"
    /// shape.
    #[error("{0}")]
    Encode(String),
}
