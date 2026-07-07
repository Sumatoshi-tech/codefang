//! Minimal analyzer interface used by the plumbing providers.
//!
//! A local definition so the plumbing providers can be expressed against a
//! stable interface without depending on the framework crate. When the
//! framework exposes the canonical trait this module should be deleted and
//! replaced by a re-export.

use std::any::Any;
use std::collections::HashMap;

use crate::uast_iface::SharedParser;

/// Type-erased value flowing between analyzers.
///
/// The pipeline threads heterogeneous outputs (changes, caches, ticks, ...)
/// between providers; `Box<dyn Any>` carries them through a single map type
/// without a closed enum.
pub type AnyValue = Box<dyn Any + Send + Sync>;

/// Dependency / output map.
pub type ValueMap = HashMap<String, AnyValue>;

/// Configuration facts passed to [`Analyzer::configure`].
pub type Facts = HashMap<String, AnyValue>;

/// Error returned by analyzer operations.
///
/// Variants are kept coarse on purpose; the precise error taxonomy belongs to
/// the framework layer.
#[derive(Debug, thiserror::Error)]
pub enum AnalyzerError {
    /// A required dependency was missing or had an unexpected type.
    #[error("missing or mistyped dependency: {0}")]
    Dependency(String),
    /// Configuration failed.
    #[error("configuration error: {0}")]
    Config(String),
    /// An underlying git operation failed.
    #[error("git error: {0}")]
    Git(String),
    /// Any other failure, carrying a message.
    #[error("{0}")]
    Other(String),
}

impl From<git2::Error> for AnalyzerError {
    fn from(e: git2::Error) -> Self {
        AnalyzerError::Git(e.to_string())
    }
}

impl From<std::io::Error> for AnalyzerError {
    fn from(e: std::io::Error) -> Self {
        AnalyzerError::Other(e.to_string())
    }
}

/// A pipeline provider.
///
/// Implementors declare what facts they `provides` and `requires`, are
/// optionally configured with facts and a UAST parser, and `consume` a map of
/// dependency values to produce a map of output values.
pub trait Analyzer {
    /// Stable name of the provider, e.g. `"TreeDiff"`.
    fn name(&self) -> &'static str;

    /// Keys this provider writes into its output map.
    fn provides(&self) -> Vec<&'static str>;

    /// Keys this provider reads from the dependency map.
    fn requires(&self) -> Vec<&'static str>;

    /// Apply free-form configuration facts. Default: no-op.
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when a fact is invalid for this provider.
    fn configure(&mut self, _facts: &Facts) -> Result<(), AnalyzerError> {
        Ok(())
    }

    /// Provide the shared UAST parser. Default: no-op.
    fn configure_uast(&mut self, _parser: SharedParser) {}

    /// Process one commit's worth of dependencies into outputs.
    ///
    /// # Errors
    ///
    /// Returns an [`AnalyzerError`] when a dependency is missing/mistyped or
    /// the underlying computation fails.
    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError>;
}

/// Borrow a typed dependency out of the [`ValueMap`].
///
/// # Errors
///
/// Returns [`AnalyzerError::Dependency`] when the key is absent or holds a
/// value of the wrong type.
pub fn dep<'a, T: 'static>(deps: &'a ValueMap, key: &str) -> Result<&'a T, AnalyzerError> {
    deps.get(key)
        .ok_or_else(|| AnalyzerError::Dependency(format!("{key} not present")))?
        .downcast_ref::<T>()
        .ok_or_else(|| AnalyzerError::Dependency(format!("{key} has unexpected type")))
}
