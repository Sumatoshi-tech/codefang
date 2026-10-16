# Your first analysis

This guide walks you through a complete analysis of a real Go project,
explains the two analysis modes, shows you how to interpret results, and
covers every output format.

---

## Two modes of analysis

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

## Step-by-step: analyzing a Go project

Let's analyze the Codefang repository itself. Clone it if you have not
already:

```bash
git clone https://github.com/Sumatoshi-tech/codefang.git
cd codefang
```

### Step 1 -- static complexity analysis

Start with a complexity scan to find the most complex functions:

```bash
codefang run -a static/complexity --format text .
```

??? example "Sample output"

    ```
    Codefang -- Code Analysis Report
    ===============================

    static/complexity:
      crates/cf-framework/src/runner.rs
        Function run_pipeline           -- Cyclomatic: 12  Cognitive: 8
        Function build_pipeline         -- Cyclomatic: 7   Cognitive: 5
        Function init_workers           -- Cyclomatic: 4   Cognitive: 3
      crates/cf-commands/src/run.rs
        Function run                    -- Cyclomatic: 9   Cognitive: 7
        Function run_direct             -- Cyclomatic: 6   Cognitive: 4
      crates/cf-uast/src/parser.rs
        Function parse                  -- Cyclomatic: 6   Cognitive: 5
        Function detect_language        -- Cyclomatic: 5   Cognitive: 3
    ```

**How to interpret:** Functions with cyclomatic complexity above 10 are
candidates for refactoring. Cognitive complexity captures nesting depth and
control-flow breaks that make code hard for humans to understand.

### Step 2 -- Halstead metrics

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

### Step 3 -- burndown history

Now switch to history analysis. The burndown analyzer shows how code ages:

```bash
codefang run -a history/burndown --format yaml .
```

!!! info "First run may take a moment"
    History analysis walks every commit in the repository. For large
    repositories, use `--limit 1000` to cap the number of commits or
    `--since 24h` to restrict the time window.

### Step 4 -- combine static and history

Run both engines in a single invocation:

```bash
codefang run -a static/complexity,history/burndown --format json .
```

Codefang automatically splits the selected analyzers into static and history
groups, runs each pipeline, and merges the output.

---

## Understanding output formats

The `--format` flag controls how results are rendered. Every analyzer supports
the universal formats; some also support format-specific extras.

### Universal formats

| Format | Flag | Description | Best for |
|--------|------|-------------|----------|
| **JSON** | `--format json` | Structured JSON (default) | Scripting, piping to `jq`, AI agents |
| **YAML** | `--format yaml` | Human-friendly YAML | Reading, config files |
| **Plot** | `--format plot` | Self-contained interactive HTML | Dashboards, sharing |
| **Binary** | `--format bin` | Compact binary encoding | Storage, conversion |
| **Time Series** | `--format timeseries` | Merged JSON array keyed by commit | Trend analysis |

### Static-only formats

| Format | Flag | Description | Best for |
|--------|------|-------------|----------|
| **Text** | `--format text` | Pretty-printed report with headers | Terminal reading |
| **Compact** | `--format compact` | One line per analyzer per file | Grep-friendly scanning |

### Format examples

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
    codefang run -a history/burndown --format plot -o report .
    ```

    Writes `report/index.html` plus one interactive page per analyzer
    (`report/burndown.html`, …) and `report/report.json`; `-o` is required.
    Open `report/index.html` in any browser.

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

## Configuration file

Instead of passing flags every time, create a `.codefang.yaml` file in the
directory you run `codefang` from (or in `$HOME`); `--config <file>` points
at any other file. Every key is the default for one `run` flag:

```yaml title=".codefang.yaml"
# Analyzers to run when -a is not given (omitted = all).
analyzers:
  - static/complexity
  - static/halstead
  - history/burndown

# History analyzer settings.
history:
  tick_size: 24           # --tick-size, hours per tick
  burndown:
    granularity: 30       # --granularity, ticks per age band
    sampling: 30          # --sampling, ticks per sample
    track_files: false    # --burndown-files
    track_people: false   # --burndown-people
  devs:
    anonymize: false      # --anonymize
  sentiment:
    min_comment_length: 20  # --min-comment-len
```

!!! tip "Precedence order"
    An explicit CLI flag overrides a `CODEFANG_*` environment variable, which
    overrides the configuration file, which overrides the compiled default.
    Environment variables use the `CODEFANG_` prefix with the key upper-cased
    and `.` replaced by `_`:

    ```bash
    CODEFANG_HISTORY_BURNDOWN_GRANULARITY=60 codefang run -a history/burndown .
    ```

    A key that codefang does not read (for example the Go-era `pipeline:`
    tuning tree or `checkpoint:`) is ignored with one
    `warning: ignoring unsupported config keys: …` line on stderr. See
    [Configuration](../guide/configuration.md) for every key.

---

## Useful flags for history analysis

| Flag | Example | Description |
|------|---------|-------------|
| `--limit N` | `--limit 1000` | Analyze only the N newest commits |
| `--since` | `--since 2024-01-01` | Only commits at/after this date, duration, or revision |
| `--head` | `--head` | Snapshot of HEAD only (fast; ignores the two flags above) |
| `--first-parent` | `--first-parent` | Skip merge-commit side branches |
| `--quiet` / `-q` | `-q` | Suppress the `progress:` lines on stderr (alias `--silent`) |
| `--output` / `-o` | `-o report.json` | Write the report to a file instead of stdout |

---

## One run per format

A report is rendered in the format chosen by `--format` when the analysis
runs; there is no command that converts a saved report into another format.
To get several formats, run the analysis once per format (bound the window
with `--limit` or `--since` to keep each run short):

```bash
# Machine-readable JSON
codefang run -a history/burndown --format json -o report.json .

# The same analysis as YAML
codefang run -a history/burndown --format yaml -o report.yaml .

# Interactive HTML pages in ./dashboard
codefang run -a history/burndown --format plot -o dashboard .
```

---

## Next steps

You now know how to run both analysis engines, interpret their output, and
control the output format. Continue exploring:

- [CLI Reference](../guide/cli-reference.md) -- every flag and subcommand
- [Configuration](../guide/configuration.md) -- full `.codefang.yaml` schema
- [Analyzers Overview](../analyzers/index.md) -- deep dives into each analyzer
- [MCP Integration](../integrations/mcp.md) -- connect Codefang to AI agents
