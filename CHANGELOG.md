# Changelog

All notable changes to the Codefang project are documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/).

---

## [Unreleased] — Repo hygiene & race fix

### Fixed

- **Race in `internal/framework.PipelineSampler`**:
  `t1Captured` was a plain `bool` concurrently read by the sampler
  goroutine (`sample`) and written by the caller (`CaptureT1`),
  causing intermittent `DATA RACE` under `go test -race`. Converted
  to `sync/atomic.Bool` with `CompareAndSwap` — at most one t1 heap
  profile is captured regardless of which goroutine observes the
  trigger first. Removed the unused `t0Captured` field. Full
  `go test -race ./...` now green.

### Chore

- **Removed `// FRD: specs/frds/FRD-...md` comments from all `.go`
  files.** `specs/` is gitignored, so those references broke for
  anyone cloning the repo. Traceability now lives in FRDs and
  PR descriptions instead of source code.

---

## [Unreleased] — Cross-phase defaults: vendor & generated excluded

**Breaking change.** Default analysis output across both phases
now **excludes vendor and generated files** — matching the
convention of every mature multi-language analyser (eslint skips
`node_modules/`, rubocop skips `vendor/`, pylint skips `.venv/`,
scalafix skips `target/`, phpcs skips `vendor/`, GitHub Linguist
excludes vendor/generated from its language breakdown). Users who
want the pre-2026-04 behaviour back pass `--include-vendored
--include-generated` in their invocation.

### Flags (cross-phase)

- `--include-vendored` (bool, default `false`) — re-include paths
  detected as vendored by enry / Linguist. Covers `vendor/`,
  `node_modules/`, `third_party/`, `testdata/`, `dist/`,
  minified bundles, and more. Cross-language by construction.
- `--include-generated` (bool, default `false`) — re-include
  auto-generated files. Covers `*.pb.go`, `zz_generated_*.go`,
  `*_pb2.py`, `*.min.js`, and content-header markers
  (`DO NOT EDIT`, `Code generated`, `@generated`, …).
- `--extra-excluded-prefixes` (strings, default `[]`) — additional
  UNIX path prefixes to exclude, for ecosystems enry doesn't know
  about (e.g. `.venv/`, `target/`, `.gradle/`).

All three flags apply identically to both `-a 'static/*'` and `-a
'history/*'` runs — one flag set, one meaning.

### Deprecated

- `--skip-blacklist` — now a no-op (the new default already excludes
  vendor and generated). Cobra deprecation warning fires when the
  flag is passed.
- `--blacklisted-prefixes` — migrate to `--extra-excluded-prefixes`
  (identical semantics). Cobra deprecation warning fires when the
  flag is passed.

Both will be removed in the next minor release.

### Architecture

New package `internal/analyzers/plumbing/pathpolicy` exposing a pure
`Exclude(path, content, opts) bool` backed by enry.IsVendor +
`pkg/pathfilter`'s content-aware generated-file detection. Both
phases call the same helper — single source of truth, no
phase-specific drift.

### Measured impact (cross-language fixture, `-a static/complexity`)

| Invocation                                        | Total Functions |
| ------------------------------------------------- | --------------: |
| *(defaults)*                                      | 1               |
| `--include-vendored`                              | 4               |
| `--include-vendored --include-generated`          | 5               |

---

## [Unreleased] — Cross-phase consistency for `--languages`

**Motivation**: After the history-side push-down, `--languages` meant
different things depending on `-a 'history/*'` vs `-a 'static/*'`. Static
analysis silently ignored the flag — every UAST-supported file was parsed
and fed to every requested static analyzer regardless of the user's
preference. This release makes the flag cross-phase: one flag, one
meaning, both phases narrowed.

### Changes

- **`StaticService.LanguageGlobs`** — new field on the static service,
  populated from `--languages` via the existing
  `internal/analyzers/plumbing/langpath` single source of truth. Empty
  disables the filter (default behavior unchanged).
