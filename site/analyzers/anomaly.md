# Anomaly analyzer reference

The anomaly analyzer performs **temporal anomaly detection** using Z-score
analysis over a sliding window of per-tick commit metrics.

For the conceptual model — how Z-score detection works and which metrics it
tracks — see
[Understanding temporal anomaly detection](../explanation/anomaly.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

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

The `--anomaly-threshold` and `--anomaly-window` CLI flags set these options for a single run.

!!! tip "Tuning parameters"
    - **Lower threshold** (e.g., 1.5): More sensitive, flags more events. Good for quiet repositories.
    - **Higher threshold** (e.g., 3.0): Only flags extreme events. Good for noisy repositories with high variance.
    - **Smaller window** (e.g., 10): More reactive to recent changes. May flag events that are normal in a longer-term context.
    - **Larger window** (e.g., 50): Smoother baseline. Better for detecting truly unusual events against a long-term trend.

---

## Example output

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

## Output structure

The anomaly analyzer produces three main output sections:

| Section | Type | Description |
|---|---|---|
| `anomalies` | `risk` | List of detected anomalies, sorted by severity (highest Z-score first) |
| `time_series` | `time_series` | Per-tick metrics with anomaly annotations and Z-scores |
| `aggregate` | `aggregate` | Summary statistics: total ticks, anomaly rate, global mean/stddev for each metric |

---

## See also

- [Understanding temporal anomaly detection](../explanation/anomaly.md) — the mental model, Z-score analysis, architecture, use cases, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
