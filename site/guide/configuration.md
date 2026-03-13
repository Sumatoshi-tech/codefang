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
  # Advanced pipeline tuning.
  uast_spill_threshold: 32
  intra_commit_parallel_threshold: 4
  max_intra_commit_workers: 4
  max_uast_blob_size: 262144          # 256 KiB
  uast_parse_timeout: "10s"
  max_changes_per_commit: 10000
  max_diff_batch_size: 1000
  memory_budget_ratio: 50
  memory_budget_cap: "2GiB"
  memory_limit_ratio: 75
  # Extended pipeline tuning.
  uast_spill_trim_interval: 16
  native_trim_interval: 10
  max_streaming_buffering: 3
  drain_prefetch_timeout: "30s"
  sampler_interval: "2s"
  worker_ratio: 100
  uast_worker_ratio: 40
  leaf_worker_divisor: 3
  min_leaf_workers: 4
  buffer_size_multiplier: 2
  budget_limit_ratio: 95
  system_ram_limit_ratio: 90
  diff_job_buffer_multiplier: 10
  static_max_workers: 8
  malloc_trim_interval: 50
  static_memory_limit_ratio: 90

history:
  couples:
    coupling_threshold_high: 10
    ownership_few_threshold: 3
    ownership_moderate_threshold: 5
    batch_coupling_threshold: 100
    hll_precision: 10
    top_k_per_file: 100
    min_edge_weight: 2
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
    bus_factor_threshold: 0.5
    risk_threshold_critical: 90.0
    risk_threshold_high: 80.0
    risk_threshold_medium: 60.0
    active_threshold_ratio: 0.7
    default_active_days: 90
    hll_precision: 14
  file_history:
    hotspot_threshold_critical: 50
    hotspot_threshold_high: 30
    hotspot_threshold_medium: 15
  imports:
    goroutines: 4
    max_file_size: 1048576
    max_dependency_risk_rows: 30
  sentiment:
    min_comment_length: 20
    gap: 0.5
    neutralizer_weight: 0.8
    max_weight_ratio: 3.0
    positive_threshold: 0.6
    negative_threshold: 0.4
    trend_threshold: 0.1
    low_sentiment_risk_thresh: 0.2
  clones:
    max_clone_pairs: 1000
    num_hashes: 128
    num_bands: 16
    num_rows: 8
    shingle_size: 5
    similarity_type2: 0.8
    similarity_type3: 0.5
    threshold_ratio_yellow: 0.1
    threshold_ratio_red: 0.3
    threshold_pairs_yellow: 5
    threshold_pairs_red: 20
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
| `uast_spill_threshold` | `int` | `32` | File changes per commit before UAST trees are spilled to disk to cap memory. | Must be >= 0 |
| `intra_commit_parallel_threshold` | `int` | `4` | Minimum file changes in a commit before intra-commit parallel UAST parsing is used. | Must be >= 0 |
| `max_intra_commit_workers` | `int` | `4` | Maximum goroutines for parallel UAST parsing within a single commit. | Must be >= 0 |
| `max_uast_blob_size` | `int` | `262144` | Maximum blob size in bytes for UAST parsing (256 KiB). Larger files are skipped. | Must be >= 0 |
| `uast_parse_timeout` | `string` | `"10s"` | Per-file timeout for UAST parsing. Prevents pathological tree-sitter behavior. | Valid Go duration string or empty |
| `max_changes_per_commit` | `int` | `10000` | Commits with more file changes than this are skipped entirely. | Must be >= 0 |
| `max_diff_batch_size` | `int` | `1000` | Maximum number of diff requests batched together for efficiency. | Must be >= 0 |
| `memory_budget_ratio` | `int` | `50` | Percentage of system RAM to use as the auto-detected memory budget. | Must be 0–100 |
| `memory_budget_cap` | `string` | `"2GiB"` | Maximum auto-detected memory budget. | Valid byte-size string or empty |
| `memory_limit_ratio` | `int` | `75` | Percentage of system RAM to use as Go's soft memory limit. | Must be 0–100 |
| `uast_spill_trim_interval` | `int` | `16` | How often to call `MallocTrim` during UAST spill-mode parsing (every N commits). | Must be >= 0 |
| `native_trim_interval` | `int` | `10` | How often to call `malloc_trim` within a chunk (every N commits). | Must be >= 0 |
| `max_streaming_buffering` | `int` | `3` | Maximum buffering factor for streaming (1=single, 2=double, 3=triple). | Must be >= 1 |
| `drain_prefetch_timeout` | `string` | `"30s"` | Timeout for abandoned prefetch goroutines before they are leaked. | Valid Go duration string |
| `sampler_interval` | `string` | `"2s"` | Pipeline sampler polling interval for memory triage logging. | Valid Go duration string |
| `worker_ratio` | `int` | `100` | Percentage of CPU cores to use for pipeline workers. | Must be 0–100 |
| `uast_worker_ratio` | `int` | `40` | Percentage of CPU cores to use for UAST pipeline workers. | Must be 0–100 |
| `leaf_worker_divisor` | `int` | `3` | Leaf worker count = `NumCPU / divisor`. | Must be >= 1 |
| `min_leaf_workers` | `int` | `4` | Minimum number of leaf workers when enabled. | Must be >= 1 |
| `buffer_size_multiplier` | `int` | `2` | Buffer size = `workers * multiplier`. | Must be >= 1 |
| `budget_limit_ratio` | `int` | `95` | Percentage of memory budget used as Go's soft memory limit. | Must be 0–100 |
| `system_ram_limit_ratio` | `int` | `90` | Memory limit cap as percentage of system RAM. | Must be 0–100 |
| `diff_job_buffer_multiplier` | `int` | `10` | Scales the diff job queue buffer relative to pipeline buffer size. | Must be >= 1 |
| `static_max_workers` | `int` | `8` | Maximum concurrent workers for static analysis phase. | Must be >= 1 |
| `malloc_trim_interval` | `int` | `50` | Files between `malloc_trim` calls in static analysis. `-1` disables. | Must be >= -1 |
| `static_memory_limit_ratio` | `int` | `90` | Percentage of budget applied as Go's memory limit during static phase. | Must be 0–100 |

