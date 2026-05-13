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
| [NDJSON](#ndjson) | `ndjson` | `application/x-ndjson` | Streaming DWH ingestion (ClickHouse, BigQuery) |
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

Structured JSON output. This is the **default format**. The output is wrapped
in a versioned envelope with metadata, per-analyzer schema manifests, and
reports. Each analyzer's report contains typed arrays of records with
consistent identifiers (`source_file`, `language`, `directory` on function
records; `start_time`/`end_time` on time-series ticks; split `name`/`email`
on developer records).

```bash
codefang run --format json .
```

??? example "Example Output (Combined Static + History)"

    ```json
    {
      "version": "codefang.run.v1",
      "metadata": {
        "repo_path": "/home/user/sources/myproject",
        "repo_name": "myproject",
        "analyzed_at": "2026-04-07T23:33:00Z",
        "codefang_version": "0.1.0"
      },
      "analyzers": [
        {
          "id": "static/complexity",
          "mode": "static",
          "schema": {
            "function_complexity": {
              "type": "list",
              "grain": "function",
              "description": "Per-function cyclomatic and cognitive complexity"
            },
            "aggregate": {
              "type": "aggregate",
              "description": "Summary statistics"
            }
          },
          "report": {
            "function_complexity": [
              {
                "name": "RunStreaming",
                "source_file": "internal/framework/runner.go",
                "language": "go",
                "directory": "internal/framework",
                "cyclomatic_complexity": 11,
                "cognitive_complexity": 15,
                "nesting_depth": 3,
                "lines_of_code": 85,
                "complexity_density": 0.129,
                "risk_level": "MEDIUM"
              }
            ],
            "aggregate": {
              "total_functions": 312,
              "average_complexity": 2.6,
              "max_complexity": 11,
              "health_score": 82.5
            }
          }
        },
        {
          "id": "history/sentiment",
          "mode": "history",
          "schema": {
            "time_series": {
              "type": "time_series",
              "grain": "tick",
              "description": "Per-tick sentiment scores"
            }
          },
          "report": {
            "time_series": [
              {
                "tick": 0,
                "start_time": "2024-01-15T10:30:00Z",
                "end_time": "2024-01-16T08:45:00Z",
                "sentiment": 0.72,
                "classification": "positive",
                "comment_count": 5,
                "commit_count": 12
              }
            ]
          }
        }
      ]
    }
    ```

**Key output fields added for analytics/DWH consumption:**

| Field | Present On | Description |
|-------|-----------|-------------|
| `source_file` | All function records | Relative file path (e.g., `"pkg/api/server.go"`) |
| `language` | All function records | Detected language (e.g., `"go"`, `"python"`) |
| `directory` | All function records | Parent directory (e.g., `"pkg/api"`) |
| `start_time` | All time-series ticks | RFC 3339 tick start timestamp |
| `end_time` | All time-series ticks | RFC 3339 tick end timestamp |
| `email` | Developer records | Separated from name (no more pipe-delimited) |
| `schema` | Each analyzer section | Field type, grain, and description metadata |
| `metadata` | Top-level envelope | Repo name, analysis timestamp, version |

!!! tip "When to Use"

    - CI/CD pipelines that parse results programmatically
    - Loading into data warehouses (ClickHouse, BigQuery, Snowflake)
    - Cross-format conversion input (`--input`)
    - Building BI dashboards from function-level metrics

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

## NDJSON

**Flag:** `--format ndjson`

Newline-delimited JSON. Each analyzer produces one compact JSON line. If
metadata is present, a metadata line is emitted first. This format enables
streaming ingestion into columnar DWH systems like ClickHouse, where each
line can be parsed independently without buffering the entire file.

```bash
codefang run --format ndjson . > output.ndjson
```

??? example "Example Output"

    ```
    {"version":"codefang.run.v1","metadata":{"repo_name":"myproject","analyzed_at":"2026-04-07T23:33:00Z","codefang_version":"0.1.0"}}
    {"id":"static/complexity","mode":"static","report":{"function_complexity":[...],"aggregate":{...}}}
    {"id":"static/halstead","mode":"static","report":{"function_halstead":[...]}}
    {"id":"history/sentiment","mode":"history","report":{"time_series":[...]}}
    ```

Each line is independently parseable JSON. The file can be processed with
standard tools:

```bash
# Extract a single analyzer
grep '"static/complexity"' output.ndjson | jq .report.aggregate

# Count lines
wc -l output.ndjson

# Stream into ClickHouse
cat output.ndjson | clickhouse-client --query "INSERT INTO codefang FORMAT JSONEachRow"
```

!!! tip "When to Use"

    - Streaming ingestion into ClickHouse, BigQuery, or Kafka
    - Processing large reports without loading the full file into memory
    - Unix pipeline workflows (`grep`, `jq`, `wc`)

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

**Requires:** `--output <dir>` (`-o <dir>`)

Multi-page interactive HTML report with charts rendered by
[go-echarts](https://github.com/go-echarts/go-echarts). Each analyzer gets its
own HTML page, plus an `index.html` with navigation cards. The output directory
can be opened in any browser.

Both static and history analyzers produce the same multi-page layout.

```bash
# Generate multi-page report
codefang run -a 'static/*' --format plot -o ./report .
open ./report/index.html

# History analyzers
codefang run -a 'history/*' --format plot -o ./report .
open ./report/index.html
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
| `ndjson` | :material-check: | :material-check: | :material-check: |
| `plot` | :material-check: | :material-check: | :material-check: |
| `timeseries` | -- | :material-check: | :material-check: |

!!! note "Mixed Runs"

    When both static and history analyzers are selected (`-a '*'`), the format
    must be one of the **universal formats**: `json`, `yaml`, `plot`, or
    `timeseries`. The `text` and `compact` formats are only available when
    running static analyzers alone.

!!! info "Memory optimization"

    The `text` and `compact` formats use **summary-only aggregation**: only
    running sums and averages are kept in memory, while per-function detail
    data is not collected. This dramatically reduces memory usage on large
    codebases (up to 97% heap reduction). The `json`, `yaml`, `plot`, and
    `binary` formats use **full aggregation** since they need per-item data
    for serialization.

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
