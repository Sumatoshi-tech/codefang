# File History Analyzer

The file history analyzer tracks the **lifecycle of every file** through Git history, recording which commits touched each file, which developers modified it, and aggregating line statistics per contributor. It supports rename detection to maintain continuity across file moves.

---

## Quick Start

```bash
codefang run -a history/file-history .
```

---

## What It Measures

### Per-File Commit History

For each file present in the repository at HEAD, the analyzer records the ordered list of commits that created, modified, or deleted it.

### Per-File Contributor Breakdown

For each file, a map of developer IDs to line statistics (added, removed, changed). This shows who contributed what to each file.

### Code Hotspots

Files with high commit counts are flagged as hotspots with risk levels:

!!! warning "Hotspot risk levels"
    - **CRITICAL**: >= 50 commits
    - **HIGH**: >= 30 commits
    - **MEDIUM**: >= 15 commits
    - Files below 15 commits are not flagged

### File Churn

A composite score combining commit frequency and line change volume. High-churn files may indicate instability or areas of active development.

### Rename Support

When Git detects a file rename (e.g., `old/path.go` to `new/path.go`), the analyzer transfers the full history from the old path to the new path, maintaining a continuous record.

---

## Configuration Options

The file history analyzer has no additional configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

---

## Example Output

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
          "contributors": {
            "0": {"added": 2200, "removed": 900, "changed": 600},
            "1": {"added": 800, "removed": 700, "changed": 250},
            "2": {"added": 150, "removed": 150, "changed": 80},
            "3": {"added": 50, "removed": 50, "changed": 20}
          },
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

## Use Cases

- **Hotspot identification**: Find the files that change most often. These are the highest-risk files for bugs and the best candidates for extra test coverage and code review scrutiny.
- **Code ownership mapping**: Determine who is the primary contributor for each file to establish code ownership.
- **Onboarding guides**: New team members can see which files each developer owns to know who to ask about specific code.
- **Refactoring ROI**: Identify files with both high churn and many contributors -- refactoring these produces the largest payoff.
- **Risk assessment**: Files with a single contributor and high commit counts are both hot and concentrated -- a bus factor risk.

---

## Limitations

- **HEAD-only output**: Only files present at HEAD are included in the final output. Files that were deleted before the last commit are tracked during analysis but excluded from results.
- **Merge handling**: Merge commits are processed only once to avoid double-counting. Non-merge context changes in merge commits are skipped.
- **Rename detection**: Depends on Git's rename detection heuristics. If a file is both renamed and heavily modified in the same commit, Git may not detect the rename.
- **Churn score formula**: The churn score is `commit_count + (added + removed + changed) / 100`. This is a simple heuristic, not a rigorous statistical measure.
- **No time-series**: Unlike the burndown analyzer, file history does not produce time-series data. It provides aggregate per-file statistics only.
