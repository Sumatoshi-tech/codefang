# Quick Start

This page shows the fastest path to useful results. In five minutes you will
parse code, run static analysis, and mine Git history.

!!! tip "Prerequisites"
    Make sure both `codefang` and `uast` are installed.
    See [Installation](installation.md) if you have not set them up yet.

---

## 1. Parse Source Code into a UAST

Start by parsing a single file into a Universal Abstract Syntax Tree:

```bash
uast parse main.go
```

??? example "Expected output (truncated)"

    ```json
    {
      "language": "go",
      "file": "main.go",
      "root": {
        "type": "SourceFile",
        "roles": ["File"],
        "children": [
          {
            "type": "PackageClause",
            "roles": ["Package", "Declaration"],
            "props": { "name": "main" }
          },
          {
            "type": "FunctionDeclaration",
            "roles": ["Function", "Declaration"],
            "props": { "name": "main" },
            "children": [ "..." ]
          }
        ]
      }
    }
    ```

You can parse entire directories, force a language, or read from stdin:

```bash
uast parse --all                      # every source file in the codebase
uast parse -l python script.sh        # force Python language detection
cat main.go | uast parse -            # read from stdin
```

---

## 2. Static Analysis

Run structural analysis on a folder. The `codefang run` command with
`static/*` analyzers walks the directory, parses every supported file, and
computes metrics:

```bash
codefang run -a static/complexity --format text .
```

??? example "Expected output"

    ```
    Codefang -- Code Analysis Report
    ===============================

    static/complexity:
      pkg/framework/runner.go
        Function RunPipeline        -- Cyclomatic: 12  Cognitive: 8
        Function initWorkers        -- Cyclomatic: 4   Cognitive: 3
      cmd/codefang/commands/run.go
        Function run                -- Cyclomatic: 9   Cognitive: 7
    ```

!!! tip "Multiple static analyzers"
    Combine several analyzers in one pass:

    ```bash
    codefang run -a static/complexity,static/halstead,static/cohesion --format text .
    ```

---

## 3. History Analysis -- Burndown

Analyze how code ages and survives over your repository's lifetime:

```bash
codefang run -a history/burndown --format yaml .
```

??? example "Expected output (truncated)"

    ```yaml
    burndown:
      granularity: 30
      sampling: 30
      project:
        - [0, 1200, 1100, 980, 920, 870]
        - [0, 0, 320, 290, 270, 250]
        - [0, 0, 0, 180, 160, 140]
      ticks:
        - "2024-01-15"
        - "2024-02-14"
        - "2024-03-15"
        - "2024-04-14"
        - "2024-05-14"
        - "2024-06-13"
    ```

Each row in `project` represents lines added in a given time window; each
column shows how many of those lines survive into subsequent windows.

---

## 4. Developer Analysis

See per-developer contribution statistics with a single commit snapshot:

```bash
codefang run -a history/devs --head --format yaml .
```

??? example "Expected output (truncated)"

    ```yaml
    devs:
      developers:
        - name: "Alice"
          commits: 142
          added: 12400
          removed: 3200
          changed: 8900
        - name: "Bob"
          commits: 87
          added: 6300
          removed: 1800
          changed: 4100
    ```

!!! info "The `--head` flag"
    `--head` tells the history pipeline to analyze only the HEAD commit's
    state rather than walking the full history. It is much faster and useful
    for snapshot metrics like developer totals.

---

## 5. Run Everything at Once

Use glob patterns to run all history analyzers (or all analyzers) in a single
invocation:

=== "All history analyzers"

    ```bash
    codefang run -a "history/*" --format json .
    ```

=== "All static analyzers"

    ```bash
    codefang run -a "static/*" --format json .
    ```

=== "Everything"

    ```bash
    codefang run -a "*" --format json .
    ```

!!! tip "JSON output for scripting"
    JSON is the default format and the most machine-friendly. Pipe it into
    `jq` for quick exploration:

    ```bash
    codefang run -a history/burndown --format json . | jq '.burndown.ticks'
    ```

---

## 6. Interactive HTML Plots

Generate a self-contained HTML report with interactive charts:

```bash
codefang run -a history/burndown,history/devs --format plot .
```

This writes an HTML page to **stdout**. Redirect it to a file and open in your
browser:

```bash
codefang run -a history/burndown --format plot . > burndown.html
open burndown.html   # macOS
xdg-open burndown.html  # Linux
```

---

## 7. Pipe into AI (MCP Integration)

Codefang ships a built-in **Model Context Protocol** server so AI agents can
query analysis results programmatically:

```bash
codefang mcp serve
```

This starts a JSON-RPC server that tools like Claude Desktop, Cursor, and
other MCP-compatible clients can connect to. See the
[MCP Integration](../integrations/mcp.md) guide for configuration details.

!!! note "AI-ready output"
    Any `--format json` output can also be fed directly to an LLM for
    summarization or code review:

    ```bash
    codefang run -a static/complexity --format json . | llm "summarize the complexity hotspots"
    ```

---

## What's Next?

| Goal | Page |
|------|------|
| Understand static vs. history analysis in depth | [First Analysis](first-analysis.md) |
| Learn every CLI flag | [CLI Reference](../guide/cli-reference.md) |
| Tune output formats | [Output Formats](../guide/output-formats.md) |
| Configure via `.codefang.yaml` | [Configuration](../guide/configuration.md) |
| Set up CI/CD pipelines | [Docker & GitHub Actions](../integrations/docker-and-actions.md) |
