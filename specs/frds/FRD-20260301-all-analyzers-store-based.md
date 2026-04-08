# FRD: All Analyzers Store-Based + Legacy Removal (Phase 10)

**ID**: FRD-20260301-all-analyzers-store-based
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Step 10
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md)
**Depends on**: [FRD-20260301-pipeline-enrichment-integration](FRD-20260301-pipeline-enrichment-integration.md) (Phase 9.1)

## Problem

After Phase 9.1, only 3 analyzers (burndown, couples, file_history) had proper
`StoreWriter`/`DirectStoreWriter` + `GenerateStoreSections` implementations.
The remaining 7 analyzers fell through to the legacy gob path:
`FlushAllTicks -> ReportFromTICKs -> gob encode "report" kind -> renderFromLegacyReport`.
This dual-path architecture increased maintenance burden and prevented full
bounded-memory rendering.

## Feature

### 10.1 Seven Analyzer Store Conversions

Each analyzer gets `StoreWriter.WriteToStore` + `GenerateStoreSections`:

1. **typos** — flat list of typo records, 1 plot section
2. **imports** — import frequency records, 1 plot section
3. **sentiment** — per-tick scores + top comments, 2 plot sections
4. **quality** — 10 metric dimensions, 3 plot sections
5. **devs** — multi-dimensional (developer, language, bus_factor, activity, churn, aggregate), 6 record kinds
6. **shotness** — `DirectStoreWriter`, O(N^2) coupling via `NodeStoreRecord`, 3 plot sections
7. **anomaly** — per-commit metrics + Z-scores, 2+ plot sections; also updated enrichment pipeline

### 10.2 Legacy Removal

After all 10 analyzers implement store-based flow:

- **`runner.go`**: Removed `legacyWriteToStore` — `flushAndWriteToStore` now errors if analyzer doesn't implement `StoreWriter`
- **`render.go`**: Removed `renderFromLegacyReport`, `readLegacyReport` — `generateSectionsForAnalyzer` uses store-only path
- **`run.go`**: Rewrote `enrichAnomalyFromResults` to use `anomaly.EnrichFromReports` directly (no temp store)
- **`enrich_store.go`**: Added `EnrichFromReports` for in-memory enrichment path; removed `EnrichFromStore` bridge

### 10.3 Preserved Legacy Section Registry

`RegisterPlotSections`/`PlotSectionsFor` in `conversion.go` are **NOT removed** — still needed by:
- `unified_model.go` (for `codefang run --format plot` and `codefang convert --format plot`)
- Static analyzers (complexity, cohesion, comments, halstead, clones, static/imports)

## Types

### New per-analyzer types (store_writer.go)

Each analyzer defines kind constants and gob-safe record types:
- `typos`: `KindTypoData`, `KindAggregate`; `TypoStoreRecord`, `AggregateData`
- `imports`: `KindImportData`, `KindAggregate`; `ImportStoreRecord`, `AggregateData`
- `sentiment`: `KindTimeSeries`, `KindTopComment`, `KindAggregate`; `TimeSeriesRecord`, `TopCommentRecord`, `AggregateData`
- `quality`: `KindTimeSeries`, `KindAggregate`; `TimeSeriesRecord`, `AggregateData`
- `devs`: `KindDeveloper`, `KindLanguage`, `KindBusFactor`, `KindActivity`, `KindChurn`, `KindAggregate`
- `shotness`: `KindNodeData`, `KindAggregate`; `NodeStoreRecord`, `AggregateData`
- `anomaly`: `KindTimeSeries`, `KindAggregate`, `KindExternalAnomaly`, `KindExternalSummary`

### New sentinel error

- `framework.ErrNotStoreWriter` — returned when analyzer doesn't implement `StoreWriter`

## Implementation

### Files created (per analyzer)
- `internal/analyzers/{name}/store_writer.go` — `WriteToStore` implementation
- `internal/analyzers/{name}/store_reader.go` — `GenerateStoreSections` implementation
- `internal/analyzers/{name}/store_writer_test.go` — round-trip, equivalence, section tests
- `internal/analyzers/shotness/hibernation_test.go` — hibernation tests for shotness

### Files modified
- `internal/framework/runner.go` — removed `legacyWriteToStore`, added `ErrNotStoreWriter`
- `internal/framework/runner_test.go` — removed legacy tests, added `TestFinalizeToStore_RejectsNonStoreWriter`
- `internal/framework/export_test.go` — removed `LegacyReportKindForTest`
- `cmd/codefang/commands/render.go` — store-only `generateSectionsForAnalyzer`
- `cmd/codefang/commands/render_test.go` — uses `RegisterStorePlotSections`
- `cmd/codefang/commands/run.go` — `enrichAnomalyFromResults` uses `EnrichFromReports`
- `internal/analyzers/anomaly/enrich_store.go` — added `EnrichFromReports`, `runReportEnrichment`
- All 7 analyzer `plot.go` files — added `RegisterStorePlotSections` calls

## Tests

- Per-analyzer: RoundTrip, EquivalenceLegacy, GenerateStoreSections_RoundTrip, EmptyTicks/EmptyStore
- Framework: `TestFinalizeToStore_RejectsNonStoreWriter` (verifies error on non-StoreWriter)
- Render: all tests updated to use `RegisterStorePlotSections`
- Anomaly enrichment: `EnrichAndRewrite_RoundTrip` (store path), direct anomaly tests

## Verification

- `go vet ./...` — clean
- `make lint` — 0 issues, no dead code
- `go test ./... -count=1 -timeout 300s` — all 43 packages pass
