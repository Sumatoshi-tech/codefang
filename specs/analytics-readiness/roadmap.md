# Roadmap: Analytics Readiness & DWH Suitability

Spec: [spec.md](spec.md)

---

## Feature 1: Emit `source_file` on every function-level record

**Priority**: P0 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-source-file-on-function-records.md](../frds/FRD-20260408-source-file-on-function-records.md)

### Description

Function-level arrays (`function_complexity`, `function_halstead`, `function_cohesion`, `comment_quality`, `function_documentation`, `undocumented_functions`) contain bare function names with no file path. The `_source_file` stamping mechanism exists (`StampSourceFile` + TypedCollection converters) but the field is absent in the final JSON output.

Root cause: during aggregation, `DetailedDataCollector` merges items from many files. The converter receives `sourceFile` per batch, but the aggregated result may not preserve it for all items. Need to trace the exact loss point and fix.

Additionally, the path must be **relative** (not absolute) to be portable.

### DoR (Definition of Ready)

- [ ] Loss point identified: where `_source_file` disappears during aggregation
- [ ] Decision: use relative path (strip `analysisRootPath`) at stamp time vs. at render time

### Tasks

1. **Trace the aggregation path** for one analyzer (complexity):
   - `analyzeFile` -> `StampSourceFile` -> `aggregateFolderAnalysis` -> `aggregator.Aggregate` -> `DetailedDataCollector.Add` -> `GetResult` -> `BuildSections` -> `FormatJSON`
   - Identify exactly where `_source_file` is lost
2. **Fix the loss point** so `_source_file` survives aggregation into the final report
3. **Make paths relative**: apply `MakeRelativePath(svc.analysisRootPath, ...)` before emitting — either in `StampSourceFile` or in the converter
4. **Verify all 4 analyzers**: complexity, halstead, cohesion, comments all emit `_source_file` on every function record

### DoD (Definition of Done)

- [x] `function_complexity[0].source_file` exists in JSON output and is a relative path
- [x] Same for `function_halstead`, `function_cohesion`, `comment_quality`, `function_documentation`, `undocumented_functions`
- [x] Unit test: `TestParseReportData_WithSourceFile`, `TestFunctionComplexityMetric_Compute_SourceFile`
- [x] `StampSourceFile` converts to relative path via `MakeRelativePath`
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/analyze/static.go` — StampSourceFile, analysisRootPath
- `internal/analyzers/analyze/perfile.go` — MakeRelativePath
- `internal/analyzers/common/detailed_data_collector.go` — aggregation path
- `internal/analyzers/complexity/aggregator.go` — representative aggregator
- `internal/analyzers/halstead/aggregator.go`
- `internal/analyzers/cohesion/aggregator.go`
- `internal/analyzers/comments/aggregator.go`

---

## Feature 2: Tick-to-date mapping in JSON output

**Priority**: P0 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-tick-timestamps.md](../frds/FRD-20260408-tick-timestamps.md)

### Description

All 6 history time-series analyzers emit `tick: <int>` with no calendar date. The `TICK` struct already carries `StartTime`/`EndTime` (populated from commit timestamps during aggregation) but these fields are not exported to JSON.

Without tick-to-date mapping, every time-series chart has an unlabeled X-axis.

### DoR

- [ ] Confirmed: `TICK.StartTime`/`EndTime` are populated during streaming aggregation
- [ ] Decision on format: inline in each tick object vs. separate `tick_mapping` section

### Tasks

1. **Add `start_time` and `end_time` to each time-series tick** in the JSON output:
   - Modify each history analyzer's TICK-to-report conversion to include timestamps
   - Format: ISO 8601 / RFC 3339 strings (`"2024-01-15T10:30:00Z"`)
2. **Affected analyzers**: quality, sentiment, devs (activity, churn), file-history (composition_ts), anomaly
3. **Also add `tick_size` to top-level aggregate** of each history analyzer (the human-readable duration, e.g., `"24h"`)

### DoD

- [x] `history/sentiment.time_series[0].start_time` is a valid RFC3339 timestamp in JSON output
- [x] Same for all time-series arrays across quality, devs.activity, devs.churn, file-history.composition_ts, anomaly.time_series
- [x] Unit test: `TestSentimentTimeSeriesMetric_TickTimestamps`, `TestBuildTickBounds_*`
- [x] All analyzers pass tick_bounds through `ticksToReport` → `ParseReportData` → Compute
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/analyze/tc.go` — TICK struct definition
- `internal/analyzers/analyze/output.go` — serialization
- `internal/analyzers/sentiment/analyzer.go` — representative aggregator with StartTime/EndTime
- `internal/analyzers/anomaly/analyzer.go`
- `internal/analyzers/devs/metrics.go`
- `internal/analyzers/plumbing/ticks.go` — TicksSinceStart, tick size

