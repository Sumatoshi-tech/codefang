//! Minimal analyzer interface used by the plumbing providers.
//!
//! In the Go implementation this is `framework.Analyzer`
//! (`internal/analyzers/framework/registry.go`):
//!
//! ```go
//! type Analyzer interface {
//!     Name() string
//!     Provides() []string
//!     Requires() []string
//!     Configure(facts map[string]any) error
//!     ConfigureUAST(parser uast.Parser)
//!     Consume(deps map[string]any) (map[string]any, error)
//! }
//! ```
//!
//! The canonical home for this trait is `cf-core`. At the time this crate was
//! ported `cf-core` was still a stub, so a local definition is provided here so
//! the plumbing providers can be expressed against a stable interface. When
//! `cf-core` exposes the real trait this module should be deleted and replaced
//! by a re-export. See the crate-level `todos`.

use std::any::Any;
use std::collections::HashMap;

use crate::uast_iface::SharedParser;

/// Type-erased value flowing between analyzers, mirroring Go's `any`.
///
/// The Go pipeline threads `map[string]any` between providers; in Rust we use
/// `Box<dyn Any>` so heterogeneous outputs (changes, caches, ticks, ...) can be
/// carried through a single map type without a closed enum.
pub type AnyValue = Box<dyn Any + Send + Sync>;

/// Dependency / output map, the analogue of Go's `map[string]any`.
pub type ValueMap = HashMap<String, AnyValue>;

/// Configuration facts passed to [`Analyzer::configure`].
pub type Facts = HashMap<String, AnyValue>;

/// Error returned by analyzer operations.
///
/// Mirrors the Go convention of returning `error`. Variants are kept coarse on
/// purpose; the precise error taxonomy belongs to `cf-core`.
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

/// A pipeline provider, mirroring `framework.Analyzer`.
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
    fn configure(&mut self, _facts: &Facts) -> Result<(), AnalyzerError> {
        Ok(())
    }

    /// Provide the shared UAST parser. Default: no-op.
    fn configure_uast(&mut self, _parser: SharedParser) {}

    /// Process one commit's worth of dependencies into outputs.
    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError>;
}

/// Borrow a typed dependency out of the [`ValueMap`], returning a
/// [`AnalyzerError::Dependency`] when absent or of the wrong type.
///
/// This is the Rust analogue of the Go `deps["x"].(T)` type assertion.
pub fn dep<'a, T: 'static>(deps: &'a ValueMap, key: &str) -> Result<&'a T, AnalyzerError> {
    deps.get(key)
        .ok_or_else(|| AnalyzerError::Dependency(format!("{key} not present")))?
        .downcast_ref::<T>()
        .ok_or_else(|| AnalyzerError::Dependency(format!("{key} has unexpected type")))
}
