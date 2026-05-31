//! Provider trait boundaries for the analysis backends.
//!
//! The Go MCP handlers called directly into concrete packages: `uast.NewParser`,
//! `analyze.NewFactory(...).RunAnalyzers(...)`, and the full
//! `gitlib`+`framework`+8-analyzer history pipeline behind
//! `analyze.OutputHistoryResults(..., FormatJSON, ...)`.
//!
//! In the Rust workspace those crates are either not yet implemented
//! (`cf-gitlib`, `cf-framework`, `cf-observability` are scaffolds at port time)
//! or their public factory surface is not finalized (`cf-analyze`). Per rewrite
//! rule (5), the dependency on them is taken behind the minimal traits below;
//! `cf-commands` (or the concrete crates once stable) supplies implementations
//! and injects them via [`crate::server::ServerDeps`]. The handler *logic*
//! (validation, ordering, defaults, error wording) lives in the `tools_*`
//! modules and is fully ported and unit-tested against fakes.

use crate::errors::ToolError;
use crate::gojson::JsonValue;
use cf_uast_node::Node;

/// Abstraction over the UAST parser, mirroring `uast.NewParser()` +
/// `IsSupported` + `Parse`. Used by both `codefang_analyze` and `uast_parse`.
pub trait UastParser {
    /// Whether the parser supports the given (synthetic) filename.
    ///
    /// Mirrors `parser.IsSupported(filename)`.
    fn is_supported(&self, filename: &str) -> bool;

    /// Parses `code` for `filename`, returning the UAST root.
    ///
    /// Mirrors `parser.Parse(ctx, filename, code)`. On failure return a
    /// [`ToolError`] whose message becomes the wrapped `parse code: <err>`.
    fn parse(&self, filename: &str, code: &[u8]) -> Result<Node, ToolError>;
}

/// Abstraction over the static-analysis factory.
///
/// Minimal trait standing in for the Go `analyze.Factory`. `run` mirrors
/// `factory.RunAnalyzers(ctx, root, names)`: given a parsed root node and the
/// selected analyzer names, it returns the result map as a [`JsonValue`] (a
/// map-origin object, byte-sorted on encode to match Go `map[string]any`).
pub trait StaticAnalysisProvider {
    /// Runs the named analyzers over `root`, returning the combined result map.
    fn run(&self, root: &Node, names: &[String]) -> Result<JsonValue, ToolError>;
}

/// Options passed to a [`HistoryAnalysisProvider`], mirroring the inputs the Go
/// `executeHistory` derives before running the pipeline.
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

/// Abstraction over the history-analysis pipeline.
///
/// Minimal trait standing in for the Go `executeHistory` body
/// (`gitlib.LoadRepository` → `framework` runner/coordinator →
/// `analyze.OutputHistoryResults(FormatJSON)` → decode to `any`). The concrete
/// implementation is supplied by `cf-commands` (or `cf-analyze`/`cf-framework`
/// once their public surfaces land) and lives on [`crate::server::ServerDeps`].
pub trait HistoryAnalysisProvider {
    /// Runs the selected history analyzers over the repository and returns the
    /// merged JSON-shaped result tree.
    fn run(&self, opts: &HistoryRunOptions) -> Result<JsonValue, ToolError>;
}
