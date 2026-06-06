# Codefang Go → Rust Architecture Map

This document is the authoritative architecture reference for porting **codefang** (Go,
module `github.com/Sumatoshi-tech/codefang`) to Rust with **byte-identical report output**
on the reference corpus (`~/sources/kubernetes`). It consolidates discovery across the CLI
surface, report serialization, analyzer inventory, libgit2/git2go usage, tree-sitter usage,
third-party dependencies, the build/CGO/libgit2 linkage, and the internal package layering,
and ends with the consolidated byte-identity risk register and the topological port order.

The Go project keeps **libgit2** (via git2go/v34). The Rust port keeps libgit2 too (via the
`git2` crate with `vendored-libgit2`). Nothing in machine report output may use
`serde_json`/`serde_yaml`: Go-byte-compatible encoders (`cf-gojson`, `cf-goyaml`) are the only
serializers allowed on the report path.

---

## 1. Binaries & CLI Tree

Two cobra binaries are built from `cmd/`:

- **`codefang`** — `cmd/codefang/main.go` (`Use: "codefang"`)
- **`uast`** — `cmd/uast/main.go` (`Use: "uast"`)

Both, on `rootCmd.Execute()` error: `fmt.Fprintf(os.Stderr, "Error: %v\n", err)` then
`os.Exit(1)`. `codefang` root sets `SilenceUsage=true` + `SilenceErrors=true` (cobra prints
nothing; main.go prints). `uast` root does **not** set those (default cobra usage/error
printing on flag-parse errors).

Process-level: `codefang` `ensureMallocTunables()` re-execs the process with
`MALLOC_ARENA_MAX=2`, `MALLOC_MMAP_THRESHOLD_=32768`, `MALLOC_TRIM_THRESHOLD_=16384`,
`MALLOC_MMAP_MAX_=65536` when `MALLOC_ARENA_MAX` is unset. This affects memory only, not CLI
surface — replicate (glibc/Linux) or skip.

### 1.1 `codefang` root

PersistentFlags (inherited by all subcommands):

| flag | short | type | default | help |
|------|-------|------|---------|------|
| `--verbose` | `-v` | bool | false | enable detailed output |
| `--quiet` | `-q` | bool | false | suppress output |
| `--profile` | | bool | false | enable pprof server (localhost:6060) and memory watchdog |

Subcommands registered: `run`, `render`, `version`. Cobra auto-adds `-h/--help`,
`completion`, `help`. **`mcp` is NOT shipped** — `cmd/codefang/commands/mcp.go` begins with
`//go:build ignore` and is never `AddCommand`'d. Do not port it.

`version` prints to STDOUT: `codefang %s (commit: %s, built: %s)\n`
(`version.Version`/`Commit`/`Date`; defaults `dev`/`none`/`unknown`).

### 1.2 `codefang run [path]`

`Args=cobra.MaximumNArgs(1)` (optional positional path, redundant with `--path/-p`).
`--list-analyzers` prints analyzer IDs to STDOUT and returns. `--format plot` requires
`--output` else error `--output flag is required when --format plot`.

**Static flags** (long, short, type, default, help):

```
--analyzers           -a  []string  nil       Analyzer IDs or glob patterns (example: static/complexity,history/*,*)
--format                  string    "json"    Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact
--ndjson                  bool      false      With --format timeseries: emit one JSON line per commit (NDJSON)
--input                   string    ""         Input report path for cross-format conversion
--input-format            string    "auto"     Input format: auto, json, bin
--gogc                    int       0          GC percent for history pipeline (0 = auto, >0 = exact)
--ballast-size            string    "0"        Optional GC ballast size for history pipeline (0 = disabled)
--silent                  bool      false      Disable progress output
--no-color                bool      false      Disable colored static output
--path                -p  string    "."        Folder/repository path to analyze
--debug-trace             bool      false      Enable 100% trace sampling for debugging
--cpuprofile              string    ""         Write CPU profile to file
--heapprofile             string    ""         Write heap profile to file
--limit                   int       0          Limit number of commits to analyze (0 = no limit)
--first-parent            bool      false      Follow only first parent of merge commits
--head                    bool      false      Analyze only HEAD commit
--since                   string    ""         Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)
--workers                 int       0          Number of parallel workers (0 = use CPU count)
--static-workers          int       0          Number of parallel static analysis workers (0 = min(CPU count, 8))
--per-file            -F  bool      false      Include per-file breakdowns and summary statistics in static output
--buffer-size             int       0          Size of internal pipeline channels (0 = workers*2)
--commit-batch-size       int       0          Commits per processing batch (0 = default 100)
--blob-cache-size         string    ""         Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)
--diff-cache-size         int       0          Max diff cache entries (0 = default 10000)
--blob-arena-size         string    ""         Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)
--memory-budget           string    ""         Memory budget for auto-tuning (e.g., '512MB', '2GB')
--max-changes-per-commit  int       0          Skip commits whose tree diff exceeds this many changes (0 = default 10000)
--config                  string    ""         Configuration file path (default: .codefang.yaml in CWD or $HOME)
--list-analyzers          bool      false      List all available analyzer IDs and exit
--diagnostics-addr        string    ""         Start diagnostics HTTP server (health/metrics) at this address (e.g., :6060)
--output              -o  string    ""         Output directory for plot HTML files (required with --format plot)
--keep-store              bool      false      Keep temp ReportStore directory after rendering (with --format plot)
--tmp-dir                 string    ""         Directory for temporary spill files (default: system temp)
```

**Exclusion flags** (`registerExclusionFlags`):

```
--include-vendored          bool      false   Re-include vendored dependencies (enry/Linguist) in analysis
--include-generated         bool      false   Re-include auto-generated files in analysis
--extra-excluded-prefixes   []string  nil     Additional UNIX path prefixes to exclude on top of enry heuristics
```

**Persistence flags** (`registerPersistenceFlags`) — note `--checkpoint`/`--resume` default **TRUE**:

```
--checkpoint          bool    true    Enable checkpointing for crash recovery
--checkpoint-dir      string  ""      Checkpoint directory (default: ~/.codefang/checkpoints)
--resume              bool    true    Resume from checkpoint if available
--clear-checkpoint    bool    false   Clear existing checkpoint before run
--cache-dir           string  ""      Incremental analysis cache directory
--no-cache            bool    false   Force full re-analysis, overwriting any existing cache
```

**Dynamic analyzer flags** (`registerAnalyzerFlags`) — one flag per analyzer
`ConfigurationOption`. Type map: Bool→bool, Int→int, String/Path→string, Strings→[]string,
Float→float64. All long-only. Several defaults are host-derived (`runtime.NumCPU()`,
`max(NumCPU/divisor,1)`); compute identically at runtime and preserve 0-sentinels:

