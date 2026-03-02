# Output Formats

Codefang supports six output formats. Each is suited to a different use case,
from human review to CI pipelines to interactive exploration. Select a format
with the `--format` flag:

```bash
codefang run -a static/complexity --format text .
```

---

## Format Overview

| Format | Flag Value | Content Type | Best For |
|--------|-----------|--------------|----------|
| [Text](#text) | `text` | Plain text | Human review in a terminal |
| [JSON](#json) | `json` | `application/json` | Programmatic consumption, CI pipelines |
| [YAML](#yaml) | `yaml` | `text/yaml` | Human-readable structured data, config integration |
| [Compact](#compact) | `compact` | Plain text | Quick summaries, log ingestion |
| [Time Series](#time-series) | `timeseries` | `application/json` | Chronological analysis, dashboards |
| [Plot](#plot) | `plot` | `text/html` | Interactive charts, reports, presentations |

---

## Text

**Flag:** `--format text`

Human-readable table output with optional color. This is the most readable
format for terminal review. Static analyzers render section headers, aligned
columns, and summary lines. History analyzers render a version header followed
by structured key-value data.

Use `--verbose` (`-v`) to expand full detail in static reports. Use `--no-color`
to strip ANSI escape codes (useful when piping to a file).

```bash
codefang run -a static/complexity --format text -v .
```

??? example "Example Output"

    ```
    Complexity Analysis
    ===================

    File                          Functions   Avg Complexity   Max Complexity
    ----------------------------  ----------  ---------------  --------------
    internal/framework/runner.go       12          4.2              11
    internal/analyzers/burndown/...    8           3.8              9
    pkg/gitlib/repository.go      15          3.1              7
    cmd/codefang/commands/run.go  22          2.9              8

    Summary
    -------
    Total files:       47
    Total functions:   312
    Average:           2.6
    Maximum:           11 (internal/framework/runner.go:RunStreaming)
    ```

!!! tip "When to Use"

    - Reviewing results directly in a terminal session
    - Quick manual inspection during development
    - Sharing results in pull request comments (with `--no-color`)

---

## JSON

**Flag:** `--format json`

Structured JSON output. This is the **default format**. Each analyzer produces
a well-defined JSON schema. Static analyzers emit a single JSON object;
history analyzers emit per-analyzer JSON objects.

```bash
codefang run -a static/complexity --format json .
```

??? example "Example Output"

    ```json
    {
      "complexity": {
        "files": [
          {
            "path": "internal/framework/runner.go",
            "functions": [
              {
                "name": "RunStreaming",
                "complexity": 11,
                "lines": 85,
                "start_line": 42,
                "end_line": 127
              },
              {
                "name": "NewRunnerWithConfig",
                "complexity": 3,
                "lines": 22,
                "start_line": 15,
                "end_line": 37
              }
            ],
            "summary": {
              "total_functions": 12,
              "average_complexity": 4.2,
              "max_complexity": 11
            }
          }
        ],
        "summary": {
          "total_files": 47,
          "total_functions": 312,
          "average_complexity": 2.6,
          "max_complexity": 11
        }
      }
    }
    ```

!!! tip "When to Use"

    - CI/CD pipelines that parse results programmatically
    - Feeding data into external tools or databases
    - Cross-format conversion input (`--input`)

---

## YAML

**Flag:** `--format yaml`

YAML-formatted output. Produces the same logical structure as JSON but in YAML
syntax. Useful when the output will be merged with other YAML-based tooling or
when readability of structured data is preferred over plain tables.

```bash
codefang run -a static/complexity --format yaml .
```

??? example "Example Output"

    ```yaml
    complexity:
      files:
        - path: internal/framework/runner.go
          functions:
            - name: RunStreaming
              complexity: 11
              lines: 85
              start_line: 42
              end_line: 127
            - name: NewRunnerWithConfig
              complexity: 3
              lines: 22
              start_line: 15
              end_line: 37
          summary:
            total_functions: 12
            average_complexity: 4.2
            max_complexity: 11
      summary:
        total_files: 47
        total_functions: 312
        average_complexity: 2.6
        max_complexity: 11
    ```

!!! tip "When to Use"

    - Integration with YAML-native workflows (Ansible, Kubernetes configs)
    - Human review of structured data without JSON bracket noise
    - Diffing results across runs with standard text diff tools

---

## Compact

**Flag:** `--format compact`

Minimal single-line-per-analyzer output. Each analyzer emits a one-line summary
with key metrics. No headers, no detail rows.

```bash
codefang run -a 'static/*' --format compact .
```

??? example "Example Output"

    ```
    complexity  files=47  functions=312  avg=2.6  max=11
    comments    files=47  ratio=0.18  missing_doc=23
    halstead    files=47  avg_volume=842.3  avg_difficulty=12.1
    cohesion    files=47  avg_lcom=0.34
    imports     files=47  total=189  unique=62
    ```

!!! tip "When to Use"

    - Log aggregation systems that expect single-line records
    - Quick at-a-glance summaries in scripts
    - Embedding in commit messages or Slack notifications

---

## Time Series

**Flag:** `--format timeseries`

A unified chronological JSON array that merges data from **all selected history
analyzers** into a single stream keyed by commit. Each entry contains commit
metadata plus per-analyzer data for that commit.

This format is only meaningful for history analyzers. It requires at least one
analyzer that implements the `CommitTimeSeriesProvider` interface (anomaly,
devs, quality, sentiment).

```bash
codefang run -a history/devs,history/sentiment --format timeseries .
```

??? example "Example Output"

    ```json
    {
      "version": "codefang.timeseries.v1",
      "tick_size_hours": 24,
      "analyzers": [
        "devs",
        "sentiment"
      ],
      "commits": [
        {
          "hash": "a1b2c3d4e5f6...",
          "timestamp": "2025-03-15T10:30:00Z",
          "author": "alice@example.com",
          "tick": 0,
          "devs": {
            "added": 142,
            "removed": 38,
            "changed": 5,
            "languages": {
              "Go": { "added": 120, "removed": 30 },
              "YAML": { "added": 22, "removed": 8 }
            }
          },
          "sentiment": {
            "positive": 2,
            "negative": 0,
            "neutral": 1,
            "score": 0.67
          }
        },
        {
          "hash": "f6e5d4c3b2a1...",
          "timestamp": "2025-03-16T14:22:00Z",
          "author": "bob@example.com",
          "tick": 1,
          "devs": {
            "added": 57,
            "removed": 12,
            "changed": 3,
            "languages": {
              "Go": { "added": 57, "removed": 12 }
            }
          },
          "sentiment": {
            "positive": 0,
            "negative": 1,
            "neutral": 0,
            "score": -0.33
          }
        }
      ]
    }
    ```

**Schema details:**

| Field | Type | Description |
|-------|------|-------------|
| `version` | `string` | Schema version. Always `codefang.timeseries.v1`. |
| `tick_size_hours` | `float64` | Duration of one tick in hours (default: 24). |
| `analyzers` | `[]string` | Ordered list of analyzer flags that contributed data. |
| `commits` | `[]object` | Chronologically ordered commit entries. |
| `commits[].hash` | `string` | Full commit hash. |
| `commits[].timestamp` | `string` | ISO 8601 / RFC 3339 timestamp. |
| `commits[].author` | `string` | Commit author identifier. |
| `commits[].tick` | `int` | Tick index (integer time bucket). |
| `commits[].<analyzer>` | `object` | Per-analyzer data; key matches the analyzer flag name. |

!!! tip "When to Use"

    - Building custom dashboards (Grafana, Jupyter, Observable)
    - Correlating metrics across analyzers over time
    - Feeding into anomaly detection or ML pipelines

---

## Plot

**Flag:** `--format plot`

Self-contained interactive HTML page with charts rendered by
[go-echarts](https://github.com/go-echarts/go-echarts). The output is a single
HTML file that can be opened in any browser. When multiple analyzers are
selected, they are combined into a single multi-section page.

```bash
# Generate and open in browser
codefang run -a 'history/*' --format plot . > report.html
open report.html

# Single analyzer
codefang run -a history/burndown --format plot . > burndown.html
```

??? example "What the Output Contains"

    The generated HTML file includes:

    - **Interactive line charts** for time-series data (burndown curves, sentiment trends)
    - **Bar charts** for distribution data (complexity per file, language breakdown)
    - **Heatmaps** for correlation data (developer coupling matrices)
    - **Tooltips** with detailed values on hover
    - **Zoom and pan** controls for large datasets
    - **Section headers** with analyzer names and descriptions
    - **Responsive layout** that adapts to browser width

    All JavaScript and CSS is inlined -- no external dependencies or CDN
    requests are made. The file works completely offline.

!!! tip "When to Use"

    - Sharing visual reports with stakeholders
    - Presentations and code review meetings
    - Exploratory analysis where interactive drill-down is valuable

---

## Format Comparison

The following table summarizes which formats are available for which analyzer
categories:

| Format | Static Analyzers | History Analyzers | Mixed Runs |
|--------|:---------------:|:-----------------:|:----------:|
| `text` | :material-check: | -- | -- |
| `compact` | :material-check: | -- | -- |
| `json` | :material-check: | :material-check: | :material-check: |
| `yaml` | :material-check: | :material-check: | :material-check: |
| `plot` | :material-check: | :material-check: | :material-check: |
| `timeseries` | -- | :material-check: | :material-check: |

!!! note "Mixed Runs"

    When both static and history analyzers are selected (`-a '*'`), the format
    must be one of the **universal formats**: `json`, `yaml`, `plot`, or
    `timeseries`. The `text` and `compact` formats are only available when
    running static analyzers alone.

---

## Cross-Format Conversion

You can convert a previously generated report to a different format without
re-running analysis. First, generate a binary (or JSON) report, then convert:

```bash
# Step 1: Generate binary report
codefang run -a 'history/*' --format bin . > report.bin

# Step 2: Convert to interactive plot
codefang run -a 'history/*' --input report.bin --format plot > report.html

# Step 3: Convert the same data to YAML
codefang run -a 'history/*' --input report.bin --format yaml

# Step 4: Convert to unified time-series
codefang run -a 'history/*' --input report.bin --format timeseries
```

The `--input-format` flag controls how the input file is parsed. It defaults to
`auto` which detects the format from the file content:

| Value | Description |
|-------|-------------|
| `auto` | Detect from content (binary magic bytes or JSON) |
| `json` | Force JSON parsing |
| `bin` | Force binary parsing |
