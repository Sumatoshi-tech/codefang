# codefang Rust Rewrite — Architecture Map

> Source of truth for porting **github.com/Sumatoshi-tech/codefang** (Go 1.26) to Rust.
> Goal: behavioral and (where required) byte-for-byte parity for two binaries — `codefang` and `uast`.
> Derived from 8 parallel discovery investigations, cross-checked against source. Last verified: 2026-05-30.

---

## 0. Top-level facts

- **Module:** `github.com/Sumatoshi-tech/codefang`, `go 1.26`. A nested module exists at `pkg/uast` (own `go.mod`).
- **Two binaries:** `codefang` (`cmd/codefang`) and `uast` (`cmd/uast`). Both are `spf13/cobra` roots; config via `spf13/viper`. Rust port must produce BOTH with distinct command trees.
- **Git backend:** libgit2 1.5.0, vendored as a git submodule at `third_party/libgit2`, **statically linked via cgo** through `github.com/libgit2/git2go/v34 v34.0.0` PLUS a hand-written C shim in `pkg/gitlib/clib`. No go-git anywhere.
- **Parsing:** tree-sitter via `github.com/alexaandru/go-tree-sitter-bare v1.11.0` (runtime) + 69 `go-sitter-forest/<lang>` grammar submodules. A `.uastmap` PEG DSL maps tree-sitter CST → project UAST.
- **No Rust exists yet** in this checkout (no `crates/`, no `Cargo.toml`).

---

## 1. Binary / CLI trees

### 1.1 `codefang` (entrypoint `cmd/codefang/main.go`)

- **Root:** `Use "codefang"`, Short `"Codefang Code Analysis - Unified code analysis tool"`. Sets `SilenceUsage=true` and `SilenceErrors=true`. On any `Execute` error prints `Error: %v\n` to **STDERR** and `os.Exit(1)`. Subcommands wired in `main.go`: **run, render, version**. (`mcp.go` has `//go:build ignore` and is NOT added — **do not port** the `mcp` command, or gate it behind a disabled feature.)
- **Persistent root flags:**
  - `--verbose`/`-v` bool (false) — "enable detailed output"
  - `--quiet`/`-q` bool (false) — "suppress output"
  - `--profile` bool (false) — "enable pprof server (localhost:6060) and memory watchdog"
- **PersistentPreRun:** if `--profile`, start pprof HTTP on `localhost:6060` + memory watchdog (writes `/tmp/maps_baseline.txt` etc).
- **Startup side effect (BEFORE arg parse):** `ensureMallocTunables()` — if `MALLOC_ARENA_MAX` is unset, sets `MALLOC_ARENA_MAX=2`, `MALLOC_MMAP_THRESHOLD_=32768`, `MALLOC_TRIM_THRESHOLD_=16384`, `MALLOC_MMAP_MAX_=65536`, then `syscall.Exec` re-execs the process. Rust should reproduce (set env + `exec` self) for memory-behavior parity, or document as intentionally dropped.

#### `codefang version`
`Run` (not `RunE`, always exit 0). Prints to STDOUT: `codefang %s (commit: %s, built: %s)\n` from `pkg/version` (Version/Commit/Date; defaults `dev`/`none`/`unknown`, ldflags-injected). No flags.

#### `codefang run [path]`
- Short `"Run static and history analyzers"`; Long `"Run selected static and history analyzers."`. `Args = cobra.MaximumNArgs(1)` (optional positional path overrides `--path`). `RunE`.
- **Literal flags** (long/short type default — help):
  - `--analyzers`/`-a` []string nil — "Analyzer IDs or glob patterns (example: static/complexity,history/*,*)"
  - `--format` string "json" — "Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact"
  - `--ndjson` bool false — "With --format timeseries: emit one JSON line per commit (NDJSON)"
  - `--input` string "" — "Input report path for cross-format conversion"
  - `--input-format` string "auto" — "Input format: auto, json, bin"
  - `--gogc` int 0 — "GC percent for history pipeline (0 = auto, >0 = exact)"
  - `--ballast-size` string "0" — "Optional GC ballast size for history pipeline (0 = disabled)"
  - `--silent` bool false — "Disable progress output"
  - `--no-color` bool false — "Disable colored static output"
  - `--path`/`-p` string "." — "Folder/repository path to analyze"
  - `--debug-trace` bool false — "Enable 100% trace sampling for debugging"
  - `--cpuprofile` string "" / `--heapprofile` string ""
  - `--limit` int 0 — "Limit number of commits to analyze (0 = no limit)"
  - `--first-parent` bool false / `--head` bool false
  - `--since` string "" — "Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)"
  - `--workers` int 0 / `--static-workers` int 0
  - `--include-vendored` bool false / `--include-generated` bool false (multi-sentence help — copy verbatim)
  - `--extra-excluded-prefixes` []string nil
  - `--per-file`/`-F` bool false
  - `--buffer-size` int 0 / `--commit-batch-size` int 0
  - `--blob-cache-size` string "" / `--diff-cache-size` int 0 / `--blob-arena-size` string "" / `--memory-budget` string ""
  - `--max-changes-per-commit` int 0 (multi-sentence help — copy verbatim)
  - `--checkpoint` bool **true** (tri-state via `Changed()`) / `--checkpoint-dir` string "" / `--resume` bool **true** (tri-state) / `--clear-checkpoint` bool false
  - `--cache-dir` string "" / `--no-cache` bool false
  - `--config` string "" — "Configuration file path (default: .codefang.yaml in CWD or $HOME)"
  - `--list-analyzers` bool false — prints to STDOUT, exit 0
  - `--diagnostics-addr` string ""
  - `--output`/`-o` string "" — "Output directory for plot HTML files (required with --format plot)"
  - `--keep-store` bool false / `--tmp-dir` string ""
