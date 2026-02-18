# Burndown Analyzer

The burndown analyzer tracks **code survival over time** by following every line of code through Git history. It produces burndown charts showing how code written at different points in time persists, is modified, or is deleted. Optionally, it tracks per-file and per-developer breakdowns.

---

## Quick Start

```bash
codefang run -a history/burndown .
```

With per-file and per-developer tracking:

```bash
codefang run -a history/burndown \
  --burndown-files \
  --burndown-people \
  .
```

---

## What It Measures

### Global Code Survival

A time-series matrix where each row is a sampling point and each column is an age band. The value at `[sample][band]` is the number of lines that were last edited during that band and still survive at that sample point.

This matrix produces the classic **burndown chart**: stacked area plots showing how much code from each era remains.

### Per-File Burndown

When `--burndown-files` is enabled, the analyzer produces a separate survival matrix for each file, enabling file-level burndown charts.

### Per-Developer Burndown

When `--burndown-people` is enabled, the analyzer tracks which developer last edited each line. This reveals:

- **Developer survival rates**: How much of each developer's code persists
- **Interaction matrix**: Which developers modify each other's code

!!! warning "Memory usage"
    Per-file and per-developer tracking significantly increases memory usage. For repositories with more than 100k commits, consider enabling hibernation (on by default).

---

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `Burndown.Granularity` | `int` | `30` | Number of time ticks per age band. Controls the width of each band in the burndown chart. |
| `Burndown.Sampling` | `int` | `30` | How frequently to record a snapshot (in ticks). Lower values give more data points but increase memory. |
| `Burndown.TrackFiles` | `bool` | `false` | Record per-file burndown statistics. |
| `Burndown.TrackPeople` | `bool` | `false` | Record per-developer burndown and interaction matrix. |
| `Burndown.HibernationThreshold` | `int` | `1000` | Minimum node count in a branch before memory compression triggers. |
| `Burndown.HibernationOnDisk` | `bool` | `true` | Save hibernated state to disk to reduce memory pressure. |
| `Burndown.HibernationDirectory` | `string` | `""` | Temporary directory for hibernated state files. Uses system temp if empty. |
| `Burndown.Debug` | `bool` | `false` | Validate internal tree structures at each step (slow; for development only). |
| `Burndown.Goroutines` | `int` | `NumCPU` | Number of goroutines for parallel per-file processing within a commit. |

Set options via the configuration file or CLI flags:

```yaml
# .codefang.yml
history:
  burndown:
    granularity: 30
    sampling: 30
    track_files: true
    track_people: true
    hibernation_threshold: 1000
    hibernation_to_disk: true
    goroutines: 8
```

---

## Example Output

=== "JSON"

    ```json
    {
      "aggregate": {
        "total_current_lines": 52340,
        "total_peak_lines": 78200,
        "overall_survival_rate": 0.669,
        "analysis_period_days": 730,
        "num_bands": 25,
        "num_samples": 25,
        "tracked_files": 342,
        "tracked_developers": 12
      },
      "global_survival": [
        {
          "sample_index": 0,
          "total_lines": 1200,
          "survival_rate": 0.015,
          "band_breakdown": [1200, 0, 0]
        },
        {
          "sample_index": 24,
          "total_lines": 52340,
          "survival_rate": 0.669,
          "band_breakdown": [320, 890, 1450, 2100, 4800, "..."]
        }
      ],
      "file_survival": [
        {
          "path": "pkg/core/engine.go",
          "current_lines": 450,
          "top_owner_name": "alice",
          "top_owner_percentage": 72.3
        }
      ],
      "developer_survival": [
        {
          "name": "alice",
          "current_lines": 18200,
          "peak_lines": 25000,
          "survival_rate": 0.728
        }
      ],
      "interactions": [
        {
          "author_name": "alice",
          "modifier_name": "bob",
          "lines_modified": 342,
          "is_self_modify": false
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    aggregate:
      total_current_lines: 52340
      total_peak_lines: 78200
      overall_survival_rate: 0.669
      analysis_period_days: 730
    global_survival:
      - sample_index: 0
        total_lines: 1200
        survival_rate: 0.015
      - sample_index: 24
        total_lines: 52340
        survival_rate: 0.669
    ```

---

## Use Cases

- **Project health monitoring**: Track the overall code survival rate. A declining rate may indicate churn or instability.
- **Developer contribution analysis**: Understand whose code persists and who rewrites existing code.
- **Code age visualization**: Generate burndown charts showing how much ancient code remains in the codebase.
- **Refactoring impact**: Measure how much code a refactoring effort actually replaced.
- **Team dynamics**: The interaction matrix reveals collaboration patterns -- who reviews and modifies whose code.

---

## Limitations

- **Sequential processing**: Burndown tracks cumulative per-line state across all commits and must process commits sequentially. It cannot be parallelized across commits (though per-file processing within a commit is parallelized via goroutines).
- **Memory intensive**: Every line in every file is tracked throughout history. Large repositories (100k+ commits, 10k+ files) can require several GB of RAM. Use hibernation options to manage memory.
- **Binary files excluded**: Binary files are automatically skipped since they cannot be meaningfully diff'd line-by-line.
- **Rename tracking**: File renames are tracked using Git's rename detection. If Git does not detect a rename (e.g., content changed significantly), the file appears as a deletion + insertion.
- **Tick resolution**: The default 24-hour tick means that all commits within the same day share one tick. Sub-day granularity is not supported.