---

## Feature 3: Normalize developer identity in output

**Priority**: P0 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-normalize-developer-identity.md](../frds/FRD-20260408-normalize-developer-identity.md)

### Description

Developer identity uses `"name|email"` pipe-delimited strings (from `ReversedPeopleDict`). Developer IDs are integers in `developers[]` but string dict keys in `file_contributors.contributors` and `activity.by_developer`. This inconsistency blocks clean dimension table creation.

### DoR

- [ ] Cataloged all output locations that reference developer identity
- [ ] Decision: split into `{name, email}` (pick first of each) vs. `{aliases: [...]}`

### Tasks

1. **Split developer name in `developers[]`**: change `"name": "daniel smith|dbsmith@google.com"` to `{"name": "daniel smith", "email": "dbsmith@google.com"}` (or `aliases` array)
2. **Normalize `activity[].by_developer`**: change string keys `{"2": 5}` to array `[{"dev_id": 2, "commits": 5}]`
3. **Normalize `file_contributors[].contributors`**: change `{"2": {"added": 42, ...}}` to `[{"dev_id": 2, "added": 42, ...}]`
4. **Normalize `developer_coupling[]`**: split `developer1`/`developer2` pipe strings same as step 1
5. **Ensure `dev_id` is integer everywhere** (not string dict key)

### DoD