```
--granularity                      int      30                      How many time ticks there are in a single band.
--sampling                         int      30                      How frequently to record the state in time ticks.
--burndown-files                   bool     false                   Record detailed statistics per each file.
--burndown-people                  bool     false                   Record detailed statistics per each developer.
--burndown-hibernation-threshold   int      1000                    Min allocated memory in each branch to be compressed.
--burndown-hibernation-disk        bool     true                    Save hibernated state to disk (no-op default).
--burndown-hibernation-dir         string   ""                      Temporary directory for hibernated state.
--burndown-debug                   bool     false                   Validate the trees at each step.
--burndown-goroutines              int      NumCPU                  Goroutines for parallel processing.
--anomaly-threshold                float64  DefaultAnomalyThreshold Z-score threshold for anomaly detection.
--anomaly-window                   int      DefaultAnomalyWindow    Sliding window size in ticks.
--no-diff-cleanup                  bool     false                   Do not apply heuristics to improve diffs.
--no-diff-whitespace               bool     false                   Ignore whitespace when computing diffs.
--diff-timeout                     int      <default>               Max ms a single diff calculation may elapse.
--diff-goroutines                  int      NumCPU                  Goroutines for diff calculation.
--empty-commits                    bool     false                   Take empty commits (trivial merges) into account.
--anonymize                        bool     false                   Anonymize developer names (Developer-A, ...).
--typos-max-distance               int      4                       Max Levenshtein distance for a typo-fix pair.
--people-dict                      string   ""                      Path to developer->name|email associations.
--exact-signatures                 bool     false                   Disable separate name/email matching.
--tick-size                        int      <default>               How long each 'tick' represents in hours.
--shotness-dsl-struct              string   filter(.roles has "Function")  UAST DSL to filter nodes.
--shotness-dsl-name                string   .props.name             UAST DSL to determine node names.
--fail-on-missing-submodules       bool     false                   (blob_cache)
--blob-cache-goroutines            int      NumCPU                  Goroutines for parallel blob loading.
--min-comment-len                  int      20                      Min comment length to analyze.
--sentiment-gap                    float64  0.5                     Sentiment value threshold.
--uast-changes-goroutines          int      max(NumCPU/divisor,1)   Goroutines for parallel UAST parsing.
--whitelist                        string   ""                      Whitelist regexp for files to analyze.
--languages                        []string ["all"]                 Languages to analyze ("all" disables filter).
--skip-blacklist          [DEPRECATED] bool false                   (deprecated; see below)
--blacklisted-prefixes    [DEPRECATED] []string <defaults>          (deprecated; see below)
```

Deprecated (`MarkDeprecated`, still accepted, hidden):
- `skip-blacklist` → "use --include-vendored=false and --include-generated=false (the new defaults). See CHANGELOG for migration."
- `blacklisted-prefixes` → "use --extra-excluded-prefixes; the old flag name is preserved for back-compat but will be removed in the next minor release."

STDOUT vs STDERR: report output → `cmd.OutOrStdout()` (STDOUT); `--list-analyzers` → STDOUT;
progress/log lines via std `log` → STDERR; `--silent` disables progress; `--format plot` writes
HTML to `--output` dir. SIGINT/SIGTERM via `signal.NotifyContext`.

### 1.3 `codefang render <store-dir>`

`Args=cobra.ExactArgs(1)`. Single flag `--output/-o string ""` "output directory for HTML files".
Empty `--output` → error `output directory is required (use --output)`. Other errors:
`no analyzer data found in store`, `no section renderer registered`. Creates dir (0o750),
writes HTML pages + `report.json` (0o640) `{analyzer_ids, pages}`. Skipped analyzers → slog Warn
(STDERR) "skipping analyzer".

### 1.4 `uast` root

PersistentFlags: `--config string ""` "config file (default is $HOME/.uast.yaml)" (no short);
`--verbose/-v bool false`; `--quiet/-q bool false`. Subcommands: `parse`, `diff`, `query`,
`explore`, `analyze`, `completion`, `version`, `validate`, `mapping`, `lsp`, `server`.
`version` → STDOUT `uast %s (commit: %s, built: %s)\n`.

| subcommand | args | flags / behavior |
|------------|------|------------------|
| `parse [files...]` | 0+ (0 → stdin) | `-l/--language ""`, `-o/--output ""`, `-f/--format "json"` (json/compact/tree/none), `-p/--progress false`, `--all false`, `-w/--workers 0`. `tree` falls through to `unsupported format` (preserve buggy behavior). |
| `diff file1 file2` | ExactArgs(2) | `-o/--output ""`, `-f/--format "unified"` (unified/summary/json). |
| `query [query] [files...]` | 1 query + 0+ files (0 → `query expression required`) | `-i/--input ""`, `-o/--output ""`, `-f/--format "json"` (json/compact/count), `-t/--interactive false`. NOTE `-i`=input, `-t`=interactive. |
| `explore [file]` | optional (empty → `no file specified for exploration`) | `-l/--language ""`. REPL prompt `explore> `. |
| `analyze [files...]` | 0+ (0 → `no files specified for analysis`) | `-o/--output ""`, `-f/--format "text"` (text/json/html). |
| `validate <file.json\|->` | ExactArgs(1) | `--schema "pkg/uast/spec/uast-schema.json"` (default/empty → embedded), `--color false`, `--no-color false`. **Exit codes: 2** on IO/JSON/schema-load/validate-engine error; **1** parsed-but-invalid; **0** valid. |
| `mapping` | positional = input files | `--node-types ""`, `--mapping ""`, `--format "text"` (text/json), `--coverage false`, `--generate false`, `--show-treesitter false`, `--language ""`, `--extensions ""`. |
| `completion [shell]` | ExactArgs(1) | bash/zsh/fish/powershell → STDOUT; unknown → `unsupported shell: <s>`. |
| `lsp` | none | stdio LSP server for `.uastmap`/query DSL. |
| `server` | none | `-p/--port "8080"`, `-s/--static ""`. HTTP: POST /api/parse, POST /api/query, GET /api/mappings, GET /api/mappings/<name>. |

### 1.5 Exit codes

- `0` success (both binaries).
- `1` generic error (both, via `os.Exit(1)` on Execute error); also `uast validate` parsed-but-invalid.
- `2` `uast validate` load/JSON/schema-load/validate-engine failures. clap default error exit is 2; success 0; use explicit `process::exit` to mirror validate's 2/1/0.

### 1.6 Config (`.codefang.yaml`)

Loaded via viper (`internal/config/loader.go`): env prefix `CODEFANG_`, nested keys `.`→`_`,
`AutomaticEnv`. Search when `--config` empty: CWD then `$HOME`, config name `codefang`, yaml only.
Missing file is **not** an error. Precedence: **CLI flag > env > file > default**.

