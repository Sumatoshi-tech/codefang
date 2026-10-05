# Burndown analyzer reference

The burndown analyzer tracks **code survival over time** by following every
line of code through Git history: per time band, the lines written in that
band that are still alive at each later sample. Optionally, it tracks
per-file and per-developer breakdowns and the developer interaction matrix.

For the conceptual model — what code survival means and how the
line-tracking algorithm works — see
[Understanding code burndown](../explanation/burndown.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

---

## Options

The analyzer reads its options from the `codefang run` flags. The shared
history flags (`--head`, `--limit`, `--since`, `--tick-size`,
`--max-changes-per-commit`, `--languages`, the identity flags) apply as
documented in the [CLI reference](../guide/cli-reference.md); burndown
always walks the first parent, whatever `--first-parent` says.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--granularity` | `int` | `30` | Ticks per age band of the survival matrix. Must be `> 0`; a non-positive value is rejected (`Error: invalid --granularity: must be > 0, got 0`). |
| `--sampling` | `int` | `30` | Ticks per sample row. Must be `> 0` (a non-positive value is rejected); independent of `--granularity`. |
| `--burndown-files` | `bool` | `false` | Per-file survival: fills `file_survival`, `tracked_files` and the ndjson `FileDeltas`. |
| `--burndown-people` | `bool` | `false` | Per-developer survival and the interaction matrix: fills `developer_survival`, `interactions`, `tracked_developers`, the ndjson `PeopleDeltas` / `MatrixDeltas` and the text "Top Developers" section. With `--burndown-files` it also fills each file's `ownership`. |

!!! note "Configuration file"
    The `history.burndown.*` keys of `.codefang.yaml` describe the same
    options in the configuration schema, but `codefang run` applies the
    flags above only.

### Window semantics

In a `--limit`, `--since` or `--head` window the tracked state is seeded
from the window's base tree (the first in-window commit's parent) at tick
0, so each commit's `LinesAdded` / `LinesRemoved` are the lines it really
changed and `--head` reports the same as `--limit 1`. The base lines form
band 0 of the global curve and carry no author. Commits over
`--max-changes-per-commit` are dropped before the ticks are assigned, as
devs does.

---

## Output formats

The examples below come from a ten-commit fixture
(`bins/codefang/tests/burndown_e2e.rs`) run with `--burndown-files
--burndown-people`.

### Text (terminal)

```text
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ Burndown: repoA                                                         310d ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

  Summary
  ────────────────────────────────────────────────────────────────────────────
  Current Lines      15
  Peak Lines         31
  Survival Rate      [█████████░░░░░░░░░░░] 48.4%

  Code Age Distribution
  ────────────────────────────────────────────────────────────────────────────
  < 1 month      █░░░░░░░░░░░░░░░░░░░   6%  (1)
  1-3 months     █░░░░░░░░░░░░░░░░░░░   6%  (1)
  3-6 months     █████████░░░░░░░░░░░  46%  (7)
  6-12 months    ████████░░░░░░░░░░░░  40%  (6)

  Top Developers (by surviving lines)
  ────────────────────────────────────────────────────────────────────────────
  alice                  11  [██████████░░░░░░░░░░] 50.0%
  bob                     4  [████████░░░░░░░░░░░░] 44.4%
```

The header carries the repository name and `analysis_period_days`. The age
distribution groups the last sample's bands by their age at the end of the
window (band `i` of `n` is `(n − i) · granularity · tick_hours / 24 / 30`
months old; the newest band is the youngest) into at most five buckets.
"Top Developers" lists up to five entries of `developer_survival` and is
printed only with `--burndown-people`. Survival rates are color-coded:
green (> 70 %), yellow (50–70 %), red (< 50 %).

### Plot (HTML)

`--format plot` writes an interactive page with the summary statistics
(current lines, peak lines, survival rate, analysis period, developer and
file counts when tracked) and the stacked burndown chart — one series per
age band labelled by age (`3mo`, `2y`, …), or per calendar year when the
history spans two or more years.

### JSON / YAML

Both formats encode the same value; an empty list is `[]` in both.

=== "JSON"

    ```json
    {
      "aggregate": {
        "total_current_lines": 15,
        "total_peak_lines": 31,
        "overall_survival_rate": 0.4838709677419355,
        "analysis_period_days": 310,
        "num_bands": 11,
        "num_samples": 11,
        "tracked_files": 3,
        "tracked_developers": 2
      },
      "global_survival": [
        {
          "sample_index": 0,
          "total_lines": 10,
          "survival_rate": 0.3225806451612903,
          "band_breakdown": [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        },
        {
          "sample_index": 10,
          "total_lines": 15,
          "survival_rate": 0.4838709677419355,
          "band_breakdown": [0, 0, 0, 3, 3, 1, 6, 0, 1, 0, 1]
        }
      ],
      "file_survival": [
        {
          "path": "script.py",
          "current_lines": 8,
          "peak_lines": 9,
          "survival_rate": 0.8888888888888888,
          "ownership": {"0": 4, "1": 4},
          "top_owner_id": 0,
          "top_owner_name": "alice",
          "top_owner_percentage": 50
        },
        {
          "path": "b.go",
          "current_lines": 0,
          "peak_lines": 15,
          "survival_rate": 0,
          "ownership": {}
        }
      ],
      "developer_survival": [
        {"id": 0, "name": "alice", "current_lines": 11, "peak_lines": 22, "survival_rate": 0.5},
        {"id": 1, "name": "bob", "current_lines": 4, "peak_lines": 9, "survival_rate": 0.4444444444444444}
      ],
      "interactions": [
        {
          "author_id": 0,
          "author_name": "alice",
          "modifier_id": 0,
          "modifier_name": "alice",
          "lines_modified": 7,
          "is_self_modify": true
        },
        {
          "author_id": 1,
          "author_name": "bob",
          "modifier_id": 0,
          "modifier_name": "alice",
          "lines_modified": 5,
          "is_self_modify": false
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    aggregate:
        total_current_lines: 15
        total_peak_lines: 31
        overall_survival_rate: 0.4838709677419355
        analysis_period_days: 310
        num_bands: 11
        num_samples: 11
        tracked_files: 3
        tracked_developers: 2
    global_survival:
        - sample_index: 0
          total_lines: 10
          survival_rate: 0.3225806451612903
          band_breakdown:
            - 10
            - 0
    file_survival:
        - path: script.py
          current_lines: 8
          peak_lines: 9
          survival_rate: 0.8888888888888888
          ownership:
            0: 4
            1: 4
          top_owner_id: 0
          top_owner_name: alice
          top_owner_percentage: 50
    developer_survival:
        - id: 0
          name: alice
          current_lines: 11
          peak_lines: 22
          survival_rate: 0.5
    interactions:
        - author_id: 0
          author_name: alice
          modifier_id: 0
          modifier_name: alice
          lines_modified: 7
          is_self_modify: true
    ```

Fields:

| Field | Meaning |
|---|---|
| `aggregate.total_peak_lines` | Sum over bands of each band's maximum: the lines ever written that lived until a sample boundary. |
| `aggregate.total_current_lines` | Live lines at the last sample. |
| `aggregate.overall_survival_rate` | `current / peak`, never above 1. |
| `aggregate.analysis_period_days` | `last_tick · tick_size_hours / 24` — the window's tick span. |
| `aggregate.num_bands`, `num_samples` | `last_tick / granularity + 1`, `last_tick / sampling + 1`. |
| `aggregate.tracked_files`, `tracked_developers` | Entries of `file_survival` / `developer_survival` (0 when the flag is off). |
| `global_survival[]` | One row per sample: `total_lines`, `survival_rate` (`total / peak`) and `band_breakdown` (live lines per age band, band 0 the oldest). |
| `file_survival[]` | Per file (final path, renames followed), most surviving lines first: `current_lines`, `peak_lines`, `survival_rate`; with `--burndown-people` also `ownership` (live lines per developer id) and, for a live file, `top_owner_id` / `top_owner_name` / `top_owner_percentage`. |
| `developer_survival[]` | Per developer, most surviving lines first: `current_lines`, `peak_lines`, `survival_rate`. |
| `interactions[]` | Per `(author, modifier)` pair, most lines first: `lines_modified` — lines written by the author that the modifier deleted or replaced inside the window; `is_self_modify` when both are the same developer. |

### NDJSON and timeseries

`--format ndjson` writes one record per commit of the window (the base
tree is not a commit and has no record):

```json
{"hash":"f08da9bc…","tick":186,"author_id":0,"timestamp":"2021-07-06T12:00:00Z","analyzer":"burndown","data":{"GlobalDeltas":{"186":{"0":-7,"186":6,"31":-5}},"FileDeltas":null,"PeopleDeltas":{"0":{"186":{"0":-7,"186":6}},"1":{"186":{"31":-5}}},"MatrixDeltas":{"0":{"0":7},"1":{"0":5}},"LinesAdded":6,"LinesRemoved":12}}
```

- `GlobalDeltas` — `current tick → written tick → delta` of the commit;
- `FileDeltas` — the same per final path (`null` without
  `--burndown-files`);
- `PeopleDeltas` — the same per author of the affected lines, and
  `MatrixDeltas` — `author → modifier → lines deleted or replaced` (both
  `null` without `--burndown-people`);
- `LinesAdded` / `LinesRemoved` — gross counts: a replaced line is one
  removed and one added (devs reports it as *changed* instead).

`--format timeseries` lists every commit with its own tick and the
`{"lines_added", "lines_removed"}` pair.

---

## See also

- [Understanding code burndown](../explanation/burndown.md) — the mental
  model, algorithm, use cases, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.