- **Path-based walker hooks** — both `StaticService.streamFiles` (UAST
  walker) and `StaticService.rawFilePhase` visit-check the basename
  against the glob set via `matchesLanguageGlobs` before sending the
  path downstream. Filtered files never reach the UAST parser or any
  analyzer.
- **Runtime wiring** — `runStaticAnalyzers` and `runStaticPlotAnalyzers`
  build the globs via a shared `applyStaticLanguageFilter` helper.
  Unknown language tokens fail fast on static-only runs with the same
  error shape as the history side.
- **Executor signatures** — `staticExecutor` and `staticPlotExecutor`
  gain a `languages []string` parameter; test stubs updated
  mechanically.

### Non-goals

- No content-aware post-pass on the static side (the UAST parser's
  own language router is the final authority for matched files; a
  second pass would duplicate work).
- No changes to the history side.

---

## [Unreleased] — Performance: `--languages` filter push-down into libgit2

**Motivation**: The `--languages` flag used to be applied *after* libgit2 had
already produced a full tree diff. Every delta crossed the cgo boundary, was
materialised in Go, and only then dropped by the analyzer if its detected
language wasn't in the allow-list. On polyglot repositories with a narrow
filter, libgit2 was doing 4× the tree-diff work it needed to.

### Changes

- **New package `internal/analyzers/plumbing/langpath`** — pure Go
  `Globs(langs []string) (globs []string, wantsAll bool, err error)` backed
  by enry's generated Linguist dataset (`data.ExtensionsByLanguage` +
  `data.LanguagesByFilename`). Single source of truth; 100 % test coverage.
- **New C ABI `cf_tree_diff_v2`** in `pkg/gitlib/clib/{codefang_git.h,diff_ops.c}`
  accepts a pathspec array which it forwards to libgit2's
  `git_diff_options.pathspec`. The old `cf_tree_diff` is retired in favour of
  `cf_tree_diff_v2` via `CGOBridge.TreeDiffWithPathspec`.
- **`TreeDiffRequest.Pathspec` + `BlobPipeline.TreeDiffPathspec` +
  `CoordinatorConfig.TreeDiffPathspec`** thread the pathspec from the
  analyzer through the pipeline to every worker call.
- **`TreeDiffAnalyzer.Pathspec` + `applyLanguageConfig`** resolve aliases via
  `enry.GetLanguageByAlias` (so `--languages golang` / `js` / `ts` now work,
  not just canonical Linguist names) and pre-compute the pathspec at
  `Configure` time.
- **Fail-fast on unknown languages**: `--languages notalang` now returns
  `failed to configure TreeDiff: tree-diff pathspec: unknown language: "notalang"`
  instead of silently producing an empty report.

### Measured impact

On a 500-commit × 200-file × 4-language synthetic fixture with
`--languages go`:

| Metric                      | Before  | After   | Δ      |
| --------------------------- | ------: | ------: | -----: |
| Wall time                   | 0.44 s  | 0.29 s  | −34 %  |
| Max RSS                     | 74 MB   | 66 MB   | −11 %  |
| `cgocall` cumulative CPU    | 800 ms  | 510 ms  | −36 %  |
| Unique functions in profile | 286     | 209     | −27 %  |
| JSON report                 |    —    |    —    | byte-identical |

Regression guard (no `--languages` filter): wall time 0.51 s → 0.49 s,
within noise.

### Non-goals (for this changeset)

- No new user flags.
- The Go-side `shouldIncludeChange` language filter remains as the precise
  post-pass (pathspec is deliberately over-inclusive for
  content-disambiguated extensions such as `.h`, `.pl`, `.m`, `.r`).

---

## [Unreleased] — Analytics Readiness & DWH Suitability

**Motivation**: A comprehensive data analyst review of Codefang's JSON output revealed that while the data was analytically rich (17 analyzers, 1M+ function-level rows, time-series, coupling data), it was structurally hostile to analytics tooling and DWH loading. Function records had bare names with no file paths, time-series ticks had no calendar dates, developer identities used pipe-delimited strings, and nested maps blocked efficient columnar ingestion. This release systematically fixes every identified blocker, raising the data quality score from **2.1/5 to 4.6/5**.

