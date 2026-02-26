# Your First Analysis

This guide walks you through a complete analysis of a real Go project,
explains the two analysis modes, shows you how to interpret results, and
covers every output format.

---

## Two Modes of Analysis

Codefang has two fundamentally different analysis engines. Understanding when
to use each one is key to getting the most out of the tool.

=== "Static Analysis (UAST-based)"

    **What it does:** Parses source code into a Universal Abstract Syntax Tree
    and computes structural metrics from the current state of the code.

    **When to use:** You want to understand the *current quality* of your
    codebase -- complexity hotspots, cohesion problems, documentation gaps.

    **How it works:**

    ```mermaid
    graph LR
        A[Source files] --> B[UAST Parser<br/>Tree-sitter]
        B --> C[Unified AST]
        C --> D[Static Analyzers]
        D --> E[Report]
    ```

    **Available analyzers:**

    | Analyzer | ID | What it measures |
    |----------|----|------------------|
    | Complexity | `static/complexity` | Cyclomatic and cognitive complexity per function |
    | Cohesion | `static/cohesion` | LCOM-style class/module cohesion |
    | Halstead | `static/halstead` | Halstead software science metrics (volume, difficulty, effort) |
    | Comments | `static/comments` | Comment density and documentation coverage |
    | Imports | `static/imports` | Import/dependency graph structure |

=== "History Analysis (Git-based)"

    **What it does:** Walks the Git commit history and computes metrics that
    track how code evolves over time.

    **When to use:** You want to understand *how your project changes* --
    which code survives, who contributes what, which files change together.

    **How it works:**

    ```mermaid
    graph LR
        A[Git Repository] --> B[libgit2<br/>Commit Walker]
        B --> C[Diff Pipeline]
        C --> D[History Analyzers]
        D --> E[Report]
    ```

    **Available analyzers:**

    | Analyzer | ID | What it measures |
    |----------|----|------------------|
    | Burndown | `history/burndown` | Code age and survival over time |
    | Developers | `history/devs` | Per-developer contribution stats |
    | Couples | `history/couples` | File and developer coupling |
    | File History | `history/file-history` | Per-file change frequency and churn |
    | Sentiment | `history/sentiment` | Commit message sentiment trends |
    | Shotness | `history/shotness` | Function-level change frequency |
    | Typos | `history/typos` | Identifier typos across history |
    | Anomaly | `history/anomaly` | Automated anomaly detection |

---

## Step-by-Step: Analyzing a Go Project

Let's analyze the Codefang repository itself. Clone it if you have not
already:

```bash
git clone https://github.com/Sumatoshi-tech/codefang.git
cd codefang
```

### Step 1 -- Static Complexity Analysis

Start with a complexity scan to find the most complex functions:

```bash
codefang run -a static/complexity --format text .
```

??? example "Sample output"

    ```
    Codefang -- Code Analysis Report
    ===============================

    static/complexity:
      internal/framework/runner.go
        Function RunPipeline            -- Cyclomatic: 12  Cognitive: 8
        Function buildPipeline          -- Cyclomatic: 7   Cognitive: 5
        Function initWorkers            -- Cyclomatic: 4   Cognitive: 3
      cmd/codefang/commands/run.go
        Function run                    -- Cyclomatic: 9   Cognitive: 7
        Function runDirect              -- Cyclomatic: 6   Cognitive: 4
      pkg/uast/parser.go
        Function Parse                  -- Cyclomatic: 6   Cognitive: 5
        Function detectLanguage         -- Cyclomatic: 5   Cognitive: 3
    ```

**How to interpret:** Functions with cyclomatic complexity above 10 are
candidates for refactoring. Cognitive complexity captures nesting depth and
control-flow breaks that make code hard for humans to understand.

### Step 2 -- Halstead Metrics

Measure the information-theoretic complexity of your modules:

```bash
codefang run -a static/halstead --format text .
```

??? example "Sample output"

    ```
    static/halstead:
      internal/framework/runner.go
        Vocabulary: 142   Length: 389   Volume: 2776.4
        Difficulty: 28.3  Effort: 78572  Est. Bugs: 0.93
      pkg/uast/parser.go
        Vocabulary: 98    Length: 241   Volume: 1598.2
        Difficulty: 19.7  Effort: 31485  Est. Bugs: 0.53
    ```

**How to interpret:** High *difficulty* values indicate dense, hard-to-maintain
code. The *estimated bugs* metric (Volume / 3000) gives a rough defect
prediction.

### Step 3 -- Burndown History

Now switch to history analysis. The burndown analyzer shows how code ages:

```bash
codefang run -a history/burndown --format yaml .
```

!!! info "First run may take a moment"
    History analysis walks every commit in the repository. For large
    repositories, use `--limit 1000` to cap the number of commits or
    `--since 24h` to restrict the time window.

### Step 4 -- Combine Static and History

Run both engines in a single invocation:

```bash
codefang run -a static/complexity,history/burndown --format json .
```

Codefang automatically splits the selected analyzers into static and history
groups, runs each pipeline, and merges the output.

---

## Understanding Output Formats

The `--format` flag controls how results are rendered. Every analyzer supports
the universal formats; some also support format-specific extras.

### Universal Formats

