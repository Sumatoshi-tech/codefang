# Code Quality Analyzer

The code quality analyzer tracks **complexity, Halstead metrics, comment quality, and cohesion** across Git history. For each commit, it runs four static analyzers on UAST-parsed changed files and produces per-file quality metrics.

---

## Quick Start

```bash
codefang run -a history/quality .
```

!!! note "Requires UAST"
    The quality analyzer needs UAST support to parse source code. It is automatically enabled when the UAST pipeline is available.

---

## Architecture

The quality analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` runs four static analyzers (complexity, Halstead, comments, cohesion) on each changed file's UAST, returning per-file metrics as a `TC{Data: *TickQuality}`. The analyzer retains no per-commit state.
2. **Aggregation phase**: A `quality.Aggregator` collects TCs, merges `TickQuality` data by time bucket (tick), and produces `TICK` results.
3. **Serialization phase**: `SerializeTICKs()` converts aggregated TICKs into JSON, YAML, binary, or HTML plot output via `ComputeAllMetrics()`.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## What It Measures

### Per-File Metrics

For each changed file in a commit, the analyzer computes:

| Category | Metrics |
|---|---|
| **Complexity** | Cyclomatic complexity, cognitive complexity, max single-function complexity, function count |
| **Halstead** | Volume, effort, delivered bugs |
| **Comments** | Overall comment score, documentation coverage |
| **Cohesion** | Cohesion score |

### Statistical Aggregation

Per-tick statistics are computed from per-file arrays:

- **Mean, median, P95, max** for complexity and Halstead volume
- **Sum** for delivered bugs and Halstead volume
- **Min** for comment score and cohesion (worst-case tracking)
- **Total** files analyzed and functions counted

---

## Output Formats

| Format | Flag | Description |
|---|---|---|
| JSON | `--format json` | `ComputedMetrics` with `time_series` and `aggregate` fields |
| YAML | `--format yaml` | Same structure as JSON |
| Plot | `--format plot` | HTML page with complexity and Halstead charts plus summary stats |
| Binary | `--format binary` | Compact binary envelope for programmatic consumption |

---

## Configuration

The quality analyzer has no configurable options. It uses the UAST pipeline's language detection and parsing configuration.