Top-level keys: `analyzers []string`; `pipeline.*` (workers, memory_budget, blob_cache_size,
diff_cache_size, blob_arena_size, commit_batch_size, gogc, ballast_size, memory_limit,
worker_timeout, uast_spill_threshold=32, intra_commit_parallel_threshold=4,
max_intra_commit_workers=4, max_uast_blob_size=262144, uast_parse_timeout="10s",
max_changes_per_commit=10000, max_diff_batch_size=1000, memory_budget_ratio=50,
memory_budget_cap="2GiB", memory_limit_ratio=75, uast_spill_trim_interval=16,
native_trim_interval=10, max_streaming_buffering=3, drain_prefetch_timeout="30s",
sampler_interval="2s", worker_ratio=100, uast_worker_ratio=40, leaf_worker_divisor=3,
min_leaf_workers=4, buffer_size_multiplier=2, budget_limit_ratio=95,
system_ram_limit_ratio=90, diff_job_buffer_multiplier=10, static_max_workers=8,
malloc_trim_interval=50, static_memory_limit_ratio=90);
`history.burndown` (granularity=30, sampling=30, track_files=false, track_people=false,
hibernation_threshold=1000, hibernation_to_disk=true, hibernation_directory, debug=false,
goroutines=0); `history.couples`, `history.devs`, `history.file_history`, `history.imports`,
`history.sentiment`, `history.clones`, `history.shotness` (dsl_struct, dsl_name),
`history.typos` (max_distance=4); `checkpoint` (enabled=true, dir, resume=true, clear_prev=false).
(Full per-key default tree is in `internal/config/loader.go`.)

---

## 2. Report Formats & Serialization Rules

Five concrete output families dispatched by `--format`: `json`, `yaml`,
`plot` (HTML/echarts), `bin`, `timeseries` (+`ndjson`), `ndjson`, `text`, `compact`.
**No markdown and no CSV output anywhere.** `go-humanize` is input-parsing only.

### 2.1 Serialization paths (each is load-bearing & distinct)

| path | call site | encoder | indent | trailing `\n` | escapeHTML |
|------|-----------|---------|--------|---------------|------------|
| Static JSON | `analyze/static.go:818` `FormatJSON` | `json.NewEncoder + SetIndent("","  ") + Encode` | 2-space | yes (Encode) | yes (default) |
| History per-analyzer JSON | `analyze/base_history.go:156` | `json.Marshal` | **compact** | **no** | yes |
| History YAML | same | `yaml.Marshal` (yaml.v3) | 2-space | yaml's own | n/a |
| History binary | same | `reportutil.EncodeBinaryEnvelope` | — | — | yes (payload) |
| Conversion / combined | `analyze/conversion.go:302` | `json.NewEncoder + SetIndent("","  ") + Encode` | 2-space | yes | yes |
| Timeseries | `analyze/timeseries.go:154` | `json.NewEncoder + SetIndent` | 2-space | yes | yes |
| Timeseries NDJSON | `analyze/timeseries.go:167` | `json.NewEncoder` no indent | none | per-line | yes |
| NDJSON streaming | `analyze/streaming_sink.go:33` | `json.NewEncoder` no indent | none | per-line | yes |
| Generic reporter JSON | `common/reporter.go:76` | `json.MarshalIndent("","  ")` | 2-space | **no** | yes |

`SetEscapeHTML(false)` is **never** called anywhere → every JSON output escapes `<`, `>`, `&`
as `<`, `>`, `&`. (This is exactly why the port uses `cf-gojson`, not serde_json.)

### 2.2 Field order & map key ordering

- JSON field order = **Go struct declaration order**, NOT json-tag-alphabetical. Source-of-truth
  structs: `renderer/json.go`, `conversion.go`, `timeseries.go`, `streaming_sink.go`.
  e.g. `JSONSection.score` is emitted last despite its tag.
- `JSONReport{overall_score_label, sections, overall_score}`;
  `JSONSection{title, score_label, status, metrics, distribution(omitempty), issues, files(*[],omitempty), score}`;
  `JSONMetric{label,value}`; `JSONDistribution{label, percent, count}`;
  `JSONIssue{name,location,value,severity}`. `distribution,omitempty` omits empty slices;
  `files` is a pointer to distinguish empty-array from omitted.
- `map[string]any` serialization: `encoding/json` sorts string keys alphabetically.
  `MetricSet.ToJSON` (`computed_metrics.go`) and `MergedCommitData.MarshalJSON`
  (`timeseries.go`) emit maps → use sorted keys (BTreeMap). `MergedCommitData` merges fixed meta
  keys `hash/timestamp/author/tick` INTO the same map, so they get alphabetized among analyzer keys.
- `UnifiedModel` tags (`conversion.go:27-37`): `AnalyzerResult{id,mode,schema(omitempty),report}`;
  `UnifiedModel{version,metadata(omitempty),analyzers}`.

### 2.3 Float / number / int formatting

- JSON numbers: Go `strconv` shortest round-trip float64. Rust `ryu`/cf-gojson must match
  (verify exponent/`.0` edge cases on real data).
- Text: `fmt %.1f`/`%.2f`/`%.3f` round-half-to-even (Rust `format!` matches).
- `reportutil`: `FormatInt=strconv.Itoa`, `FormatFloat="%.1f"`, `FormatPercent="%.1f%%"` of `v*100`.
- Terminal mixed rounding: `FormatScore=int(math.Round(score*10))` (round) vs
  `DrawPercentBar=int(percent*100)` (truncate). Match per-call.
- burndown `text.go` custom comma grouping (`1,234,567`) via recursive `formatInt64`/`formatUint64`;
  `reportutil` has NO comma grouping.

### 2.4 Text / table / terminal

- Only go-pretty usage: `common/formatter.go:213` — `table.StyleLight`, `SeparateRows`/`SeparateColumns`/
  `DrawBorder`/`SeparateHeader`=false, header/row/footer, `Render()`. Gated by `text` format + `ShowTables`.
  Reproduce StyleLight glyphs, auto-width, `%v` value formatting.
- `terminal/box.go` DrawHeader heavy box `┏━┓┃┗┛`; `DrawSeparator` repeats `─`. Padding/truncation
  use **byte len()**, not rune count — Rust must use `.len()` (bytes).
- ANSI: `terminal/color.go` `\033[3xm ... \033[0m` unless NoColor (`NO_COLOR` env or `--no-color`);
  width from `COLUMNS` env else 80. Env-dependent text bytes.
- `renderer/renderer.go` fixed column widths (`MetricLabelWidth=20`), `strings.Join('\n')`.

### 2.5 Binary envelope

`reportutil/binary.go:30`: `'CFB1'` magic + **little-endian** `uint32` length + `json.Marshal`
payload (compact, escapeHTML on). Combined runs concatenate envelopes.