- **Dynamic per-analyzer flags:** `registerAnalyzerFlags()` iterates every analyzer's `ListConfigurationOptions()` and registers one flag per option (Bool/Int/String/StringSlice/Float64/Path). Includes `--languages` ([]string). Exact names/defaults/help come from `internal/analyzers/*/*.go` — **must be dumped at runtime** (`codefang run --help`) for byte-identity. Two are deprecated via `MarkDeprecated`:
  - `--skip-blacklist` → "use --include-vendored=false and --include-generated=false (the new defaults). See CHANGELOG for migration."
  - `--blacklisted-prefixes` → "use --extra-excluded-prefixes; the old flag name is preserved for back-compat but will be removed in the next minor release."
- **Tri-state flags:** `--checkpoint` and `--resume` (both default true) are read via `Flags().Changed(name)` so file/config defaults apply only when the CLI flag was NOT supplied. In Rust use `Option<bool>`/value-source detection, not a plain bool.
- **I/O discipline:** results → STDOUT (`cmd.OutOrStdout`); progress → STDERR prefixed `progress: ` (suppressed by `--silent` or `--quiet`). Static verbose progress uses Go std `log` (STDERR). Errors bubble to root → STDERR + exit 1.
- **Sentinel errors:** `ErrNoAnalyzersSelected` (lists: anomaly, burndown, couples, devs, file-history, imports, quality, sentiment, shotness, typos), `ErrUnknownAnalyzer`, `ErrRepositoryLoad` ("failed to load repository"), `ErrPlotOutputRequired` ("--output flag is required when --format plot").

#### `codefang render <store-dir>`
- Short `"Render stored analysis results as multi-page HTML"`. `Args = cobra.ExactArgs(1)`.
- Flag: `--output`/`-o` string "" — "output directory for HTML files".
- `RunE` returns `ErrNoOutputDir` ("output directory is required (use --output)") if `-o` empty. Other sentinels: `ErrEmptyStore` ("no analyzer data found in store"), `ErrNoSectionRenderer` ("no section renderer registered").
- Writes HTML files + `report.json` (mode 0640) into the output dir; warnings via `slog`. Errors → STDERR + exit 1.

### 1.2 `uast` (entrypoint `cmd/uast/main.go`)

- **Root:** `Use "uast"`, Short `"UAST (Universal Abstract Syntax Tree) parser and analyzer"`. On `Execute` error prints `Error: %v\n` to STDERR + `os.Exit(1)`. **DOES NOT set `SilenceErrors`/`SilenceUsage`** → cobra also prints usage+error (asymmetry vs `codefang`; must replicate).
- **Persistent flags:** `--config` string "" ("config file (default is $HOME/.uast.yaml)"), `--verbose`/`-v` bool false, `--quiet`/`-q` bool false.
- **Subcommands:** parse, diff, query, explore, analyze, completion, version, validate, mapping, lsp, server.

| Command | Args | Flags | Output / notes |
|---|---|---|---|
| `version` | — | — | STDOUT `uast %s (commit: %s, built: %s)\n`; exit 0 |
| `parse [files...]` | variadic | `--language`/`-l` "", `--output`/`-o` "", `--format`/`-f` "json" (json,compact,tree,none), `--progress`/`-p` false, `--all` false, `--workers`/`-w` 0 | STDOUT or `-o`; progress to STDERR. Sentinels `ErrUnsupportedParseFmt`, `ErrNoSourceFiles`; reads stdin if no files |
| `diff file1 file2` | ExactArgs(2) | `--output`/`-o` "", `--format`/`-f` "unified" (unified,summary,json) | `ErrUnsupportedFileType`, `ErrUnsupportedDiffFmt` |
| `query [query] [files...]` | manual: `ErrQueryExprRequired` if 0 args | `--input`/`-i` "", `--output`/`-o` "", `--format`/`-f` "json" (json,compact,count), `--interactive`/`-t` false | stdin if no files & no `--input`. `ErrUnsupportedQFmt` |
| `explore [file]` | manual: `ErrNoFileSpecified` | `--language`/`-l` "" | REPL → STDOUT. `ErrUnsupportedExploreFile` |
| `analyze [files...]` | manual: `ErrNoFilesSpecified` | `--output`/`-o` "", `--format`/`-f` "text" (text,json,html) | `ErrUnsupportedAnaFmt` |
| `completion [shell]` | ExactArgs(1) | — | bash/zsh/fish/powershell → STDOUT. `ErrUnsupportedShell` |
| `validate <file.json\|->` | ExactArgs(1) | `--schema` "pkg/uast/spec/uast-schema.json", `--color` false, `--no-color` false | **Special exit codes** (see below) |
| `mapping` | variadic (extra = inputs for `--show-treesitter`) | `--node-types` "", `--mapping` "", `--format` "text" (text,json), `--coverage` false, `--generate` false, `--show-treesitter` false, `--language` "", `--extensions` "" | `ErrNodeTypesRequired`, `ErrNoInputFiles`, `ErrNoRootNode`, `ErrUnsupportedLanguage` |
| `lsp` | — | — | LSP over stdio (`lsp.NewServer().Run()`) |
| `server` | — | `Run` not `RunE`; `--port`/`-p` "8080", `--static`/`-s` "" | HTTP on `:PORT`; slog→STDERR. Routes: `POST /api/parse`, `POST /api/query`, `GET /api/mappings`, `GET /api/mappings/<name>` |

