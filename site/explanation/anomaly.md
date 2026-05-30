# Understanding temporal anomaly detection

This page explains the mental model behind the anomaly analyzer: how Z-score
analysis over a sliding window flags unusual activity, which metrics it tracks,
and how the aggregation pipeline is structured. For configuration keys and the
output schema, see the [Anomaly reference](../analyzers/anomaly.md).

---

## What it measures

The anomaly analyzer performs **temporal anomaly detection** using Z-score
analysis over a sliding window of per-tick commit metrics. It detects sudden
quality degradation -- unusual spikes in churn, file changes, or other metrics
that deviate significantly from the historical baseline.

### Z-score analysis

For each time tick, the analyzer computes Z-scores for six metrics using a trailing sliding window:

| Metric | Description |
|---|---|
| **Net churn** | `lines_added - lines_removed` |
| **Files changed** | Number of files modified in the tick |
| **Lines added** | Total lines added across all commits in the tick |
| **Lines removed** | Total lines removed across all commits in the tick |
| **Language diversity** | Number of distinct programming languages modified |
| **Author count** | Number of distinct developers active in the tick |

A Z-score measures how many standard deviations a value is from the rolling mean. When any metric's absolute Z-score exceeds the threshold, the tick is flagged as anomalous.

!!! info "Z-score interpretation"
    - **|Z| < 2.0**: Within normal variation
    - **|Z| 2.0 - 3.0**: Unusual, worth investigating
    - **|Z| > 3.0**: Highly anomalous, likely a significant event

### Sliding window

The Z-score is computed against a trailing window of the previous N ticks (default 20). This means the "normal" baseline adapts to gradual trends, and only sudden deviations are flagged.

### Anomaly records

Each detected anomaly includes:

- **Tick index**: When it occurred
- **Z-scores**: Per-metric Z-scores showing which dimension is anomalous
- **Max absolute Z-score**: The highest Z-score across all metrics (used for severity ranking)
- **Raw metrics**: The actual values for that tick
- **Files**: List of files changed in the anomalous tick

Anomalies are sorted by severity (highest absolute Z-score first).

---

## Architecture

The anomaly analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` extracts file changes, line stats, language diversity, and author ID, returning per-commit metrics as a `TC{Data: *CommitAnomalyData}`. The analyzer retains no per-commit state.
2. **Aggregation phase**: An `anomaly.Aggregator` collects TCs, merges `CommitAnomalyData` into per-tick `TickMetrics` (aggregating files, lines, languages, and unique authors), and produces `TICK` results.
3. **Z-score detection**: `ticksToReport()` runs sliding-window Z-score analysis on six dimensions over the aggregated tick metrics to detect anomalies.
4. **Serialization phase**: `SerializeTICKs()` converts aggregated TICKs into JSON, YAML, binary, or HTML plot output via `ComputeAllMetrics()`.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## Use cases

- **Quality gates**: Flag releases or sprints that contain anomalous commit patterns as higher risk.
- **Incident correlation**: Overlay anomaly timelines with production incident timelines to find patterns.
- **Process monitoring**: Detect process breakdowns -- e.g., a sudden spike in single-author commits to many files may indicate bypassed code review.
- **Onboarding safety**: Monitor anomaly rates during onboarding periods. New developers may produce unusual patterns.
- **Seasonal pattern detection**: Over long time periods, the analyzer reveals cyclical patterns (release crunches, holiday slowdowns).
- **Multi-metric alerts**: Unlike simple threshold alerts on individual metrics, the Z-score approach adapts to the repository's natural variance.

---

## Limitations

- **Cold start**: The first `window_size` ticks have no or limited history for Z-score computation. Anomalies during this period are less reliable.
- **Gradual drift**: Z-scores detect sudden deviations, not gradual trends. A slow but steady increase in churn will update the rolling mean and never trigger an anomaly.
- **Zero-variance sentinel**: When the window has zero standard deviation (all values identical) and the current value differs, a sentinel Z-score of 100 is assigned. This ensures detection but may inflate severity rankings.
- **Tick granularity**: Metrics are aggregated per tick (default 24-hour window). Sub-tick anomalies are not detected.
- **Not causal**: The analyzer detects statistical anomalies, not root causes. Investigation is required to determine whether an anomaly represents a real problem.
- **Multi-dimensional**: An anomaly is flagged when *any* single metric exceeds the threshold. This means false positive rates increase with the number of metrics (currently six).

---

## See also

- [Anomaly reference](../analyzers/anomaly.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
