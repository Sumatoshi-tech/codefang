# Configuration

Codefang uses a layered configuration system built on [Viper](https://github.com/spf13/viper).
Settings can come from a YAML config file, environment variables, or CLI flags,
and they merge with a well-defined priority order.

---

## Configuration Sources

### Search Order

When no explicit `--config` flag is provided, Codefang searches for a
`.codefang.yaml` file in the following locations (first match wins):

| Priority | Location |
|----------|----------|
| 1 | Current working directory (`./.codefang.yaml`) |
| 2 | User home directory (`$HOME/.codefang.yaml`) |

### Merge Priority

When the same setting is specified in multiple sources, the highest-priority
source wins:

```
CLI flags  >  Environment variables  >  Config file  >  Compiled defaults
```

!!! example "Override Example"

    A config file sets `pipeline.workers: 2`, but the command line passes
    `--workers 8`. The effective value is **8** because CLI flags have the
    highest priority.

### Explicit Config Path

Use `--config` to point at a specific file:

```bash
codefang run -a 'history/*' --config /etc/codefang/production.yaml .
```

---

## Environment Variables

All configuration keys can be set via environment variables using the
`CODEFANG_` prefix. Nested keys use `_` as a separator.

| Config Key | Environment Variable |
|-----------|---------------------|
| `pipeline.workers` | `CODEFANG_PIPELINE_WORKERS` |
| `pipeline.memory_budget` | `CODEFANG_PIPELINE_MEMORY_BUDGET` |
| `history.burndown.granularity` | `CODEFANG_HISTORY_BURNDOWN_GRANULARITY` |
| `history.sentiment.gap` | `CODEFANG_HISTORY_SENTIMENT_GAP` |
| `checkpoint.enabled` | `CODEFANG_CHECKPOINT_ENABLED` |

```bash
# Set workers via environment
export CODEFANG_PIPELINE_WORKERS=4
export CODEFANG_PIPELINE_MEMORY_BUDGET=2GiB
codefang run -a 'history/*' .
```

---

## Full Configuration Reference

Below is the complete `.codefang.yaml` file with all fields set to their
compiled defaults:

```yaml
analyzers: []

pipeline:
  workers: 0              # 0 = auto (GOMAXPROCS)
  memory_budget: ""       # e.g. "4GiB"
  blob_cache_size: ""     # e.g. "1GB" (default when empty)
  diff_cache_size: 0      # 0 = default (10000)
  blob_arena_size: ""     # e.g. "4MB" (default when empty)
  commit_batch_size: 0    # 0 = default (100)
  gogc: 0                 # 0 = Go default (100)
  ballast_size: "0"       # "0" = disabled
  memory_limit: ""        # e.g. "8GiB"
  worker_timeout: ""      # e.g. "60s"

history:
  burndown:
    granularity: 30
    sampling: 30
    track_files: false
    track_people: false
    hibernation_threshold: 1000
    hibernation_to_disk: true
    hibernation_directory: ""
    debug: false
    goroutines: 0
  devs:
    consider_empty_commits: false
    anonymize: false
  imports:
    goroutines: 4
    max_file_size: 1048576
  sentiment:
    min_comment_length: 20
    gap: 0.5
  shotness:
    dsl_struct: 'filter(.roles has "Function")'
    dsl_name: ".props.name"
  typos:
    max_distance: 4
  anomaly:
    threshold: 2.0
    window_size: 20

checkpoint:
  enabled: true
  dir: ""
  resume: true
  clear_prev: false
```

---

## Section Reference

### `analyzers`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `analyzers` | `[]string` | `[]` | Default list of analyzer IDs to run when `-a` is not provided on the command line. Accepts the same IDs and glob patterns as the `-a` flag. |

```yaml
analyzers:
  - static/complexity
  - history/burndown
  - history/devs
```

---

### `pipeline`

Resource and tuning knobs for the analysis pipeline.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `workers` | `int` | `0` | Number of parallel workers. `0` uses `GOMAXPROCS`. | Must be >= 0 |
| `memory_budget` | `string` | `""` | Total memory budget for auto-tuning cache sizes (e.g. `"512MB"`, `"2GiB"`). Empty means no budget-based tuning. | Valid byte-size string or empty |
| `blob_cache_size` | `string` | `""` | Maximum size of the blob cache (e.g. `"256MB"`, `"1GB"`). Empty uses the built-in default of 1 GB. | Valid byte-size string or empty |
| `diff_cache_size` | `int` | `0` | Maximum number of entries in the diff cache. `0` uses the built-in default of 10000. | Must be >= 0 |
| `blob_arena_size` | `string` | `""` | Memory arena allocation for blob loading (e.g. `"4MB"`). Empty uses the built-in default of 4 MB. | Valid byte-size string or empty |
| `commit_batch_size` | `int` | `0` | Number of commits processed per batch. `0` uses the built-in default of 100. | Must be >= 0 |
| `gogc` | `int` | `0` | Go garbage collector target percentage. `0` uses the Go default of 100. Higher values reduce GC frequency at the cost of memory. | Must be >= 0 |
| `ballast_size` | `string` | `"0"` | GC ballast allocation size. `"0"` disables ballast. Useful for reducing GC pauses in memory-rich environments. | Valid byte-size string |
| `memory_limit` | `string` | `""` | Hard memory limit passed to the Go runtime (`GOMEMLIMIT`). Empty means no limit. | Valid byte-size string or empty |
| `worker_timeout` | `string` | `""` | Maximum duration a single worker may run before being terminated (e.g. `"60s"`, `"5m"`). Empty means no timeout. | Valid Go duration string or empty |

!!! tip "Memory Budget Auto-Tuning"

    When `memory_budget` is set, Codefang automatically calculates optimal
    values for `blob_cache_size`, `diff_cache_size`, and `blob_arena_size`.
    Explicit values for those fields take precedence over the auto-tuned ones.

---

### `history.burndown`

Controls the burndown (code ownership aging) analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `granularity` | `int` | `30` | Time granularity in days for burndown bands. | Must be > 0 |
| `sampling` | `int` | `30` | Sampling interval in days for burndown snapshots. | Must be > 0 |
| `track_files` | `bool` | `false` | Track per-file burndown data. Increases memory usage. | -- |
| `track_people` | `bool` | `false` | Track per-person burndown data. Increases memory usage. | -- |
| `hibernation_threshold` | `int` | `1000` | Number of file entries before hibernation activates. | -- |
| `hibernation_to_disk` | `bool` | `true` | Spill hibernated state to disk instead of keeping in memory. | -- |
| `hibernation_directory` | `string` | `""` | Directory for hibernated state files. Empty uses a temp directory. | -- |
| `debug` | `bool` | `false` | Enable verbose debug output for the burndown analyzer. | -- |
| `goroutines` | `int` | `0` | Parallel goroutines for burndown computation. `0` uses a sensible default. | -- |

---

### `history.devs`

Controls the developer activity analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `consider_empty_commits` | `bool` | `false` | Include empty (no-diff) commits in developer statistics. | -- |
| `anonymize` | `bool` | `false` | Replace developer names with anonymous identifiers in output. | -- |

---

### `history.imports`

Controls the import/dependency history analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `goroutines` | `int` | `4` | Number of parallel goroutines for import extraction. | Must be > 0 |
| `max_file_size` | `int` | `1048576` | Maximum file size in bytes to analyze for imports (1 MiB default). | Must be > 0 |

---

### `history.sentiment`

Controls the comment sentiment analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `min_comment_length` | `int` | `20` | Minimum comment character length to include in sentiment analysis. | Must be > 0 |
| `gap` | `float64` | `0.5` | Sentiment classification gap threshold. Comments with scores within this gap of neutral are considered neutral. | Must be between 0.0 and 1.0 |

---

### `history.shotness`

Controls the shotness (function co-change) analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `dsl_struct` | `string` | `filter(.roles has "Function")` | DSL expression to identify structural elements (functions, methods) for co-change tracking. | Valid UAST DSL expression |
| `dsl_name` | `string` | `.props.name` | DSL expression to extract the name of each structural element. | Valid UAST DSL path expression |

!!! example "Custom Shotness Targets"

    Track co-change at the class level instead of functions:

    ```yaml
    history:
      shotness:
        dsl_struct: 'filter(.type == "Class")'
        dsl_name: ".props.name"
    ```

---

### `history.typos`

Controls the typo detection analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `max_distance` | `int` | `4` | Maximum Levenshtein edit distance for two identifiers to be considered a potential typo pair. | Must be > 0 |

---

### `history.anomaly`

Controls the temporal anomaly detection analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `threshold` | `float64` | `2.0` | Z-score threshold for flagging a commit as anomalous. Lower values are more sensitive. | Must be > 0 |
| `window_size` | `int` | `20` | Sliding window size (in commits) for computing the rolling mean and standard deviation. | Must be >= 2 |

---

### `checkpoint`

Controls checkpoint and resume behavior for long-running history analyses.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `enabled` | `bool` | `true` | Enable periodic checkpointing for crash recovery. | -- |
| `dir` | `string` | `""` | Directory for checkpoint files. Empty uses `~/.codefang/checkpoints`. | Valid directory path or empty |
| `resume` | `bool` | `true` | Automatically resume from an existing checkpoint if one is found. | -- |
| `clear_prev` | `bool` | `false` | Delete any existing checkpoint data before starting a new run. | -- |

!!! warning "Checkpoint Directory Permissions"

    The checkpoint directory must be writable by the user running Codefang.
    When using the default (`~/.codefang/checkpoints`), the directory is
    created automatically on first use.

---

## Minimal Examples

=== "CI / Headless"

    ```yaml title=".codefang.yaml"
    analyzers:
      - history/burndown
      - history/devs

    pipeline:
      workers: 2
      memory_budget: "1GiB"

    checkpoint:
      enabled: false
    ```

=== "Large Repository"

    ```yaml title=".codefang.yaml"
    pipeline:
      workers: 8
      memory_budget: "8GiB"
      gogc: 50
      commit_batch_size: 200

    history:
      burndown:
        hibernation_to_disk: true
        goroutines: 4

    checkpoint:
      enabled: true
      dir: "/tmp/codefang-checkpoint"
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