**`uast validate` exit codes (via `os.Exit`, not cobra):** valid → 0; validation FAILED → 1; bad JSON / schema read / open / engine error → 2 (`exitCodeValidationFailure`). `--no-color` wins over `--color`. Result text + compliance % to STDOUT (colored); decode/open/schema errors to STDERR.

### 1.3 Config resolution (`internal/config`)

- viper: file name `.codefang` (yaml), searched CWD then `$HOME` (or explicit `--config`). Missing file is NOT an error.
- Env override: prefix `CODEFANG_`, nested `.`→`_` (e.g. `CODEFANG_PIPELINE_WORKERS`, `CODEFANG_HISTORY_BURNDOWN_GRANULARITY`), `AutomaticEnv`.
- **Precedence:** CLI flags > env > file > defaults. Rust port must mirror exactly (use clap + a layered config crate).
- Top-level keys: `analyzers` ([]string), `pipeline`, `history`, `checkpoint`. (Full default tables are in `internal/config/defaults.go` / `loader.go`; e.g. `pipeline.workers=0`, `pipeline.uast_parse_timeout="10s"`, `history.burndown.granularity=30`, `checkpoint.enabled=true`.)
- **Doc caveat:** sentiment key authoritative form is `low_sentiment_risk_threshold` (the shipped `.codefang.yaml` comment shows `low_sentiment_risk_thresh` — implement the long form).

---

## 2. Report formats & exact serialization rules

> There is **NO** `internal/output`, `internal/visualization`, or `internal/report` package. The real machinery lives in `pkg/textutil`, `internal/analyzers/common/{renderer,formatter,plotpage,terminal,reportutil}`, `internal/analyzers/analyze/*`, `pkg/persist`. **No CSV, no Markdown** report format exists.

### 2.1 The canonical JSON writer — `pkg/textutil/textutil.go::WriteJSON`
```go
enc := json.NewEncoder(w); if pretty { enc.SetIndent("", "  ") }; enc.Encode(v)
```
- **No `SetEscapeHTML(false)` anywhere** → Go default HTML escaping is **ON** (`<`,`>`,`&` → `< > &`).
- `Encode` appends exactly **one** trailing `\n`.
- `pretty=true` → 2-space indent; `pretty=false` → compact single line + trailing `\n`.
- Used by `codefang render` `report.json` (pretty), and `uast parse` (`json`=pretty, `compact`=compact).

> **Rust:** serde_json defaults to NO HTML escaping and NO trailing newline. The port must enable HTML-style escaping globally for report JSON, add `\n` where Go uses `Encode`/`Marshal`-yaml, and sort map keys.

### 2.2 The five JSON site configurations (all escape ON; differ in indent/newline)
| Site | Indent | Trailing `\n` |
|---|---|---|
| `textutil.WriteJSON(pretty=true)` (render report.json, conversion JSON `conversion.go:305`, persist `JSONCodec`) | 2-space | yes |
| `textutil.WriteJSON(pretty=false)` (uast parse compact) | none | yes |
| `RenderMetricsJSON` = `json.Marshal` (`metrics_output.go:38`) | none | **no** |
| reportutil binary payload = `json.Marshal` (`binary.go:31`) | none | **no** |
| conversion NDJSON (`conversion.go:342-365`) | none | one `\n` per record |

A single global serializer cannot serve all sites — select indent/newline per emission site.

### 2.3 Field ordering & map sorting
- **Structs:** Go declaration order via `json`/`yaml` tags. In `renderer/json.go`, `score`/`overall_score` are declared **LAST** so serialize last. Rust = serde struct field order (keep score fields last).
- **Maps:** alphabetical — Go's json/yaml runtime always sorts map keys, plus explicit `sort.Strings`/`mapx.SortedKeys`/`sort.Slice`. Rust = `BTreeMap` or pre-sort.

### 2.4 omitempty vs initialized-empty-slice nuance (`renderer/json.go`)
- `JSONSection.Files` (`*[]...,omitempty`) and `Distribution` (`,omitempty`) are set to **non-nil empty slices** (`make([]...,0)`) → emit `[]`, NOT omitted.
- Analyzer fields with `,omitempty` (source_file, language, directory, metadata, schema, clone_type_distribution, external_anomalies/summaries, files, languages) DO drop when empty.
- `timeseries.MergedCommitData.Analyzers` is `json:"-"` → always excluded.
- Rust: emit `[]` for initialized-empty cases; skip for omitempty-empty; skip `json:"-"`.

### 2.5 YAML — `gopkg.in/yaml.v3` (config is INPUT-only; report YAML is OUTPUT)
- `RenderMetricsYAML` (`metrics_output.go:52`) and conversion YAML (`conversion.go:315`) use `yaml.Marshal`: 2-space indent, no `---`, alphabetical map keys, yaml.v3 scalar quoting (numbers/bools/null/yes/no/on/off quoted; single vs double quote selection), 80-col folding, single trailing `\n`. **No Rust YAML crate reproduces yaml.v3's emitter byte-for-byte** — hardest text format.

