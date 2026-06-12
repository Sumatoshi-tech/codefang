//! MCP server wiring.
//!
//! The [`ServerDeps`] injectable dependencies, the [`Server`] wrapper, tool
//! registration, sorted tool-name listing, the stdio/transport entry points,
//! and the metrics / tracing handler decorators.
//!
//! ## Observability boundary
//!
//! Rather than depend on `cf-observability` directly, the metrics/tracing
//! hooks are taken behind the small [`Metrics`] / [`Tracer`] traits defined
//! here (DESIGN rule 5). `cf-commands` wires the real recorder/tracer; with
//! `None` the decorators are no-ops.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::errors::ToolError;
use crate::providers::{HistoryAnalysisProvider, StaticAnalysisProvider, UastParser};
use crate::result::{ToolOutput, ToolResult};
use crate::tools::{
    AnalyzeInput, HistoryInput, UastParseInput, TOOL_NAME_ANALYZE, TOOL_NAME_HISTORY, TOOL_NAME_UAST,
};
use crate::tools_analyze::handle_analyze;
use crate::tools_history::handle_history;
use crate::tools_uast::handle_uast_parse;

/// MCP server implementation name.
pub const SERVER_NAME: &str = "codefang";
/// MCP server implementation version.
pub const SERVER_VERSION: &str = "1.0.0";
/// Expected number of registered tools.
pub const TOOL_COUNT: usize = 3;

/// Prefix for MCP tool span names and metric keys.
pub const MCP_SPAN_PREFIX: &str = "mcp.";
/// Metadata key for `trace_id` in MCP tool responses.
pub const TRACE_ID_META_KEY: &str = "trace_id";

/// Tool description constants (tool-contract wording; agents see these in
/// `tools/list`).
pub mod descriptions {
    /// Description of the `codefang_analyze` tool.
    pub const ANALYZE: &str = "Analyze source code for quality metrics \
(complexity, cohesion, halstead, comments, imports). \
Accepts inline code and a language identifier.";

    /// Description of the `uast_parse` tool.
    pub const UAST: &str = "Parse source code into a Universal Abstract Syntax Tree (UAST). \
Returns a JSON representation of the AST structure.";

    /// Description of the `codefang_history` tool.
    pub const HISTORY: &str = "Analyze Git repository history for trends \
(burndown, couples, devs, file-history, imports, sentiment, shotness, typos). \
Accepts a repository path and optional parameters.";
}

/// RED-metrics recorder hook — the subset of `cf-observability`'s RED metrics
/// the per-tool decorator needs.
pub trait Metrics: Send + Sync {
    /// Increments the in-flight gauge for `key`, returning a guard whose drop
    /// decrements it.
    fn track_inflight(&self, key: &str) -> Box<dyn InflightGuard>;

    /// Records a completed request with `status` (`"ok"`/`"error"`) and its
    /// duration.
    fn record_request(&self, key: &str, status: &str, dur: Duration);
}

/// Guard returned by [`Metrics::track_inflight`]; dropping it decrements the
/// gauge.
pub trait InflightGuard {}

/// Tracing hook — the subset of an OTel tracer the per-tool decorator needs.
pub trait Tracer: Send + Sync {
    /// Starts a server span named `name` carrying an `mcp.tool = <tool>`
    /// attribute, returning a span guard.
    fn start_span(&self, name: &str, tool: &str) -> Box<dyn Span>;
}

/// A started span. Dropping it ends the span.
pub trait Span {
    /// Whether this span is sampled.
    fn is_sampled(&self) -> bool;
    /// The trace id rendered as a hex string.
    fn trace_id(&self) -> String;
}

/// Injectable dependencies for the MCP server: the observability hooks plus
/// the analysis providers. All fields are optional; a missing provider means
/// the corresponding tool returns an error result.
#[derive(Default)]
pub struct ServerDeps {
    /// Optional RED metrics recorder. `None` disables per-tool metrics.
    pub metrics: Option<Box<dyn Metrics>>,
    /// Optional tracer for per-tool-call spans. `None` disables tracing.
    pub tracer: Option<Box<dyn Tracer>>,
    /// UAST parser used by `codefang_analyze` and `uast_parse`.
    pub parser: Option<Box<dyn UastParser + Send + Sync>>,
    /// Static-analysis provider used by `codefang_analyze`.
    pub static_provider: Option<Box<dyn StaticAnalysisProvider + Send + Sync>>,
    /// History-analysis provider used by `codefang_history`.
    pub history_provider: Option<Box<dyn HistoryAnalysisProvider + Send + Sync>>,
}

impl std::fmt::Debug for ServerDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerDeps")
            .field("metrics", &self.metrics.is_some())
            .field("tracer", &self.tracer.is_some())
            .field("parser", &self.parser.is_some())
            .field("static_provider", &self.static_provider.is_some())
            .field("history_provider", &self.history_provider.is_some())
            .finish()
    }
}