### 2.6 Plot/HTML (template-port-required, likely out of byte-scope)

`plotpage/*` + `html/template` (`page.html`/`header.html`/`section.html`/`scripts.html`) +
go-echarts/v2. `extractChartContent` string-slices echarts full-page HTML between
`<div class="container">` and `</body>`, rewrites class, strips `<style>`. `components.go`
Table injects cells as `template.HTML` (raw), Text uses `HTMLEscapeString`; `reportTable`
sorts keys, json.Marshal values, truncates at 500 chars. Byte-reproducing this requires porting
templates + go-echarts verbatim — flag as template-port-required.

---

## 3. Analyzer Inventory

Core contracts: `analyze/analyzer.go` (`Analyzer`: Name/Flag/Descriptor/Configure/
ListConfigurationOptions; sub-interfaces `StaticAnalyzer.Analyze(root *node.Node)`,
`RawFileAnalyzer.AnalyzeFileContent(path, content)`), `analyze/history.go` (`HistoryAnalyzer`:
Initialize/Consume/Fork/Merge/NewAggregator/ReportFromTICKs/Serialize),
`analyze/base_history.go` (`BaseHistoryAnalyzer[M]`: Name=Descriptor.ID, Flag=segment after
`history/`). Output schemas centrally declared in `analyze/schema_registry.go`.

### 3.1 Plumbing / core (8) — infrastructure, not reportable leaves

| Name | path | role |
|------|------|------|
| `TreeDiff` | plumbing/tree_diff.go:70 | per-commit added/modified/deleted changes; feeds everything |
| `IdentityDetector` | plumbing/identity.go:43 | author/committer → canonical dev IDs |
| `TicksSinceStart` | plumbing/ticks.go:37 | bucket commits into time ticks |
| `BlobCache` | plumbing/blob_cache.go:37 | hash→blob cache; goroutine fan-out keyed by hash |
| `FileDiff` | plumbing/file_diff.go:50 | per-file line diffs (native diff); goroutine fan-out |
| `LinesStats` | plumbing/line_stats.go:30 | lines added/removed/changed |
| `LanguagesDetection` | plumbing/languages.go:259 | per-file language via enry |
| `UASTChanges` | plumbing/uast.go:49 | parse+diff changed files to UAST changes; goroutine, spill-to-disk |

### 3.2 Static UAST analyzers (6)

| Name | Flag | ID | output keys |
|------|------|----|-------------|
| clones | clone-detection | static/clones | clone_pairs, clone_type_distribution, total_functions, total_clone_pairs, clone_ratio |
| complexity | complexity-analysis | static/complexity | function_complexity, distribution, high_risk_functions, aggregate |
| comments | comments-analysis | static/comments | comment_quality, function_documentation, undocumented_functions, aggregate |
| halstead | halstead-analysis | static/halstead | function_halstead, distribution, high_effort_functions, aggregate |
| cohesion | cohesion-analysis | static/cohesion | function_cohesion, distribution, low_cohesion_functions, aggregate (LCOM-HS) |
| imports | imports-analysis | static/imports | import_list, dependencies, categories, aggregate |

### 3.3 Static raw-file analyzer (1)

| Name | Flag | ID | output keys |
|------|------|----|-------------|
| composition | composition | static/composition | breakdown, percentages, total_files (enry classify all files) |

### 3.4 History leaf analyzers (11)

| Name | ID | inputs | output keys |
|------|----|--------|-------------|
| TemporalAnomaly | history/anomaly | TreeDiff, Ticks, LineStats, Languages, Identity | anomalies, time_series, aggregate |
| burndown | history/burndown | BlobCache, Ticks, Identity, FileDiff, TreeDiff | global_survival, file_survival, developer_survival, aggregate (goroutine fan-out at history.go:599; StoreWriter chunked) |
| Couples | history/couples | Identity, TreeDiff | file_coupling, developer_coupling, file_ownership, aggregate |
| devs | history/devs | Identity, TreeDiff, Ticks, Languages, LineStats | developers, languages, busfactor, activity, churn, aggregate (6 sort sites; heavy map-order risk) |
| FileHistoryAnalysis | history/file-history | Identity, TreeDiff, LineStats, BlobCache | file_churn, file_contributors, hotspots, composition, composition_ts, aggregate |
| ImportsPerDeveloper | history/imports | UAST, Identity, Ticks | import_list, dependencies, categories, aggregate |
| quality | history/quality | UAST, Ticks | time_series, aggregate (runs static analyzers per-commit) |
| sentiment | history/sentiment | UAST, Ticks | time_series, trend, low_sentiment_periods, aggregate |
| Shotness | history/shotness | FileDiff, UAST | node_hotness, node_coupling, hotspot_nodes, aggregate (DSL-configurable) |
| typos | history/typos | UAST, BlobCache, FileDiff | typos |

Wiring: `cmd/codefang/commands/run.go:1988` `buildPipeline()` builds core plumbing + a `Leaves`
map; `defaultHistoryLeaves()` ranges the map (nondeterministic registration order → must sort by
ID before execution/output); `defaultUASTAnalyzers()` lists the 6 static; `defaultRawFileAnalyzers()`
lists composition. `Factory.maxParallel = runtime.NumCPU()`; merge order must be deterministic.

No `math/rand`/`crypto/rand` in any analyzer. `time.Now()` only in metadata
(`analyze/metadata.go:23` `AnalyzedAt` RFC3339 — normalize/exclude from parity) and tick clock-skew
guards (not in payloads).

---

## 4. git2go / libgit2 Usage

All git2go (`github.com/libgit2/git2go/v34`) usage is centralized in `pkg/gitlib` (no other
production package imports it). The rest of the codebase consumes domain types
(Repository, Commit, Tree, TreeEntry, Blob, Diff, RevWalk, Hash, Signature). Rust mirror:
`rust/crates/cf-gitlib`.