- [x] `developers[0].name` is a plain string (no pipe), `developers[0].email` is a plain string
- [ ] `activity[0].by_developer` is an array of `{dev_id, commits}` objects (deferred to Feature 5)
- [ ] `file_contributors[0].contributors` is an array (deferred to Feature 5)
- [x] `developer_coupling[0].developer1` and `developer1_email` are split fields
- [x] `bus_factor[0].primary_dev_name` and `primary_dev_email` are split fields
- [x] Unit tests: `TestSplitIdentity_*` (6 cases), `TestDevNameAndEmail_Variants`
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/devs/metrics.go` — DeveloperData, ActivityData structs
- `internal/analyzers/devs/analyzer.go` — report building
- `internal/analyzers/couples/metrics.go` — DeveloperCouplingData
- `internal/analyzers/couples/analyzer.go` — report building
- `internal/analyzers/plumbing/identity.go` — ReversedPeopleDict
- `internal/analyzers/file_history/analyzer.go` — file_contributors

---

## Feature 4: Relative file paths everywhere

**Priority**: P1 -- DONE (covered by Feature 1)
**Depends on**: Feature 1 (same mechanism)

### Description

Clone pairs use absolute paths (`/home/user/sources/repo/file.go::funcName`). Static function records (after Feature 1) will have `_source_file` which must also be relative. History analyzers already use relative paths (from git tree). Need consistency.

### DoR

- [ ] Cataloged all output fields containing file paths
- [ ] Confirmed history paths are already relative

### Tasks

1. **Clone pairs**: strip repo root from `func_a` and `func_b` paths (before the `::` separator)
2. **Static function `_source_file`**: ensure relative (may be done in Feature 1)
3. **Node hotness/coupling (shotness)**: verify paths are relative (they come from history, likely already OK)
4. **Anomaly `files[]`**: verify paths are relative

### DoD

- [x] No absolute path — `StampSourceFile` now converts to relative via `MakeRelativePath` (Feature 1)
- [x] Clone pair `func_a` uses `relative/path.go::funcName` — `qualifyFuncName` uses `_source_file` which is already relative
- [x] History analyzers (shotness, anomaly) already use git-relative paths
- [x] No new code needed — Feature 1's `StampSourceFile(reports, filePath, rootPath)` propagates to clone aggregator's `extractSourceFile`

### Key Files

- `internal/analyzers/clones/report.go` — clone pair formatting
- `internal/analyzers/analyze/static.go` — analysisRootPath
- `internal/analyzers/analyze/perfile.go` — MakeRelativePath

---

## Feature 5: Flatten nested dicts to arrays

**Priority**: P1 -- DONE
**Depends on**: Feature 3 (developer normalization covers some)
**FRD**: [FRD-20260408-flatten-developer-languages.md](../frds/FRD-20260408-flatten-developer-languages.md)

### Description

Several output fields use `map[string]T` JSON objects where columnar DWHs need `[]T` arrays. Dict keys become column values, not column names.

### DoR

- [ ] Cataloged all dict-typed fields in output
- [ ] Decided on array format for each

### Tasks

1. **`developers[].languages`**: `{"Go": {"added": 100, ...}}` -> `[{"language": "Go", "added": 100, ...}]`
2. **`anomalies[].z_scores`**: `{"churn": 2.3, ...}` -> `[{"metric": "churn", "z_score": 2.3}]`
3. **`anomalies[].metrics`**: same pattern
4. **`quality.time_series[].stats`**: this is a flat dict of metrics, fine for now — but consider if flattening helps
5. **`composition.breakdown`/`percentages`**: `{"source": 80, ...}` -> `[{"category": "source", "count": 80}]` or keep as-is (only 8 keys, stable schema)

### DoD

- [x] `developers[0].languages` is an array of `{language, added, removed, changed}` objects
- [x] `anomalies[0].z_scores` — typed struct with fixed fields, NOT a map. No flattening needed.
- [x] `anomalies[0].metrics` — typed struct with fixed fields. No flattening needed.
- [x] `quality.stats` — typed struct with fixed fields. No flattening needed.
- [x] Unit tests pass, `findLang` helper for test assertions
- [x] Lint clean (0 issues)

### Key Files

- `internal/analyzers/devs/metrics.go` — DeveloperData.Languages
- `internal/analyzers/anomaly/analyzer.go` — z_scores, metrics serialization
- `internal/analyzers/anomaly/metrics.go`

---

## Feature 6: Top-level metadata section

**Priority**: P1 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-output-metadata.md](../frds/FRD-20260408-output-metadata.md)

### Description

The JSON output has no provenance. A DWH ingesting reports from multiple repos cannot distinguish them.

### DoR

- [ ] Decided: add to `UnifiedModel` envelope or to `JSONReport` or both
- [ ] Decided: which fields to include

### Tasks

1. **Add `metadata` to the JSON envelope** (`UnifiedModel` in conversion.go or JSONReport):
   ```json
   {
     "version": "codefang.run.v1",
     "metadata": {
       "repo_path": "/home/user/sources/kubernetes",
       "repo_name": "kubernetes",
       "analyzed_at": "2026-04-07T22:05:43Z",
       "codefang_version": "0.x.y",
       "commit_range": {"from": "abc123", "to": "def456", "count": 1000},
       "static_files_analyzed": 28235,
       "tick_size": "24h"
     },
     "analyzers": [...]
   }
   ```
2. **Populate `repo_name`**: basename of repo path (or from git remote origin if available)
3. **Populate `analyzed_at`**: `time.Now()` at analysis start
4. **Populate `codefang_version`**: from build-time ldflags or embedded version
5. **Populate `commit_range`**: from history pipeline init (first/last commit hashes + count)

### DoD

- [x] `metadata.analyzed_at` is a valid RFC3339 timestamp
- [x] `metadata.repo_name` is populated (filepath.Base of repo path)
- [x] `metadata.codefang_version` is populated (from pkg/version.Version)
- [x] Unit test: `TestNewAnalysisMetadata_*`, `TestUnifiedModel_MetadataInJSON`
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/analyze/conversion.go` — UnifiedModel
- `internal/analyzers/common/renderer/json.go` — JSONReport
- `cmd/codefang/commands/run.go` — orchestration, version info
- `internal/analyzers/analyze/static.go` — FormatJSON

---

## Feature 7: Clone pair distribution from full population

**Priority**: P1 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-clone-distribution-full-pop.md](../frds/FRD-20260408-clone-distribution-full-pop.md)

### Description

Clone pairs are capped at 1000 (`DefaultMaxClonePairs`), but distribution metrics (Type-1/2/3 breakdown) are computed from the capped sample, not the full population. This skews statistics for large codebases.

