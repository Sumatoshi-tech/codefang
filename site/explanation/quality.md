# Understanding code quality over time

This page explains the mental model behind the code quality analyzer: which
static metrics it tracks per commit and how it aggregates them per tick. For
configuration keys and the output schema, see the
[Quality reference](../analyzers/quality.md).

---

## What it measures

The code quality analyzer tracks **complexity, Halstead metrics, comment
quality, and cohesion** across Git history. For each commit, it runs four
static analyzers on UAST-parsed changed files and produces per-file quality
metrics.

### Per-file metrics

For each changed file in a commit, the analyzer computes:

| Category | Metrics |
|---|---|
| **Complexity** | Cyclomatic complexity, cognitive complexity, max single-function complexity, function count |
| **Halstead** | Volume, effort, delivered bugs |
| **Comments** | Overall comment score, documentation coverage |
| **Cohesion** | Cohesion score |

### Statistical aggregation

Per-tick statistics are computed from per-file arrays:

- **Mean, median, P95, max** for complexity and Halstead volume
- **Sum** for delivered bugs and Halstead volume
- **Min** for comment score and cohesion (worst-case tracking)
- **Total** files analyzed and functions counted

---

## Architecture

The quality analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` runs four static analyzers (complexity, Halstead, comments, cohesion) on each changed file's UAST, returning per-file metrics as a `TC{Data: *TickQuality}`. The analyzer retains no per-commit state.
2. **Aggregation phase**: A `quality.Aggregator` collects TCs, merges `TickQuality` data by time bucket (tick), and produces `TICK` results.
3. **Serialization phase**: `SerializeTICKs()` converts aggregated TICKs into JSON, YAML, binary, or HTML plot output via `ComputeAllMetrics()`.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## See also

- [Quality reference](../analyzers/quality.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
