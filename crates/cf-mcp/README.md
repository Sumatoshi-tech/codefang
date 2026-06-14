# cf-mcp

An MCP (Model Context Protocol) server that exposes codefang's analysis as
tools an AI agent can discover and invoke.

## What and why

The server registers three tools over stdio transport:

- `codefang_analyze` — static code analysis of inline source (complexity,
  cohesion, halstead, comments, imports).
- `uast_parse` — parse inline source into a Universal Abstract Syntax Tree
  (UAST), optionally filtered by node type.
- `codefang_history` — Git repository history analysis (burndown, couples,
  devs, file-history, imports, sentiment, shotness, typos).

Tool results serialize through the report-compatible encoder in `gojson` (never
`serde_json`), reproducing the reference tool-output profile byte for byte:
two-space indent, HTML escapes on, no trailing newline.

## Not shipped by default

Everything lives behind the non-default Cargo feature **`mcp`**. With the
feature off the crate compiles to an empty shell, so the default workspace build
never pulls in the MCP/async machinery. `cf-commands` opts in explicitly.

## Usage

```rust
use cf_mcp::{ToolError, ToolResult};
use cf_mcp::tools::validate_code_input;
use cf_mcp::gojson::JsonValue;

// Inputs are validated before analysis runs.
assert!(validate_code_input("package main", "go").is_ok());
assert_eq!(validate_code_input("", "go"), Err(ToolError::EmptyCode));

// Tool results serialize with the frozen two-space-indent profile.
let value = JsonValue::sorted_object(vec![("score".to_string(), JsonValue::Int(7))]);
assert_eq!(ToolResult::json(&value).first_text(), "{\n  \"score\": 7\n}");
```

This is the crate-level doctest in `src/lib.rs`, run by
`cargo test --doc -p cf-mcp --features mcp`.

## Build

```sh
cargo build -p cf-mcp --features mcp
cargo test -p cf-mcp --features mcp
```

## Deeper docs

See the crate rustdoc (`cargo doc -p cf-mcp --features mcp --open`) — the tool
schemas (`tools`), error contract (`errors`), result encoding (`result`), and
the server/transport layers (`server`, `transport`).