### 2.6 Terminal text (hand-rolled + go-pretty)
- `renderer/renderer.go` `SectionRenderer`: 2-space indent, `terminal.DrawHeader/DrawSeparator/DrawPercentBar`, 2-column metrics `PadRight(label,20)+PadRight(value,12)`, issues `PadRight(name,25)/(location,35)`. Parts joined by `\n`. **Padding uses BYTE length (`len`)**, not rune/display width — Rust must use byte length to match alignment.
- `formatter.go` generic table: `go-pretty/v6` `table.StyleLight` with `SeparateRows/SeparateColumns/DrawBorder/SeparateHeader` all FALSE; cells `fmt %v`; keys alphabetical; `table.Render()` has **no trailing newline**.
- ANSI color from `fatih/color` via `internal/analyzers/common/terminal`. Disabled on non-TTY / `NO_COLOR` / `--no-color` → piped output is plain.
- Float/percent verbs: `%.2f`, `%.3f`, `%.1f%%` (percent = `x*100`), `%v`. Reproduce Go fmt rounding and Go shortest-float for encoded numbers.

### 2.7 HTML report (`internal/analyzers/common/plotpage`)
- Project's OWN `html/template` (`//go:embed templates/*.html`), funcMap `{odd}`, composes `header.html`/`section.html`/`scripts.html`/`page.html`. go-echarts v2.6.7 builds per-chart `<div>+<script>` fragments embedded via `template.HTML` after `extractChartContent`/`removeStyleTags`. `LogoDataURI` = `data:image/png;base64,` + embedded PNG.
- Go `html/template` contextual auto-escaping applies to plain interpolations; `template.HTML`/`template.CSS`/`template.URL`-typed values emitted unescaped.
- **Byte identity is impractical** — requires reproducing both the template files (whitespace, attribute order) AND go-echarts fragment output (generated element IDs, embedded option JSON float/key formatting, `echarts.min.js`). Treat numeric series as the parity surface; chart HTML as out-of-scope unless tests pin it.

### 2.8 Durable / binary
- `pkg/persist/codec.go`: `JSONCodec` (escape ON, optional 2-space indent, trailing `\n`); `GobCodec` (`encoding/gob` — Go-specific, **not byte-portable**; determine whether any durable report uses gob vs JSON).
- `reportutil/binary.go`: 8-byte header — magic `CFB1` in bytes[0:4], `binary.LittleEndian` uint32 payload length in bytes[4:8], then compact `json.Marshal` payload (escape ON, no newline). Envelopes concatenated and decoded sequentially.

---

## 3. Analyzer inventory (`internal/analyzers/*`)

### 3.1 Interfaces & families
- **Base `Analyzer`** (`analyze/analyzer.go:78`): `Name()`, `Flag()`, `Descriptor()`, `ListConfigurationOptions()`, `Configure(facts)`.
- **STATIC family:** `StaticAnalyzer` (`Analyze(root *node.Node)`), `RawFileAnalyzer` (`AnalyzeFileContent(path, content)`), both embed `FormattableAnalyzer` (`Thresholds`, `CreateAggregator`, `FormatReport{,JSON,YAML,Plot,Binary}`). Optional `VisitorProvider`.
- **HISTORY family:** `HistoryAnalyzer` (`history.go:80`): `Initialize/Consume/WorkingStateSize/AvgTCSize/NewAggregator/SerializeTICKs/ReportFromTICKs/Fork/Merge/Serialize`. `Context` carries Time, Commit, Index, IsMerge, Changes, BlobCache, FileDiffs, UASTChanges, UASTSpillPath. Most leaves embed `BaseHistoryAnalyzer[M]` (`base_history.go:419`; `Name()=Desc.ID`, `Flag()`=part after `history/`). Companions: `StoreWriter`, `DirectStoreWriter`, `Parallelizable`. Flush via `FlushableAnalyzer` (`framework/streaming.go:669`).
- **Execution:** static analyzers fan out via `pipeline.WorkerPool` into a name-keyed map under mutex; history analyzers Fork→Consume→Merge (additive) then aggregate TCs into TICKs → Report. Burndown shards per-file work across internal goroutines (fnv `getShardIndex`).
- **Registration** (`cmd/codefang/commands/run.go:919` `NewRegistry`): static UAST = clones, complexity, comments, halstead, cohesion, imports; raw-file = composition; history = anomaly, burndown, couples, devs, filehistory, imports(history), quality, sentiment, shotness, typos. Two name namespaces: legacy `Name()` (e.g. "Couples","TemporalAnomaly") vs descriptor ID (e.g. "history/couples","history/anomaly").

