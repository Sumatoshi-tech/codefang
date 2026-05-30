# Developers analyzer reference

The developers analyzer computes **per-developer contribution statistics**
across Git history, including commit counts, line changes, language breakdown,
bus factor risk, and activity time series.

For the conceptual model — what each statistic means and how the bus factor is
derived — see [Understanding developer analysis](../explanation/developers.md).
To run it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

| Option | Type | Default | Description |
|---|---|---|---|
| `Devs.ConsiderEmptyCommits` | `bool` | `false` | Include empty commits (e.g., trivial merges) in commit counts |
| `Devs.Anonymize` | `bool` | `false` | Replace developer names with pseudonyms (Developer-A, Developer-B, etc.) |

```yaml
# .codefang.yml
history:
  devs:
    consider_empty_commits: false
    anonymize: false
```

---

## Example output

=== "JSON"

    ```json
    {
      "developers": [
        {
          "id": 0,
          "name": "alice",
          "email": "alice@example.com",
          "commits": 342,
          "lines_added": 28500,
          "lines_removed": 12300,
          "lines_changed": 8400,
          "net_lines": 16200,
          "languages": [
            {"language": "Go", "added": 22000, "removed": 9800, "changed": 6200},
            {"language": "Python", "added": 6500, "removed": 2500, "changed": 2200}
          ],
          "first_tick": 0,
          "last_tick": 120,
          "active_ticks": 85
        }
      ],
      "languages": [
        {
          "name": "Go",
          "total_lines": 45000,
          "total_contribution": 67800,
          "contributors": {"0": 54600, "1": 13200}
        }
      ],
      "busfactor": [
        {
          "language": "Python",
          "bus_factor": 1,
          "total_contributors": 2,
          "primary_dev_id": 0,
          "primary_dev_name": "alice",
          "primary_dev_email": "alice@example.com",
          "primary_percentage": 67.1,
          "secondary_dev_id": 1,
          "secondary_dev_name": "bob",
          "secondary_dev_email": "bob@example.com",
          "secondary_percentage": 32.9,
          "risk_level": "MEDIUM"
        }
      ],
      "activity": [
        {
          "tick": 0,
          "start_time": "2024-01-15T10:30:00Z",
          "end_time": "2024-01-16T08:45:00Z",
          "total_commits": 5,
          "by_developer": [
            {"dev_id": 0, "commits": 3},
            {"dev_id": 1, "commits": 2}
          ]
        }
      ],
      "churn": [
        {
          "tick": 0,
          "start_time": "2024-01-15T10:30:00Z",
          "end_time": "2024-01-16T08:45:00Z",
          "lines_added": 450,
          "lines_removed": 120,
          "net_change": 330
        }
      ],
      "aggregate": {
        "total_commits": 850,
        "total_developers": 5,
        "active_developers": 3,
        "analysis_period_ticks": 120,
        "project_bus_factor": 2,
        "total_languages": 4
      }
    }
    ```

    **Key fields for analytics:**

    - `developers[].email` — split from previously pipe-delimited name
    - `developers[].languages` — flattened from map to sorted array
    - `activity[].by_developer` — flattened from `map[int]int` to `[{dev_id, commits}]` array
    - `activity[].start_time` / `end_time` — RFC 3339 tick boundaries
    - `churn[].start_time` / `end_time` — RFC 3339 tick boundaries
    - `busfactor[].primary_dev_email` / `secondary_dev_email` — split identity fields

=== "YAML"

    ```yaml
    aggregate:
      total_commits: 850
      total_lines_added: 95000
      total_lines_removed: 42000
      total_developers: 5
      active_developers: 3
      project_bus_factor: 2
      total_languages: 4
    developers:
      - name: alice
        commits: 342
        lines_added: 28500
        lines_removed: 12300
        net_lines: 16200
    languages:
      - name: Go
        total_lines: 45000
        total_contribution: 67800
    busfactor:
      - language: Python
        bus_factor: 1
        total_contributors: 2
        primary_dev_name: alice
        primary_percentage: 67.1
        risk_level: MEDIUM
    ```

---

## See also

- [Understanding developer analysis](../explanation/developers.md) — the mental model, bus factor methodology, architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