| Format | Flag | Description | Best for |
|--------|------|-------------|----------|
| **JSON** | `--format json` | Structured JSON (default) | Scripting, piping to `jq`, AI agents |
| **YAML** | `--format yaml` | Human-friendly YAML | Reading, config files |
| **Plot** | `--format plot` | Self-contained interactive HTML | Dashboards, sharing |
| **Binary** | `--format bin` | Compact binary encoding | Storage, conversion |
| **Time Series** | `--format timeseries` | Merged JSON array keyed by commit | Trend analysis |

### Static-Only Formats

| Format | Flag | Description | Best for |
|--------|------|-------------|----------|
| **Text** | `--format text` | Pretty-printed report with headers | Terminal reading |
| **Compact** | `--format compact` | One line per analyzer per file | Grep-friendly scanning |

### Format Examples

=== "JSON (default)"

    ```bash
    codefang run -a static/complexity --format json .
    ```

    ```json
    {
      "static/complexity": {
        "files": {
          "internal/framework/runner.go": {
            "functions": [
              {
                "name": "RunPipeline",
                "cyclomatic": 12,
                "cognitive": 8,
                "line_start": 45,
                "line_end": 102
              }
            ]
          }
        }
      }
    }
    ```

=== "Text"

    ```bash
    codefang run -a static/complexity --format text .
    ```

    ```
    Codefang -- Code Analysis Report
    ===============================

    static/complexity:
      internal/framework/runner.go
        Function RunPipeline        -- Cyclomatic: 12  Cognitive: 8
    ```

=== "Compact"

    ```bash
    codefang run -a static/complexity --format compact .
    ```

    ```
    complexity  internal/framework/runner.go  RunPipeline  cyc=12  cog=8
    complexity  pkg/uast/parser.go       Parse        cyc=6   cog=5
    ```

=== "YAML"

    ```bash
    codefang run -a history/burndown --format yaml .
    ```

    ```yaml
    burndown:
      granularity: 30
      sampling: 30
      project:
        - [0, 1200, 1100, 980]
      ticks:
        - "2024-06-01"
        - "2024-07-01"
        - "2024-08-01"
    ```

=== "Plot (HTML)"

    ```bash
    codefang run -a history/burndown --format plot . > report.html
    ```

    Opens as an interactive chart in any browser. Multiple analyzers produce
    a multi-section dashboard with tabs.

=== "Time Series"

    ```bash
    codefang run -a history/burndown,history/devs --format timeseries .
    ```

    ```json
    [
      {
        "tick": "2024-06-01",
        "burndown": { "surviving_lines": 1200 },
        "devs": { "active_developers": 3 }
      },
      {
        "tick": "2024-07-01",
        "burndown": { "surviving_lines": 1100 },
        "devs": { "active_developers": 4 }
      }
    ]
    ```

---

## Configuration File

Instead of passing flags every time, create a `.codefang.yaml` file in your
repository root (or `$HOME`):

```yaml title=".codefang.yaml"
# Analyzers to run by default (empty = all registered).
analyzers:
  - static/complexity
  - static/halstead
  - history/burndown

# Pipeline resource tuning.
pipeline:
  workers: 4              # parallel workers (0 = auto)
  memory_budget: "4GiB"   # auto-tune caches to fit this budget
  blob_cache_size: "512MiB"

# History analyzer settings.
history:
  burndown:
    granularity: 30       # days per time bucket
    sampling: 30
    track_files: false
    track_people: false
  devs:
    anonymize: false
  sentiment:
    min_comment_length: 20
    gap: 0.5

# Checkpoint for crash recovery on large repos.
checkpoint:
  enabled: true
  resume: true
```

!!! tip "Precedence order"
    CLI flags override environment variables, which override the configuration
    file. Environment variables use the `CODEFANG_` prefix with underscores
    for nesting:

    ```bash
    CODEFANG_PIPELINE_WORKERS=8 codefang run -a history/burndown .
    ```

---

## Useful Flags for History Analysis

| Flag | Example | Description |
|------|---------|-------------|
| `--limit N` | `--limit 1000` | Analyze only the last N commits |
| `--since` | `--since 2024-01-01` | Only commits after this date |
| `--head` | `--head` | Snapshot of HEAD only (fast) |
| `--first-parent` | `--first-parent` | Skip merge-commit side branches |
| `--workers N` | `--workers 8` | Parallel pipeline workers |
| `--memory-budget` | `--memory-budget 2GB` | Auto-tune caches |
| `--checkpoint` | `--checkpoint` | Enable crash recovery (on by default) |
| `--silent` | `--silent` | Suppress progress output |

---

## Converting Between Formats

You can convert an existing report to a different format without re-running
the analysis:

```bash
# Run once and save as binary
codefang run -a history/burndown --format bin . > report.bin

# Convert to YAML later
codefang run --input report.bin --format yaml

# Convert to an HTML plot
codefang run --input report.bin --format plot > dashboard.html
```

!!! note "Supported input formats"
    The `--input` flag accepts **JSON** and **binary** files. Use
    `--input-format auto` (the default) to let Codefang detect the format, or
    specify `--input-format json` or `--input-format bin` explicitly.

---

## Next Steps

You now know how to run both analysis engines, interpret their output, and
control the output format. Continue exploring:

- [CLI Reference](../guide/cli-reference.md) -- every flag and subcommand
- [Configuration](../guide/configuration.md) -- full `.codefang.yaml` schema
- [Analyzers Overview](../analyzers/index.md) -- deep dives into each analyzer
- [MCP Integration](../integrations/mcp.md) -- connect Codefang to AI agents