| Go op | site | git2 mapping |
|-------|------|--------------|
| OpenRepository | repository.go:19 | `Repository::open` |
| Head + Target OID | repository.go:42 | `repo.head()?.target()` |
| LookupCommit/Blob/Tree | repository.go:53/63/73 | `find_commit`/`find_blob`/`find_tree` |
| Walk | repository.go:83 | `repo.revwalk()` |
| Log: push + sort + first-parent | repository.go:99 | `push(oid)`, `set_sorting(TIME\|TOPOLOGICAL\|REVERSE)`, `simplify_first_parent()` |
| DiffTreeToTree | repository.go:168 | `diff_tree_to_tree(Option<&Tree>, Option<&Tree>, Option<&mut DiffOptions>)` |
| RevparseSingle + AsCommit (ResolveTime) | helpers.go:81 | `revparse_single(spec)?.peel_to_commit()` |
| Commit metadata (Id/Author/Committer/Message/Parent*/Tree*) | commit.go:45+ | `commit.id()`/`author()`/`committer()`/`message()`/`parent*()`/`tree*()` |
| RevWalk Next/Iterate | commit.go:201, revwalk.go:47 | `Revwalk: Iterator<Item=Result<Oid>>`; bool-continue callback → loop |
| Tree traversal (EntryByIndex/Path, TreeEntry Name/Id/Type) | tree.go:18+ | `tree.get(i)`/`get_path()`; `TreeEntry::name/id/kind()` (kind is Option) |
| Blob Id/Size/Contents | blob.go:17+ | `blob.id/size/content()->&[u8]` (must `.to_vec()`) |
| Diff NumDeltas/Delta/Stats/ForEach | diff.go:16+ | `diff.deltas()`/`get_delta(i)`/`stats()`/`foreach(file,binary,hunk,line)` |
| Delta-status → ChangeAction | changes.go:71 | `git2::Delta::{Added,Deleted,Modified,Renamed,Copied,...}` |
| Hash↔Oid | hash.go:57 | `Oid::from_bytes`/`oid.as_bytes()` (Go uses raw `[20]byte`; git2 Oid opaque) |

**Awkward paths the Rust port changes:**
- `cgo_bridge.go:87` extracts the raw `git_repository*` via **reflection** on git2go's private
  `ptr` field and feeds a custom C shim (`clib/*.c`: `cf_batch_load_blobs`, `cf_tree_diff_v2`,
  `cf_batch_diff_blobs`). The Rust port **drops the entire C-shim batch path** in favor of
  ordinary per-thread git2 lookups (`cf-gitlib/src/batch.rs`); `codefang_git.h`/`blob_ops.c`/
  `diff_ops.c` are NOT ported.
- `worker.go:104` `runtime.LockOSThread()` pins all CGO calls to one OS thread. Removed in Rust
  (no cgo crossing; each thread opens its own git2 handle; Repository is !Send/!Sync per-handle).
- Lifetime/borrow: git2 Commit/Tree/Blob/Revwalk borrow `'repo`; RAII Drop replaces `Free()`.
- Signature time: git2go `When` is `time.Time`; git2 `when()` is `git2::Time` (epoch+offset) —
  `commit.rs:341 time_before` reimplements the comparison.

---

## 5. Tree-sitter Usage

Source → language-agnostic **UAST**. Engine accessed via go-tree-sitter-bare (CGO binding to
libtree-sitter) + go-sitter-forest (**68** per-language grammar modules, each
`GetLanguage() unsafe.Pointer`).

Flow: `pkg/uast/languages.go` maps name → forest fn, wraps with `sitter.NewLanguage` (sync.Map
cache); `DSLParser` (`parser_dsl.go`) pools a `sitter.Parser`, `ParseString` → `*Tree`, walks
recursively converting each tree-sitter node into a canonical `node.Node` via a mapping rule from a
per-language `.uastmap` DSL. Tree-sitter S-expression **queries/captures** are used only when a
rule has a `Pattern` (`pkg/mapping/pattern_matcher.go`: `NewQuery` → `QueryCursor.Matches` → first
match, `@capture` via `CaptureNameForID` + `Node.Content`).

A second, unrelated DSL `FindDSL` (e.g. `filter(.roles has "Function")`) queries the **produced
UAST** (shotness, uast server/query) — NOT tree-sitter. Rust equivalent: `cf-uast-node/src/dsl/`.

The Go port reads tree-sitter internal struct fields via `unsafe` to cut CGO crossings
(`cgo_helpers.go`, `cgo_named_children_batch.c`, etc.) — a Go-only optimization; the Rust port
uses the safe `tree-sitter` crate API (`Node::start_byte/start_position/kind_id/named_child`) and
does **not** replicate the unsafe layout.

