# Anomaly Analyzer

The anomaly analyzer performs **temporal anomaly detection** using Z-score analysis over a sliding window of per-tick commit metrics. It detects sudden quality degradation -- unusual spikes in churn, file changes, or other metrics that deviate significantly from the historical baseline.

---

## Quick Start

```bash
codefang run -a history/anomaly .
```

With custom threshold and window:

```bash
codefang run -a history/anomaly \
  --anomaly-threshold 2.5 \
  --anomaly-window 30 \
  .
```

---

## What It Measures

### Z-Score Analysis

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

### Sliding Window

The Z-score is computed against a trailing window of the previous N ticks (default 20). This means the "normal" baseline adapts to gradual trends, and only sudden deviations are flagged.

### Anomaly Records

Each detected anomaly includes:

- **Tick index**: When it occurred
- **Z-scores**: Per-metric Z-scores showing which dimension is anomalous
- **Max absolute Z-score**: The highest Z-score across all metrics (used for severity ranking)
- **Raw metrics**: The actual values for that tick
- **Files**: List of files changed in the anomalous tick

Anomalies are sorted by severity (highest absolute Z-score first).

---

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `TemporalAnomaly.Threshold` | `float` | `2.0` | Z-score threshold for anomaly detection (in standard deviations). Ticks where any metric exceeds this threshold are flagged. |
| `TemporalAnomaly.WindowSize` | `int` | `20` | Sliding window size in ticks for computing rolling mean and standard deviation. Minimum value: 2. |

```yaml
# .codefang.yml
history:
  anomaly:
    threshold: 2.0
    window_size: 20
```

!!! tip "Tuning parameters"
    - **Lower threshold** (e.g., 1.5): More sensitive, flags more events. Good for quiet repositories.
    - **Higher threshold** (e.g., 3.0): Only flags extreme events. Good for noisy repositories with high variance.
    - **Smaller window** (e.g., 10): More reactive to recent changes. May flag events that are normal in a longer-term context.
    - **Larger window** (e.g., 50): Smoother baseline. Better for detecting truly unusual events against a long-term trend.

---

## Example Output

=== "JSON"

    ```json
    {
      "anomalies": [
        {
          "tick": 45,
          "z_scores": {
            "net_churn": 4.2,
            "files_changed": 3.8,
            "lines_added": 3.5,
            "lines_removed": 1.2,
            "language_diversity": 0.5,
            "author_count": 0.3
          },
          "max_abs_z_score": 4.2,
          "metrics": {
            "files_changed": 142,
            "lines_added": 8500,
            "lines_removed": 1200,
            "net_churn": 7300,
            "language_diversity": 3,
            "author_count": 2
          },
          "files": ["pkg/core/engine.go", "pkg/core/parser.go", "..."]
        }
      ],
      "time_series": [
        {
          "tick": 0,
          "metrics": {
            "files_changed": 12,
            "lines_added": 450,
            "lines_removed": 120,
            "net_churn": 330,
            "language_diversity": 2,
            "author_count": 3
          },
          "is_anomaly": false,
          "churn_z_score": 0.0
        },
        {
          "tick": 45,
          "metrics": {
            "files_changed": 142,
            "lines_added": 8500,
            "lines_removed": 1200,
            "net_churn": 7300,
            "language_diversity": 3,
            "author_count": 2
          },
          "is_anomaly": true,
          "churn_z_score": 4.2
        }
      ],
      "aggregate": {
        "total_ticks": 120,
        "total_anomalies": 5,
        "anomaly_rate": 4.17,
        "threshold": 2.0,
        "window_size": 20,
        "churn_mean": 340.5,
        "churn_stddev": 180.2,
        "files_mean": 15.3,
        "files_stddev": 8.7,
        "lang_diversity_mean": 2.1,
        "lang_diversity_stddev": 0.8,
        "author_count_mean": 2.8,
        "author_count_stddev": 1.2
      }
    }
    ```

=== "YAML"

    ```yaml
    anomalies:
      - tick: 45
        max_abs_z_score: 4.2
        z_scores:
          net_churn: 4.2
          files_changed: 3.8
        metrics:
          files_changed: 142
          lines_added: 8500
          net_churn: 7300
    aggregate:
      total_ticks: 120
      total_anomalies: 5
      anomaly_rate: 4.17
      threshold: 2.0
      window_size: 20
    time_series:
      - tick: 0
        is_anomaly: false
        churn_z_score: 0.0
      - tick: 45
        is_anomaly: true
        churn_z_score: 4.2
    ```

---

## Output Structure

The anomaly analyzer produces three main output sections:

| Section | Type | Description |
|---|---|---|
| `anomalies` | `risk` | List of detected anomalies, sorted by severity (highest Z-score first) |
| `time_series` | `time_series` | Per-tick metrics with anomaly annotations and Z-scores |
| `aggregate` | `aggregate` | Summary statistics: total ticks, anomaly rate, global mean/stddev for each metric |

---

## Use Cases

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
- **Tick granularity**: Metrics are aggregated per tick (default 24 hours). Sub-tick anomalies are not detected.
- **Not causal**: The analyzer detects statistical anomalies, not root causes. Investigation is required to determine whether an anomaly represents a real problem.
- **Multi-dimensional**: An anomaly is flagged when *any* single metric exceeds the threshold. This means false positive rates increase with the number of metrics (currently six).
