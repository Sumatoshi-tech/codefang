//! History-analyzer contracts and the analyzer-mode discriminant.
//!
//! The heavy Git/UAST types that the run-context / commit interfaces
//! reference live in higher crates (`cf-gitlib`, `cf-plumbing`, `cf-uast`,
//! `cf-uast-node`); they are expressed here as **minimal associated/opaque
//! types** so this crate stays below the framework and does not link
//! git2/tree-sitter directly. See the deferred-dependency note in `lib.rs`
//! and the crate's structured-output notes.
//!
//! The two pieces that are fully realized here — because they cross the
//! serialization boundary and must be byte-exact — are:
//!
//! - [`AnalyzerMode`] ([`MODE_STATIC`] / [`MODE_HISTORY`]) used in
//!   [`crate::conversion::AnalyzerResult`] and analyzer descriptors;
//! - the [`HistoryAnalyzer`] trait surface that [`crate::BaseHistoryAnalyzer`]
//!   implements.

use std::io::Write;

use cf_pipeline::ConfigurationOption;

use crate::analyzer::{Analyzer, Report};
use crate::descriptor::Descriptor;
use crate::interfaces::{Aggregator, AggregatorOptions};
use crate::tc::{Tc, Tick};

/// `ModeStatic` — analyzers that run during the UAST/static phase (`"static"`).
///
/// The mode is a string on the wire and `MODE_STATIC`/`MODE_HISTORY` are its
/// values; the string is what appears in `AnalyzerResult.mode` JSON output, so
/// the literal is reproduced exactly.
pub const MODE_STATIC: &str = "static";
/// `ModeHistory` — analyzers that run during the git-history phase (`"history"`).
pub const MODE_HISTORY: &str = "history";

/// Analyzer mode discriminant — a thin newtype over the wire string value so it
/// serializes byte-identically as a JSON/YAML string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnalyzerMode(pub String);

impl AnalyzerMode {
    /// The static mode value.
    #[must_use]
    pub fn static_mode() -> Self {
        Self(MODE_STATIC.to_string())
    }

    /// The history mode value.
    #[must_use]
    pub fn history() -> Self {
        Self(MODE_HISTORY.to_string())
    }

    /// The underlying string value (what gets serialized).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this is one of the two valid modes. Used by
    /// [`crate::conversion::UnifiedModel::validate`].
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.0 == MODE_STATIC || self.0 == MODE_HISTORY
    }
}

impl std::fmt::Display for AnalyzerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque per-commit plumbing snapshot.
///
/// An opaque plumbing snapshot: the framework treats
/// it as opaque; concrete snapshot types are defined in the plumbing package.
pub type PlumbingSnapshot = Option<Box<dyn std::any::Any + Send>>;

/// Optionally implemented by leaf analyzers that support parallel Fork/Merge
/// execution.
pub trait Parallelizable {
    /// True if this analyzer cannot be parallelized (cumulative state).
    fn sequential_only(&self) -> bool;
    /// True if `consume` is CPU-intensive and benefits from W workers.
    fn cpu_heavy(&self) -> bool;
    /// Captures the current plumbing output state (opaque).
    fn snapshot_plumbing(&self) -> PlumbingSnapshot {
        None
    }
    /// Restores plumbing state from a previously captured snapshot.
    fn apply_snapshot(&mut self, _snapshot: PlumbingSnapshot) {}
    /// Releases any resources owned by the snapshot.
    fn release_snapshot(&mut self, _snapshot: PlumbingSnapshot) {}
}

/// The contract for history-based analyzers.
///
/// The `Repository` / per-commit `Context` parameters are abstracted as
/// associated types so this crate does not depend on the (unported) `cf-gitlib`
/// / `cf-plumbing` / `cf-uast` crates. A concrete analyzer crate binds them to
/// the real git/uast types.
pub trait HistoryAnalyzer: Analyzer {
    /// The repository handle type (bound to `cf_gitlib::Repository` downstream).
    type Repository;
    /// The per-commit analysis context (bound to the ported `Context`).
    type Context;

    /// Initializes the analyzer for a repository.
    ///
    /// # Errors
    /// Propagates analyzer-specific initialization failures.
    fn initialize(&mut self, repository: &Self::Repository) -> Result<(), AnalyzerError>;

    /// Consumes one commit, returning its per-commit `TC`.
    /// Plumbing analyzers return a zero-value `TC` (`data: None`).
    ///
    /// # Errors
    /// Propagates analyzer-specific consumption failures.
    fn consume(&mut self, ac: &Self::Context) -> Result<Tc, AnalyzerError>;

