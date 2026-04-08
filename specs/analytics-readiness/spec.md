# Analytics Readiness & DWH Suitability

## Problem

Codefang's JSON output is rich (17 analyzers, 1M+ function-level rows, time-series, coupling data) but structurally hostile to analytics tooling. A data analyst cannot build dashboards without significant ETL engineering.

Key blockers identified from a production run against kubernetes (28K files, 1000 commits, 249MB JSON):

### P0 - Blocks analytics entirely

1. **Function records lack file paths**: `function_complexity[]`, `function_halstead[]`, `function_cohesion[]`, `comment_quality[]` have bare names ("ForKind") with no `_source_file`. 1M+ rows are unjoinable to files.
2. **Ticks have no calendar dates**: All 6 time-series analyzers use opaque integer ticks (0-123) with no mapping to real dates. TICK structs carry StartTime/EndTime in memory but don't export them to JSON.
3. **Developer identity is denormalized**: `"name|email"` pipe format, inconsistent ID types (int in `developers[]`, string dict keys in `file_contributors.contributors`).

### P1 - Blocks efficient DWH usage

4. **Absolute file paths**: Clone pairs use `/home/user/sources/repo/...` absolute paths. Not portable across machines.
5. **Nested dicts instead of arrays**: `by_developer`, `contributors`, `languages`, `z_scores` are `map[string]T` — need custom UNNEST ETL.
6. **No top-level metadata**: No repo name, URL, analysis timestamp, codefang version in output.
7. **Clone pair explosion**: 22.4M pairs (O(n^2)) — already capped at 1000 in output, but distribution metrics computed from capped sample, not full population.

### P2 - Nice to have for rich analytics

8. **No NDJSON for static**: NDJSON exists for history only.
9. **No language field on functions**: Must infer from file extension.
10. **No directory field**: Must parse paths at query time for directory-level aggregation.
11. **No schema manifest**: No self-describing schema in output.
12. **Empty analyzers**: burndown.developer_survival, burndown.file_survival, history/imports, history/typos return empty.

## Codebase Findings

### _source_file mechanism (EXISTS but has gap)

- `StampSourceFile` (static.go) stamps `TypedCollection.SourceFile` per file after analysis
- Converters (e.g., complexity `convertFunctionReportItems`) DO add `_source_file` when `sourceFile != ""`
- This flows through `DetailedDataCollector.AddToResult()` which calls `tc.ToMaps(tc.Items, tc.SourceFile)`
- **Gap**: In the final aggregated report, function records may lose `_source_file` during aggregation — the DetailedDataCollector collects items from many files but the TypedCollection converter is called per-file-batch. Need to verify the aggregation path preserves the field.

### Tick timestamps (EXIST in memory, not exported)

- `TICK` struct has `StartTime` and `EndTime` fields (analyze/tc.go)
- Populated by aggregators during `Add(tc)` from `tc.Timestamp`
- `TicksSinceStart` plumbing analyzer maps tick -> commit hashes
- Tick size is configurable (default 24h)
- **Gap**: No analyzer exports tick timestamps to JSON. Need a `tick_mapping` section.

### Developer identity (well-structured internally)

- `IdentityDetector` plumbing assigns stable integer IDs
- `ReversedPeopleDict` ([]string) maps ID -> pipe-delimited identity string
- Pipe format: `"name1|name2|email1|email2"` (all aliases sorted)
- **Gap**: Output uses raw pipe string. Need split into `{name, email}` or `{aliases: [...]}`.

### Path handling (partial)

- `MakeRelativePath(rootPath, filePath)` exists in perfile.go
- Only applied in per-file JSON enrichment, NOT to function records or clone pairs
- `analysisRootPath` stored on StaticService

### Clone capping (already implemented)

- `DefaultMaxClonePairs = 1000`
- `total_clone_pairs` reports exact count (22M)
- Distribution computed from capped sample (known limitation)

### NDJSON (history only)

- `NDJSONLine` struct exists for streaming per-commit output
- History framework supports it via `StreamingSink`
- Static has no equivalent

### Metadata (not captured)

- No repo name/URL in pipeline
- No analysis timestamp in output
- Checkpoint has `CreatedAt` but not exposed to analyzers
