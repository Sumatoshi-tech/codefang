//! Stdio transport + JSON-RPC dispatch for the MCP server.
//!
//! Ports Go `Server.Run` (`StdioTransport`) and `Server.RunWithTransport`.
//!
//! ## Why a hand-rolled, sync loop rather than rmcp/tokio
//!
//! The Go server delegated transport + JSON-RPC framing to
//! `modelcontextprotocol/go-sdk`. The Rust counterpart would be the `rmcp` crate
//! over `tokio`, but those are not yet in the workspace dependency set and adding
//! an async runtime to a feature-gated, not-shipped crate is disproportionate at
//! this port stage (DESIGN rule 4: external crates are integrated centrally; see
//! the crate todos / `externalCrates`). To keep `cf-mcp` self-contained and
//! behavior-faithful in the meantime, this module implements the small subset of
//! the MCP wire protocol the three tools need — `initialize`, `tools/list`,
//! `tools/call` — over newline-delimited JSON-RPC on a synchronous
//! [`std::io::BufRead`] / [`std::io::Write`] pair. Swapping to rmcp later only
//! changes this file.
//!
//! The JSON-RPC envelope itself (ids, `jsonrpc`, `result`/`error`) is **not** a
//! machine-format report and is not under the byte-identity guarantee, so it is
//! built with `serde_json`; only the tool report payload flows through the
//! Go-compatible encoder in [`crate::gojson`] (via [`crate::result::ToolResult`]).
//! See `DESIGN.md` §2 — the lens covers report bytes, not the transport envelope.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::server::{descriptions, Server, SERVER_NAME, SERVER_VERSION};
use crate::tools::{
    AnalyzeInput, HistoryInput, UastParseInput, TOOL_NAME_ANALYZE, TOOL_NAME_HISTORY, TOOL_NAME_UAST,
};

/// Error type for the transport loop, mirroring Go's
/// `fmt.Errorf("mcp server: %w", err)`.
#[derive(Debug)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mcp server: {}", self.0)
    }
}

impl std::error::Error for TransportError {}

/// A cancellation signal, mirroring Go's `context.Context` cancellation.
///
/// Go's `Server.Run(ctx)` returns an error when the context is already canceled.
/// We model just that observable behavior with a simple predicate the caller
/// supplies; `run`/`run_with_transport` check it before serving and return the
/// wrapped error if already canceled (matching `TestServer_Run_CancelledContext`).
pub trait Cancellation {
    /// Whether cancellation has been requested.
    fn is_cancelled(&self) -> bool;
}

/// Never-cancelled signal.
pub struct NeverCancelled;
impl Cancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Already-cancelled signal (used by the cancelled-context test).
pub struct AlreadyCancelled;
impl Cancellation for AlreadyCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

impl Server {
    /// Starts the MCP server on stdio transport, blocking until EOF.
    ///
    /// Reproduces Go `Server.Run(ctx)` over `StdioTransport`. If `ctx` is already
    /// canceled it returns `mcp server: context canceled` without serving,
    /// matching `TestServer_Run_CancelledContext`. Errors are wrapped
    /// `mcp server: <err>`.
    ///
    /// # Errors
    /// Returns a [`TransportError`] on a canceled context or an I/O failure.
    pub fn run(&self, ctx: &dyn Cancellation) -> Result<(), TransportError> {
        if ctx.is_cancelled() {
            return Err(TransportError("context canceled".to_string()));
        }
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let reader = stdin.lock();
        let writer = stdout.lock();
        self.serve(reader, writer)
    }

    /// Starts the MCP server on the given synchronous byte transport.
    ///
    /// Reproduces Go `Server.RunWithTransport(ctx, transport)`. Used by the
    /// in-memory integration tests.
    ///
    /// # Errors
    /// Returns a [`TransportError`] on a canceled context or an I/O failure.
    pub fn run_with_transport<R, W>(
        &self,
        ctx: &dyn Cancellation,
        reader: R,
        writer: W,
    ) -> Result<(), TransportError>
    where
        R: BufRead,
        W: Write,
    {
        if ctx.is_cancelled() {
            return Err(TransportError("context canceled".to_string()));
        }
        self.serve(reader, writer)
    }

