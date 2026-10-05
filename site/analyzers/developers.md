# Developers analyzer reference

The developers analyzer computes **per-developer contribution statistics**
across Git history: commit counts, lines added / removed / changed with a
per-language breakdown, bus factor risk, and activity time series.

For the conceptual model — what each statistic means and how the bus factor
is derived — see
[Understanding developer analysis](../explanation/developers.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Options

The analyzer reads its options from the `codefang run` flags. The shared
history flags (`--head`, `--limit`, `--since`, `--first-parent`,
`--tick-size`, `--max-changes-per-commit`, `--languages`, `--people-dict`,
`--exact-signatures`, `--anonymize`) apply as documented in the
[CLI reference](../guide/cli-reference.md).

| Flag | Type | Default | Description |
|---|---|---|---|
| `--empty-commits` | `bool` | `false` | Count commits whose filtered diff is empty (trivial merges, `--allow-empty` commits, commits touching only excluded paths) as commits with zero lines. |
| `--anonymize` | `bool` | `false` | Replace developer names with pseudonyms (`Developer-A`, `Developer-B`, …). |

!!! note "Configuration file"
    The `history.devs.*` keys of `.codefang.yaml` describe the same
    options in the configuration schema, but `codefang run` applies the
    flags above only.

---

## Example output

From a twelve-commit fixture with three developers
(`bins/codefang/tests/devs_e2e.rs`); the `activity` and `churn` lists are
cut to their first tick.

=== "JSON"

    ```json
    {
      "aggregate": {
        "total_commits": 10,
        "total_lines_added": 30,
        "total_lines_removed": 15,
        "total_developers": 3,
        "active_developers": 1,
        "estimated_total_developers": 3,
        "estimated_active_developers": 1,
        "analysis_period_ticks": 310,
        "project_bus_factor": 2,
        "total_languages": 2
      },
      "developers": [
        {
          "id": 0,
          "name": "alice",
          "email": "alice@example.com",
          "commits": 6,
          "lines_added": 16,
          "lines_removed": 0,
          "lines_changed": 0,
          "net_lines": 16,
          "languages": [
            {"language": "Go", "added": 11, "removed": 0, "changed": 0},
            {"language": "Python", "added": 5, "removed": 0, "changed": 0}
          ],
          "first_tick": 0,
          "last_tick": 310,
          "active_ticks": 6
        },
        {
          "id": 2,
          "name": "carol",
          "email": "carol@example.com",
          "commits": 1,
          "lines_added": 6,
          "lines_removed": 12,
          "lines_changed": 0,
          "net_lines": -6,
          "languages": [
            {"language": "Go", "added": 6, "removed": 12, "changed": 0}
          ],
          "first_tick": 155,
          "last_tick": 155,
          "active_ticks": 1
        }
      ],
      "languages": [
        {
          "name": "Go",
          "total_lines": 22,
          "total_contribution": 37,
          "contributors": {"0": 11, "1": 8, "2": 18}
        }
      ],
      "busfactor": [
        {
          "language": "Go",
          "bus_factor": 2,
          "total_contributors": 3,
          "primary_dev_id": 2,
          "primary_dev_name": "carol",
          "primary_dev_email": "carol@example.com",
          "primary_percentage": 48.64864864864865,
          "secondary_dev_id": 0,
          "secondary_dev_name": "alice",
          "secondary_dev_email": "alice@example.com",
          "secondary_percentage": 29.72972972972973,
          "risk_level": "LOW"
        }
      ],
      "activity": [
        {
          "tick": 0,
          "start_time": "2021-01-01T12:00:00Z",
          "end_time": "2021-01-01T12:00:00Z",
          "by_developer": [{"dev_id": 0, "commits": 1}],
          "total_commits": 1
        }
      ],
      "churn": [
        {
          "tick": 0,
          "start_time": "2021-01-01T12:00:00Z",
          "end_time": "2021-01-01T12:00:00Z",
          "lines_added": 10,
          "lines_removed": 0,
          "net_change": 10
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    aggregate:
        total_commits: 10
        total_lines_added: 30
        total_lines_removed: 15
        total_developers: 3
        active_developers: 1
        estimated_total_developers: 3
        estimated_active_developers: 1
        analysis_period_ticks: 310
        project_bus_factor: 2
        total_languages: 2
    developers:
        - id: 0
          name: alice
          email: alice@example.com
          commits: 6
          lines_added: 16
          lines_removed: 0
          lines_changed: 0
          net_lines: 16
    busfactor:
        - language: Go
          bus_factor: 2
          total_contributors: 3
          primary_dev_id: 2
          primary_dev_name: carol
          primary_percentage: 48.64864864864865
          secondary_dev_id: 0
          secondary_dev_name: alice
          risk_level: LOW
    ```

Fields:

| Field | Meaning |
|---|---|
| `aggregate.total_commits` | Commits counted (merges included, empty commits only with `--empty-commits`). |
| `aggregate.total_developers`, `active_developers` | Exact counts; active = with commits in the last 90 days of the window. |
| `aggregate.estimated_total_developers`, `estimated_active_developers` | The same cardinalities estimated by a HyperLogLog sketch (precision 14, about 0.8 % error); equal to the exact counts on small inputs. |
| `aggregate.analysis_period_ticks` | The newest tick of the window. |
| `aggregate.project_bus_factor` | Smallest number of developers covering 50 % of all contributions (`added + removed`). |
| `developers[]` | Sorted by commits (id on a tie): `email` is omitted when unknown; `languages` is `null` in JSON (`[]` in YAML) for a developer with no line statistics; `first_tick` / `last_tick` / `active_ticks` describe the active period. |
| `languages[]` | Sorted by `total_lines` (lines added): `total_contribution` is `added + removed`; `contributors` maps developer id → contribution. |
| `busfactor[]` | Per language, highest risk first: `bus_factor`, `total_contributors`, the primary and secondary owners (the secondary is present whenever the language has two or more contributors — developer id 0 included), `primary_percentage` / `secondary_percentage`, `risk_level`. |
| `activity[]`, `churn[]` | Per tick, with the tick's first and last committer time: commits per developer / lines added, removed and the net change. |

### NDJSON and timeseries

`--format ndjson` writes one record per commit with line statistics
(`{"commits", "lines_added", "lines_removed", "lines_changed", "author_id",
"languages"}`); `--format timeseries` lists every kept commit in tick order
with the same entry under `"devs"`.

---

## See also

- [Understanding developer analysis](../explanation/developers.md) — the
  mental model, bus factor methodology, architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run developer analysis.
