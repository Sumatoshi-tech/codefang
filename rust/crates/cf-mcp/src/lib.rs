//! MCP (Model Context Protocol) server exposing Codefang analysis as tools.
//!
//! This is the Rust port of the Go `internal/mcp` package. It implements an MCP
//! server over stdio transport that registers three tools an AI agent can
//! discover and invoke:
//!
//! - [`TOOL_NAME_ANALYZE`](tools::TOOL_NAME_ANALYZE) (`codefang_analyze`) —
//!   static code analysis of inline source (complexity, cohesion, halstead,
//!   comments, imports).
//! - [`TOOL_NAME_UAST`](tools::TOOL_NAME_UAST) (`uast_parse`) — parse inline
//!   source into a Universal Abstract Syntax Tree (UAST), optionally filtered by
//!   node type.
//! - [`TOOL_NAME_HISTORY`](tools::TOOL_NAME_HISTORY) (`codefang_history`) — Git
//!   repository history analysis (burndown, couples, devs, file-history,
//!   imports, sentiment, shotness, typos).
//!
//! # Not shipped by default
//!
//! The Go `mcp` command carries a `//go:build ignore` constraint and is *not*
//! wired into the `codefang` binary. We reproduce that exactly: everything below
//! lives behind the non-default Cargo feature **`mcp`**. With the feature off the
//! crate compiles to an empty shell, so the default workspace build never pulls
//! the MCP/async machinery in and `cf-commands` must opt in explicitly. See
//! `DESIGN.md` §1 and §3.
//!
//! # Byte-identity discipline
//!
//! Tool results are serialized through the Go-compatible encoder in [`gojson`]
//! (the same `GoValue`/`GoMap` model `cf-uast-node` uses; it will migrate to the
//! shared `cf-gojson` crate when that lands — DESIGN rule 5). The Go handlers
//! used `json.MarshalIndent(value, "", "  ")`, which indents with two spaces,
//! escapes HTML (`<`, `>`, `&`, `U+2028`, `U+2029`), and appends **no** trailing
//! newline; [`result::ToolResult::json`] reproduces that exactly. The report
//! payload never goes through `serde_json`. See `DESIGN.md` §2.3.
//!
//! # serde scope
//!
//! `serde` / `serde_json` appear in this crate **only** for decoding inbound
//! tool arguments and for the (non-binding) JSON-RPC transport envelope. They are
//! never used to encode the machine-format report bytes.

#![cfg_attr(not(feature = "mcp"), allow(unused))]

// ---------------------------------------------------------------------------
// Feature-off shell.
//
// When the `mcp` feature is disabled the crate exposes nothing of substance, so
// the default workspace build does not depend on serde/serde_json/transport.
// This mirrors the Go `//go:build ignore` on the mcp command.
// ---------------------------------------------------------------------------
#[cfg(not(feature = "mcp"))]
mod disabled {
    //! Placeholder present only when the `mcp` feature is off.
}

#[cfg(feature = "mcp")]
pub mod errors;
#[cfg(feature = "mcp")]
pub mod gojson;
#[cfg(feature = "mcp")]
pub mod providers;
#[cfg(feature = "mcp")]
pub mod result;
#[cfg(feature = "mcp")]
pub mod server;
#[cfg(feature = "mcp")]
pub mod tools;
#[cfg(feature = "mcp")]
pub mod tools_analyze;
#[cfg(feature = "mcp")]
pub mod tools_history;
#[cfg(feature = "mcp")]
pub mod tools_uast;

#[cfg(feature = "mcp")]
pub mod transport;

#[cfg(feature = "mcp")]
pub use errors::ToolError;
#[cfg(feature = "mcp")]
pub use providers::{
    HistoryAnalysisProvider, HistoryRunOptions, StaticAnalysisProvider, UastParser,
};
#[cfg(feature = "mcp")]
pub use result::{ToolOutput, ToolResult};
#[cfg(feature = "mcp")]
pub use server::{Server, ServerDeps};
#[cfg(feature = "mcp")]
pub use tools::{
    AnalyzeInput, HistoryInput, UastParseInput, MAX_CODE_INPUT_BYTES, TOOL_NAME_ANALYZE,
    TOOL_NAME_HISTORY, TOOL_NAME_UAST,
};
#[cfg(feature = "mcp")]
pub use tools_history::{all_history_keys, ALL_HISTORY_KEYS, DEFAULT_MCP_COMMIT_LIMIT};