### 3.2 Per-analyzer summary
| Analyzer | Family | ID / Name() | Category | Key algorithm | Determinism note |
|---|---|---|---|---|---|
| complexity | static UAST | complexity | AST/structure | cyclomatic + cognitive + nesting | sorted (`sort.Slice`) → deterministic |
| cohesion | static UAST | cohesion | AST/structure | LCOM-HS + Bloom shared-var | **function-table order nondeterministic** (map range, no sort); scalars stable |
| comments | static UAST | comments | AST/structure | comment-block grouping (sorted by line) | deterministic |
| halstead | static UAST | halstead | AST/structure | Halstead metrics; CMS for >=1000-token fns | **per-function table order nondeterministic** (map range, no sort); scalars stable |
| clones | static UAST | static/clones | AST/structure | MinHash(128)+LSH(16×8) | fixed seeds → reproducible (fixture-tested) |
| imports | static UAST | imports | AST/structure | import extraction, dedup | deterministic |
| composition | raw-file | static/composition | language-stats | enry file classification | deterministic per file |
| imports/history | history | history/imports | git-history | 4-level author→lang→import→tick map | additive merge → order-independent |
| quality | history | history/quality | technical-debt | composes complexity+halstead+comments+cohesion per commit (scalars only) | hash-keyed maps → order-independent |
| typos | history | history/typos | quality | Levenshtein (default dist 4), single-id subs | dedup by Wrong\|Correct → deterministic |
| sentiment | history | history/sentiment | sentiment | VADER (govader) + multilingual lexicon + SE neutralizers | uses commit time, not now; commutative |
| devs | history (seq) | history/devs | developer/churn | per-author line stats + HLL | additive merge; **no time.Now in scoring** |
| couples | history | history/couples (Couples) | churn/coupling | file & dev co-change matrices + Bloom + HLL | sorted indices → safe |
| file_history | history | history/file-history | churn | path→hashes + per-dev LineStats | **per-file Hashes slice order nondeterministic** |
| burndown | history (seq) | history/burndown | churn/git-history | per-line last-edit tick via sharded treaps | shard merge index-keyed → deterministic |
| shotness | history | history/shotness (Shotness) | AST+git-history | co-change of DSL-selected entities | additive merge → order-independent |
| anomaly | history | history/anomaly (TemporalAnomaly) | statistical | trailing-window Z-scores | ticks sorted → deterministic |

**Plumbing providers** (`internal/analyzers/plumbing/`): BlobCache, FileDiff (native C diff), IdentityDetector, **LanguagesDetection** (enry, language-stats provider), LinesStats, **TicksSinceStart** (`ticks.go:163` uses `now+maxClockSkew` future-commit guard), TreeDiff, UASTChanges. Static-mode language stats: `analyze/static_language.go`.

---

## 4. git2go / libgit2 usage (→ Rust `git2` crate)

All libgit2 access is concentrated in `pkg/gitlib/` (there is **no `pkg/git/`**). 10 production files import `git2go`. Cross-package consumer: `internal/analyzers/plumbing/blob_cache.go` opens a fresh handle per goroutine via `gitlib.OpenRepository(repo.Path())`.

| Concern | Go (git2go) | Rust (git2) mapping |
|---|---|---|
| open / HEAD / lookup | `OpenRepository`, `Head()+Target()+Free()`, `LookupCommit/Blob/Tree` | `Repository::open`, `repo.head()?.target()`, `find_commit/blob/tree`. **Delete all `.Free()`** (Drop) |
| revwalk | `repo.Walk()`, `Push`, `Sorting(SortTime\|Topological\|Reverse)`, `SimplifyFirstParent()`, `walk.Iterate(func(*Commit) bool)` | `repo.revwalk()`, `push/push_head`, `set_sorting(Sort::…)`, `simplify_first_parent()`. **Revwalk yields `Result<Oid>` (pull)** not Commit — look up commit + convert bool-return to break |
| commit meta | `Id/Author/Committer/Message/ParentCount/Parent/ParentId/TreeId/Tree` | same names. **`Signature.When` is `time.Time` vs git2 `git2::Time`** (secs+offset) — reimplement `--since` filter on raw seconds |
| tree | `Tree.Id/EntryCount/EntryByIndex/EntryByPath`, `TreeEntry.Name/Id/Type`; hand-rolled `walkTree` | `Tree::len/get/get_path`, `TreeEntry::name()/id/kind`. **Entry borrows Tree** — copy name/oid/kind eagerly; prefer `Tree::walk` |
| diff | `DefaultDiffOptions`+`DiffTreeToTree`; `diff.ForEach(fileCb→hunkCb, DiffDetail)` | `DiffOptions::new()`+`diff_tree_to_tree(Option<&Tree>,…)` (nil→None, clean). **`Diff::foreach` takes FOUR separate FnMut callbacks** — the nested-closure-returns-closure pattern needs RefCell/restructure |
| delta classify | 11-variant `git2go.Delta*` consts | `git2::Delta::{Added,Deleted,Modified,Renamed,Copied,…}` (casing differs: `Delta::Typechange`). Rename/copy needs `DiffFindOptions::find_similar` |
| blob | `blob.Contents()` (GC-owned, safe after Free) | `blob.content()` → `&[u8]` borrowed. **`cached_blob` pattern (free blob, keep bytes) is ILLEGAL** — `.to_vec()` eagerly everywhere |
| OID↔Hash | `*git2go.Oid` ptr/array; `HashFromOid`/`ToOid`; SHA-1 only (HashSize=20) | `git2::Oid` is Copy (Eq/Hash/Ord); `Oid::from_bytes/as_bytes/from_str` |
| worker | per-goroutine repo, `runtime.LockOSThread`, channel of requests | `Repository` is `!Send+!Sync` — **no `Arc<Repository>`**; open by path inside each thread |
| cgo bridge | reflection into git2go's unexported `ptr`; custom clib batch ops (`cf_batch_load_blobs[_arena]`, `cf_tree_diff_v2`, `cf_batch_diff_blobs`); `cf_configure_memory` | `Repository::raw()` replaces reflection; clib reuse needs `libgit2-sys`+`build.rs`+unsafe, OR rewrite in safe git2 (CGO-overhead motivation evaporates). **Biggest reimplementation decision.** `malloc_trim` has no crate API |