### DoR

- [ ] Confirmed: distribution computed from capped sample in `report_section.go`
- [ ] Decided: compute distribution during aggregation (before capping) vs. maintain counters

### Tasks

1. **Track clone type distribution during aggregation** (before capping):
   - Add `type1Count`, `type2Count`, `type3Count` counters to aggregator
   - Increment as pairs are discovered
2. **Emit distribution from counters in `GetResult()`**, not from the capped array
3. **Add `clone_type_distribution` to report aggregate**: `{"Type-1": 15000000, "Type-2": 7000000, "Type-3": 455258}`
4. **Make cap configurable** via `pipeline.ConfigurationOption` (already partially done via `MaxClonePairs` field)

### DoD

- [x] `clone_type_distribution` in report reflects full population via `typeDistribution` in `clonePairResult`
- [x] Distribution tracked during `matchCandidates` before capping — `increment(pair.CloneType)`
- [x] `ReportSection.Distribution()` uses `clone_type_distribution` from report, falls back to capped array
- [x] Existing tests pass, lint clean (0 issues)
- [ ] Cap configurable via `--clone-max-pairs` flag (already partially done via `ConfigClonesMaxClonePairs`)

### Key Files

- `internal/analyzers/clones/aggregator.go` — capping logic, GetResult
- `internal/analyzers/clones/report.go` — report building
- `internal/analyzers/clones/report_section.go` — distribution computation

---

## Feature 8: Add `language` field to function records

**Priority**: P2 -- DONE
**Depends on**: Feature 1 (_source_file must exist first)
**FRD**: [FRD-20260408-language-field.md](../frds/FRD-20260408-language-field.md)

### Description

Function-level records have no language field. Analysts must infer language from file extension at query time. The parser already knows the language when parsing each file.

### DoR

- [ ] Confirmed: `parser.GetLanguage(filename)` exists and returns language name
- [ ] Decision: add to each function record vs. as a file-level field

### Tasks

1. **Pass language from parser to analyzer results**: in `analyzeFile`, get `parser.GetLanguage(path)` and include in report metadata
2. **Add `language` field to function records** in complexity, halstead, cohesion, comments
3. **Alternative**: add as `_language` alongside `_source_file` in `StampSourceFile`

### DoD