### Architecture: Pipeline Stage Refactor

#### `RawFileAnalyzer` and `FormattableAnalyzer` interfaces

Replaced the `FileContentAnalyzer` + `WalksAllFiles` marker interface pattern with a proper pipeline stage architecture.

**Before**: Analyzers that needed raw file access (not UAST) had to implement `StaticAnalyzer` with a no-op `Analyze(*node.Node)`, plus two marker interfaces discovered at runtime via type assertions.

**After**: Two clean interface hierarchies — `StaticAnalyzer` for UAST-based analysis and `RawFileAnalyzer` for raw file analysis — both embed a shared `FormattableAnalyzer` base. `StaticService` holds separate slices. `AnalyzeFolder` uses `pipeline.RunPhases` with explicit `rawFilePhase` and `uastPhase` stages.

**Why it matters for BI**: The pipeline refactor enabled `StampSourceFile` to receive `rootPath` and convert all file paths to relative — a prerequisite for portable DWH data. It also enabled `StampLanguage` to inject detected language into every function record.

**Files changed**:
- `internal/analyzers/analyze/analyzer.go` — new `FormattableAnalyzer`, `RawFileAnalyzer` interfaces; `StaticAnalyzer` refactored to embed `FormattableAnalyzer`
- `internal/analyzers/analyze/static.go` — `StaticService` gains `UASTAnalyzers` + `RawFileAnalyzers` slices; `AnalyzeFolder` uses `pipeline.RunPhases`
- `internal/analyzers/composition/analyzer.go` — implements `RawFileAnalyzer` directly (removed no-op `Analyze`, `NeedsAllFiles`)
- `internal/analyzers/analyze/registry.go` — `NewRegistry` accepts three slices
- `cmd/codefang/commands/run.go` — split `defaultStaticAnalyzers` into `defaultUASTAnalyzers` + `defaultRawFileAnalyzers`
- `internal/analyzers/analyze/perfile.go` — `PerFileEnricher` uses `[]FormattableAnalyzer`
- `internal/analyzers/common/renderer/json.go` — `EnrichWithPerFileData` uses `[]FormattableAnalyzer`

---

### Static Analyzers: New Fields on Every Function Record

#### `source_file` — File path on every function record

**Motivation**: 152,000+ function records in the JSON output had bare names like `"ForKind"` with no indication of which file they belonged to. This made it impossible to join function metrics to file-level data, build file heatmaps, or drill down from "bad function" to "where in the repo."

**Root cause**: The `_source_file` stamping mechanism existed and worked through aggregation, but `FormatReportBinary` called `ComputeAllMetrics` which parsed `[]map[string]any` items into typed structs. Those structs had no `SourceFile` field, silently dropping the value during struct conversion.

**Fix**: Added `SourceFile string` to all input `FunctionData` and output data structs (`FunctionComplexityData`, `FunctionHalsteadData`, `FunctionCohesionData`, all comment data structs, `HighRiskFunctionData`, `HighEffortFunctionData`, `LowCohesionFunctionData`, `UndocumentedFunctionData`). Populated from `_source_file` map key during `parseFunctionData` → `Compute()`. Updated `StampSourceFile` to accept `rootPath` and convert to relative via `MakeRelativePath`.