---

## 5. tree-sitter usage (→ Rust `tree-sitter` + grammar crates)

- Code lives in `pkg/uast/` (+ `pkg/uast/pkg/{node,mapping,spec}`, `pkg/uast/lsp`). Two deps: `go-tree-sitter-bare v1.11.0` (runtime, imported `sitter`) and 69 `go-sitter-forest/<lang>` grammar submodules.
- **Native S-expression queries ARE used:** `pattern_matcher.go` uses `sitter.NewQuery` + `QueryCursor.Matches` + `query.CaptureNameForID`. **TreeCursor IS used:** `parser_dsl.go` `GoToFirstChild/GoToNextSibling`. (These correct two earlier discovery errors.)
- **Parser creation:** `GetLanguage(name)` → `languageFuncs[name]` → `sitter.NewLanguage(fn())`, memoized in `sync.Map`. `DSLParser` keeps a `sync.Pool` of `*sitter.Parser` and calls `ParseString(ctx, nil, content)`.
- **Registry / abstraction:** `languages.go` (name→grammar table), `types.go` (`LanguageParser` interface), `loader.go` (lazy DSL parser init + 512-bit FNV-1a bloom for negative extension lookups), `parser.go` (facade over `//go:embed uastmaps/*.uastmap`). Mappings precompiled into 2.7 MB `embedded_mappings.gen.go`.
- **AST walk:** `toCanonicalNode` recursion; `<8` named children → `NamedChild(idx)`, `>=8` → CGO batch helper, fallback TreeCursor. Aggressive unsafe reads of the C `TSNode`/`SubtreeHeapData` layout (linux/amd64). **All of this is a Go-FFI perf workaround — the Rust native API makes it unnecessary.**
- **DSL = two layers:** tree-sitter queries (node-level capture) AND a PEG `.uastmap` rule engine (`mapping.peg`) with `Rule{Name, Extends, Pattern, Conditions, UASTSpec{Type, Token, Roles[], Props, Children}}`, inheritance resolved/merged at runtime. **The Rust port must re-implement this PEG DSL + rule engine + UAST node model** — tree-sitter does not provide it.
- **Pipeline safeguards** (`internal/analyzers/plumbing/uast.go`): 256 KiB blob cap, 10 s per-file parse timeout (pathological inputs make tree-sitter allocate native memory unbounded), tree pooling.

**The 69 supported languages** (authoritative, from `languages.go`; gated by presence of a `.uastmap` file):
ansible, bash, c, c_sharp, clojure, cmake, commonlisp, cpp, crystal, css, csv, dart, dockerfile, dotenv, elixir, elm, fish, fortran, git_config, gitattributes, gitignore, go, gosum, gotmpl, gowork, graphql, groovy, haskell, hcl, helm, html, ini, java, javascript, json, kotlin, latex, lua, make, markdown, markdown_inline, nim, nim_format_string, perl, php, powershell, properties, proto, proxima, prql, psv, python, r, rego, ruby, rust, rust_with_rstml, scala, sql, ssh_config, swift, tcl, toml, tsx, typescript, xml, yaml, zig.

Crate availability: mainstream langs map to official `tree-sitter-*` crates (typescript covers tsx+typescript; tree-sitter-md covers markdown+markdown_inline; c_sharp→tree-sitter-c-sharp). Several forest grammars lack first-party Rust crates and need community crates or vendored C: ansible, crystal, csv, dotenv, git_config, gitattributes, gitignore, gosum, gotmpl, gowork, helm, ini, nim, nim_format_string, properties, proxima, prql, psv, rego, rust_with_rstml, ssh_config, tcl. **Verify each on crates.io before committing to full parity.**

---

## 6. Non-trivial dependency usage (→ Rust crates)

| Go dep (exact pin) | Use | Report-byte impact | Rust mapping / risk |
|---|---|---|---|
| **`github.com/src-d/enry/v2 v2.1.0`** (FROZEN 2019 fork, NOT modern go-enry) | language detection + IsVendor/IsBinary/IsImage/IsDocumentation/IsConfiguration/IsDotFile + GetLanguage/GetLanguageByAlias/GetLanguageExtensions + `data.LanguagesByFilename` | **YES — drives which files are counted & language labels** | Reproduce THIS fork's `languages.yml` tables, alias/extension maps, vendor/generated regexes, resolution order, and canonical language-name strings byte-for-byte. Content heuristics use **Oniguruma** (`src-d/go-oniguruma`) — match Oniguruma, not Rust `regex`. **Do NOT use modern go-enry or hyperpolyglot.** |
| **`github.com/jonreiter/govader v0.0.0-20250429093935-f6505c8d03cc`** | VADER sentiment in `sentiment/scorer.go` (WIRED into reports) | **YES — compound/pos/neg/neu floats serialize** | Reproduce the bundled `vader_lexicon` snapshot at that commit, booster/negation/punctuation/ALL-CAPS/'but'-clause logic, `compound = x/sqrt(x*x+15)`. Mirror govader, not Python VADER. |
| `go-echarts/v2 v2.6.7` | dashboard charts (`devs/dashboard_*`, render) | chart JSON/HTML only | option-JSON + HTML scaffolding heavy surface; usually out-of-scope for byte-identity (match numeric series) |
| `jedib0t/go-pretty/v6 v6.6.7` | human-format tables | table text bytes | port StyleLight column/padding/border; comfy-table/tabled differ |
| `dustin/go-humanize v1.0.1` | number/byte/time strings | output strings only | match thresholds/suffixes/relative-phrase tables; **only used in `internal/framework/config.go`, NOT report serializers** |
| `pierrec/lz4/v4 v4.1.22` | — | **NONE — zero imports** | do NOT add a Rust lz4 dep |
| `prometheus/client_golang v1.23.2` + `go.opentelemetry.io/otel v1.40.0` | telemetry (~34 files) | none | `tracing`+`opentelemetry`+`opentelemetry-otlp`; behavioral parity only |
| `tliron/glsp v0.2.2` | LSP server (`uast lsp`) | none (JSON-RPC) | `tower-lsp`/`lsp-server`; behavioral parity |
| `spf13/cobra v1.9.1` + `spf13/viper v1.21.0` | CLI + config | help text only | clap + figment/config; replicate flag>env>file>default precedence |
| `xeipuuv/gojsonschema v1.2.0` | JSON-schema validation (`uast validate`) | error strings if emitted | `jsonschema` crate; reproduce draft-04 error wording if surfaced |
| `sergi/go-diff v1.4.0` (`diffmatchpatch`) | UAST/tree diff, `uast diff` | diff-output bytes | faithful diff-match-patch port + same `DiffCleanupSemantic` call order |
| `fatih/color v1.18.0` | TTY ANSI | TTY only | `owo-colors`/`colored`; replicate NO_COLOR/isatty gating |

