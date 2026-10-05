# Code quality analyzer reference

The code quality analyzer tracks **complexity, Halstead metrics, comment
quality, and cohesion** across Git history. For every commit it runs the four
static analyzers on the UAST of each changed file and records one sample per
file; the samples are bucketed per tick.

For the conceptual model — which metrics it tracks and how it aggregates them
per tick — see [Understanding code quality over
time](../explanation/quality.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Configuration options

The quality analyzer has no analyzer-specific options. It honors the shared
history flags (`--limit`, `--since`, `--head`, `--tick-size`, `--languages`,
`--max-changes-per-commit`, the vendor / generated policy) documented in the
[CLI reference](../guide/cli-reference.md).

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses the shared history flags and the UAST pipeline's language support |

---

## Which files count

For each commit, the analyzer looks at the Insert and Modify changes that
survive the shared change filter and parses the **after** version of each
file:

- A file whose UAST parse **succeeds** contributes exactly one sample. The
  numbers are the static analyzers' numbers for that file
  (`static/complexity`, `static/halstead`, `static/comments`,
  `static/cohesion`, each fed the file bytes), with one difference: function
  and comment discovery is capped at depth 10 below the file root.
- A file whose parse **fails**, an unsupported language, a binary blob, or a
  blob above 256 KiB contributes nothing — it is not counted in
  `files_analyzed`.
- A parsed file **without functions** contributes its complexity, Halstead
  and comment scalars (the function-based ones are `0`) but **no cohesion
  sample**: the static cohesion analyzer reports "no data" for such a file,
  and so does quality.
- Deleted files contribute nothing.

A tick (default: one day of committer time) in which no file was analyzed —
for example, a commit touching only text files — has **no entry** in the time
series and does not take part in the aggregate.

---

## Per-tick statistics

Every `time_series` entry carries the statistics of the files analyzed in
that tick:

| Field | Definition |
|---|---|
| `complexity_mean` / `complexity_median` / `complexity_p95` / `complexity_max` | over each file's summed cyclomatic complexity (`total_complexity`) |
| `halstead_vol_mean` / `halstead_vol_median` / `halstead_vol_p95` / `halstead_vol_sum` | over each file's Halstead volume |
| `delivered_bugs_sum` | sum of each file's Halstead delivered bugs |
| `comment_score_mean` / `comment_score_min` | over each file's good-comment ratio (`overall_score`) |
| `doc_coverage_mean` | mean of each file's documented-declaration ratio |
| `cohesion_mean` / `cohesion_min` | over the cohesion scores of the files **with functions**; `0` when no file in the tick has one (no data) |
| `files_analyzed` | number of parsed files |
| `total_functions` | sum of each file's function count |
| `max_complexity` | highest single-function cyclomatic complexity |

The P95 is the linearly interpolated 95th percentile.

### Aggregate

| Field | Definition |
|---|---|
| `total_ticks` | ticks with at least one analyzed file |
| `total_files_analyzed` | sum of `files_analyzed` over those ticks |
| `complexity_median_mean` / `complexity_p95_mean` / `halstead_vol_median_mean` / `comment_score_mean_mean` | mean of the per-tick value |
| `total_delivered_bugs` | sum of the per-tick `delivered_bugs_sum` |
| `min_comment_score` | minimum of the per-tick `comment_score_min` |
| `cohesion_mean_mean` / `min_cohesion` | mean of the per-tick `cohesion_mean` / minimum of the per-tick `cohesion_min`, over the ticks that hold a cohesion sample (`0` when none does) |

Because every mean and minimum is taken over the same set of ticks,
`min_cohesion <= cohesion_mean_mean` and
`min_comment_score <= comment_score_mean_mean` always hold.

---

## Output formats

| Format | Flag | Description |
|---|---|---|
| JSON | `--format json` | `{"time_series": [...], "aggregate": {...}}` |
| YAML | `--format yaml` | Same structure as JSON |
| NDJSON | `--format ndjson` | One record per commit with that commit's per-file sample vectors |
| Time series | `--format timeseries` | Per-commit summary (`complexity_median`, `cognitive_median`, `max_complexity`, `functions`, `halstead_vol_median`, `halstead_effort_median`, `delivered_bugs_sum`, `comment_score_min`, `doc_coverage_mean`, `cohesion_min`, `files_analyzed`) |
| Plot | `--format plot` | HTML page with complexity and Halstead charts plus summary stats |
| Binary | `--format bin` | Compact binary envelope for programmatic consumption |

### JSON output

```json
{
  "time_series": [
    {
      "tick": 0,
      "start_time": "2021-01-01T12:00:00Z",
      "end_time": "2021-01-01T12:00:00Z",
      "stats": {
        "complexity_mean": 2, "complexity_median": 2, "complexity_p95": 2, "complexity_max": 2,
        "halstead_vol_mean": 25.85, "halstead_vol_median": 25.85, "halstead_vol_p95": 25.85, "halstead_vol_sum": 25.85,
        "delivered_bugs_sum": 0.0086,
        "comment_score_mean": 1, "comment_score_min": 1, "doc_coverage_mean": 1,
        "cohesion_mean": 1, "cohesion_min": 1,
        "files_analyzed": 1, "total_functions": 1, "max_complexity": 2
      }
    }
  ],
  "aggregate": {
    "total_ticks": 1,
    "total_files_analyzed": 1,
    "complexity_median_mean": 2,
    "complexity_p95_mean": 2,
    "halstead_vol_median_mean": 25.85,
    "total_delivered_bugs": 0.0086,
    "comment_score_mean_mean": 1,
    "min_comment_score": 1,
    "cohesion_mean_mean": 1,
    "min_cohesion": 1
  }
}
```

---

## See also

- [Understanding code quality over time](../explanation/quality.md) — the mental model, per-file metrics, and aggregation.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
