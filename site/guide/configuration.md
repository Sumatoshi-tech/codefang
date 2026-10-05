# Configuration

A `.codefang.yaml` file and `CODEFANG_*` environment variables set the
**defaults** of `codefang run` flags (`crates/cf-config`). Every key on this
page maps to exactly one flag; a key that is not listed here is not read,
and a file that sets one prints a single warning naming it.

---

## Configuration sources

### Search order

| Priority | Location |
|----------|----------|
| 1 | The file named by `--config <file>` (it must be readable) |
| 2 | `./.codefang.yaml` (the working directory) |
| 3 | `$HOME/.codefang.yaml` |

The first file found is the only one read. A missing file at priority 2 or
3 is not an error; an explicit `--config` that cannot be read is:

```text
Error: cannot read --config ./missing.yaml: No such file or directory (os error 2)
```

`--config /dev/null` runs with the compiled defaults only, whatever the
working directory or `$HOME` holds.

### Precedence

```text
explicit flag  >  CODEFANG_* environment  >  config file  >  compiled default
```

Each configured value becomes the default of its flag, so a flag given on
the command line always wins:

```bash
# .codefang.yaml sets history.anomaly.window_size: 30 — this run uses 10
codefang run -a history/anomaly --anomaly-window 10 .
```

`-v` names the file a run used (`progress: config file: ./.codefang.yaml`,
or `none`).

### Environment variables

Every key has a variable: `CODEFANG_` + the key upper-cased with `.`
replaced by `_`. `CODEFANG_ANALYZERS` takes a comma- or space-separated list.

| Config key | Environment variable |
|------------|----------------------|
| `history.burndown.granularity` | `CODEFANG_HISTORY_BURNDOWN_GRANULARITY` |
| `history.devs.anonymize` | `CODEFANG_HISTORY_DEVS_ANONYMIZE` |
| `history.anomaly.window_size` | `CODEFANG_HISTORY_ANOMALY_WINDOW_SIZE` |
| `analyzers` | `CODEFANG_ANALYZERS` |

```bash
export CODEFANG_HISTORY_BURNDOWN_GRANULARITY=60
export CODEFANG_ANALYZERS=history/burndown,history/devs
codefang run .
```

A variable whose value does not fit its key fails the run:
`Error: invalid environment variable CODEFANG_HISTORY_TICK_SIZE: …`.

---

## Supported keys

| Key | Type | Flag it defaults | Compiled default | Rule |
|-----|------|------------------|------------------|------|
| `analyzers` | list of strings | `-a` / `--analyzers` | every analyzer | ids or globs, as `-a` |
| `pipeline.max_changes_per_commit` | int | `--max-changes-per-commit` | `0` (= 10000) | `>= 0` |
| `pipeline.memory_budget` | size (`2GB`, `512MiB`) or bytes | `--memory-budget` | half the RAM, at most 2 GiB | `>= 256MiB` |
| `pipeline.chunk_size` | int (commits) | `--chunk-size` | planned from the budget | `>= 1` |
| `history.tick_size` | int (hours) | `--tick-size` | `24` | `> 0` |
| `history.burndown.granularity` | int (ticks) | `--granularity` | `30` | `> 0` |
| `history.burndown.sampling` | int (ticks) | `--sampling` | `30` | `> 0` |
| `history.burndown.track_files` | bool | `--burndown-files` | `false` | |
| `history.burndown.track_people` | bool | `--burndown-people` | `false` | |
| `history.devs.consider_empty_commits` | bool | `--empty-commits` | `false` | |
| `history.devs.anonymize` | bool | `--anonymize` | `false` | |
| `history.sentiment.min_comment_length` | int (characters) | `--min-comment-len` | `20` | `>= 0` (`0` disables the filter) |
| `history.shotness.dsl_struct` | string | `--shotness-dsl-struct` | `rfilter(.roles has "Function")` | empty = default |
| `history.shotness.dsl_name` | string | `--shotness-dsl-name` | `.props.name` | empty = default |
| `history.shotness.keep_ended` | bool | `--shotness-keep-ended` | `false` | |
| `history.typos.max_distance` | int | `--typos-max-distance` | `4` | `> 0` |
| `history.anomaly.threshold` | float | `--anomaly-threshold` | `2.0` | `> 0` |
| `history.anomaly.window_size` | int (ticks) | `--anomaly-window` | `20` | `>= 2` |