**Serialization substrate** (where all byte-identity lands): Go `encoding/json` (2-space, HTML-escape default, map keys sorted, struct decl order, shortest-float) + `gopkg.in/yaml.v3`. serde_json/serde_yaml must replicate ordering + escaping + float formatting.

---

## 7. Build / CGO / libgit2

- **Source of truth: `Makefile`.** `make`/`make all`/`make build` build BOTH binaries into `build/bin/` (GOBIN). Every compile/test/lint target exports:
  ```
  CGO_ENABLED=1
  CGO_CFLAGS=-I$(CURDIR)/third_party/libgit2/install/include
  CGO_LDFLAGS=-L.../install/lib64 -L.../install/lib -lgit2 -lpthread
  PKG_CONFIG_PATH=.../install/lib64/pkgconfig:.../install/lib/pkgconfig
  ```
- **libgit2 is 1.5.0** (`version.h` + `libgit2.pc`; SOVERSION 1.5). `git describe` of the submodule (`v0.16.0-12722-gfbea439d`) is **misleading** — pin the gitlink commit `fbea439d4b6fc91c6b619d01b85ab3b7746e4c19` or libgit2 1.5.x. git2go v34.0.0 targets the 1.5 ABI.
- **cmake options (must match for parity):** `-DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF -DBUILD_CLI=OFF -DUSE_SSH=OFF -DUSE_HTTPS=OFF -DUSE_BUNDLED_ZLIB=ON -DCMAKE_BUILD_TYPE=Release`. Static linking needs `-lpthread` AND `-lrt` (`Libs.private: -lrt`).
- **Go links libgit2 two ways:** git2go v34 binding + a hand-written CGO C shim (`pkg/gitlib/clib/{utils,blob_ops,diff_ops}.c` + `codefang_git.h` via `cgo_bridge.go`). `cf_configure_memory` tunes libgit2 mwindow/cache limits and glibc malloc arenas.
- **Rust:** point a `git2`/`libgit2-sys` system/pre-built build at `third_party/libgit2/install` (force system mode — the crate would otherwise vendor a different libgit2 and enable https/ssh). A new ADR should supersede `docs/adr/0003-libgit2-via-cgo.md` recording the `git2`-crate decision and the fate of the clib batching layer.
- **Release:** `.goreleaser.yml` sets only `CGO_ENABLED=1` (no libgit2 flags, no `make libgit2` hook) → relies on ambient env. Docker builds fully-static on Alpine/musl.
- **Fresh checkout caveat:** `make libgit2` does NOT run `git submodule update --init` — a clean clone must do that first.

---

## 8. Internal package layering (verified DAG, 0 cycles)

Built from `go list .Imports` (non-test, default build). Tarjan SCC + DFS back-edge detection report **0 cycles**.

**KEY DAG fact:** `internal/framework` DOES import `internal/analyzers/{analyze,common,plumbing}` in non-test files (`runner.go`, `streaming.go`, `uast_pipeline.go`), but `internal/analyzers/analyze` does **NOT** import framework. Ordering: `analyze` (low) → `common,plumbing` → `framework` (high). **The Rust port must keep `analyze` a LOWER crate than `framework`** — inverting creates a real cycle.

