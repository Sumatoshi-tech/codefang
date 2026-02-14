# Configuration System

Codefang supports project-level configuration via `.codefang.yaml` files,
environment variables, and CLI flags.

## Config File Search

When no `--config` flag is provided, codefang searches for `.codefang.yaml` in:

1. Current working directory
2. `$HOME`

If no file is found, compiled defaults are used. This is not an error.

## Merge Priority

Configuration values are merged with the following priority (highest wins):

1. **CLI flags** — `--workers 8` always wins
2. **Environment variables** — `CODEFANG_PIPELINE_WORKERS=8`
3. **Config file** — `.codefang.yaml`
4. **Compiled defaults** — built into the binary

## CLI Flag

```
codefang run --config /path/to/.codefang.yaml .
```

The `--config` flag specifies an explicit config file path. When set, the
search path is not used.

## Environment Variables

All config keys can be set via environment variables using the `CODEFANG_`
prefix. Nested keys use `_` as separator:

```bash
export CODEFANG_PIPELINE_WORKERS=8
export CODEFANG_HISTORY_BURNDOWN_GRANULARITY=15
export CODEFANG_HISTORY_SENTIMENT_GAP=0.3
```

## Config File Structure

```yaml
# List of analyzers to run (empty = all)
analyzers: []

pipeline:
  workers: 0              # 0 = auto (GOMAXPROCS)
  memory_budget: ""       # e.g. "4GiB"
  blob_cache_size: ""
  diff_cache_size: 0
  blob_arena_size: ""
  commit_batch_size: 0
  gogc: 0                 # 0 = Go default (100)
  ballast_size: "0"
  memory_limit: ""        # e.g. "8GiB" — sets debug.SetMemoryLimit
  worker_timeout: ""      # e.g. "60s" — stall detection timeout per worker request

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
    max_file_size: 1048576  # 1 MiB

  sentiment:
    min_comment_length: 20
    gap: 0.5

  shotness:
    dsl_struct: 'filter(.roles has "Function")'
    dsl_name: ".props.name"

  typos:
    max_distance: 4

checkpoint:
  enabled: true
  dir: ""
  resume: true
  clear_prev: false
```

## Validation

Config values are validated at load time. Invalid values produce an error
and prevent execution:

- `pipeline.workers` must be >= 0
- `pipeline.diff_cache_size` must be >= 0
- `pipeline.commit_batch_size` must be >= 0
- `pipeline.gogc` must be >= 0
- `pipeline.memory_limit` must be a valid human-readable size (e.g. "4GiB", "512MiB") or empty
- `pipeline.worker_timeout` must be a valid Go duration (e.g. "60s", "2m") or empty
- `history.burndown.granularity` must be >= 0
- `history.burndown.sampling` must be >= 0
- `history.sentiment.min_comment_length` must be >= 0
- `history.sentiment.gap` must be in [0.0, 1.0]
- `history.typos.max_distance` must be >= 0
- `history.imports.goroutines` must be >= 0
- `history.imports.max_file_size` must be >= 0

## Package

The configuration system is implemented in `pkg/config/`:

- `types.go` — `Config` struct with `mapstructure` tags
- `loader.go` — `LoadConfig()` using `spf13/viper`
- `defaults.go` — compiled default constants
- `errors.go` — sentinel validation errors
