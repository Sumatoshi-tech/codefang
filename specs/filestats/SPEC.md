# CodeFang — Product Change Specification
**Based on:** Engineering feedback session transcript, 27 March 2026
**Product:** [github.com/Sumatoshi-tech/codefang](https://github.com/Sumatoshi-tech/codefang)
**Authors:** Dmitriy Gaevskiy, Dmitriy Nosov
**Spec version:** 1.2 — corrected: schema verified against source code (`internal/analyzers/common/renderer/json.go`); CLI syntax fixed to match `codefang run`; Feature 3 updated to build on existing `--format plot` / `codefang render` infrastructure; acknowledged existing checkpoint support; removed duplicate content

***

## Executive Summary

This specification describes three change initiatives derived from the engineering feedback session. The session identified gaps in (1) per-file granularity of static-analyzer output — highest priority, now verified against actual source code; (2) incremental/time-windowed analysis runs via caching (extending existing checkpoint infrastructure); and (3) visual dashboard output for CI reports and management (extending existing `--format plot` support). Change-Risk Factor integration (Ваня's metric) is explicitly deferred and not part of this version.

***

## Actual Current Output Schema

The JSON output structure is defined in `internal/analyzers/common/renderer/json.go` (types `JSONReport`, `JSONSection`, `JSONMetric`, `JSONDistribution`, `JSONIssue`).

### Confirmed Schema

```json
{
  "overall_score_label": "8/10",
  "overall_score": 0.8,
  "sections": [
    {
      "title": "COMPLEXITY",
      "score_label": "8/10",
      "score": 0.8,
      "status": "Human-readable status string",
      "metrics": [
        { "label": "Metric Name", "value": "123" }
      ],
      "distribution": [
        { "label": "Simple (1-5)", "percent": 1.0, "count": 1 }
      ],
      "issues": [
        { "name": "FunctionName", "location": "pkg/foo/bar.go:42", "value": "CC=14 | Cog=16 | Nest=1", "severity": "poor" }
      ]
    }
  ]
}
```

**Key observations from the source code:**

- The top-level `JSONReport` contains `overall_score`, `overall_score_label`, and `sections[]`. There is **no** top-level `title` field.
- Each `JSONSection` has a `title` field (the analyzer name, e.g., "COMPLEXITY", "HALSTEAD").
- **Sections are per-analyzer, not per-file.** Each section represents one analyzer's aggregated results across all analyzed files.
- There is no `file_path` field in `JSONSection`. Per-file provenance is tracked internally via `_source_file` metadata on collection items (`StampSourceFile` in `static.go`), but this is not exposed in the JSON output.

| Analyzer | `title` | `overall_score` type | `score: -1` meaning |
|----------|---------|---------------------|---------------------|
| cohesion | `COHESION` | float 0–1 | N/A |
| complexity | `COMPLEXITY` | float 0–1 | N/A |
| halstead | `HALSTEAD` | float 0–1 | N/A |
| imports | `IMPORTS` | `-1` (`ScoreInfoOnly`) | Informational only — no score |

**Critical finding:** The `sections` array contains one entry **per analyzer**, NOT per file. Per-file breakdowns do not exist in the current JSON output. The problem Nosov's team faces (launching the analyzer once per file) is caused by the absence of per-file data in the output — the entire Feature 1 must produce this capability.

### Existing CLI Interface

The analyzer is invoked via:
```
codefang run [path] --analyzers <ids> --format <format>
```

Key existing flags (from `cmd/codefang/commands/run.go`):
- `--analyzers`, `-a`: Analyzer IDs or glob patterns (e.g., `static/complexity,history/*`)
- `--format`: Output format (`json`, `yaml`, `plot`, `bin`, `timeseries`, `ndjson`, `text`, `compact`; default: `json`)
- `--path`, `-p`: Folder/repository path (default: `.`)
- `--output`, `-o`: Output directory for plot HTML files
- `--since`: Only analyze commits after this time (e.g., `24h`, `2024-01-01`, RFC3339)
- `--checkpoint`, `--resume`, `--checkpoint-dir`, `--clear-checkpoint`: Existing crash-recovery checkpointing
- `--workers`, `--static-workers`: Parallelism controls
- `--memory-budget`: Memory budget for auto-tuning

***

## Feature 1 — Per-File Output Mode

### Background

Nosov's team needs per-file breakdown without running the analyzer N times. The current JSON output provides one section per analyzer with aggregated metrics — there is no per-file breakdown. The `--per-file` flag must add per-file sections to the output while preserving the existing aggregated view.

### User Story

> **As a** platform engineer running codefang in CI,
> **I want** to control whether the analyzer emits per-file metric sections or only an aggregated summary,
> **So that** lightweight pipeline runs get a single score while deep-dive runs get full file-level breakdowns — in a single invocation, without running the analyzer once per file.

### Definition of Ready (DoR)

- [ ] Aggregation formula confirmed per analyzer: which fields are summed vs averaged vs max-taken (see table below — requires Gaevskiy sign-off)
- [ ] Behavior of `score: -1` informational analyzers (e.g., IMPORTS) in aggregation agreed
- [ ] Golden-file fixture for the new per-file output shape prepared

### Functional Requirements

| ID | Requirement |
|----|-------------|
| FR-1.1 | `codefang run` MUST accept a `--per-file` boolean flag (short alias: `-F`) applicable to static analyzers |
| FR-1.2 | **Without `--per-file`** (default, unchanged): output MUST match the current schema — `overall_score`, `overall_score_label`, and `sections[]` with one entry per analyzer containing aggregated metrics |
| FR-1.3 | **With `--per-file`**: each analyzer section MUST include a `files` array containing per-file entries with `file_path`, `score`, `score_label`, `status`, `metrics`, `distribution`, and `issues` |
| FR-1.4 | Each `files[]` entry MUST have the same structure as a section but with metrics computed from that single file |
| FR-1.8 | **With `--per-file`**: each aggregated section MUST include a `summary_stats` object containing `min`, `p25`, `p50`, `p75`, `p95`, `max`, `avg` for every numeric metric, computed across per-file values |
| FR-1.5 | For informational analyzers (`score: -1`, e.g., IMPORTS), per-file entries MUST list the imports found in each file; the `issues` list in the file entry MUST have `"location"` set to the source `file_path` |
| FR-1.6 | `files[].file_path` MUST always be relative to repository root |
| FR-1.7 | The flag MUST be composable with all existing `--format` options |

### Proposed Per-File Schema Extension

```json
{
  "overall_score_label": "8/10",
  "overall_score": 0.8,
  "sections": [
    {
      "title": "COMPLEXITY",
      "score_label": "8/10",
      "score": 0.8,
      "status": "Good - reasonable complexity",
      "metrics": [
        { "label": "Total Functions", "value": "156" }
      ],
      "summary_stats": {
        "Total Functions": { "min": 1, "p25": 3, "p50": 7, "p75": 14, "p95": 28, "max": 42, "avg": 9.8 }
      },
      "distribution": [ ... ],
      "issues": [ ... ],
      "files": [
        {
          "file_path": "pkg/foo/bar.go",
          "score_label": "6/10",
          "score": 0.6,
          "status": "Fair - some complex functions",
          "metrics": [
            { "label": "Total Functions", "value": "12" }
          ],
          "distribution": [ ... ],
          "issues": [ ... ]
        }
      ]
    }
  ]
}
```

The `files` array is only present when `--per-file` is set. The top-level section fields remain the aggregated view (current behavior, unchanged).

### Aggregation Rules

Each numeric metric in the aggregated section MUST include a statistical distribution computed across all per-file values:

| Statistic | Key | Description |
|-----------|-----|-------------|
| Minimum | `min` | Lowest per-file value |
| 25th percentile | `p25` | First quartile |
| 50th percentile (median) | `p50` | Median value |
| 75th percentile | `p75` | Third quartile |
| 95th percentile | `p95` | Near-maximum, excluding outliers |
| Maximum | `max` | Highest per-file value |
| Average | `avg` | Arithmetic mean across all files |

The `summary_stats` object is added to each aggregated section alongside the existing `metrics` array:

```json
{
  "title": "COMPLEXITY",
  "score": 0.8,
  "score_label": "8/10",
  "status": "Good - reasonable complexity",
  "metrics": [
    { "label": "Total Functions", "value": "156" }
  ],
  "summary_stats": {
    "Total Functions":    { "min": 1, "p25": 3,  "p50": 7,  "p75": 14, "p95": 28, "max": 42, "avg": 9.8 },
    "Avg Complexity":     { "min": 1, "p25": 2,  "p50": 4,  "p75": 7,  "p95": 12, "max": 18, "avg": 5.1 },
    "Max Complexity":     { "min": 1, "p25": 3,  "p50": 6,  "p75": 11, "p95": 20, "max": 35, "avg": 8.2 },
    "Total Complexity":   { "min": 1, "p25": 8,  "p50": 22, "p75": 45, "p95": 90, "max": 150,"avg": 30.5 },
    "Cognitive Total":    { "min": 0, "p25": 5,  "p50": 15, "p75": 35, "p95": 70, "max": 120,"avg": 22.0 },
    "Decision Points":    { "min": 0, "p25": 2,  "p50": 8,  "p75": 18, "p95": 40, "max": 65, "avg": 12.3 }
  },
  "distribution": [ ... ],
  "issues": [ ... ],
  "files": [ ... ]
}
```

This applies uniformly to all analyzers — every numeric metric reported per-file gets the same 7-stat distribution in the aggregated section. The existing `metrics` array continues to show the current aggregated totals (sum, weighted avg, etc.) for backward compatibility.

#### Metric-Specific Total Rules

The `metrics[].value` field in the aggregated section (the single rolled-up number) follows these rules:

| Analyzer | Metric Field | Total in `metrics[].value` |
|----------|-------------|---------------------------|
| COMPLEXITY | Total Functions | sum |
| COMPLEXITY | Avg Complexity | weighted avg by function count |
| COMPLEXITY | Max Complexity | max |
| COMPLEXITY | Total Complexity | sum |
| COMPLEXITY | Cognitive Total | sum |
| COMPLEXITY | Decision Points | sum |
| HALSTEAD | Total Functions | sum |
| HALSTEAD | Distinct Operators (n1) | sum |
| HALSTEAD | Distinct Operands (n2) | sum |
| HALSTEAD | Total Operators (N1) | sum |
| HALSTEAD | Total Operands (N2) | sum |
| HALSTEAD | Vocabulary | union count |
| HALSTEAD | Volume | sum |
| HALSTEAD | Difficulty | weighted avg |
| HALSTEAD | Effort | sum |
| HALSTEAD | Est. Bugs | sum |
| COHESION | Total Functions | sum |
| COHESION | LCOM Score | avg |
| COHESION | Cohesion Score | avg |
| COHESION | Avg Cohesion | avg |
| IMPORTS | Unique Imports | count of deduplicated import names across all files |
| IMPORTS | Total Files | count |

### Acceptance Criteria

```
GIVEN a repository with N source files

WHEN codefang run --analyzers static/* --format json is run WITHOUT --per-file
THEN output matches current schema exactly (no breaking change)
  AND sections[] has one entry per analyzer with aggregated metrics

WHEN codefang run --analyzers static/* --format json --per-file is run
THEN output contains all current fields unchanged
  AND each section contains a "files" array
  AND files[] length equals N (source files only, no vendor/generated)
  AND each file entry has file_path, score, score_label, status, metrics, issues
  AND each section contains "summary_stats" with min, p25, p50, p75, p95, max, avg for every numeric metric
  AND summary_stats values are computed from the per-file metric values
  AND aggregated metrics[].value equals the roll-up of file values per metric-specific total rules
  AND total wall-clock time is ≤ 2x baseline
```

### Architectural Outline

Changes span four layers: CLI, Service, Aggregation, and Renderer.

#### Layer 1 — CLI (`cmd/codefang/commands/run.go`)

- Add `--per-file` / `-F` boolean flag to `RunCommand`.
- Pass flag value through to `StaticService` (new field).
- No changes to history pipeline dispatch.

#### Layer 2 — Service (`internal/analyzers/analyze/static.go`)

- Add `PerFile bool` field to `StaticService`.
- `AnalyzeFolder()` already calls `StampSourceFile(reportMap, filePath)` which tags every collection item with `_source_file`. This metadata is the key to reconstructing per-file sections.
- When `PerFile` is true, `StaticService` must propagate the mode to aggregators via `AggregationModeAware` (or a new `PerFileAware` interface).
- `BuildSections()` remains unchanged — it produces aggregated `ReportSection` per analyzer as today.
- New method `BuildPerFileSections(results) map[analyzerName][]ReportSection` — groups results by `_source_file`, calls `CreateReportSection` per file. Each `ReportSection` carries its `file_path`.
- `FormatJSON()` passes both aggregated sections and per-file sections to the renderer.

#### Layer 3 — Aggregation (`internal/analyzers/*/aggregator.go`)

Each static analyzer aggregator (complexity, halstead, cohesion, imports) currently implements `ResultAggregator`:

```
Aggregate(results map[string]Report)  // merges per-file reports into one
GetResult() Report                    // returns aggregated report
```

When `--per-file` is active:

- Aggregators must **retain per-file Report snapshots** before merging. Add a `perFileReports map[string]Report` (keyed by `_source_file` path) to each aggregator.
- `GetResult()` returns the aggregated report as today. A new `GetPerFileResults() map[string]Report` returns the retained per-file data.
- **`summary_stats` computation**: New shared utility in `internal/analyzers/common/stats/` computes `{min, p25, p50, p75, p95, max, avg}` from a `[]float64`. Called after all files are aggregated, iterating `perFileReports` to extract each numeric metric's per-file values.
- Base aggregator in `internal/analyzers/common/` should provide the retention and stats logic to avoid duplication across all four analyzers.

#### Layer 4 — Renderer (`internal/analyzers/common/renderer/json.go`)

- Add `Files []JSONFileEntry` to `JSONSection` (omitempty — absent without `--per-file`).
- Add `SummaryStats map[string]StatsSummary` to `JSONSection` (omitempty).

```go
type JSONFileEntry struct {
    FilePath     string             `json:"file_path"`
    ScoreLabel   string             `json:"score_label"`
    Status       string             `json:"status"`
    Metrics      []JSONMetric       `json:"metrics"`
    Distribution []JSONDistribution `json:"distribution,omitempty"`
    Issues       []JSONIssue        `json:"issues"`
    Score        float64            `json:"score"`
}

type StatsSummary struct {
    Min float64 `json:"min"`
    P25 float64 `json:"p25"`
    P50 float64 `json:"p50"`
    P75 float64 `json:"p75"`
    P95 float64 `json:"p95"`
    Max float64 `json:"max"`
    Avg float64 `json:"avg"`
}
```

- `SectionsToJSON()` signature changes to accept optional per-file data. Alternatively, introduce `SectionsToJSONPerFile(sections, perFileSections, summaryStats)`.
- Text and compact renderers (`RenderText`, `RenderCompact`) are unaffected — they already use `AggregationModeSummaryOnly`.
- Plot renderer: per-file data enables new per-file distribution charts in `plotpage` sections.

#### Data Flow (with `--per-file`)

```
CLI: --per-file flag
  ↓
StaticService.PerFile = true
  ↓
AnalyzeFolder():
  for each file:
    analyzer.Analyze(uast) → Report
    StampSourceFile(report, filePath)     ← already exists
    aggregator.Aggregate(report)          ← now also retains per-file snapshot
  ↓
aggregator.GetResult() → aggregated Report     ← unchanged
aggregator.GetPerFileResults() → map[path]Report  ← NEW
  ↓
BuildSections(aggregated) → []ReportSection        ← unchanged
BuildPerFileSections(perFile) → map[analyzer][]ReportSection  ← NEW
ComputeSummaryStats(perFile) → map[analyzer]map[metric]StatsSummary  ← NEW
  ↓
Renderer.SectionsToJSON():
  JSONSection.Files = per-file entries
  JSONSection.SummaryStats = stats
  ↓
JSON output
```

### Definition of Done (DoD)

- [ ] `--per-file` / `-F` implemented for ALL static analyzers
- [ ] Default output (no flag) is **unchanged** — no breaking change
- [ ] Golden-file tests pass for both modes on each analyzer
- [ ] Unit tests: flag absent, flag present, empty repo, binary-only repo, IMPORTS per-file attribution
- [ ] `go test -race` passes
- [ ] Performance: single `--per-file` run ≤ 2x baseline time
- [ ] `--help` updated
- [ ] PR approved by Gaevskiy + one additional reviewer

***

## Feature 2 — Incremental History Analysis via Caching

### Background

Nosov requested `--since <date>` windowing on the history/burndown analyzer for weekly CI runs (e.g., "only look at commits since last Monday"). Gaevskiy identified a fundamental algorithmic issue: the burndown analyzer tracks lines across the **entire ordered commit graph**. Inserting a date cutoff mid-stream breaks line attribution — lines appear to vanish, producing silent data corruption. The `--since` approach is architecturally unsafe.

The agreed resolution is a **full-history run with incremental state caching**: the first run processes everything; subsequent runs replay only new commits on top of a persisted snapshot.

### Existing Infrastructure

Codefang already has crash-recovery checkpointing:
- `--checkpoint` (default: true) / `--resume` (default: true) / `--checkpoint-dir` / `--clear-checkpoint`
- Interface: `checkpoint.Checkpointable` with `SaveCheckpoint(dir)`, `LoadCheckpoint(dir)`, `CheckpointSize()`
- Implemented by: burndown, couples, file-history analyzers

Feature 2 extends this into a **production-grade incremental cache** that survives across separate invocations and only replays new commits.

### User Story

> **As a** CI pipeline operator running codefang weekly,
> **I want** subsequent runs to reuse cached analysis state from previous runs,
> **So that** I get accurate incremental metric deltas quickly without re-processing the full history every time.

### User Journey

```
Week 0 — first run:
  codefang run --path ./bank --cache-dir /var/codefang/cache --format json -o metrics.json
  → Full history analyzed (minutes on large repos)
  → State snapshot saved to: cache-dir/HEAD_<sha>.bin

Week 1 — incremental run:
  codefang run --path ./bank --cache-dir /var/codefang/cache --format json -o metrics.json
  → Detects cached state at last known HEAD
  → Replays only 47 new commits
  → Updates snapshot
  → Prints: "Replaying 47 commits vs 500,000 full history (est. 98% time saved)"
  → Emits full metrics with complete historical context preserved

Developer inspects output:
  → Burndown continuity preserved (no gaps)
  → --since used as output filter: shows delta since Monday without re-running analysis
```

### Definition of Ready (DoR)

- [ ] Cache serialization format decided — potentially extend existing `checkpoint.Checkpointable` interface (see OQ-4)
- [ ] Cache invalidation strategy documented: force-push, history rewrite, shallow clone behavior
- [ ] Maximum cache file size limit agreed for 500k-commit repos
- [ ] `--since` fate decided: deprecated or repurposed as post-analysis output filter (see OQ-2)

### Functional Requirements

| ID | Requirement |
|----|-------------|
| FR-2.1 | `--cache-dir <path>` flag MUST persist a binary state snapshot after each completed run |
| FR-2.2 | On subsequent runs with a valid cache, only commits newer than cached HEAD MUST be replayed |
| FR-2.3 | On stale cache (force-push / history rewrite detected), the tool MUST warn to stderr and fall back to full re-analysis |
| FR-2.4 | `--since <date>` MUST be repurposed as a **post-analysis output filter** (show delta since date) — MUST NOT truncate the history walk |
| FR-2.5 | Cache files MUST be keyed by repo root commit SHA + branch name |
| FR-2.6 | The tool MUST print estimated time savings when using cache: `"Replaying N commits vs M total"` |
| FR-2.7 | `--no-cache` flag MUST force full re-analysis and overwrite any existing cache |

### Non-Functional Requirements

| ID | Requirement |
|----|-------------|
| NFR-2.1 | Incremental run with < 100 new commits: < 30 seconds on 8-core/32 GB server |
| NFR-2.2 | Cache file: ≤ 500 MB for up to 500,000-commit repos |
| NFR-2.3 | Full-history analysis on very large repos (e.g., 500k commits): ≤ 30 minutes |

### Acceptance Criteria

```
GIVEN a repository with 500,000 commits and a valid cache from the previous run
WHEN 50 new commits are added and codefang run is re-invoked with --cache-dir
THEN only the 50 new commits are replayed
  AND output is byte-for-byte identical to a full re-run on the same commit range
  AND runtime is < 30 seconds
  AND --since as output filter returns only delta metrics without re-running analysis
```

### Architectural Outline

Changes span four layers: CLI, Checkpoint/Cache, Pipeline/Framework, and Runner.

#### Layer 1 — CLI (`cmd/codefang/commands/run.go`)

- Add `--cache-dir <path>` flag to `RunCommand`. Distinct from `--checkpoint-dir` (crash recovery) — this is a persistent cross-run cache.
- Add `--no-cache` flag to force full re-analysis.
- Repurpose `--since` semantics: parse as before, but pass to output formatting (post-filter) instead of `HistoryRunOptions.Since` (commit selection). This is a **breaking change** to `--since` behavior.
- Wire `cache-dir` into `HistoryRunOptions` as a new `CacheDir` field.

#### Layer 2 — Cache Layer (`internal/checkpoint/` → extend or new `internal/cache/`)

The existing checkpoint system (`internal/checkpoint/`) provides crash-recovery within a single run. The incremental cache extends this to persist **completed** analysis state across invocations.

**Option A — Extend `checkpoint.Manager`:**
- Add `SaveCache(analyzers, aggregators, headSHA, branch)` — writes a finalized snapshot after successful completion (vs. mid-run checkpoint).
- Add `LoadCache(headSHA, branch) → (analyzers, aggregatorSpillState, lastCommitIndex)` — restores state from a prior completed run.
- Cache key: `SHA256(rootCommitSHA + branch)` stored under `<cache-dir>/<key>/`.
- Cache metadata: `cache.json` with `{version, head_sha, branch, root_sha, commit_count, analyzer_ids, timestamp}`.

**Option B — New `internal/cache/` package** (cleaner separation):
- `IncrementalCache` wraps the same `Checkpointable` interface but adds:
  - Head SHA tracking and validation.
  - Force-push / history-rewrite detection: compare stored root commit SHA with current repo root.
  - Staleness detection: if stored HEAD is not an ancestor of current HEAD → warn + full re-run.

**Both options reuse** the existing `Checkpointable` interface on analyzers (burndown, couples, file-history already implement `SaveCheckpoint`/`LoadCheckpoint`). The `GenericAggregator[S,T]` already supports `SpillState()`/`RestoreSpillState()` for persisting aggregator state.

#### Layer 3 — Framework (`internal/framework/runner.go`)

- `Runner.Run()` currently has four phases: init analyzers → init aggregators → process commits → finalize.
- Insert **Phase 0 — Cache Probe** before Phase 1:
  - Check `CacheDir` for valid cache matching current repo.
  - If valid: `LoadCheckpoint()` on each analyzer, `RestoreSpillState()` on aggregators, set `startIndex` to cached commit count.
  - If stale/missing: log warning, proceed with full run.
- Modify **Phase 3 — Process Commits**:
  - `CommitStreamer` must skip commits `[0, startIndex)` and only stream `[startIndex, HEAD]`.
  - The `Coordinator` already receives a commit list — filter it before passing.
- Insert **Phase 5 — Cache Write** after Phase 4:
  - On successful completion: `SaveCheckpoint()` on each analyzer, persist aggregator spill state, write cache metadata with new HEAD SHA.

#### Layer 4 — Output Filter for `--since` (`cmd/codefang/commands/run.go`)

- After `Runner.Run()` returns the full `map[HistoryAnalyzer]Report`:
  - If `--since` is set, filter `[]TICK` to only those with `EndTime >= since`.
  - Pass filtered TICKs to `analyzer.ReportFromTICKs()` for report generation.
  - This preserves full-history accuracy while showing only the requested time window.

#### Data Flow (incremental run)

```
CLI: --cache-dir /var/codefang/cache
  ↓
Runner Phase 0 — Cache Probe:
  cache-dir/<key>/cache.json → {head_sha: "abc123", commit_count: 500000}
  Validate: is "abc123" ancestor of current HEAD? ← git merge-base --is-ancestor
  ↓ valid
  analyzer.LoadCheckpoint(cache-dir/<key>/analyzer_N/)
  aggregator.RestoreSpillState(cache-dir/<key>/spill/)
  startIndex = 500000
  ↓
Runner Phase 3 — Process Commits:
  CommitStreamer: skip [0, 500000), stream [500000, 500047]
  Coordinator: BlobPipeline → DiffPipeline → UASTPipeline (47 commits only)
  Analyzers: Consume() 47 commits, Add() to aggregators
  ↓
Runner Phase 4 — Finalize:
  FlushAllTicks() → full []TICK (including historical state from cache)
  ReportFromTICKs() → complete Report
  ↓
Runner Phase 5 — Cache Write:
  analyzer.SaveCheckpoint(cache-dir/<key>/analyzer_N/)
  Save aggregator spill state
  Write cache.json with new head_sha
  ↓
Output (optionally filtered by --since)
```

### Definition of Done (DoD)

- [ ] Cache write/read implemented — extending or replacing existing checkpoint infrastructure
- [ ] Force-push / history-rewrite detection tested
- [ ] Determinism test: full-run == incremental-run on identical commit range
- [ ] `--since` behavior change documented as **BREAKING** in CHANGELOG with migration note
- [ ] `--cache-dir`, `--no-cache` documented in `--help`
- [ ] Performance benchmark results documented in PR for reference repos
- [ ] PR approved by Gaevskiy

***

## Feature 3 — Extended Visual Dashboard Output

### Background

Gaevskiy demonstrated charts from external analysis tools showing contributor workload pie charts, burndown area plots, and coupling heatmaps. Codefang already has visualization support via `--format plot` and `codefang render`, using go-echarts for multi-page HTML output (pie, bar, line charts). Existing plot support covers: anomaly, burndown, cohesion, complexity, couples, halstead analyzers. Feature 3 extends this with additional chart types and CI-friendly output options.

### Existing Infrastructure

- `codefang run --format plot --output <dir>`: Generates per-analyzer HTML pages with go-echarts
- `codefang render <store-dir> --output <output-dir>`: Renders stored results to multi-page HTML
- `plotpage.MultiPageRenderer`: Renders per-analyzer pages with index navigation
- Per-analyzer `PlotSections` functions generate chart data
- Supported chart types: Pie, Bar, Line (via `plotpage` package)

### User Stories

> **As a** engineering manager,
> **I want** a contributor workload pie chart distinguishing humans from bots,
> **So that** I can identify bus-factor risk and remove bot noise from contribution stats.

> **As a** tech-lead,
> **I want** a burndown area plot over full repository history,
> **So that** I can distinguish codebases that refactor regularly from those that only grow by isolated feature blocks.

> **As a** developer,
> **I want** a file coupling heatmap,
> **So that** I can identify hidden architectural dependencies between files that change together.

### Customer Journey Map (CJM)

| Stage | Actor | Action | Tool Output | Pain Point / Opportunity |
|-------|-------|--------|-------------|--------------------------|
| **Trigger** | Eng. Manager | Schedules weekly CI pipeline | — | No automated summary report combining all charts |
| **Execution** | CI agent | `codefang run --format plot --output ./reports` | Per-analyzer HTML pages | Missing: contributor workload, coupling heatmap, combined dashboard |
| **Review** | Tech-lead | Opens `reports/index.html` | Multi-page HTML with navigation | Some chart types missing (heatmap, area) |
| **Triage** | Tech-lead | Opens coupling page | Coupling chart | Heatmap visualization not yet available |
| **Action** | Manager | Opens contributor page | Contributor workload | Bot accounts inflate stats; no detection today |
| **Archiving** | CI agent | Attaches HTML dir as build artifact | — | No single-file summary option |

### Definition of Ready (DoR)

- [ ] New chart types and their data sources documented (which analyzer metrics feed which chart)
- [ ] Bot-detection heuristics agreed (GitHub Actions, Dependabot, Renovate, email patterns)
- [ ] Heatmap rendering approach decided (go-echarts HeatMap or custom SVG)

### Functional Requirements

| ID | Requirement |
|----|-------------|
| FR-3.1 | `codefang run --format plot` MUST generate the following additional chart types beyond existing support: contributor workload distribution, file coupling heatmap, analyzer score-over-time line chart |
| FR-3.2 | Bot filtering: `--exclude-bots` (auto-detect common patterns); `--exclude-author <pattern>` for custom patterns |
| FR-3.3 | `codefang render` MUST produce a combined dashboard `index.html` with all charts (extending existing multi-page index) |
| FR-3.4 | `--chart-format svg` option MUST produce standalone SVG files alongside HTML pages for embedding in CI artifacts |
| FR-3.5 | `report.json` MUST be emitted alongside charts (raw data for external dashboards) |

### Acceptance Criteria

```
GIVEN a repository analyzed with full history
WHEN codefang run --format plot --output ./reports --exclude-bots is executed
THEN reports/index.html exists with navigation to all chart pages
  AND contributor workload chart does NOT include authors matching bot patterns
  AND coupling heatmap page exists
  AND report.json contains raw data for all charts
  AND all HTML files are renderable in Chrome/Firefox
```

### Architectural Outline

Changes span four layers: CLI, Analyzer, Plot/Visualization, and Render.

#### Layer 1 — CLI (`cmd/codefang/commands/run.go`, `render.go`)

- Add `--exclude-bots` boolean flag — enables automatic bot author filtering.
- Add `--exclude-author <pattern>` string slice flag — custom regex patterns for author exclusion.
- Add `--chart-format svg` option — emits standalone SVG files alongside HTML.
- Both flags apply to `--format plot` and `codefang render`.
- Wire flags into `HistoryRunOptions` (new `ExcludeBots bool`, `ExcludeAuthorPatterns []string`).

#### Layer 2 — Analyzer Layer (`internal/analyzers/`)

**Bot filtering** applies to history analyzers that track author identity:

- `internal/plumbing/identity.go` — The `IdentityDetector` (core/plumbing analyzer) resolves author identities. Add a `BotFilter` that tags identities as bot/human based on:
  - Known patterns: `*[bot]@*`, `*github-actions*`, `*dependabot*`, `*renovate*`, `*noreply@*`.
  - Custom patterns from `--exclude-author`.
- `internal/analyzers/devs/` — Developer expertise analyzer. Filter bot authors from workload aggregation. Add `PlotSections` function that generates contributor workload pie chart data (new).
- `internal/analyzers/couples/` — Coupling analyzer already has plot support. Add heatmap data export alongside existing coupling chart.

**New chart data sources:**

| Chart | Source Analyzer | Data |
|-------|----------------|------|
| Contributor workload pie | `history/devs` | Author → lines added/removed, filtered by bot flag |
| File coupling heatmap | `history/couples` | File pair co-change matrix from `CouplingResult` |
| Score-over-time line | all static via `ReportStore` | Per-run score snapshots (requires `report.json` history) |

#### Layer 3 — Plot/Visualization (`internal/analyzers/common/plotpage/`)

- **Heatmap component**: Add `HeatMap` type implementing `Renderable` interface. Uses go-echarts `charts.HeatMap` or custom SVG template for the coupling matrix.
- **Area chart component**: Add `AreaChart` type for burndown area plots (go-echarts supports area via `charts.Line` with `AreaStyle`).
- **Pie chart for contributors**: Already supported via `charts.Pie` in plotpage — wire to devs analyzer data.
- Register new `PlotSections` functions in `internal/analyzers/analyze/static.go` → `PlotSectionsFor()` dispatch table.

New plot sections to register:

```go
// internal/analyzers/devs/plot.go (new file)
func PlotSections(report Report) ([]plotpage.Section, error)
  → Section{Title: "Contributor Workload", Chart: PieChart{...}}

// internal/analyzers/couples/plot.go (extend existing)
func PlotSections(report Report) ([]plotpage.Section, error)
  → append: Section{Title: "File Coupling Heatmap", Chart: HeatMap{...}}
```

#### Layer 4 — Render (`cmd/codefang/commands/render.go`, `internal/analyzers/common/renderer/`)

- `codefang render` already reads from `ReportStore` and calls `MultiPageRenderer`. Extend to:
  - Include new chart pages from devs and enhanced couples analyzers.
  - Emit `report.json` alongside HTML pages — serialize all `ReportStore` data as a single JSON file.
  - `RebuildIndex()` already scans `*.html` and generates `index.html` — new pages are picked up automatically.
- **SVG export**: When `--chart-format svg` is set, after each `Page.Render()` call, also invoke a `RenderSVG()` method that renders each `Section.Chart` as a standalone SVG file in the output directory.

#### Layer 5 — Bot Filtering Pipeline

```
CLI: --exclude-bots --exclude-author "renovate.*"
  ↓
HistoryRunOptions.ExcludeBots = true
HistoryRunOptions.ExcludeAuthorPatterns = ["renovate.*"]
  ↓
Runner Phase 1 — Init Analyzers:
  IdentityDetector.SetBotPatterns(builtinPatterns + customPatterns)
  ↓
Runner Phase 3 — Process Commits:
  for each commit:
    IdentityDetector.Consume(ctx) → TC with AuthorID
    AuthorID flagged as bot? → analyzer receives is_bot metadata
    devs.Consume(): skips bot authors in workload aggregation
    couples.Consume(): optionally excludes bot-authored changes
  ↓
Runner Phase 4 — Finalize:
  devs.ReportFromTICKs() → Report with human-only contributor data
  couples.ReportFromTICKs() → Report with coupling matrix
  ↓
FormatPlotPages():
  devs PlotSections → Contributor Workload Pie (humans only)
  couples PlotSections → Coupling Heatmap (new) + existing coupling chart
  MultiPageRenderer.RenderIndex() → index.html with all pages
  Emit report.json → serialized raw data
```

### Definition of Done (DoD)

- [ ] New chart types implemented and visually reviewed on sample repos
- [ ] Bot detection covers GitHub Actions, Dependabot, Renovate, custom `--exclude-author` patterns
- [ ] Integration test on a reference public repo: HTML pages exist and are non-empty
- [ ] `--format plot` documented in README with example screenshots
- [ ] PR approved by Gaevskiy + Nosov

***

## Cross-Cutting Concerns

### Output Format Conventions

All new fields MUST conform to the confirmed schema from `internal/analyzers/common/renderer/json.go`.

- `null` for unavailable/non-applicable values — never omit, never use `0` as sentinel
- `score: -1` convention (`ScoreInfoOnly`) for informational analyzers MUST be preserved
- `snake_case` for all new JSON keys
- `files` array MUST be `[]` (empty array, not omitted) when `--per-file` is set but no files match for an analyzer

### Performance SLA Table

| Repository Scale | Full Run SLA | Incremental Run SLA (Feature 2) |
|------------------|-------------|--------------------------------|
| < 3,000 commits | < 60 s | < 10 s |
| 3,000–50,000 commits | < 5 min | < 30 s |
| 50,000–500,000 commits | < 30 min | < 2 min |

### Testing Strategy

- **Unit tests**: deterministic fixture repos; golden-file comparison
- **Integration tests**: public reference repos (go-git, codefang itself)
- **Regression gate**: no existing CLI flag may change its output without a `BREAKING` CHANGELOG entry
- **Performance gate**: CI benchmark must not regress > 10% vs baseline on the 3,000-commit fixture repo

***

## Prioritized Backlog

| Priority | Feature | Blocking Dependencies | Complexity |
|----------|---------|----------------------|------------|
| P0 | FR-1: `--per-file` flag + per-file sections in output | OQ-5 (aggregation rules) | Medium |
| P1 | FR-2: Incremental cache (extending checkpoint infra) | OQ-2, OQ-4 | High |
| P2 | FR-3: Extended visual dashboard | FR-1 (per-file data), FR-2 (full history) | High |

***

## Open Questions

| # | Question | Owner | Blocks |
|---|----------|-------|--------|
| OQ-2 | `--since`: deprecated entirely or repurposed as post-analysis output filter? | Gaevskiy | FR-2 kickoff |
| OQ-3 | Heatmap rendering: go-echarts HeatMap component or custom SVG? | Gaevskiy | FR-3 kickoff |
| OQ-4 | Incremental cache format: extend existing `checkpoint.Checkpointable` interface or separate format? | Gaevskiy | FR-2 kickoff |
| OQ-5 | Confirm or correct per-analyzer aggregation rules table in FR-1 | Gaevskiy | FR-1 implementation |