    /// Core newline-delimited JSON-RPC loop.
    fn serve<R, W>(&self, reader: R, mut writer: W) -> Result<(), TransportError>
    where
        R: BufRead,
        W: Write,
    {
        for line in reader.lines() {
            let line = line.map_err(|e| TransportError(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }

            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    write_line(&mut writer, &parse_error_response(&e.to_string()))?;
                    continue;
                }
            };

            // Notifications (no id) get no response.
            let id = request.get("id").cloned();
            let response = self.handle_rpc(&request);
            if let (Some(id), Some(mut response)) = (id, response) {
                if let Value::Object(ref mut map) = response {
                    map.insert("id".to_string(), id);
                }
                write_line(&mut writer, &response)?;
            }
        }
        Ok(())
    }

    /// Handles a single JSON-RPC request, returning the response value (or `None`
    /// for notifications that take no reply).
    fn handle_rpc(&self, request: &Value) -> Option<Value> {
        let method = request.get("method").and_then(Value::as_str)?;
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => Some(self.rpc_initialize()),
            "tools/list" => Some(self.rpc_tools_list()),
            "tools/call" => Some(self.rpc_tools_call(&params)),
            m if m.starts_with("notifications/") => None,
            other => Some(method_not_found_response(other)),
        }
    }

    fn rpc_initialize(&self) -> Value {
        json!({
            "jsonrpc": "2.0",
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
            }
        })
    }

    fn rpc_tools_list(&self) -> Value {
        let tools = json!([
            {
                "name": TOOL_NAME_ANALYZE,
                "description": descriptions::ANALYZE,
                "inputSchema": schema_for_analyze(),
            },
            {
                "name": TOOL_NAME_UAST,
                "description": descriptions::UAST,
                "inputSchema": schema_for_uast(),
            },
            {
                "name": TOOL_NAME_HISTORY,
                "description": descriptions::HISTORY,
                "inputSchema": schema_for_history(),
            },
        ]);
        json!({ "jsonrpc": "2.0", "result": { "tools": tools } })
    }

    fn rpc_tools_call(&self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        let (result, _output) = match name {
            TOOL_NAME_ANALYZE => {
                let input: AnalyzeInput = serde_json::from_value(args).unwrap_or_default();
                self.dispatch_analyze(&input)
            }
            TOOL_NAME_UAST => {
                let input: UastParseInput = serde_json::from_value(args).unwrap_or_default();
                self.dispatch_uast(&input)
            }
            TOOL_NAME_HISTORY => {
                let input: HistoryInput = serde_json::from_value(args).unwrap_or_default();
                self.dispatch_history(&input)
            }
            other => {
                return json!({
                    "jsonrpc": "2.0",
                    "error": { "code": -32602, "message": format!("unknown tool: {other}") }
                });
            }
        };

        let content: Vec<Value> = result
            .content
            .iter()
            .map(|text| json!({ "type": "text", "text": text }))
            .collect();

        json!({
            "jsonrpc": "2.0",
            "result": { "content": content, "isError": result.is_error }
        })
    }
}

fn schema_for_analyze() -> Value {
    json!({
        "type": "object",
        "properties": {
            "analyzers": { "type": "array", "items": { "type": "string" } },
            "code": { "type": "string" },
            "language": { "type": "string" }
        },
        "required": ["code", "language"]
    })
}

fn schema_for_uast() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": { "type": "string" },
            "language": { "type": "string" },
            "query": { "type": "string" }
        },
        "required": ["code", "language"]
    })
}

fn schema_for_history() -> Value {
    json!({
        "type": "object",
        "properties": {
            "analyzers": { "type": "array", "items": { "type": "string" } },
            "first_parent": { "type": "boolean" },
            "limit": { "type": "integer" },
            "repo_path": { "type": "string" },
            "since": { "type": "string" }
        },
        "required": ["repo_path"]
    })
}

fn parse_error_response(message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "error": { "code": -32700, "message": format!("parse error: {message}") }
    })
}

fn method_not_found_response(method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "error": { "code": -32601, "message": format!("method not found: {method}") }
    })
}

fn write_line<W: Write>(writer: &mut W, value: &Value) -> Result<(), TransportError> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| TransportError(e.to_string()))?;
    bytes.push(b'\n');
    writer.write_all(&bytes).map_err(|e| TransportError(e.to_string()))?;
    writer.flush().map_err(|e| TransportError(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ServerDeps;

    #[test]
    fn cancelled_context_returns_error() {
        // Mirrors TestServer_Run_CancelledContext.
        let srv = Server::new(ServerDeps::default());
        let input: &[u8] = b"";
        let mut output = Vec::new();
        let err = srv
            .run_with_transport(&AlreadyCancelled, input, &mut output)
            .unwrap_err();
        assert!(err.to_string().starts_with("mcp server: "));
    }

    #[test]
    fn tools_list_returns_three_tools_with_schemas() {
        // Mirrors TestMCPServer_InMemoryTransport_ToolsList.
        let srv = Server::new(ServerDeps::default());
        let resp = srv.rpc_tools_list();
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"codefang_analyze"));
        assert!(names.contains(&"codefang_history"));
        assert!(names.contains(&"uast_parse"));
        for t in tools {
            assert!(t.get("inputSchema").is_some(), "tool missing input schema");
        }
    }

    #[test]
    fn initialize_reports_server_info() {
        let srv = Server::new(ServerDeps::default());
        let resp = srv.rpc_initialize();
        assert_eq!(resp["result"]["serverInfo"]["name"], "codefang");
        assert_eq!(resp["result"]["serverInfo"]["version"], "1.0.0");
    }

    #[test]
    fn call_analyze_with_empty_code_is_error() {
        // Mirrors TestMCPServer_InMemoryTransport_CallAnalyze_Error.
        let srv = Server::new(ServerDeps::default());
        let params = json!({
            "name": "codefang_analyze",
            "arguments": { "code": "", "language": "go" }
        });
        let resp = srv.rpc_tools_call(&params);
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn serve_handles_initialize_then_eof() {
        // Drive the full loop over an in-memory transport with one request.
        let srv = Server::new(ServerDeps::default());
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n";
        let reader = std::io::BufReader::new(&input[..]);
        let mut output: Vec<u8> = Vec::new();
        srv.run_with_transport(&NeverCancelled, reader, &mut output)
            .unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("serverInfo"));
        assert!(text.contains("\"id\":1"));
    }

    #[test]
    fn serve_skips_notifications() {
        let srv = Server::new(ServerDeps::default());
        let input =
            b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n";
        let reader = std::io::BufReader::new(&input[..]);
        let mut output: Vec<u8> = Vec::new();
        srv.run_with_transport(&NeverCancelled, reader, &mut output)
            .unwrap();
        assert!(output.is_empty(), "notifications take no reply");
    }
}
