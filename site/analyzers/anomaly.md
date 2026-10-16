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
| `TemporalAnomaly.Threshold` | `float` | `2.0` | Z-score threshold for anomaly detection (in standard deviations). A tick is flagged when any metric's absolute Z-score **strictly exceeds** it. Must be `> 0`. |
| `TemporalAnomaly.WindowSize` | `int` | `20` | Trailing window size in ticks for computing the rolling mean and standard deviation. Must be `>= 2`. |

```yaml
# .codefang.yml
history:
  anomaly:
    threshold: 2.0
    window_size: 20
```

The `--anomaly-window` CLI flag sets the window for a single run; an
out-of-range value is a named error before any walk
(`Error: invalid --anomaly-window: must be >= 2, got 1`).
`--anomaly-threshold` sets the Z-score threshold (default `2.0`, must be
`> 0`: `Error: invalid --anomaly-threshold: must be > 0, got 0`).
`--tick-size` sets the width of one tick (default 24 hours).

!!! tip "Tuning parameters"
    - **Lower threshold** (e.g., 1.5): More sensitive, flags more events. Good for quiet repositories.
    - **Higher threshold** (e.g., 3.0): Only flags extreme events. Good for noisy repositories with high variance.
    - **Smaller window** (e.g., 10): More reactive to recent changes. May flag events that are normal in a longer-term context.
    - **Larger window** (e.g., 50): Smoother baseline. Better for detecting truly unusual events against a long-term trend.

---

## Window semantics

The Z-score of tick *i* is computed against the ticks **before** it:
the previous `window_size` ticks, or every earlier tick while the history
is shorter than the window. The mean and the **population** standard
deviation of that window give `z = (value - mean) / stddev`.

- The first tick has no window and the second tick has one observation:
  both score `0` on every metric — a single observation has no spread, so
  it never flags anything.
- A window of two or more identical values (zero variance) whose next
  value differs scores the signed sentinel `±100`; an identical next value
  scores `0`.

Ticks only exist where commits are: a day without commits is not a tick.

---

## Per-tick metrics

| Metric | Definition |
|---|---|
| `files_changed` | Number of **distinct** files touched by the tick's commits (a file modified by three commits counts once; a deleted file counts and is named) |
| `lines_added` / `lines_removed` | Summed over the tick's commits (merge commits contribute no line stats) |
| `net_churn` | `lines_added - lines_removed` |
| `language_diversity` | Number of distinct languages among the touched files |
| `author_count` | Number of distinct authors |

---

## Example output

Ten quiet days followed by a busy day (`codefang run -a history/anomaly --format json .`):

=== "JSON"

    ```json
    {
      "anomalies": [
        {
          "tick": 10,
          "z_scores": {
            "net_churn": 179,
            "files_changed": 100,
            "lines_added": 189,
            "lines_removed": 100,
            "language_diversity": 0,
            "author_count": 0
          },
          "max_abs_z_score": 189,
          "metrics": {
            "files_changed": 11,
            "lines_added": 200,
            "lines_removed": 10,
            "net_churn": 190,
            "language_diversity": 1,
            "author_count": 1
          },
          "files": ["burst1.txt", "burst2.txt", "...", "q1.txt"]
        }
      ],
      "time_series": [
        {
          "tick": 0,
          "start_time": "2021-01-01T12:00:00Z",
          "end_time": "2021-01-01T12:00:00Z",
          "metrics": {
            "files_changed": 1,
            "lines_added": 10,
            "lines_removed": 0,
            "net_churn": 10,
            "language_diversity": 1,
            "author_count": 1
          },
          "is_anomaly": false,
          "churn_z_score": 0
        },
        {
          "tick": 10,
          "start_time": "2021-01-11T12:00:00Z",
          "end_time": "2021-01-11T12:20:00Z",
          "metrics": {
            "files_changed": 11,
            "lines_added": 200,
            "lines_removed": 10,
            "net_churn": 190,
            "language_diversity": 1,
            "author_count": 1
          },
          "is_anomaly": true,
          "churn_z_score": 179
        }
      ],
      "aggregate": {
        "total_ticks": 11,
        "total_anomalies": 1,
        "anomaly_rate": 9.090909090909092,
        "threshold": 2,
        "window_size": 20,
        "churn_mean": 27.272727272727273,
        "churn_stddev": 51.46771434438011,
        "files_mean": 1.9090909090909092,
        "files_stddev": 2.8747978728797806,
        "lang_diversity_mean": 1,
        "lang_diversity_stddev": 0,
        "author_count_mean": 1,
        "author_count_stddev": 0
      }
    }
    ```

    The quiet days alternate 10 and 12 added lines (mean 11, population
    standard deviation 1), so the busy day's net churn of 190 scores
    `(190 - 11) / 1 = 179`; every quiet day touched exactly one file, so
    the 11 distinct files score the zero-variance sentinel `100`.

=== "YAML"

    ```yaml
    codefang (v2):
      version: ...
      hash: ...
    TemporalAnomaly:
      anomalies:
        - tick: 10
          z_scores:
            net_churn: 179
            files_changed: 100
          max_abs_z_score: 189
          metrics:
            files_changed: 11
            lines_added: 200
            net_churn: 190
          files:
            - burst1.txt
      time_series:
        - tick: 0
          is_anomaly: false
          churn_z_score: 0
        - tick: 10
          is_anomaly: true
          churn_z_score: 179
      aggregate:
        total_ticks: 11
        total_anomalies: 1
        threshold: 2
        window_size: 20
    ```

---

## Output structure

| Section | Description |
|---|---|
| `anomalies` | The flagged ticks, sorted by severity (highest `max_abs_z_score` first, ties by tick): `tick`, the six `z_scores`, `max_abs_z_score`, the raw `metrics`, and `files` (the distinct files touched in the tick, sorted). Always a list — `[]` when nothing is flagged. |
| `time_series` | One entry per tick: `tick`, `start_time` / `end_time` (committer-time bounds, RFC3339 UTC), the raw `metrics`, `is_anomaly`, `churn_z_score`. The raw values appear once, under `metrics`. |
| `aggregate` | `total_ticks`, `total_anomalies`, `anomaly_rate` (percent), the `threshold` and `window_size` used, and the global mean / population standard deviation of net churn, files changed, language diversity and author count over every tick. |

`bin` carries the json value in the CFB1 envelope; `yaml` renders it under
`TemporalAnomaly:`. The per-commit `ndjson` records carry each commit's
`files_changed`, `lines_added`, `lines_removed`, `net_churn`, `files`
(a delete names the deleted file), `languages` and `author_id`.

---

## See also

- [Understanding temporal anomaly detection](../explanation/anomaly.md) — the mental model, Z-score analysis, architecture, use cases, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
