# MCP Server

Codefang includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server
that exposes analysis capabilities as tools for AI agents.

## Quick Start

```bash
codefang mcp
```

This starts an MCP server on stdio transport. AI agents (Claude Code, Cursor,
Windsurf, etc.) connect via stdin/stdout using JSON-RPC 2.0.

### Debug Mode

```bash
codefang mcp --debug
```

Enables structured debug logging to stderr for request/response tracing.

## Tools

The MCP server exposes three tools:

### codefang_analyze

Run static analysis on inline source code.

**Parameters:**

| Name       | Type     | Required | Description                                                    |
|------------|----------|----------|----------------------------------------------------------------|
| code       | string   | yes      | Source code to analyze                                         |
| language   | string   | yes      | Programming language (e.g. `go`, `python`, `javascript`)       |
| analyzers  | string[] | no       | Analyzer names to run (default: all). Options: `complexity`, `comments`, `halstead`, `cohesion`, `imports` |

**Example request:**

```json
{
  "code": "package main\nfunc main() {}\n",
  "language": "go",
  "analyzers": ["complexity"]
}
```

### uast_parse

Parse source code into a Universal Abstract Syntax Tree (UAST).

**Parameters:**

| Name     | Type   | Required | Description                                              |
|----------|--------|----------|----------------------------------------------------------|
| code     | string | yes      | Source code to parse                                     |
| language | string | yes      | Programming language (e.g. `go`, `python`, `javascript`) |
| query    | string | no       | Node type filter (e.g. `Function`)                       |

**Example request:**

```json
{
  "code": "package main\nfunc hello() {}\n",
  "language": "go",
  "query": "Function"
}
```

### codefang_history

Analyze Git repository history for trends and patterns.

**Parameters:**

| Name         | Type     | Required | Description                                                         |
|--------------|----------|----------|---------------------------------------------------------------------|
| repo_path    | string   | yes      | Absolute path to a Git repository                                   |
| analyzers    | string[] | no       | History analyzers to run (default: all). Options: `burndown`, `couples`, `devs`, `file-history`, `imports`, `sentiment`, `shotness`, `typos` |
| limit        | int      | no       | Maximum number of commits to analyze (default: 1000)                |
| since        | string   | no       | Only analyze commits after this time (e.g. `24h` or `2024-01-01`)  |
| first_parent | bool     | no       | Follow only the first parent of merge commits                       |

**Example request:**

```json
{
  "repo_path": "/home/user/project",
  "analyzers": ["couples", "burndown"],
  "limit": 500,
  "first_parent": true
}
```

## Agent Configuration

### Claude Code

Add to your MCP settings (`.claude/settings.json` or project-level):

```json
{
  "mcpServers": {
    "codefang": {
      "command": "codefang",
      "args": ["mcp"]
    }
  }
}
```

### Generic MCP Client

Any MCP-compatible client can connect via stdio transport:

```bash
codefang mcp
```

The server advertises its tools via the standard `tools/list` method.
All tool arguments are validated against auto-generated JSON schemas.

## Error Handling

Tool errors are returned as MCP `CallToolResult` with `isError: true` and
a human-readable error message. Common errors:

- Empty `code` or `language` parameter
- Code input exceeding 1 MB size limit
- Unsupported programming language
- Invalid or non-existent repository path
- Unknown analyzer name

## Architecture

The MCP server lives in `pkg/mcp/`:

- `server.go` — Server wrapper with tool registration and transport handling
- `tools.go` — Shared types, constants, sentinel errors, and helpers
- `tools_analyze.go` — `codefang_analyze` handler using `analyze.Factory`
- `tools_uast.go` — `uast_parse` handler using `uast.NewParser()`
- `tools_history.go` — `codefang_history` handler with full plumbing pipeline

The CLI command is in `cmd/codefang/commands/mcp.go`.

The server uses the official Go MCP SDK (`github.com/modelcontextprotocol/go-sdk`)
with typed input schemas via `AddTool[In, Out]()` generics.
