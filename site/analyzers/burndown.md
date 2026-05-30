# Burndown analyzer reference

The burndown analyzer tracks **code survival over time** by following every
line of code through Git history. Optionally, it tracks per-file and
per-developer breakdowns.

For the conceptual model — what code survival means and how the line-tracking
algorithm works — see [Understanding code burndown](../explanation/burndown.md).
To run it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

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

## Output formats

### Text (terminal)

The `--format text` output produces a concise terminal summary:

```text
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Burndown: project-name                730d    ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

  Summary
  ───────────────────────────────────────────────
  Current Lines    52,340
  Peak Lines       78,200
  Survival Rate    [████████████████░░░░] 66.9%

  Code Age Distribution
  ───────────────────────────────────────────────
  < 1 mo        ████████████████████  34%  (17,796)
  1–3 mo        ████████████░░░░░░░░  24%  (12,562)
  3–6 mo        ████████░░░░░░░░░░░░  18%  ( 9,421)
  6–12 mo       ██████░░░░░░░░░░░░░░  14%  ( 7,328)
  > 12 mo       ████░░░░░░░░░░░░░░░░  10%  ( 5,233)

  Top Developers (by surviving lines)
  ───────────────────────────────────────────────
  alice           18,200  [█████████████████░░░] 72.8%
  bob              8,400  [█████████████░░░░░░░] 65.1%
  charlie          5,100  [████████░░░░░░░░░░░░] 48.2%
```

The text output shows at most 5 age bands and 5 developers (`mo` = months). Survival rates are color-coded: green (>70%), yellow (50-70%), red (<50%).

### Plot (HTML)

The `--format plot` output generates an interactive HTML page with:

- **Summary section**: Key statistics (current lines, peak lines, survival rate, analysis period, developer/file counts)
- **Burndown chart**: Stacked area chart showing code survival by age band or by year (for projects spanning 2+ years)
- **Interpretation hints**: Guidance on reading the chart

### JSON / YAML

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
          "ownership": {"0": 325, "1": 125},
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

## See also

- [Understanding code burndown](../explanation/burndown.md) — the mental model, algorithm, use cases, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