!!! tip "Memory Budget Auto-Tuning"

    When `memory_budget` is set, Codefang automatically calculates optimal
    values for `blob_cache_size`, `diff_cache_size`, and `blob_arena_size`.
    Explicit values for those fields take precedence over the auto-tuned ones.

---

### `history.couples`

Controls the file coupling and ownership analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `coupling_threshold_high` | `int` | `10` | Minimum co-change count for a file pair to be classified as "high" coupling. | Must be >= 0 |
| `ownership_few_threshold` | `int` | `3` | Maximum number of contributors for a file to be in the "few owners" bucket. | Must be >= 0 |
| `ownership_moderate_threshold` | `int` | `5` | Maximum number of contributors for a file to be in the "moderate owners" bucket. | Must be >= 0 |
| `batch_coupling_threshold` | `int` | `100` | Maximum number of file pairs per commit considered for coupling. Limits quadratic growth on large commits. | Must be >= 0 |
| `hll_precision` | `int` | `10` | HyperLogLog precision for contributor count sketches. Higher = more accurate, more memory. Valid range: 4–18. | Must be 4–18 when set |
| `top_k_per_file` | `int` | `100` | Maximum coupling pairs per file in store output. | Must be >= 0 |
| `min_edge_weight` | `int` | `2` | Minimum co-change count for a coupling edge to be included. | Must be >= 0 |

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
| `bus_factor_threshold` | `float64` | `0.5` | Cumulative ownership fraction at which the bus factor count stops. | Must be 0.0–1.0 |
| `risk_threshold_critical` | `float64` | `90.0` | Ownership percentage above which a file is at critical risk (single-owner). | Must be 0–100 |
| `risk_threshold_high` | `float64` | `80.0` | Ownership percentage above which a file is at high risk. | Must be 0–100 |
| `risk_threshold_medium` | `float64` | `60.0` | Ownership percentage above which a file is at medium risk. | Must be 0–100 |
| `active_threshold_ratio` | `float64` | `0.7` | Fraction of the most-active developer's commits required to consider a developer "active". | Must be 0.0–1.0 |
| `default_active_days` | `int` | `90` | Lookback window in days for determining whether a developer is currently active. | Must be >= 0 |
| `hll_precision` | `int` | `14` | HyperLogLog precision for developer count sketches. Higher = more accurate, more memory. Valid range: 4–18. | Must be 4–18 when set |

