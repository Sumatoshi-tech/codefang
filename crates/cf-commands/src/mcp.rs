//! `codefang mcp`: the Model Context Protocol server over stdio.
//!
//! The server, its JSON-RPC transport and the tool contracts live in
//! `cf-mcp`; this module supplies the three analysis backends, each the
//! CLI's own pipeline, so a tool returns exactly the report `codefang run`
//! renders for the same input:
//!
//! * `uast_parse` and `codefang_analyze` parse with the embedded UAST parser;
//! * `codefang_analyze` writes the snippet to a temporary directory and
//!   renders `run -a static/<name> --format json` there, per analyzer;
//! * `codefang_history` renders `run -a history/<names> --format json` with
//!   the tool's limit, `--first-parent` and `--since`, then forgets the
//!   process-wide UAST results, so a long-running server holds nothing
//!   between calls.

use cf_mcp::errors::ToolError;
use cf_mcp::gojson::JsonValue;
use cf_mcp::providers::{
    HistoryAnalysisProvider, HistoryRunOptions, Snippet, StaticAnalysisProvider, UastParser,
};
use cf_mcp::server::{Server, ServerDeps};
use cf_mcp::transport::NeverCancelled;
use cf_uast_node::Node;

/// The `mcp` subcommand: no flags.
#[must_use]
pub fn build_mcp_command() -> clap::Command {
    clap::Command::new("mcp").about(
        "Serve the codefang analysis tools (codefang_analyze, uast_parse, codefang_history) \
to an MCP client over stdio",
    )
}

/// Runs the server until the client closes stdin; returns the exit code.
#[must_use]
pub fn mcp_subcommand() -> i32 {
    let server = Server::new(ServerDeps {
        parser: Some(Box::new(CodefangParser)),
        static_provider: Some(Box::new(CodefangStatic)),
        history_provider: Some(Box::new(CodefangHistory)),
        ..ServerDeps::default()
    });
    match server.run(&NeverCancelled) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: mcp server: {e}");
            1
        }
    }
}

/// The embedded UAST parser (one per thread, as in the history walks).
struct CodefangParser;

impl UastParser for CodefangParser {
    /// `code` plus the primary extension of the Linguist language the token
    /// names (`python` → `code.py`); an unknown token keeps `code.<token>`.
    fn filename_for(&self, language: &str) -> String {
        cf_langpath::primary_extension(language).map_or_else(
            || cf_mcp::tools::synthetic_filename(language),
            |ext| format!("code{ext}"),
        )
    }

    fn is_supported(&self, filename: &str) -> bool {
        crate::handlers::history::with_uast_parser(|parser| parser.is_supported(filename))
    }

    fn parse(&self, filename: &str, code: &[u8]) -> Result<Node, ToolError> {
        crate::handlers::history::with_uast_parser(|parser| parser.parse(filename, code))
            .map_err(|e| ToolError::wrap("parse", e.to_string()))
    }
}

/// The CLI's static analyzers over the snippet.
struct CodefangStatic;

impl StaticAnalysisProvider for CodefangStatic {
    fn run(&self, snippet: &Snippet<'_>, names: &[String]) -> Result<JsonValue, ToolError> {
        let dir =
            tempfile::tempdir().map_err(|e| ToolError::wrap("create a work dir", e.to_string()))?;
        std::fs::write(dir.path().join(snippet.filename), snippet.code)
            .map_err(|e| ToolError::wrap("write the code", e.to_string()))?;
        let path = dir
            .path()
            .to_str()
            .ok_or_else(|| ToolError::wrap("work dir", "not UTF-8"))?;
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let id = format!("static/{name}");
            let report = render_json(&["run", "-a", &id, "--format", "json", path])?;
            entries.push((name.clone(), report));
        }
        Ok(JsonValue::sorted_object(entries))
    }
}

/// The CLI's history analyzers over the repository.
struct CodefangHistory;

impl HistoryAnalysisProvider for CodefangHistory {
    fn run(&self, opts: &HistoryRunOptions) -> Result<JsonValue, ToolError> {
        let ids = opts
            .analyzers
            .iter()
            .map(|key| format!("history/{key}"))
            .collect::<Vec<_>>()
            .join(",");
        let limit = opts.limit.to_string();
        let mut argv = vec!["run", "-a", &ids, "--format", "json", "--limit", &limit];
        if opts.first_parent {
            argv.push("--first-parent");
        }
        if !opts.since.is_empty() {
            argv.extend(["--since", &opts.since]);
        }
        argv.push(&opts.repo_path);
        let document = render_json(&argv);
        crate::handlers::uast_walk::forget_results();
        let document = document?;
        match opts.analyzers.as_slice() {
            [only] => Ok(JsonValue::sorted_object(vec![(only.clone(), document)])),
            _ => per_analyzer_reports(document),
        }
    }
}

/// Renders a `run` command line and returns its JSON report.
fn render_json(argv: &[&str]) -> Result<JsonValue, ToolError> {
    let bytes = crate::render_run(argv).map_err(|e| ToolError::wrap("run analyzers", e))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| ToolError::wrap("read the report", e.to_string()))?;
    Ok(JsonValue::from(&value))
}

/// The `{analyzer key: report}` map of a multi-analyzer document
/// (`{version, metadata, analyzers: [{id, mode, report}]}`).
fn per_analyzer_reports(document: JsonValue) -> Result<JsonValue, ToolError> {
    let missing = || ToolError::wrap("read the report", "no analyzers array");
    let JsonValue::Object { entries, .. } = document else {
        return Err(missing());
    };
    let analyzers = entries
        .into_iter()
        .find_map(|(key, value)| (key == "analyzers").then_some(value))
        .ok_or_else(missing)?;
    let JsonValue::Array(items) = analyzers else {
        return Err(missing());
    };
    let mut reports = Vec::with_capacity(items.len());
    for item in items {
        let JsonValue::Object { entries, .. } = item else {
            return Err(missing());
        };
        let mut id = None;
        let mut report = None;
        for (key, value) in entries {
            match (key.as_str(), value) {
                ("id", JsonValue::Str(s)) => id = Some(s),
                ("report", value) => report = Some(value),
                _ => {}
            }
        }
        let (Some(id), Some(report)) = (id, report) else {
            return Err(missing());
        };
        let key = id.strip_prefix("history/").unwrap_or(&id).to_owned();
        reports.push((key, report));
    }
    Ok(JsonValue::sorted_object(reports))
}
