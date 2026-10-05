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

For each time tick, the analyzer computes Z-scores for six metrics using a trailing window:

| Metric | Description |
|---|---|
| **Net churn** | `lines_added - lines_removed` |
| **Files changed** | Number of **distinct** files touched in the tick (a file modified by several commits counts once; a deleted file counts) |
| **Lines added** | Total lines added across all commits in the tick |
| **Lines removed** | Total lines removed across all commits in the tick |
| **Language diversity** | Number of distinct programming languages modified |
| **Author count** | Number of distinct developers active in the tick |

A Z-score measures how many standard deviations a value is from the rolling mean. When any metric's absolute Z-score strictly exceeds the threshold, the tick is flagged as anomalous.

!!! info "Z-score interpretation"
    - **|Z| < 2.0**: Within normal variation
    - **|Z| 2.0 - 3.0**: Unusual, worth investigating
    - **|Z| > 3.0**: Highly anomalous, likely a significant event

### Trailing window

The Z-score of a tick is computed against the previous `window_size` ticks (default 20) — the ticks that exist when the history is shorter — using the mean and the population standard deviation of that window. The "normal" baseline therefore adapts to gradual trends, and only sudden deviations are flagged.

A window needs at least two observations: the first tick has no window and the second has a single observation, so both score `0`. When a window of two or more identical values is followed by a different value, there is no variance to scale by and the signed sentinel `±100` is assigned.

### Anomaly records

Each detected anomaly includes:

- **Tick index**: When it occurred
- **Z-scores**: Per-metric Z-scores showing which dimension is anomalous
- **Max absolute Z-score**: The highest Z-score across all metrics (used for severity ranking)
- **Raw metrics**: The actual values for that tick
- **Files**: The distinct files changed in the anomalous tick

Anomalies are sorted by severity (highest absolute Z-score first, ties by tick). When nothing is flagged the list is empty, never absent.

---

## Architecture

The math lives in the `cf-anomaly` crate; the history walk in `cf-commands` supplies the per-commit inputs:

1. **Per-commit product**: for every commit the walk takes the filtered tree diff against the first parent, the files it names (a delete names the deleted file), the line stats (skipped for merges) and the language of every touched blob — a pure function of the commit, computed in parallel across cores.
2. **Chronological reduce**: the shared history core stamps every commit with its identity and tick in oldest-first order; `cf_anomaly::aggregate_commits_to_ticks` merges the commits of a tick (lines add up; files, languages and authors merge as sets).
3. **Z-score detection**: `cf_anomaly::detect_anomalies_from_ticks` runs the trailing-window Z-scores on the six dimensions with the validated `Config` (`--anomaly-window`; threshold 2.0).
4. **Serialization**: `compute_all_metrics` produces one value tree (anomalies, time series, aggregate) that json, yaml and bin encode; the plot path adds the cross-analyzer enrichment over the co-selected analyzers' time series.

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

- **Cold start**: The first ticks have little history: the first two score `0` on every metric, and the following ones are scored against a window shorter than `window_size`. Anomalies early in a history are less reliable.
- **Gradual drift**: Z-scores detect sudden deviations, not gradual trends. A slow but steady increase in churn will update the rolling mean and never trigger an anomaly.
- **Zero-variance sentinel**: When the window (two or more observations) has zero standard deviation and the current value differs, a sentinel Z-score of `±100` is assigned. This ensures detection but may inflate severity rankings.
- **Tick granularity**: Metrics are aggregated per tick (default 24 hours, `--tick-size`). Sub-tick anomalies are not detected, and days without commits are not ticks.
- **Not causal**: The analyzer detects statistical anomalies, not root causes. Investigation is required to determine whether an anomaly represents a real problem.
- **Multi-dimensional**: An anomaly is flagged when *any* single metric exceeds the threshold. This means false positive rates increase with the number of metrics (currently six).

---

## See also

- [Anomaly reference](../analyzers/anomaly.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