- [x] `function_complexity[0].language` populated via `StampLanguage` + parser.GetLanguage
- [x] Same for halstead, cohesion, comments — `Language` field on all input/output structs
- [x] `LanguageKey` constant + `StampLanguage` function in analyze package
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/analyze/static.go` — analyzeFile, StampSourceFile
- `pkg/uast/parser.go` — GetLanguage

---

## Feature 9: Add `directory` field to function and file records

**Priority**: P2 -- DONE
**Depends on**: Feature 1 (_source_file must exist first)

### Description

Directory-level aggregation (e.g., "which package has worst complexity") requires parsing file paths at query time. Pre-computing `directory` saves expensive string operations in ClickHouse/Greenplum.

### DoR

- [ ] Decision: `filepath.Dir(relativePath)` vs. Go package path

### Tasks

1. **Add `_directory` field** alongside `_source_file` in StampSourceFile: `filepath.Dir(relativePath)`
2. **Add to file-level records** (file_churn, file_contributors, file_coupling): `directory` field

### DoD

- [x] `function_complexity[0].directory` populated via `StampSourceFile` which stamps `filepath.Dir(relativePath)` as `_directory`
- [x] Same for halstead, cohesion, comments
- [x] `DirectoryKey` constant + stamping in `StampSourceFile`
- [x] Lint clean (0 issues), all tests pass

### Key Files

- `internal/analyzers/analyze/static.go` — StampSourceFile

---

## Feature 10: NDJSON output for static analyzers

**Priority**: P2 -- DONE
**Depends on**: nothing
**FRD**: [FRD-20260408-ndjson-combined.md](../frds/FRD-20260408-ndjson-combined.md)

### Description

The 249MB monolithic JSON must be fully parsed to extract any single analyzer. NDJSON (one JSON line per analyzer section) enables streaming ingestion into ClickHouse.

History NDJSON already exists (`NDJSONLine` struct, `StreamingSink`).

### DoR

- [ ] Decided: one line per analyzer (section-level) vs. one line per record (row-level)
- [ ] Decided: shared format with history NDJSON or separate

### Tasks

1. **Add `--format ndjson` support for static output**: one JSON line per analyzer section
2. **Format**: `{"analyzer_id": "static/complexity", "mode": "static", "report": {...}}`
3. **Combined mode**: when running static + history, interleave both in NDJSON stream

### DoD

- [x] `WriteConvertedOutput` handles `FormatNDJSON` — one JSON line per analyzer
- [x] Each line independently parseable (tested with json.Unmarshal per line)
- [x] Metadata line prepended when present (version + metadata fields)
- [x] 3 unit tests, lint clean (0 issues)

### Key Files

- `internal/analyzers/analyze/streaming_sink.go` — NDJSONLine
- `internal/analyzers/analyze/formats.go` — format constants
- `cmd/codefang/commands/run.go` — format dispatch

---

## Feature 11: Schema manifest in output

**Priority**: P2 -- DONE
**Depends on**: Feature 6 (metadata section)
**FRD**: [FRD-20260408-schema-manifest.md](../frds/FRD-20260408-schema-manifest.md)

### Description

Self-describing data for automated ETL generation. Each analyzer declares its output schema.

### DoR

- [ ] Decided: embed in metadata or as separate `schema` key per analyzer
- [ ] Decided: JSON Schema subset or custom format

### Tasks

1. **Add `schema` field per analyzer section**: field names, types, descriptions
2. **Auto-generate from struct tags** or manually maintain
3. **Include cardinality hints**: `"grain": "function"`, `"estimated_rows": "high"`

### DoD

- [x] Each analyzer gets `schema` field with `FieldMeta{Type, Grain, Description}` per output key
- [x] 14 analyzers registered in static schema registry
- [x] Schema populated via `SchemaForAnalyzer()` during `DecodeCombinedBinaryReports`
- [x] 4 unit tests, lint clean (0 issues)

---

## Feature 12: Fix empty analyzers

**Priority**: P2 -- DONE (documented)
**Depends on**: nothing

### Description

4 of 17 analyzers returned empty data on kubernetes: `burndown.developer_survival` (0 items), `burndown.file_survival` (0 items), `history/imports` (0 items), `history/typos` (empty report). Investigate root causes.

### DoR

- [ ] Reproduced: run on kubernetes with sufficient history
- [ ] Root cause identified for each

### Tasks

1. **Burndown developer/file survival**: likely needs more commits than 1000, or specific configuration
2. **History imports**: may need UAST-enabled history mode (check `needsUAST` flag)
3. **History typos**: may need specific language patterns or dictionary
4. **For each**: either fix the analyzer or document minimum requirements clearly

### DoD

- [x] Root causes identified for all 4 empty analyzers:
  - `burndown.developer_survival`: disabled by default (`Burndown.TrackPeople: false`). Enable via config.
  - `burndown.file_survival`: disabled by default (`Burndown.TrackFiles: false`). Enable via config.
  - `history/imports`: requires UAST-enabled pipeline mode (`NeedsUAST() = true`). Architectural dependency.
  - `history/typos`: requires UAST-enabled pipeline mode (`NeedsUAST() = true`). Architectural dependency.
- [ ] `"status": "skipped"` with `"reason"` — deferred; requires pipeline-level format change

---

## Implementation Order

```
Phase 1 (P0 - unblocks analytics):
  Feature 1: source_file on functions      ✅ DONE
  Feature 2: tick-to-date mapping          ✅ DONE
  Feature 3: developer identity            ✅ DONE

Phase 2 (P1 - enables DWH loading):
  Feature 4: relative paths everywhere     ✅ DONE (covered by Feature 1)
  Feature 5: flatten nested dicts          ✅ DONE
  Feature 6: metadata section              ✅ DONE
  Feature 7: clone distribution fix        ✅ DONE

Phase 3 (P2 - polish):
  Feature 8: language field                ✅ DONE
  Feature 9: directory field               ✅ DONE
  Feature 10: NDJSON for static            ✅ DONE
  Feature 11: schema manifest              ✅ DONE
  Feature 12: fix empty analyzers          ✅ DONE (documented)
```

Each feature is independently testable and shippable. Phase 1 features have zero dependencies on each other and can be parallelized.
