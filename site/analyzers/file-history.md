# File history analyzer reference

The file history analyzer tracks the **lifecycle of every file** through Git
history, recording which commits touched each file, which developers modified
it, aggregating line statistics per contributor, and classifying the changed
files by category.

For the conceptual model — what the lifecycle data means, how hotspots, churn
and the composition are derived — see
[Understanding file history analysis](../explanation/file-history.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The file history analyzer has no additional configuration options. It uses
the shared history core: the commit window (`--limit`, `--since`, `--head`,
`--first-parent`), the identity detector (`--exact-signatures`,
`--people-dict`, `--anonymize`), the tick size (`--tick-size`) and the change
filter (`--languages`, vendor / generated policy) documented in the
[CLI reference](../guide/cli-reference.md).

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

---

## Output

### JSON / YAML

Six top-level sections. Every per-file section lists only the files present
in the **newest commit of the window** (the window head); a file deleted
inside the window is absent.

| Section | Field | Meaning |
|---|---|---|
| `file_churn[]` | `commit_count` | Commits in the window that touched the file, following renames |
| `file_churn[]` | `contributor_count` | Distinct developers with line stats on the file |
| `file_churn[]` | `total_lines_added` / `_removed` / `_changed` | Sum of the per-commit line stats |
| `file_churn[]` | `churn_score` | `commit_count + (added + removed + changed) / 100` |
| `file_contributors[]` | `contributors[]` | Per developer id: `added`, `removed`, `changed` |
| `file_contributors[]` | `top_contributor_id` / `top_contributor_lines` | Developer with the most `added + changed` lines |
| `hotspots[]` | `risk_level` | `CRITICAL` (>= 50 commits), `HIGH` (>= 30), `MEDIUM` (>= 15) |
| `aggregate` | `total_files` / `total_commits` / `total_contributors` | Over the files at the window head (`total_commits` sums their commit counts) |
| `composition` | `breakdown` | Distinct files changed in the window, per category (non-zero categories only) |
| `composition` | `percentages` | The same as a percentage of the distinct files changed (every category) |
| `composition_ts[]` | `tick`, `start_time`, `end_time`, `breakdown` | Distinct files changed in that tick, per category, with the tick's first and last committer time (RFC3339 UTC) |

Categories are `source`, `documentation`, `configuration`, `vendor`,
`generated`, `dotfile`, `image`, `binary`, decided by the shared classifier
over the path **and** the blob content (a NUL-containing blob is `binary`, a
generated-code marker makes `generated`, whatever the path says). A file
counts once per scope under its latest classification; a renamed file keeps
one identity.

### Text

A summary (files, commits, contributors, average commits per file, high churn
files), the **File Composition** table (one row per non-zero category with
its share of the distinct files changed) and the **Most Modified Files** list
(top 10 by commit count).

### Plot (HTML)

1. **Most Modified Files** — bar chart of the 20 files with the most commits
2. **File Composition Over Time** — stacked area of the distinct files changed per tick, by category

### NDJSON / timeseries

`--format ndjson` emits one record per walked commit with its
`PathActions` (`Path`, `Action` 0 insert / 1 delete / 2 modify, `FromPath` /
`ToPath` for a rename), its `LineStatUpdates` (`null` for a merge) and the
per-commit `Composition` counts. `--format timeseries` reports, per commit and
under its tick, `files_touched`, `lines_added` / `lines_removed` /
`lines_changed` and the `inserts` / `deletes` / `modifies` counts.

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
      "hotspots": [
        {
          "path": "pkg/core/engine.go",
          "commit_count": 87,
          "churn_score": 146.5,
          "risk_level": "CRITICAL"
        }
      ],
      "aggregate": {
        "total_files": 342,
        "total_commits": 1250,
        "total_contributors": 8,
        "avg_commits_per_file": 3.65,
        "avg_contributors_per_file": 1.8,
        "high_churn_files": 15
      },
      "composition": {
        "breakdown": {"configuration": 12, "documentation": 30, "source": 300},
        "percentages": {
          "binary": 0, "configuration": 3.5, "documentation": 8.8, "dotfile": 0,
          "generated": 0, "image": 0, "source": 87.7, "vendor": 0
        }
      },
      "composition_ts": [
        {
          "tick": 0,
          "start_time": "2021-01-01T09:12:00Z",
          "end_time": "2021-01-01T18:40:00Z",
          "breakdown": {"documentation": 1, "source": 4}
        }
      ]
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
    composition:
      breakdown:
        source: 300
    composition_ts:
      - tick: 0
        start_time: "2021-01-01T09:12:00Z"
        end_time: "2021-01-01T18:40:00Z"
        breakdown:
          source: 4
    ```

---

## See also

- [Understanding file history analysis](../explanation/file-history.md) — the mental model, hotspots, churn, composition and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