Rust state: `cf-uast/src/languages.rs` enumerates the exact 68 names (`SUPPORTED_LANGUAGES`,
asserted == 68) but `get_language` currently returns `None` pending grammar-crate wiring.
`cf-uast-mapping` depends on `tree-sitter` 0.22 + `streaming-iterator` 0.1 (QueryCursor::matches
returns a StreamingIterator). `cf-uast-uastmaps/build.rs` regenerates the embedded mappings
(replacing Go's `embedded_mappings.gen.go`).

**The 68 languages** (all must have a pinned grammar): ansible, bash, c, c_sharp, clojure, cmake,
commonlisp, cpp, crystal, css, csv, dart, dockerfile, dotenv, elixir, elm, fish, fortran,
git_config, gitattributes, gitignore, go, gosum, gotmpl, gowork, graphql, groovy, haskell, hcl,
helm, html, ini, java, javascript, json, kotlin, latex, lua, make, markdown, markdown_inline, nim,
nim_format_string, perl, php, powershell, properties, proto, proxima, prql, psv, python, r, rego,
ruby, rust, rust_with_rstml, scala, sql, ssh_config, swift, tcl, toml, tsx, typescript, xml, yaml,
zig. Several (ansible, helm, gotmpl, proxima, nim_format_string, gosum, gowork, csv, psv, dotenv,
ssh_config, git_config, gitattributes, gitignore, properties, prql, rego) lack a maintained
upstream Rust crate → likely vendor the go-sitter-forest C sources via `cc`, at the same grammar
revision.

`.uastmap` DSL: header `[language "go", extensions: ".go"]` then
`name <- (ts_pattern) => uast(type:..., token:..., roles:..., props:..., children:..., extends:...)`,
parsed by a PEG grammar into `[]mapping.Rule`. Token specs: `self`, `text`, `@capture`,
`child:<type>`, `descendant:<type>`, `fields.<name>`. Conditions: `field == "v"` / `!=`.

---

## 6. Third-party Dependency Usage

**Byte-critical (report-affecting):**

- **src-d/enry v2.1.0** — file classification & language labels. Sites: `IsVendor`
  (pathfilter.go:44, classify.go:52, tree_diff, pathpolicy.go:36), `IsBinary`/`IsImage`
  (classify.go:44/48), `IsDocumentation`/`IsConfiguration`/`IsDotFile` (classify.go:64/68/72),
  `GetLanguage` (languages.go:355 content sniff), `GetLanguageByAlias` (tree_diff.go:147,
  langpath.go:60), `GetLanguageExtensions` (langpath.go:65). Decides which files are analyzed and
  their labels → must reproduce enry's vendor regexes, generated/binary/image/doc/config/dotfile
  heuristics, linguist languages.yml (extensions/filenames/aliases/interpreters/classifier
  order/tie-breaks), and content classifier exactly. Codebase also has its own extension fast-path
  (`languages.go:350`) that must stay consistent with enry data. Data is dumpable via
  `tools/enrydump`.
- **sergi/go-diff v1.4.0 (diffmatchpatch)** — delta engine. `file_diff.go:291-297` /
  `diff_pipeline.go:449-452`: `dmp.New`; `DiffTimeout`; `DiffLinesToRunes`; `DiffMainRunes(...,false)`;
  `DiffCleanupMerge(DiffCleanupSemanticLossless(diffs))`. Consumers: line_stats (added/removed/
  changed), burndown (line ownership/age), typos, shotness. Reproduce Myers line-mode +
  CleanupSemanticLossless + CleanupMerge + DiffTimeout semantics + DiffLinesToRunes hashing +
  whitespace stripping.
- **jonreiter/govader** — only the sentiment analyzer. `scorer.go`:
  `NewSentimentIntensityAnalyzer`, mutates Lexicon (non-ASCII multilingual), `PolarityScores().Compound`.
  Then repo layers: `vaderCompoundToScore=(compound+1)/2` clamped [0,1] as **float32**;
  `injectMultilingualLexicons`; `seDomainNeutralizers`/`seNegativeTerms` with `neutralizerWeight=0.8`;
  length weighting `commentWeightWithMax` with `maxWeightRatio=3.0`. Requires exact VADER lexicon
  valences, booster/negation lists, punctuation/caps amplifiers, compound normalization (alpha=15).
  → Rust `cf-govader` + `cf-sentiment` + `cf-sentiment-lexicons`.

**Output-conditional / non-report:**

- **go-pretty v6** — one borderless `StyleLight` table (formatter.go) in text format.
- **go-echarts v2.6.7** — HTML chart pages (byte-identity only if HTML compared).
- **go-humanize v1.0.1** — `ParseBytes` for config/flag byte sizes only (no humanized output).
- **fatih/color v1.18.0** — `uast validate` colored stdout only.
- **xeipuuv/gojsonschema v1.2.0** — `uast validate` (draft-04) compliance % + error strings.
- **cobra v1.9.1 / viper v1.21.0** — CLI tree + config loader (→ clap builder API + custom loader).
- **tliron/glsp v0.2.2** — UAST mapping-DSL LSP server (→ tower-lsp/lsp-server if in scope).
- **prometheus/client_golang v1.23.2 + opentelemetry v1.40.0** — telemetry only, no report bytes.
- **pierrec/lz4 v4.1.22** — **UNUSED** (zero `.go` references; `persist/codec.go` uses only
  gob+json). Skip in the port.

---

## 7. Build / CGO / libgit2 Linkage

Go 1.26 CGO project linking a **vendored, statically-built libgit2** (git2go/v34 v34.0.0).

- libgit2 is a git submodule at `third_party/libgit2` (url libgit2.git), submodule commit
  `fbea439d4b6fc91c6b619d01b85ab3b7746e4c19` but the **checked-out tree reports version 1.5.0**
  (`version.h`/`CMakeLists.txt`/`libgit2.pc`). **1.5.0 is what is compiled/linked.**
- Built via CMake into `third_party/libgit2/install/lib64/libgit2.a` (+ `pkgconfig/libgit2.pc`)
  with: `BUILD_SHARED_LIBS=OFF`, `USE_SSH=OFF`, `USE_HTTPS=OFF`, `USE_BUNDLED_ZLIB=ON`,
  `BUILD_TESTS=OFF`, `BUILD_CLI=OFF`, `CMAKE_BUILD_TYPE=Release`.
- Go build env (Makefile, repeated across targets):
  `PKG_CONFIG_PATH=.../install/lib64/pkgconfig:.../install/lib/pkgconfig`,
  `CGO_CFLAGS=-I.../install/include`,
  `CGO_LDFLAGS=-L.../install/lib64 -L.../install/lib -lgit2 -lpthread`, `CGO_ENABLED=1`.
  `libgit2.pc` `Libs.private: -lrt`. `STATIC=1`/Dockerfile add `-extldflags=-static`.
- Binaries → `build/bin/{codefang,uast}`. LDFLAGS inject
  `-X .../pkg/version.{Version,Commit,Date}`. Dockerfile: golang:1.26-alpine, `make libgit2`,
  cc-wrap shim (`-include stdint.h`, ansible grammar workaround), fully-static build, final
  alpine:3.21. GoReleaser does **not** set the libgit2 CGO flags (relies on env/CI).

**Rust linkage:** `git2 = { version = "0.19", features = ["vendored-libgit2"] }`. The vendored
libgit2 must match the **1.5.0** ABI/diff/blob/hash/walk semantics. Link line includes git2 +
pthread/rt (bundled zlib inside the archive). git2go/v34 targets libgit2 1.5.x, so the Rust git2
crate must bind the same 1.5.0 behavior.

---

## 8. Internal Package Layering & Topological Port Order

68 internal Go packages, no import cycles. Layering: leaf utilities (`pkg/alg/*`, `pkg/safeconv`,
`pkg/textutil`, ...) → infra/adapters (`pkg/gitlib`, `pkg/uast*`, caches, storage) → domain
(`internal/analyzers/analyze` SPI hub + concrete analyzers + plumbing) → orchestration
(`internal/framework`) → CLI (`cmd/*`).

**Key hub:** `internal/analyzers/analyze` (3885 LOC, the analyzer interface) imported by every
concrete analyzer and by `internal/framework`. Port right after gitlib + uast + alg utilities.

**Note:** `internal/framework` (Tier 5) depends on cache/checkpoint/observability/plumbing/streaming
+ analyze/common/plumbing but NOT on concrete analyzers (registration happens in
`cmd/codefang/commands`), so it sits **below** the analyzers in topo order.

Heavy LOC: pkg/uast 132558 (mostly generated grammar+tests), sentiment/lexicons 94118 (embedded —
copy verbatim), framework 5197, uast/pkg/node 4112, burndown 3993, analyze 3885.

### 8.1 Canonical Go-package → Rust-crate map (verified 2026-06-06)

Every Go package under `internal/`, `pkg/`, `cmd/` maps to exactly one real Rust crate under
`rust/crates/` (or a documented merge / support shim). Verified against the live tree: 74 crates +
2 bin crates. No orphan bare scaffold remains except `cf-plotpage` (documented below). The
`run`/`uast` CLIs live in `rust/bins/{codefang,uast}` (the `cmd/*` Tier 8/9 packages).

| Go package | Rust crate | Notes |
|---|---|---|
| pkg/alg | cf-alg | umbrella re-export over the alg/* leaf crates |
| pkg/alg/bloom | cf-alg-bloom | |
| pkg/alg/cms | cf-alg-cms | |
| pkg/alg/hll | cf-alg-hll | |
| pkg/alg/internal/hashutil | cf-alg-hashutil | |
| pkg/alg/interval | cf-alg-interval | |
| pkg/alg/levenshtein | cf-alg-levenshtein | |
| pkg/alg/lru | cf-alg-lru | |
| pkg/alg/lsh | cf-alg-lsh | |
| pkg/alg/mapx | cf-alg-mapx | |
| pkg/alg/minhash | cf-alg-minhash | |
| pkg/alg/stats | cf-alg-stats | |
| pkg/gitlib | cf-gitlib | libgit2 via git2 |
| pkg/iosafety | cf-iosafety | |
| pkg/meminfo | cf-meminfo | |
| pkg/metrics | cf-metrics | |
| pkg/pathfilter | cf-pathfilter | |
| pkg/persist | cf-persist | gob→bincode (internal state only, DESIGN §3) |
| pkg/pipeline | cf-pipeline | |
| pkg/safeconv | cf-safeconv | |
| pkg/sigutil | cf-sigutil | |
| pkg/textutil | cf-textutil | |
| pkg/uast | cf-uast | tree-sitter stack |
| pkg/uast/lsp | cf-uast-lsp | |
| pkg/uast/pkg/mapping | cf-uast-mapping | |
| pkg/uast/pkg/node | cf-uast-node | |
| pkg/uast/pkg/spec | cf-uast-spec | |
| pkg/uast/uastmaps (embedded `.uastmap`) | cf-uast-uastmaps | embedded mapping data |
| pkg/units | cf-units | |
| pkg/version | cf-version | |
| internal/analyzers/analyze | cf-analyze | SPI hub (3885 LOC) |
| internal/analyzers/plumbing | cf-analyzers-plumbing | |
| internal/analyzers/plumbing/langpath | cf-langpath | enry v2.1.0 TSV vendored |
| internal/analyzers/plumbing/pathpolicy | cf-pathpolicy | |
| internal/analyzers/common | cf-analyzers-common | aggregator/reporter/computed-metrics |
| internal/analyzers/common/renderer | cf-renderer | merged into cf-renderer |
| internal/analyzers/common/reportutil | cf-reportutil | CFB1 .bin envelope |
| internal/analyzers/common/plotpage | cf-plotpage | **SCAFFOLD** — see 8.2 |
| internal/analyzers/common/spillstore | cf-spillstore | |
| internal/analyzers/common/terminal | cf-terminal | |
| internal/analyzers/anomaly | cf-anomaly | |
| internal/analyzers/burndown | cf-analyzer-burndown | the analyzer (vs internal/burndown core) |
| internal/analyzers/clones | cf-clones | |
| internal/analyzers/cohesion | cf-cohesion | |
| internal/analyzers/comments | cf-comments | |
| internal/analyzers/complexity | cf-complexity | |
| internal/analyzers/composition | cf-composition | |
| internal/analyzers/couples | cf-couples | |
| internal/analyzers/devs | cf-devs | |
| internal/analyzers/file_history | cf-file-history | |
| internal/analyzers/halstead | cf-halstead | |
| internal/analyzers/imports | cf-imports | |
| internal/analyzers/quality | cf-quality | |
| internal/analyzers/sentiment | cf-sentiment | |
| internal/analyzers/sentiment/lexicons | cf-sentiment-lexicons | embedded lexicon (94k LOC) |
| internal/analyzers/shotness | cf-shotness | |
| internal/analyzers/typos | cf-typos | |
| internal/budget | cf-budget | |
| internal/burndown | cf-burndown-core | burndown timeline/treap core (vs the analyzer) |
| internal/cache | cf-cache | |
| internal/checkpoint | cf-checkpoint | |
| internal/config | cf-config | |
| internal/framework | cf-framework | orchestration |
| internal/identity | cf-identity | |
| internal/mcp | cf-mcp | ported but `//go:build ignore` in Go → not wired into the bins |
| internal/observability | cf-observability | |
| internal/plumbing | cf-plumbing | |
| internal/storage | cf-storage | |
| internal/streaming | cf-streaming | |
| cmd/codefang + cmd/codefang/commands | cf-commands + bins/codefang | command registry + clap bin |
| cmd/uast | bins/uast | clap bin |

**Support / third-party shim crates** (no 1:1 Go internal package — they stand in for Go stdlib /
vendored libs so OUTPUT stays byte-identical; never substitute serde):

| Rust crate | Stands in for | Used for |
|---|---|---|
| cf-gojson | `encoding/json` | byte-parity JSON marshal + shortest-float |
| cf-goyaml | `gopkg.in/yaml.v3` | byte-parity YAML emitter |
| cf-godiff | `sergi/go-diff` | diff-match-patch line stats |
| cf-govader | `govader@f6505c8d03cc` | sentiment scoring algorithm |

### 8.2 Bare scaffold inventory

Exactly ONE crate remains a bare compiling scaffold: **`cf-plotpage`** (8-line `lib.rs`, just
`CRATE_NAME`). Its Go origin (`internal/analyzers/common/plotpage`, 1629 LOC) renders multi-page
HTML for `run --format plot|html`, which writes to an output DIRECTORY (`-o`) and emits **empty
stdout** — so it produces NO byte-gated capture (MANIFEST `plotHtmlNote`: plot/html are nonBinding
by nature). It is depended on only by `cf-commands` (link-through). It is therefore an intentional,
documented deferral, NOT a correctness gap on any binding path. To be implemented when the plot/html
human-rendered views are ported (out of binding scope).

The full per-module table above is the canonical port-ordering record. Summary of tiers (leaf →
root):

- **Tier 0** (no internal deps): pkg/alg, alg/bloom, alg/internal/hashutil, alg/interval,
  alg/levenshtein, alg/mapx, alg/stats, iosafety, meminfo, metrics, pathfilter, persist, pipeline,
  safeconv, sigutil, textutil, units, version, uast/lsp, uast/pkg/mapping, uast/pkg/node,
  uast/pkg/spec, internal/config, internal/identity, internal/observability, internal/storage,
  common/plotpage, common/spillstore, common/terminal, plumbing/langpath, sentiment/lexicons.
- **Tier 1**: alg/cms, alg/hll, alg/lru, alg/minhash, internal/burndown, internal/checkpoint,
  internal/plumbing, internal/streaming, common/reportutil, plumbing/pathpolicy.
- **Tier 2**: alg/lsh, pkg/gitlib.
- **Tier 3**: pkg/uast, internal/cache.
- **Tier 4**: analyze (SPI hub), analyzers/plumbing.
- **Tier 5**: analyzers/common, analyzers/common/renderer, internal/framework.
- **Tier 6**: anomaly, burndown, clones, cohesion, comments, complexity, couples, devs,
  file_history, halstead, imports, shotness, typos, internal/budget.
- **Tier 7**: composition (→file_history), sentiment (→anomaly), quality (→complexity/halstead/
  comments/cohesion/anomaly).
- **Tier 8**: cmd/codefang/commands, cmd/uast.
- **Tier 9**: cmd/codefang.

---

## 9. Consolidated Byte-Identity Risk Register

### 9.1 CLI surface
1. `codefang mcp` is excluded (`//go:build ignore`) — do NOT add it; help/usage must match.
2. version strings exact: `codefang %s (commit: %s, built: %s)\n` / `uast %s (commit: %s, built: %s)\n` (defaults dev/none/unknown); match ldflags-injected values.
3. Top-level error `Error: %v\n` → STDERR + exit 1. `codefang` silences cobra usage/errors; `uast` does NOT (default usage on flag-parse errors).
4. `uast validate` exit codes 2/1/0 (non-trivial) — use explicit `process::exit`.
5. `run` analyzer flags are DYNAMIC; runtime-derived defaults (NumCPU etc.) must be computed identically; 0-sentinels (0=auto) preserved.
6. `--checkpoint`/`--resume`/`burndown-hibernation-disk` default TRUE.
7. Short-flag collisions: run `-F/-o/-p/-a`; uast query `-i`=input `-t`=interactive; server `-p`/`-s`; parse `-w`/`-p`.
8. Deprecated `skip-blacklist`/`blacklisted-prefixes` still accepted (hidden) with exact deprecation messages.
9. Config env mapping `CODEFANG_*` with `.`→`_`, AutomaticEnv; search CWD then $HOME, name `codefang`, yaml only; precedence CLI > env > file > default.
10. `parse --format tree` is accepted in help but errors `unsupported format` (preserve buggy behavior).
11. `ensureMallocTunables()` re-exec (decide replicate vs skip).

### 9.2 Serialization
12. `SetEscapeHTML` default true everywhere; no `SetEscapeHTML(false)` exists → escape `<`,`>`,`&` as `<`/`>`/`&`. serde_json escapes none by default → use cf-gojson.
13. Two JSON whitespace regimes: indented-encoder-with-trailing-`\n` (static/conversion/timeseries) vs compact-no-newline (history base) vs indented-no-newline (reporter). Match per call site.
14. JSON field order = Go struct declaration order, not tag-alphabetical.
15. `map[string]any` → alphabetical key sort (BTreeMap); `MergedCommitData` merges meta keys into the same alphabetized map.
16. yaml.v3 indent/trailing-newline/quoting/map-ordering vs any Rust YAML crate — use cf-goyaml.
17. go-pretty StyleLight glyphs/padding + disabled options must be exact.
18. go-echarts/v2 + html/template + extractChartContent string-slicing — effectively unreproducible without verbatim port (HTML out-of-scope or template-port-required).
19. Float JSON shortest round-trip edge cases (exponent/`.0`) — verify on real data.
20. Mixed int humanization: FormatScore round vs DrawPercentBar truncate; burndown comma grouping vs none.
21. String width/truncation uses byte len(), not rune count.
22. Env-dependent text: `COLUMNS` (default 80), `NO_COLOR`/`--no-color`.
23. Binary envelope: `CFB1` + little-endian u32 length + compact escapeHTML-on payload; concatenated.

### 9.3 Analyzers / determinism
24. Go map iteration is randomized; most emit sites sort explicitly — replicate exact sort keys & tie-breaks (devs 6, couples, file_history 4, shotness, sentiment 3, complexity 2, ...).
25. Goroutine fan-out result ordering (burndown history.go:599, plumbing file_diff/uast/blob_cache, clones aggregator, static.go) must reassemble by index/path, not completion order.
26. Parallel analyzer execution / Fork-Merge must be deterministic regardless of worker/CPU count.
27. Registration order: `defaultHistoryLeaves()` ranges a map → sort by descriptor ID before execution/output.
28. `time.Now()` metadata `AnalyzedAt` (RFC3339) — normalize/exclude from parity comparison.
29. Float computation (complexity/halstead/cohesion/quality/anomaly) must match Go formatting/rounding.
30. enry classification must match exactly (which files kept/dropped, labels).

### 9.4 git / libgit2
31. Signature time: git2go `time.Time` vs git2 `git2::Time` (epoch+offset) — reproduce author/committer time exactly (burndown/devs/file-history timestamps).
32. Revwalk sorting `TIME|TOPOLOGICAL(+REVERSE)` exact — visitation order feeds burndown (reordering → negative burndown).
33. cgo `TreeDiffWithPathspec` filters submodule(0o160000)/tree(0o040000) modes + pathspec pre-filter; plain `DiffTreeToTree` does NOT. Reproduce whichever path each analyzer used.
34. `DefaultDiffOptions` (git2go) vs `DiffOptions::new()` (git2) defaults may differ (rename detection, context lines, include_unmodified) — verify so delta classification matches.
35. Binary/line-count: Go splits between C shim (`blob_ops.c` is_binary/line_count) and `pkg/textutil` (IsBinary/CountLines, trailing-newline handling) — pick one consistent impl matching the reference reports.

### 9.5 tree-sitter / UAST
36. Each Rust grammar pinned to the exact go-sitter-forest v1.9.x grammar revision (node kinds/spans).
37. tree-sitter C runtime version must match (parser ABI/tree shape).
38. Node type-string selection (kind vs grammar symbol, aliased/anonymous nodes) must match Go's symbolNames path.
39. Positions: Go emits 1-based line/column (`+1`) with 0-based byte offsets — replicate the `+1`.
40. Query matching takes only the FIRST match and first capture per name.
41. ~17 of 68 languages lack upstream Rust crates → vendor C sources at the same revision.
42. `rust` and `rust_with_rstml` are distinct grammars — provide both.
43. Unmapped-node handling (collapse/Synthetic/`lang:type` when IncludeUnmapped) and empty-file skip must match (tree shape).

### 9.6 dependencies / build
44. enry v2.1.0 exact data + heuristics (§6).
45. diffmatchpatch exact passes + DiffTimeout (§6).
46. govader exact lexicon + repo layering + float32 rounding (§6).
47. probabilistic structures (bloom/cms/hll/minhash/lsh + hashutil seeds) must use identical seeds/mixing constants.
48. `pkg/alg/mapx` sorted-key extraction — impose identical sort order (Rust HashMap iteration nondeterministic).
49. `pkg/safeconv` truncation/rounding feeds metric values.
50. framework scheduling/streaming-chunk boundaries (streaming/budget) can affect aggregation order/checkpoint state — deterministic chunking.
51. libgit2 1.5.0 vendored archive (BUILD_SHARED_LIBS=OFF, USE_BUNDLED_ZLIB=ON, ...) — link the same behavior.
52. sentiment/lexicons (94k LOC embedded) copied verbatim, not regenerated.
