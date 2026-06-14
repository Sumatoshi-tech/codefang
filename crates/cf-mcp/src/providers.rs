//! Provider trait boundaries for the analysis backends.
//!
//! Rather than depend on the concrete parser / static-analysis / history
//! pipeline crates (whose public factory surfaces are not finalized), the
//! dependency is taken behind the minimal traits below (DESIGN rule 5);
//! `cf-commands` (or the concrete crates once stable) supplies implementations
//! and injects them via [`crate::server::ServerDeps`]. The handler *logic*
//! (validation, ordering, defaults, error wording) lives in the `tools_*`
//! modules and is fully unit-tested against fakes.

use crate::errors::ToolError;
use crate::gojson::JsonValue;
use cf_uast_node::Node;

/// Abstraction over the UAST parser. Used by both `codefang_analyze` and
/// `uast_parse`.
pub trait UastParser {
    /// Whether the parser supports the given (synthetic) filename.
    fn is_supported(&self, filename: &str) -> bool;

    /// Parses `code` for `filename`, returning the UAST root.
    ///
    /// # Errors
    ///
    /// On failure return a [`ToolError`] whose message becomes the wrapped
    /// `parse code: <err>`.
    fn parse(&self, filename: &str, code: &[u8]) -> Result<Node, ToolError>;
}

/// Abstraction over the static-analysis factory.
///
/// Given a parsed root node and the selected analyzer names, `run` returns the
/// result map as a [`JsonValue`] (a map-origin object, byte-sorted on encode —
/// report-format contract).
pub trait StaticAnalysisProvider {
    /// Runs the named analyzers over `root`, returning the combined result map.
    ///
    /// # Errors
    ///
    /// Returns a [`ToolError`] describing the first analyzer failure.
    fn run(&self, root: &Node, names: &[String]) -> Result<JsonValue, ToolError>;
}

/// Options passed to a [`HistoryAnalysisProvider`], derived from a validated
/// history tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRunOptions {
    /// Absolute repository path (already validated).
    pub repo_path: String,
    /// Selected analyzer keys (already validated against the known set).
    pub analyzers: Vec<String>,
    /// Effective commit limit (`<= 0` already normalized to the default).
    pub limit: i64,
    /// Follow only the first parent of merge commits.
    pub first_parent: bool,
    /// `since` filter string (e.g. `24h`, `2024-01-01`), empty if unset.
    pub since: String,
}

/// Abstraction over the history-analysis pipeline (repository load → runner /
/// coordinator → JSON-formatted history results).
///
/// The concrete implementation is supplied by `cf-commands` (or
/// `cf-analyze`/`cf-framework` once their public surfaces land) and lives on
/// [`crate::server::ServerDeps`].
pub trait HistoryAnalysisProvider {
    /// Runs the selected history analyzers over the repository and returns the
    /// merged JSON-shaped result tree.
    ///
    /// # Errors
    ///
    /// Returns a [`ToolError`] describing the pipeline failure.
    fn run(&self, opts: &HistoryRunOptions) -> Result<JsonValue, ToolError>;
}