---

### `history.file_history`

Controls the file history (churn and hotspot) analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `hotspot_threshold_critical` | `int` | `50` | Commit count above which a file is classified as a critical hotspot. | Must be >= 0 |
| `hotspot_threshold_high` | `int` | `30` | Commit count above which a file is classified as a high hotspot. | Must be >= 0 |
| `hotspot_threshold_medium` | `int` | `15` | Commit count above which a file is classified as a medium hotspot. | Must be >= 0 |

---

### `history.imports`

Controls the import/dependency history analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `goroutines` | `int` | `4` | Number of parallel goroutines for import extraction. | Must be > 0 |
| `max_file_size` | `int` | `1048576` | Maximum file size in bytes to analyze for imports (1 MiB default). | Must be > 0 |
| `max_dependency_risk_rows` | `int` | `30` | Maximum number of rows in the dependency risk table in plot output. | Must be >= 0 |

---

### `history.sentiment`

Controls the comment sentiment analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `min_comment_length` | `int` | `20` | Minimum comment character length to include in sentiment analysis. | Must be > 0 |
| `gap` | `float64` | `0.5` | Sentiment classification gap threshold. Comments with scores within this gap of neutral are considered neutral. | Must be between 0.0 and 1.0 |
| `neutralizer_weight` | `float64` | `0.8` | How strongly SE-domain adjustments affect the final score. `0` = no effect, `1` = full adjustment. | Must be 0.0–1.0 |
| `max_weight_ratio` | `float64` | `3.0` | Maximum weight ratio for comment length weighting. Prevents single long comments from dominating. | Must be > 0 |
| `positive_threshold` | `float64` | `0.6` | Sentiment score at or above this is classified as "positive". | Must be 0.0–1.0 |
| `negative_threshold` | `float64` | `0.4` | Sentiment score at or below this is classified as "negative". | Must be 0.0–1.0 |
| `trend_threshold` | `float64` | `0.1` | Minimum change in sentiment needed to classify a trend as "improving" or "declining". | Must be >= 0 |
| `low_sentiment_risk_thresh` | `float64` | `0.2` | Sentiment at or below this is flagged as HIGH risk (vs MEDIUM). | Must be 0.0–1.0 |

---

### `history.clones`

Controls the clone detection analyzer.

| Field | Type | Default | Description | Validation |
|-------|------|---------|-------------|------------|
| `max_clone_pairs` | `int` | `1000` | Maximum number of clone pairs reported in the aggregated result. | Must be >= 0 |
| `num_hashes` | `int` | `128` | MinHash signature size. More hashes = better accuracy, more memory. | Must be > 0 |
| `num_bands` | `int` | `16` | Number of LSH bands. `num_bands * num_rows` must equal `num_hashes`. | Must be > 0 |
| `num_rows` | `int` | `8` | Number of rows per LSH band. | Must be > 0 |
| `shingle_size` | `int` | `5` | Token shingle window size for MinHash input. | Must be > 0 |
| `similarity_type2` | `float64` | `0.8` | Minimum Jaccard similarity for Type-2 (renamed) clone detection. | Must be 0.0–1.0 |
| `similarity_type3` | `float64` | `0.5` | Minimum Jaccard similarity for Type-3 (near-miss) clone detection. | Must be 0.0–1.0 |
| `threshold_ratio_yellow` | `float64` | `0.1` | Clone ratio above which a yellow warning is issued. | Must be 0.0–1.0 |
| `threshold_ratio_red` | `float64` | `0.3` | Clone ratio above which a red warning is issued. | Must be 0.0–1.0 |
| `threshold_pairs_yellow` | `int` | `5` | Clone pair count above which a yellow warning is issued. | Must be >= 0 |
| `threshold_pairs_red` | `int` | `20` | Clone pair count above which a red warning is issued. | Must be >= 0 |

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