**JSON output key**: `"source_file"` (relative path, e.g., `"pkg/kubelet/kubelet.go"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

#### `language` — Programming language on every function record

**Motivation**: Analysts had to infer language from file extension at query time. The parser already knows the language.

**Fix**: Added `LanguageKey` constant, `StampLanguage()` function, and `Language` field to `TypedCollection` struct. Language is stamped in `analyzeFilesParallel` via `parser.GetLanguage(filePath)` and propagated through `TypedCollection` → `DetailedDataCollector.buildItems()` → `stampCollectionMetadata()` to reach the output structs.

**JSON output key**: `"language"` (e.g., `"go"`, `"bash"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

#### `directory` — Parent directory on every function record

**Motivation**: Directory-level aggregation (e.g., "which package has worst complexity") requires parsing file paths at query time, which is expensive in columnar DWH.

**Fix**: Added `DirectoryKey` constant and `Directory` field to `TypedCollection`. Stamped as `filepath.Dir(relativePath)` inside `StampSourceFile`. Propagated via `stampCollectionMetadata()` alongside language.

**JSON output key**: `"directory"` (e.g., `"pkg/kubelet"`)

**Analyzers affected**: `static/complexity`, `static/halstead`, `static/cohesion`, `static/comments`

---

### History Analyzers: Tick Timestamps

#### `start_time` / `end_time` on every time-series tick

**Motivation**: All 6 history time-series analyzers emitted `tick: <int>` with no calendar date. Every time-series chart had an unlabeled X-axis. The `TICK` struct already carried `StartTime`/`EndTime` internally but didn't export them.

**Fix**: Created `TickBounds` type and `BuildTickBounds(ticks []TICK)` helper. Each analyzer's `ticksToReport` adds `tick_bounds` to the Report. Each `ParseReportData` reads it. Each time-series output struct gains `StartTime`/`EndTime` string fields (RFC 3339). For quality and devs analyzers, added timestamp tracking to their tick accumulators (`tickAccumulator.startTime/endTime`, `TickDevData.startTime/endTime`) with min/max tracking in `extractTC` and population in `buildTick`.

**JSON output keys**: `"start_time"`, `"end_time"` (RFC 3339, e.g., `"2024-01-15T10:30:00Z"`)

**Analyzers affected**: `history/sentiment`, `history/anomaly`, `history/quality`, `history/devs` (activity + churn), `history/file-history` (composition_ts)

---

### Developer Identity Normalization

#### Split pipe-delimited names into `name` + `email`

**Motivation**: Developer identity used `"daniel smith|dbsmith@google.com"` pipe-delimited strings from `ReversedPeopleDict`. This blocked clean dimension table creation in DWH systems.

**Fix**: Created `SplitIdentity(s string) (name, email string)` in `internal/identity/split.go`. Handles pipe-delimited, exact `"name <email>"`, and plain name formats. Updated `devName()` → `devNameAndEmail()` and `getDevName()` → `getDevNameAndEmail()`.

**Fields added**:
- `DeveloperData`: `email` field
- `BusFactorData`: `primary_dev_email`, `secondary_dev_email`
- `DeveloperCouplingData`: `developer1_email`, `developer2_email`

**Analyzers affected**: `history/devs`, `history/couples`

---

### Output Structure: Flattened Arrays

#### `developers[].languages` — map → array

**Motivation**: `map[string]LineStats` with variable language-name keys cannot be UNNEST'd in columnar DWH without custom ETL.

**Fix**: Changed `DeveloperData.Languages` from `map[string]pkgplumbing.LineStats` to `[]LanguageStatsEntry`. Internal accumulation uses unexported `langMap`, converted to sorted array via `finalizeLanguages()`. Empty language strings replaced with `"Other"`.

**Before**: `{"Go": {"added": 100, "removed": 5, "changed": 3}}`
**After**: `[{"language": "Go", "added": 100, "removed": 5, "changed": 3}]`

#### `activity[].by_developer` — map → array

**Motivation**: `map[int]int` (dev_id → commit_count) serializes to JSON with string keys, blocking typed ingestion.

**Fix**: Changed to `[]DeveloperCommits` with `{dev_id, commits}` fields. Sorted by dev_id for deterministic output.

**Before**: `{"2": 5, "3": 3}`
**After**: `[{"dev_id": 2, "commits": 5}, {"dev_id": 3, "commits": 3}]`

#### `file_contributors[].contributors` — map → array

**Motivation**: `map[int]LineStats` blocked DWH UNNEST.

**Fix**: Changed to `[]ContributorEntry` with `{dev_id, added, removed, changed}` fields. Sorted by dev_id.

**Before**: `{"2": {"added": 42, "removed": 5, "changed": 3}}`
**After**: `[{"dev_id": 2, "added": 42, "removed": 5, "changed": 3}]`

---

### Output Envelope

#### Top-level `metadata` section

**Motivation**: A DWH ingesting reports from multiple repos could not distinguish them. No repo name, analysis timestamp, or version.

**Fix**: Added `AnalysisMetadata` struct with `repo_path`, `repo_name` (from `filepath.Base`), `analyzed_at` (RFC 3339), `codefang_version` (from build ldflags). Injected after `DecodeCombinedBinaryReports` in the combined render path.

```json
{
  "version": "codefang.run.v1",
  "metadata": {
    "repo_path": "/home/user/sources/kubernetes",
    "repo_name": "kubernetes",
    "analyzed_at": "2026-04-07T23:33:00Z",
    "codefang_version": "dev"
  },
  "analyzers": [...]
}
```

#### Per-analyzer `schema` manifest

**Motivation**: DWH consumers need to know field types, grain, and cardinality for automated ETL generation.

**Fix**: Added `FieldMeta` struct with `{type, grain, description}` and static `analyzerSchemas` registry covering all 17 analyzers. Each `AnalyzerResult` in the output includes a `schema` field.

```json
{
  "id": "static/complexity",
  "schema": {
    "function_complexity": {
      "type": "list",
      "grain": "function",
      "description": "Per-function cyclomatic and cognitive complexity"
    }
  },
  "report": {...}
}
```

#### NDJSON output format

**Motivation**: The monolithic JSON (467MB for kubernetes) must be fully parsed to extract any single analyzer. NDJSON enables streaming ingestion into ClickHouse.

**Fix**: Added `FormatNDJSON` case to `WriteConvertedOutput`. One JSON line per analyzer result, with optional metadata line prepended.

```bash
codefang run --format ndjson /repo > output.ndjson
```

---

### Clone Analysis

#### `clone_type_distribution` from full population

**Motivation**: Clone pairs are capped at 1,000 in the output, but the distribution metrics (Type-1/2/3 breakdown) were computed from the capped sample, skewing percentages for large codebases with 22M+ total pairs.

**Fix**: Added `typeDistribution cloneTypeCounts` to `clonePairResult`. `matchCandidates` increments per-type counters for ALL valid pairs before the cap check. Both aggregator and per-file paths emit `clone_type_distribution` in the report. `ReportSection.Distribution()` reads from the full-population distribution.

**Before**: Distribution from 1,000 capped pairs
**After**: Distribution from 22,381,694 total pairs: `{"Type-1": 12366266, "Type-2": 3307147, "Type-3": 6708281}`

#### Relative paths in clone pairs

Clone pair `func_a` / `func_b` paths changed from absolute (`/home/user/sources/repo/file.go::funcName`) to relative (`cmd/controller/app.go::newController`). Enabled by the `StampSourceFile` rootPath change.

---

### New Files Created

| File | Purpose |
|------|---------|
| `internal/analyzers/analyze/tick_bounds.go` | `TickBounds` type + `BuildTickBounds` helper |
| `internal/analyzers/analyze/metadata.go` | `AnalysisMetadata` struct + `NewAnalysisMetadata` constructor |
| `internal/analyzers/analyze/schema_registry.go` | Static schema registry for all 17 analyzers |
| `internal/identity/split.go` | `SplitIdentity(s string) (name, email string)` |

---

### Empty Analyzer Root Causes (Documented)

Investigation of 4 analyzers that returned empty data on kubernetes (1000 commits):

| Analyzer | Root Cause | Resolution |
|----------|-----------|------------|
| `burndown.developer_survival` | Disabled by default (`Burndown.TrackPeople: false`) | Enable via config |
| `burndown.file_survival` | Disabled by default (`Burndown.TrackFiles: false`) | Enable via config |
| `history/imports` | Requires UAST-enabled pipeline mode (`NeedsUAST() = true`) | Architectural dependency |
| `history/typos` | Requires UAST-enabled pipeline mode (`NeedsUAST() = true`) | Architectural dependency |