Layering: domain/leaf utilities → adapters (gitlib, uast, cache, plumbing, streaming, checkpoint, common/*) → analysis core (`analyze` → `common`/`plumbing`) → framework → analyzer plugins → composites (composition, quality) → application roots (mcp, cmd/uast, cmd/codefang/commands, cmd/codefang).

> **Note:** `cmd/uast` only needs the UAST stack + observability + pipeline, so although the longest-path layering places it at tier 7 it can be ported as early as after tier 2 (independent of framework/analyzers).

See the companion module list (StructuredOutput / port-order JSON) for per-package tier, Rust crate name, deps, purpose, and LOC.

---

## 9. Consolidated byte-identity risk list

**Serialization core**
1. **JSON HTML escaping is ON everywhere** (no `SetEscapeHTML(false)` exists). serde_json defaults OFF → mismatches every `<`,`>`,`&`. Enable HTML-style escaping globally for report JSON.
2. **Trailing-newline divergence per site:** `json.Encoder.Encode` + `yaml.Marshal` add one `\n`; `json.Marshal` + go-pretty `Render()` add none. Track per emission site.
3. **Field/key ordering split:** structs = Go declaration order (score fields LAST in `renderer/json.go`); maps = alphabetical (Go runtime sort + explicit sorts). Use serde struct order + BTreeMap/pre-sort.
4. **omitempty vs initialized-empty `[]`:** `JSONSection.Files`/`Distribution` emit `[]`; other omitempty fields drop; `Analyzers` is `json:"-"`.
5. **yaml.v3 emitter** (block style, scalar quoting, folding, key order) — no Rust crate matches byte-for-byte.
6. **Float/percent formatting** — Go `%.2f/%.3f/%.1f%%`, Go shortest-float in encoded numbers, `%v` reflection format. Reproduce Go rounding + shortest-float (watch `-0`/large-exponent edges).
7. **go-pretty StyleLight** column width / padding / no trailing newline — Rust table crates differ.
8. **HTML report** — project templates + go-echarts v2.6.7 fragments + contextual auto-escaping + base64 logo. Effectively impractical; scope to numeric series.
9. **Terminal padding uses BYTE length** (`len`), not display width — Rust must use byte length.
10. **ANSI color** — match fatih/color ESC codes and NO_COLOR/isatty/`--no-color` gating; non-TTY stays plain.
11. **`encoding/gob`** (persist `GobCodec`) — not byte-portable; determine if any durable report uses it.
12. **Binary `CFB1` envelope** — magic bytes[0:4], LE uint32 length bytes[4:8], compact escaped JSON payload, concatenated.

**Dependency fidelity**
13. **enry = src-d/enry/v2 v2.1.0** (frozen fork) — port its exact data tables, vendor/generated regexes, Oniguruma content semantics, and canonical language-name strings. Drift shifts every count.
14. **govader at commit f6505c8d03cc** — exact lexicon snapshot + algorithm + `compound = x/sqrt(x*x+15)`; serialize floats via Go shortest round-trip.
15. **sergi/go-diff** — Myers+bisect + `DiffCleanupSemantic/Efficiency` + `DiffPrettyText/Delta` formatting in identical call order.
16. **go-humanize / go-pretty / go-echarts** strings — match verbatim where they reach output (humanize only in framework config, not report serializers).
17. **viper precedence** (flag > env > config > default) + `CODEFANG_` env mapping — replicate exactly; affects analysis params and thus contents.
18. **gojsonschema draft-04** error wording if surfaced; else pass/fail parity.
19. **pierrec/lz4 unused** — add nothing.

**Analyzer determinism (already nondeterministic in Go — match the Go behavior or normalize)**
20. **`analyze/metadata.go:23` `AnalyzedAt = time.Now()`** — report envelope is never byte-identical run-to-run unless excluded/normalized.
21. **`plumbing/ticks.go:163` `now+maxClockSkew` future-commit guard** — tick assignment is now-dependent ONLY for repos with future-dated commits.
22. **halstead & cohesion per-function tables** — map-range, no sort → order nondeterministic; scalars stable.
23. **file_history `FileHistory.Hashes`** — appended in Fork/Merge order → per-file commit-hash slice order nondeterministic unless sorted downstream.
24. **NOT a risk (verified):** alg sketches use deterministic fixed seeds (minhash Splitmix64, cms Mix64+fnv, lsh fnv); no `math/rand` in analyzers. Additive/commutative merges and explicitly-sorted outputs are output-stable.

**CLI surface**
25. **uast vs codefang error asymmetry** — uast shows cobra usage on error; codefang suppresses (prints only `Error: %v`).
26. **uast validate exit codes 0/1/2** via `os.Exit`; `--no-color` wins over `--color`.
27. **Dynamic per-analyzer `run` flags** — names/defaults/help only fully knowable by dumping `codefang run --help` at runtime; cobra vs clap `--help` rendering differs (custom help templates needed for true byte-identity).
28. **Deprecated `--skip-blacklist` / `--blacklisted-prefixes`** — keep hidden/deprecated with exact messages.
29. **Multi-line Long descriptions and multi-sentence flag help** (`--max-changes-per-commit`, `--include-generated`, the `static/complexity,history/*,*` example) — copy verbatim.
30. **codefang malloc re-exec** (set `MALLOC_*` + `syscall.Exec` self before parse) — reproduce for memory parity or document as dropped.

**Build / git**
31. **libgit2 must be 1.5.0** (don't trust `git describe`); cmake options must match; force `git2` system/pre-built mode (no vendored-libgit2, no https/ssh).
32. **blob content lifetime** — git2 `content()` borrows; the Go free-blob-keep-bytes pattern is illegal — `.to_vec()` eagerly.
33. **`Repository` is `!Send+!Sync`** — open by path per thread; no `Arc<Repository>`.
34. **`diff.foreach` four-callback model** + return-code semantics (which files/hunks are visited) — wrong return silently corrupts per-file stats.
35. **Signature time** — git2 `git2::Time` (secs+offset) vs Go `time.Time`; reimplement `--since` on raw seconds (timezone off-by-one silently changes the cutoff).
