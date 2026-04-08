# FRD: Anomaly EnrichFromStore (Phase 8)

**ID**: FRD-20260301-anomaly-enrich-from-store
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 8.1, 8.2
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 8
**Depends on**: [FRD-20260301-burndown-filehistory-store-writer](FRD-20260301-burndown-filehistory-store-writer.md) (Phase 7)

## Problem

`EnrichFromReports` (enrich.go:14-56) requires all analyzer reports to be
in memory simultaneously: it receives `otherReports map[string]analyze.Report`.
In the store path (`executePlotPipeline`), reports are written to the store
one at a time and released — so `EnrichFromReports` cannot be called because
the in-memory report map is discarded (assigned to `_` in run.go:1052).

This means cross-analyzer anomaly detection is **completely skipped** in the
plot format path. The store-based enrichment function fixes this by reading
one analyzer's store at a time, extracting time series, and releasing before
moving to the next.

## Feature

### 8.1 EnrichFromStore

1. **`EnrichFromStore`** — Reads analyzer data from the store one at a time.
   For each registered `TimeSeriesExtractor`:
   - Opens the analyzer's store via `store.Open(analyzerID)`
   - Reads the legacy `"report"` kind via `GobDecode`
   - Passes the deserialized `analyze.Report` to the existing extractor
   - Closes the reader (data goes out of scope — bounded memory)
   - Runs the same `detectExternalAnomalies` logic
2. **Store-aware extractor registry** — New `StoreTimeSeriesExtractor` type
   for analyzers that use structured store kinds (not legacy "report").
   Registered via `RegisterStoreTimeSeriesExtractor`. Checked first; falls
   back to legacy report deserialization if no store extractor exists.
3. **Pipeline integration** — `executePlotPipeline` calls `EnrichFromStore`
   after `FinalizeToStore` and before `store.Close()`, writing enrichment
   results back to the anomaly analyzer's store entry.

### 8.2 EnrichFromStore Tests

1. Round-trip test: create store with synthetic legacy report data,
   enrich, verify anomalies detected.
2. Equivalence test: same data through `EnrichFromReports` and
   `EnrichFromStore` produces identical anomalies.
3. Empty store test: no matching analyzers, no crash.
4. Store extractor test: register a `StoreTimeSeriesExtractor`, verify
   it is preferred over legacy report deserialization.

## Types

### StoreTimeSeriesExtractor

```go
type StoreTimeSeriesExtractor func(reader analyze.ReportReader) (ticks []int, dimensions map[string][]float64)
```

## Behavior

### EnrichFromStore Flow

1. Get anomaly analyzer's store reader to read current anomaly report.
2. Snapshot both extractor registries (report-based + store-based).
3. For each store analyzer ID:
   a. Check if a `StoreTimeSeriesExtractor` is registered → use it.
   b. Else check if a `TimeSeriesExtractor` is registered → read legacy
      `"report"` kind, deserialize, pass to extractor.
   c. Skip if no extractor matches.
4. Collect all `ExternalAnomaly` and `ExternalSummary` records.
5. Sort (same as `EnrichFromReports`).
6. Write enrichment data back: update anomaly report in store.

### Memory Guarantee

Only one external analyzer's data is live at a time. The reader is closed
before opening the next, so memory = max(one analyzer's report).

## Acceptance Criteria

1. `EnrichFromStore` reads from store and produces equivalent anomalies.
2. Only one external analyzer's data lives in memory at a time.
3. `StoreTimeSeriesExtractor` registry with `RegisterStoreTimeSeriesExtractor`.
4. Falls back to legacy "report" GobDecode when no store extractor.
5. `go test ./internal/analyzers/anomaly/...` passes.
6. `make lint` clean.

## Non-Goals

- No changes to existing `TimeSeriesExtractor` implementations (quality, sentiment).
- No wiring into `executePlotPipeline` in this phase (that's integration in Phase 9).
- No store-native time series extractors for quality/sentiment in this phase.

## Implementation

### Files Created
- `internal/analyzers/anomaly/enrich_store.go` — `EnrichFromStore`, `runStoreEnrichment`, `extractFromStore`
- `internal/analyzers/anomaly/enrich_store_test.go` — 5 tests (Basic, Equivalence, EmptyStore, StoreExtractorPreferred, SkipsAnomalyAnalyzer)

### Files Modified
- `internal/analyzers/anomaly/registry.go` — added `StoreTimeSeriesExtractor` type, `RegisterStoreTimeSeriesExtractor`, `snapshotStoreExtractors`
- `internal/analyzers/anomaly/registry_test.go` — `withIsolatedRegistry` updated to save/restore store extractor registry
- `internal/analyzers/anomaly/enrich.go` — refactored `EnrichFromReports` to delegate to `enrichFromReportsWithExtractors`
- `.deadcode-whitelist` — 5 new entries for Phase 8 functions (wired in Phase 9)
