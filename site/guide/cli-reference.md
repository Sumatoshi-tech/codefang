# CLI Reference

Codefang ships two binaries: **`codefang`**, the analyzer engine, and **`uast`**, the
Universal Abstract Syntax Tree parser. This page is the exhaustive reference for
every command, sub-command, and flag.

---

## `codefang`

The main analysis binary. It runs static and history analyzers against source
code and Git repositories.

### `codefang run`

The primary entry point. Runs selected static and/or history analyzers on a
codebase or repository.

```bash
codefang run [path] [flags]
```

If `path` is omitted the current directory (or the value of `--path`) is used.

#### Analyzer Selection

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--analyzers` | `-a` | `[]string` | `nil` | Analyzer IDs or glob patterns. Comma-separated. |

Analyzer IDs follow a `<category>/<name>` convention. Globs are supported:

```bash
# Single analyzer
codefang run -a static/complexity .

# All static analyzers
codefang run -a 'static/*' .

# All history analyzers
codefang run -a 'history/*' .

# Multiple specific analyzers
codefang run -a static/complexity,history/burndown,history/devs .

# Everything
codefang run -a '*' .
```

??? info "Available Analyzer IDs"

    **Static analyzers:**
    `static/complexity`, `static/comments`, `static/halstead`,
    `static/cohesion`, `static/imports`

    **History analyzers:**
    `history/anomaly`, `history/burndown`, `history/couples`,
    `history/devs`, `history/file-history`, `history/imports`,
    `history/quality`, `history/sentiment`, `history/shotness`,
    `history/typos`

#### Output Flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--format` | | `string` | `json` | Output format: `json`, `text`, `compact`, `yaml`, `plot`, `bin`, `timeseries` |
| `--verbose` | `-v` | `bool` | `false` | Show full static report details |
| `--silent` | | `bool` | `false` | Suppress progress output on stderr |
| `--no-color` | | `bool` | `false` | Disable colored static output |

```bash
# Human-readable table
codefang run -a 'static/*' --format text .

# Interactive HTML charts
codefang run -a 'history/*' --format plot .

# Unified time-series JSON
codefang run -a 'history/devs,history/sentiment' --format timeseries .
```

#### Path & Input Flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--path` | `-p` | `string` | `.` | Folder or repository path to analyze |
| `--input` | | `string` | `""` | Input report path for cross-format conversion |
| `--input-format` | | `string` | `auto` | Input format: `auto`, `json`, `bin` |

!!! tip "Format Conversion"

    Use `--input` to convert a previously generated report to a different
    format without re-running analysis:

    ```bash
    codefang run -a 'history/*' --format bin . > report.bin
    codefang run -a 'history/*' --input report.bin --format plot
    ```

#### Git History Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--limit` | `int` | `0` | Maximum commits to analyze (`0` = no limit) |
| `--since` | `string` | `""` | Only analyze commits after this time |
| `--first-parent` | `bool` | `false` | Follow only first parent of merge commits |
| `--head` | `bool` | `false` | Analyze only HEAD commit |

The `--since` flag accepts multiple formats:

- **Go duration**: `24h`, `168h`, `720h`
- **Date**: `2025-01-01`
- **RFC 3339**: `2025-01-01T00:00:00Z`

```bash
# Last 7 days only
codefang run -a history/devs --since 168h .

# Since a specific date
codefang run -a history/burndown --since 2025-01-01 .

# Only the latest 500 commits
codefang run -a history/couples --limit 500 .
```

!!! note "Burndown and `--first-parent`"

    The burndown analyzer automatically enables `--first-parent` when selected.
    This is required for correct line-tracking across merge commits.

#### Pipeline Tuning Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--workers` | `int` | `0` | Parallel workers (`0` = CPU count via `GOMAXPROCS`) |
| `--buffer-size` | `int` | `0` | Internal pipeline channel size (`0` = `workers * 2`) |
| `--commit-batch-size` | `int` | `0` | Commits per processing batch (`0` = default 100) |
| `--blob-cache-size` | `string` | `""` | Max blob cache size (e.g. `256MB`, `1GB`; empty = 1 GB) |
| `--diff-cache-size` | `int` | `0` | Max diff cache entries (`0` = default 10000) |
| `--blob-arena-size` | `string` | `""` | Memory arena for blob loading (e.g. `4MB`; empty = 4 MB) |
| `--memory-budget` | `string` | `""` | Memory budget for auto-tuning (e.g. `512MB`, `2GB`) |

```bash
# Large repository with constrained memory
codefang run -a 'history/*' --workers 4 --memory-budget 2GB .

# High-throughput with large caches
codefang run -a 'history/*' --blob-cache-size 2GB --diff-cache-size 50000 .
```

#### GC Tuning Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--gogc` | `int` | `0` | GC target percentage (`0` = Go default 100) |
| `--ballast-size` | `string` | `"0"` | GC ballast allocation (`0` = disabled) |

#### Checkpoint & Resume Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--checkpoint` | `bool` | `true` | Enable checkpointing for crash recovery |
| `--checkpoint-dir` | `string` | `""` | Checkpoint directory (default: `~/.codefang/checkpoints`) |
| `--resume` | `bool` | `true` | Resume from checkpoint if available |
| `--clear-checkpoint` | `bool` | `false` | Clear existing checkpoint before run |