What each flag does is documented in the [CLI reference](cli-reference.md).
A key left out, or set to an empty value (`window_size:`), keeps the
compiled default. A value that breaks its rule fails the run before any
analysis, naming the file and the key:

```text
Error: invalid config ./.codefang.yaml: history.anomaly.window_size must be >= 2, got 1
```

A value of the wrong type fails the same way with the parser's message,
which names the key and the line.

### Full file

Every supported key, set to its compiled default:

```yaml
analyzers: []            # empty = every analyzer

pipeline:
  max_changes_per_commit: 0   # 0 = the default cap (10000)
  # memory_budget: 2GB        # unset = half the RAM, at most 2 GiB
  # chunk_size: 1000          # unset = planned from the budget

history:
  tick_size: 24
  burndown:
    granularity: 30
    sampling: 30
    track_files: false
    track_people: false
  devs:
    consider_empty_commits: false
    anonymize: false
  sentiment:
    min_comment_length: 20
  shotness:
    dsl_struct: 'rfilter(.roles has "Function")'
    dsl_name: ".props.name"
    keep_ended: false
  typos:
    max_distance: 4
  anomaly:
    threshold: 2.0
    window_size: 20
```

!!! example "Custom shotness targets"

    Track co-change at the class level instead of functions (`rfilter`
    walks the whole tree; `filter` would see only the root's direct
    children):

    ```yaml
    history:
      shotness:
        dsl_struct: 'rfilter(.type == "Class")'
        dsl_name: ".props.name"
    ```

---

## Unsupported keys

A key the binary does not read never fails a run. The file loads, the
supported keys apply, and stderr carries ONE line listing the rest in
document order:

```text
warning: ignoring unsupported config keys: pipeline.workers, checkpoint.enabled, history.burndown.goroutines
```

The keys earlier releases documented and the Rust binary does not read:

| Section | Keys |
|---------|------|
| `pipeline` | every key but `max_changes_per_commit`, `memory_budget` and `chunk_size`: `workers`, `blob_cache_size`, `diff_cache_size`, `blob_arena_size`, `commit_batch_size`, `gogc`, `ballast_size`, `memory_limit`, `worker_timeout`, the spill / trim / ratio / buffering tuning keys, `static_max_workers`, `malloc_trim_interval`, … (Go-runtime and goroutine-pool tuning) |
| `checkpoint` | `enabled`, `dir`, `resume`, `clear_prev` (no checkpoint store is linked into the binary) |
| `history.burndown` | `hibernation_threshold`, `hibernation_to_disk`, `hibernation_directory`, `debug`, `goroutines` |
| `history.couples` | every key (`coupling_threshold_high`, `ownership_*_threshold`, `batch_coupling_threshold`, `hll_precision`, `top_k_per_file`, `min_edge_weight`) |
| `history.devs` | `bus_factor_threshold`, `risk_threshold_*`, `active_threshold_ratio`, `default_active_days`, `hll_precision` |
| `history.file_history` | every key (`hotspot_threshold_*`) |
| `history.imports` | every key (`goroutines`, `max_file_size`, `max_dependency_risk_rows`) |
| `history.sentiment` | every key but `min_comment_length` (`gap`, `neutralizer_weight`, `max_weight_ratio`, `*_threshold`, `low_sentiment_risk_thresh`) |
| `history.clones` | every key |

Delete them from your file to silence the warning.

---

## Minimal examples

=== "CI / Headless"

    ```yaml title=".codefang.yaml"
    analyzers:
      - history/burndown
      - history/devs

    history:
      devs:
        anonymize: true
    ```

=== "Large Repository"

    ```yaml title=".codefang.yaml"
    pipeline:
      max_changes_per_commit: 50000   # keep legitimate mass commits

    history:
      tick_size: 168                  # one tick per week
      burndown:
        granularity: 4
        sampling: 4
    ```

=== "Security Audit"

    ```yaml title=".codefang.yaml"
    analyzers:
      - static/complexity
      - static/cohesion
      - history/anomaly

    history:
      anomaly:
        threshold: 1.5
        window_size: 30
    ```