/// MCP server wrapping the tool registry and dependencies.
///
/// Holds the registered tool names (under a lock) plus the deps. The wire
/// transport is created lazily in [`Server::run`] /
/// [`Server::run_with_transport`] (see [`crate::transport`]).
pub struct Server {
    pub(crate) deps: ServerDeps,
    tools: Mutex<Vec<String>>,
}

impl Server {
    /// Creates a new server with all Codefang tools registered
    /// (analyze/uast/history, in that order).
    #[must_use]
    pub fn new(deps: ServerDeps) -> Self {
        let srv = Self {
            deps,
            tools: Mutex::new(Vec::with_capacity(TOOL_COUNT)),
        };
        srv.register_tools();
        srv
    }

    /// Registers all tools, in their fixed order: analyze, uast, history.
    fn register_tools(&self) {
        self.track_tool(TOOL_NAME_ANALYZE);
        self.track_tool(TOOL_NAME_UAST);
        self.track_tool(TOOL_NAME_HISTORY);
    }

    fn track_tool(&self, name: &str) {
        self.tools
            .lock()
            .expect("tools lock poisoned")
            .push(name.to_string());
    }

    /// Returns the sorted names of all registered tools:
    /// `codefang_analyze, codefang_history, uast_parse`.
    ///
    /// # Panics
    ///
    /// Panics if the internal tools lock is poisoned.
    #[must_use]
    pub fn list_tool_names(&self) -> Vec<String> {
        let mut names = self.tools.lock().expect("tools lock poisoned").clone();
        names.sort();
        names
    }

    /// Dispatches a `codefang_analyze` call through the metrics/tracing
    /// decorators.
    ///
    /// Input validation always runs first; only when validation passes and a
    /// backend is missing does this return the
    /// `run analyzers: no static analysis provider configured` error. This
    /// preserves the contract ordering of observable errors.
    #[must_use]
    pub fn dispatch_analyze(&self, input: &AnalyzeInput) -> (ToolResult, ToolOutput) {
        self.decorate(TOOL_NAME_ANALYZE, || {
            if let Err(err) = crate::tools::validate_code_input(&input.code, &input.language) {
                return (ToolResult::error(&err), ToolOutput::empty());
            }
            match (&self.deps.parser, &self.deps.static_provider) {
                (Some(parser), Some(provider)) => {
                    handle_analyze(parser.as_ref(), provider.as_ref(), input)
                }
                _ => (
                    ToolResult::error(&ToolError::wrap(
                        "run analyzers",
                        "no static analysis provider configured",
                    )),
                    ToolOutput::empty(),
                ),
            }
        })
    }

    /// Dispatches a `uast_parse` call.
    ///
    /// Validates the input first, then requires a parser.
    #[must_use]
    pub fn dispatch_uast(&self, input: &UastParseInput) -> (ToolResult, ToolOutput) {
        self.decorate(TOOL_NAME_UAST, || {
            if let Err(err) = crate::tools::validate_code_input(&input.code, &input.language) {
                return (ToolResult::error(&err), ToolOutput::empty());
            }
            match &self.deps.parser {
                Some(parser) => handle_uast_parse(parser.as_ref(), input),
                None => (
                    ToolResult::error(&ToolError::wrap("create parser", "no parser configured")),
                    ToolOutput::empty(),
                ),
            }
        })
    }

    /// Dispatches a `codefang_history` call.
    ///
    /// Validates the repository path first, then requires a history provider.
    #[must_use]
    pub fn dispatch_history(&self, input: &HistoryInput) -> (ToolResult, ToolOutput) {
        self.decorate(TOOL_NAME_HISTORY, || {
            if let Err(err) = crate::tools_history::validate_history_input(input) {
                return (ToolResult::error(&err), ToolOutput::empty());
            }
            match &self.deps.history_provider {
                Some(provider) => handle_history(provider.as_ref(), input),
                None => (
                    ToolResult::error(&ToolError::wrap(
                        "load repository",
                        "no history analysis provider configured",
                    )),
                    ToolOutput::empty(),
                ),
            }
        })
    }

