//! Sentinel errors for the analyze crate.
//!
//! These mirror the Go `errors.New(...)` / `fmt.Errorf(...)` sentinels in the
//! `analyze` package. The error *strings* are reproduced verbatim so log/CLI
//! output (and `errors.Is`-style matching, modelled here by enum variants)
//! stays consistent across the Go and Rust implementations.

use thiserror::Error;

/// Errors produced by the analyze crate.
///
/// Each variant corresponds to a Go sentinel error or `fmt.Errorf` family.
/// Wrapped detail strings reproduce the Go `%s` / `%q` formatting.
#[derive(Debug, Error)]
pub enum AnalyzeError {
    /// `no registered analyzer with name`. Go: `ErrUnregisteredAnalyzer`.
    #[error("no registered analyzer with name: {0}")]
    UnregisteredAnalyzer(String),

    /// `analysis failed`. Go: `ErrAnalysisFailed`.
    #[error("analysis failed: {0}")]
    AnalysisFailed(String),

    /// `root node is nil`. Go: `ErrNilRootNode`.
    #[error("root node is nil")]
    NilRootNode,

    /// `not implemented`. Go: `ErrNotImplemented`.
    #[error("not implemented")]
    NotImplemented,

    /// `missing ComputeMetricsFn hook`. Go: `ErrMissingComputeMetrics`.
    #[error("missing ComputeMetricsFn hook")]
    MissingComputeMetrics,

    /// `unknown analyzer id`. Go: `ErrUnknownAnalyzerID`.
    #[error("unknown analyzer id: {0}")]
    UnknownAnalyzerId(String),

    /// `duplicate analyzer id`. Go: `ErrDuplicateAnalyzerID`.
    #[error("duplicate analyzer id: {0}")]
    DuplicateAnalyzerId(String),

    /// `invalid analyzer mode`. Go: `ErrInvalidAnalyzerMode`.
    #[error("invalid analyzer mode for {id}: expected {expected}, got {got}")]
    InvalidAnalyzerMode {
        /// Analyzer ID whose mode mismatched.
        id: String,
        /// The mode expected from the registration category.
        expected: String,
        /// The mode declared by the descriptor.
        got: String,
    },

    /// `invalid analyzer glob`. Go: `ErrInvalidAnalyzerGlob`.
    #[error("invalid analyzer glob {pattern:?}: {cause}")]
    InvalidAnalyzerGlob {
        /// The malformed glob pattern.
        pattern: String,
        /// Underlying match error.
        cause: String,
    },

    /// `unsupported format`. Go: `ErrUnsupportedFormat`.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    /// `invalid unified model`. Go: `ErrInvalidUnifiedModel`.
    #[error("invalid unified model: {0}")]
    InvalidUnifiedModel(String),

    /// `invalid mixed format`. Go: `ErrInvalidMixedFormat`.
    #[error("invalid mixed format: {0}")]
    InvalidMixedFormat(String),

    /// `invalid static format`. Go: `ErrInvalidStaticFormat`.
    #[error("invalid static format: {0}")]
    InvalidStaticFormat(String),

    /// `invalid history format`. Go: `ErrInvalidHistoryFormat`.
    #[error("invalid history format: {0}")]
    InvalidHistoryFormat(String),

    /// `invalid input format`. Go: `ErrInvalidInputFormat`.
    #[error("invalid input format: {0}")]
    InvalidInputFormat(String),

    /// `unexpected binary envelope count`. Go: `ErrBinaryEnvelopeCount`.
    #[error("unexpected binary envelope count: {0}")]
    BinaryEnvelopeCount(String),

    /// A wrapped conversion/encode error preserving the Go "{step}: {cause}"
    /// shape.
    #[error("{0}")]
    Encode(String),
}