```bash
# Disable checkpointing entirely
codefang run -a history/burndown --checkpoint=false .

# Resume a previously interrupted run
codefang run -a 'history/*' --resume .

# Start fresh, clearing old checkpoint data
codefang run -a 'history/*' --clear-checkpoint .
```

#### Profiling & Debug Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--cpuprofile` | `string` | `""` | Write CPU profile to file |
| `--heapprofile` | `string` | `""` | Write heap profile to file |
| `--debug-trace` | `bool` | `false` | Enable 100% OpenTelemetry trace sampling |

```bash
# CPU profile a large run
codefang run -a 'history/*' --cpuprofile cpu.prof .

# Full debug tracing
codefang run -a 'history/*' --debug-trace .
```

---

### `codefang mcp`

Start a Model Context Protocol (MCP) server on stdio transport. This exposes
Codefang analysis capabilities as tools that AI agents can discover and invoke.

```bash
codefang mcp [flags]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--debug` | `bool` | `false` | Enable debug logging to stderr |

The MCP server exposes the following tools:

| Tool | Description |
|------|-------------|
| `codefang_analyze` | Static code analysis (complexity, cohesion, halstead, comments, imports) |
| `codefang_history` | Git history analysis (burndown, couples, devs, sentiment, etc.) |
| `uast_parse` | Parse source code into Universal AST |

```bash
# Start MCP server for agent integration
codefang mcp

# With debug logging
codefang mcp --debug
```

---

## `uast`

The UAST binary parses source files into a Universal Abstract Syntax Tree and
provides querying, diffing, and exploration utilities.

### `uast parse`

Parse one or more source files into UAST format.

```bash
uast parse [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--language` | `-l` | `string` | `""` | Force language detection override |
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `json` | Output format: `json`, `compact`, `tree` |
| `--progress` | `-p` | `bool` | `false` | Show progress for multiple files |
| `--all` | | `bool` | `false` | Parse all source files in the codebase recursively |

```bash
# Parse a single file
uast parse main.go

# Parse all Go files with progress
uast parse -p *.go

# Force language detection
uast parse -l go main.c

# Read from stdin
cat main.go | uast parse -

# Save to file
uast parse -o output.json main.go

# Parse entire codebase
uast parse --all
```

---

### `uast query`

Query UAST nodes using the functional DSL.

```bash
uast query <expression> [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--input` | `-i` | `string` | `""` | Input file (UAST JSON or source code) |
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `json` | Output format: `json`, `compact`, `count` |
| `--interactive` | `-t` | `bool` | `false` | Interactive query mode |

```bash
# Find all functions
uast query "filter(.type == 'Function')" main.go

# Find exported items
uast query "filter(.roles has 'Exported')" *.go

# Count all nodes
uast query "reduce(count)" main.go

# Query from piped UAST JSON
uast parse main.go | uast query "filter(.type == 'Call')" -

# Count format
uast query -f count "filter(.type == 'Function')" main.go

# Interactive mode
uast query -t main.go
```

??? info "DSL Syntax Reference"

    | Expression | Description |
    |-----------|-------------|
    | `filter(.type == "Function")` | Filter by node type |
    | `filter(.type == "Call")` | Find function calls |
    | `filter(.type == "Identifier")` | Find identifiers |
    | `filter(.type == "Literal")` | Find literal values |
    | `filter(.roles has "Exported")` | Filter by role annotation |
    | `reduce(count)` | Count matching nodes |

---

### `uast diff`

Compare two files at the UAST structural level and report changes.

```bash
uast diff <file1> <file2> [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `unified` | Output format: `unified`, `summary`, `json` |

```bash
# Unified diff output (default)
uast diff old.go new.go

# Summary of change types
uast diff -f summary old.go new.go

# Machine-readable JSON
uast diff -f json old.go new.go

# Save to file
uast diff -o changes.json -f json old.go new.go
```

---

### `uast explore`

Start an interactive session for exploring the UAST structure of a file.

```bash
uast explore [file] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--language` | `-l` | `string` | `""` | Force language detection override |

```bash
uast explore main.go
```

Available interactive commands:

| Command | Description |
|---------|-------------|
| `tree` | Show AST tree structure |
| `stats` | Show node type statistics |
| `find <type>` | Find nodes by type |
| `query <dsl>` | Execute a DSL query |
| `help` | Show available commands |
| `quit` | Exit exploration |

---

### `uast server`

Start an HTTP development server that provides UAST parsing and querying
via a REST API.

```bash
uast server [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--port` | `-p` | `string` | `8080` | Port to listen on |
| `--static` | `-s` | `string` | `""` | Directory to serve static files from |

```bash
# Start on default port
uast server

# Custom port with static file serving
uast server -p 3000 -s ./web
```

**API Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/parse` | Parse source code to UAST |
| `POST` | `/api/query` | Query a UAST with DSL expression |
| `GET` | `/api/mappings` | List available language mappings |
| `GET` | `/api/mappings/<name>` | Get a specific language mapping |

---

### `uast analyze`

Quick analysis shortcut that parses and analyzes source files in one step.

```bash
uast analyze [files...] [flags]
```

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--output` | `-o` | `string` | `""` | Output file (default: stdout) |
| `--format` | `-f` | `string` | `text` | Output format: `text`, `json`, `html` |

```bash
# Analyze a single file
uast analyze main.go

# Analyze all Go files as JSON
uast analyze -f json *.go

# Generate an HTML report
uast analyze -o report.html -f html *.go
```
