# File history analyzer reference

The file history analyzer tracks the **lifecycle of every file** through Git
history, recording which commits touched each file, which developers modified
it, and aggregating line statistics per contributor.

For the conceptual model — what the lifecycle data means and how hotspots and
churn are derived — see
[Understanding file history analysis](../explanation/file-history.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The file history analyzer has no additional configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

---

## Example output

=== "JSON"

    ```json
    {
      "file_churn": [
        {
          "path": "pkg/core/engine.go",
          "commit_count": 87,
          "contributor_count": 4,
          "total_lines_added": 3200,
          "total_lines_removed": 1800,
          "total_lines_changed": 950,
          "churn_score": 146.5
        },
        {
          "path": "cmd/main.go",
          "commit_count": 12,
          "contributor_count": 2,
          "total_lines_added": 280,
          "total_lines_removed": 45,
          "total_lines_changed": 30,
          "churn_score": 15.55
        }
      ],
      "hotspots": [
        {
          "path": "pkg/core/engine.go",
          "commit_count": 87,
          "churn_score": 146.5,
          "risk_level": "CRITICAL"
        }
      ],
      "file_contributors": [
        {
          "path": "pkg/core/engine.go",
          "contributors": [
            {"dev_id": 0, "added": 2200, "removed": 900, "changed": 600},
            {"dev_id": 1, "added": 800, "removed": 700, "changed": 250},
            {"dev_id": 2, "added": 150, "removed": 150, "changed": 80},
            {"dev_id": 3, "added": 50, "removed": 50, "changed": 20}
          ],
          "top_contributor_id": 0,
          "top_contributor_lines": 2800
        }
      ],
      "aggregate": {
        "total_files": 342,
        "total_commits": 1250,
        "total_contributors": 8,
        "avg_commits_per_file": 3.65,
        "avg_contributors_per_file": 1.8,
        "high_churn_files": 15
      }
    }
    ```

=== "YAML"

    ```yaml
    file_churn:
      - path: pkg/core/engine.go
        commit_count: 87
        contributor_count: 4
        churn_score: 146.5
    hotspots:
      - path: pkg/core/engine.go
        commit_count: 87
        risk_level: CRITICAL
    aggregate:
      total_files: 342
      total_commits: 1250
      avg_commits_per_file: 3.65
      high_churn_files: 15
    ```

---

## See also

- [Understanding file history analysis](../explanation/file-history.md) — the mental model, hotspots, churn, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