    /// Estimated bytes of analyzer-internal working state.
    fn working_state_size(&self) -> i64;
    /// Estimated bytes of TC payload emitted per commit.
    fn avg_tc_size(&self) -> i64;

    /// Creates a per-analyzer aggregator, or `None`.
    fn new_aggregator(&self, opts: AggregatorOptions) -> Option<Box<dyn Aggregator>>;

    /// Writes aggregated `TICK`s in `format` to `writer`.
    ///
    /// # Errors
    /// Returns [`AnalyzerError::NotImplemented`] when not wired, or a
    /// serialization error otherwise.
    fn serialize_ticks(
        &self,
        ticks: &[Tick],
        format: &str,
        writer: &mut dyn Write,
    ) -> Result<(), AnalyzerError>;

    /// Converts aggregated `TICK`s into a [`Report`].
    ///
    /// # Errors
    /// Returns [`AnalyzerError::NotImplemented`] for analyzers without an
    /// aggregator.
    fn report_from_ticks(&self, ticks: &[Tick]) -> Result<Report, AnalyzerError>;

    /// Forks `n` independent copies for parallel processing.
    fn fork(
        &self,
        n: usize,
    ) -> Vec<Box<dyn HistoryAnalyzer<Repository = Self::Repository, Context = Self::Context>>>;
    /// Merges forked branches back into self.
    fn merge(
        &mut self,
        branches: Vec<
            Box<dyn HistoryAnalyzer<Repository = Self::Repository, Context = Self::Context>>,
        >,
    );

    /// Serializes a finalized report in `format`.
    ///
    /// # Errors
    /// Returns a serialization error (e.g. [`AnalyzerError::UnsupportedFormat`]).
    fn serialize(
        &self,
        result: &Report,
        format: &str,
        writer: &mut dyn Write,
    ) -> Result<(), AnalyzerError>;
}

/// Error type for the analyzer interfaces.
///
/// `NotImplemented` is the "not implemented" sentinel; `UnsupportedFormat`
/// carries through [`crate::formats::FormatError`]; `Other` boxes
/// analyzer-specific failures.
#[derive(Debug)]
pub enum AnalyzerError {
    /// A stub method that is not yet wired ("not implemented" sentinel).
    NotImplemented,
    /// The requested serialization format is unsupported.
    UnsupportedFormat(crate::formats::FormatError),
    /// `ComputeMetricsFn` hook was nil (`ErrMissingComputeMetrics`).
    MissingComputeMetrics,
    /// Any other analyzer-specific error, preserving its message.
    Other(String),
}

impl std::fmt::Display for AnalyzerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => write!(f, "{}", crate::ERR_NOT_IMPLEMENTED),
            Self::UnsupportedFormat(e) => write!(f, "{e}"),
            Self::MissingComputeMetrics => {
                write!(f, "{}", crate::base_history::ERR_MISSING_COMPUTE_METRICS)
            }
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for AnalyzerError {}

impl From<crate::formats::FormatError> for AnalyzerError {
    fn from(e: crate::formats::FormatError) -> Self {
        Self::UnsupportedFormat(e)
    }
}

/// Optionally implemented by analyzers that write chunked records directly to a
/// report store, bypassing monolithic report maps. `ReportWriter` is provided by the store layer,
/// so this is a marker the downstream crate parameterizes.
pub trait StoreWriter<W> {
    /// Streams aggregated `TICK`s as records to the writer.
    ///
    /// # Errors
    /// Propagates store-write failures.
    fn write_to_store(&self, ticks: &[Tick], w: &mut W) -> Result<(), AnalyzerError>;
}

/// Helper bound used by [`crate::BaseHistoryAnalyzer`] to expose its descriptor /
/// config-options without requiring the full [`HistoryAnalyzer`] surface; this
/// keeps the trait usable by analyzers that have not yet implemented every
/// method.
pub trait HistoryMeta {
    /// Stable analyzer descriptor.
    fn descriptor(&self) -> Descriptor;
    /// Configurable options for this analyzer.
    fn list_configuration_options(&self) -> Vec<ConfigurationOption>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_values_match_reference() {
        assert_eq!(AnalyzerMode::static_mode().as_str(), "static");
        assert_eq!(AnalyzerMode::history().as_str(), "history");
    }

    #[test]
    fn mode_validity() {
        assert!(AnalyzerMode::static_mode().is_valid());
        assert!(AnalyzerMode::history().is_valid());
        assert!(!AnalyzerMode("bogus".into()).is_valid());
    }

    #[test]
    fn not_implemented_message_matches_go() {
        assert_eq!(AnalyzerError::NotImplemented.to_string(), "not implemented");
    }
}