    /// Applies the metrics + tracing decoration around a handler call:
    /// - metrics: tracks inflight and records a request with status `ok`/`error`
    ///   keyed by `mcp.<tool>`;
    /// - tracing: opens a server span `mcp.<tool>`; when sampled, appends a
    ///   `trace_id=<id>` text item to the result.
    fn decorate<F>(&self, tool: &str, handler: F) -> (ToolResult, ToolOutput)
    where
        F: FnOnce() -> (ToolResult, ToolOutput),
    {
        let metric_key = format!("{MCP_SPAN_PREFIX}{tool}");

        // --- metrics: inflight + timing (withMetrics) ---
        let _inflight = self
            .deps
            .metrics
            .as_ref()
            .map(|m| m.track_inflight(&metric_key));
        let start = Instant::now();

        // --- tracing: open span (withTracing) ---
        let span = self
            .deps
            .tracer
            .as_ref()
            .map(|t| t.start_span(&format!("{MCP_SPAN_PREFIX}{tool}"), tool));

        let (mut result, output) = handler();

        // Append trace_id when the span is sampled.
        if let Some(span) = &span {
            if span.is_sampled() {
                result.push_text(format!("{TRACE_ID_META_KEY}={}", span.trace_id()));
            }
        }

        // --- metrics: record request status ---
        if let Some(m) = self.deps.metrics.as_ref() {
            let status = if result.is_error { "error" } else { "ok" };
            m.record_request(&metric_key, status, start.elapsed());
        }
        // `_inflight` and `span` drop here, decrementing the gauge and ending
        // the span.

        (result, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn new_server_registers_three_tools() {
        let srv = Server::new(ServerDeps::default());
        let tools = srv.list_tool_names();
        assert_eq!(tools.len(), 3);
        assert!(tools.contains(&"codefang_analyze".to_string()));
        assert!(tools.contains(&"codefang_history".to_string()));
        assert!(tools.contains(&"uast_parse".to_string()));
    }

    #[test]
    fn list_tool_names_is_sorted() {
        let srv = Server::new(ServerDeps::default());
        assert_eq!(
            srv.list_tool_names(),
            vec!["codefang_analyze", "codefang_history", "uast_parse"]
        );
    }

    #[test]
    fn server_constants_match_contract() {
        assert_eq!(SERVER_NAME, "codefang");
        assert_eq!(SERVER_VERSION, "1.0.0");
        assert_eq!(TOOL_COUNT, 3);
        assert_eq!(MCP_SPAN_PREFIX, "mcp.");
        assert_eq!(TRACE_ID_META_KEY, "trace_id");
    }

    #[test]
    fn descriptions_match_contract() {
        assert!(descriptions::ANALYZE.starts_with("Analyze source code for quality metrics"));
        assert!(descriptions::UAST.contains("Universal Abstract Syntax Tree"));
        assert!(descriptions::HISTORY.contains("Analyze Git repository history for trends"));
    }

    #[test]
    fn dispatch_history_without_provider_validates_first() {
        let srv = Server::new(ServerDeps::default());
        let (res, _) = srv.dispatch_history(&HistoryInput::default());
        assert!(res.is_error);
        assert!(res.first_text().contains("repo_path parameter is required"));
    }

    #[test]
    fn dispatch_uast_without_parser_is_error() {
        let srv = Server::new(ServerDeps::default());
        let input = UastParseInput {
            code: "package main".into(),
            language: "go".into(),
            query: String::new(),
        };
        let (res, _) = srv.dispatch_uast(&input);
        assert!(res.is_error);
    }

    // --- decorator behavior (withMetrics / withTracing) ---

    struct CountingMetrics {
        inflight: Arc<AtomicUsize>,
        recorded: Arc<Mutex<Vec<(String, String)>>>,
    }
    struct Guard(Arc<AtomicUsize>);
    impl InflightGuard for Guard {}
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl Metrics for CountingMetrics {
        fn track_inflight(&self, _key: &str) -> Box<dyn InflightGuard> {
            self.inflight.fetch_add(1, Ordering::SeqCst);
            Box::new(Guard(self.inflight.clone()))
        }
        fn record_request(&self, key: &str, status: &str, _dur: Duration) {
            self.recorded
                .lock()
                .unwrap()
                .push((key.to_string(), status.to_string()));
        }
    }

    struct SampledTracer {
        started: Arc<AtomicBool>,
    }
    struct SampledSpan;
    impl Span for SampledSpan {
        fn is_sampled(&self) -> bool {
            true
        }
        fn trace_id(&self) -> String {
            "deadbeef".to_string()
        }
    }
    impl Tracer for SampledTracer {
        fn start_span(&self, _name: &str, _tool: &str) -> Box<dyn Span> {
            self.started.store(true, Ordering::SeqCst);
            Box::new(SampledSpan)
        }
    }

    #[test]
    fn metrics_record_status_and_decrement_inflight() {
        let inflight = Arc::new(AtomicUsize::new(0));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let deps = ServerDeps {
            metrics: Some(Box::new(CountingMetrics {
                inflight: inflight.clone(),
                recorded: recorded.clone(),
            })),
            ..Default::default()
        };
        let srv = Server::new(deps);
        // history with empty repo path → error status, key "mcp.codefang_history".
        let _ = srv.dispatch_history(&HistoryInput::default());
        assert_eq!(inflight.load(Ordering::SeqCst), 0, "inflight decremented");
        let rec = recorded.lock().unwrap();
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].0, "mcp.codefang_history");
        assert_eq!(rec[0].1, "error");
    }

    #[test]
    fn tracing_appends_trace_id_when_sampled() {
        let started = Arc::new(AtomicBool::new(false));
        let deps = ServerDeps {
            tracer: Some(Box::new(SampledTracer {
                started: started.clone(),
            })),
            ..Default::default()
        };
        let srv = Server::new(deps);
        let (res, _) = srv.dispatch_history(&HistoryInput::default());
        assert!(started.load(Ordering::SeqCst), "span started");
        assert_eq!(res.content.last().unwrap(), "trace_id=deadbeef");
    }
}
