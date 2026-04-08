# FRD: Pipeline Enrichment Integration (Phase 9.1)

**ID**: FRD-20260301-pipeline-enrichment-integration
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Step 9.1
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 9
**Depends on**: [FRD-20260301-anomaly-enrich-from-store](FRD-20260301-anomaly-enrich-from-store.md) (Phase 8)

## Problem

`executePlotPipeline` runs analysis through the store path (`FinalizeToStore`
→ `renderFromStore`) but **completely skips** cross-analyzer anomaly enrichment.
The non-plot path previously called `enrichAnomalyReport` → `EnrichFromReports`
on in-memory reports, maintaining a dual code path for enrichment.

## Feature

### 9.1 Unified Store-Based Enrichment

1. **`enrichAnomalyFromStore`** — Plot pipeline helper in `run.go` that:
   - Finds the anomaly analyzer in `allAnalyzers` by type assertion
   - Reads the current anomaly report from the store via legacy `"report"` kind
   - Calls `anomaly.EnrichFromStore(anomalyReport, store, windowSize, threshold)`
   - Writes the enriched anomaly report back to the store
2. **`enrichAnomalyFromResults`** — Non-plot pipeline helper that:
   - Creates a temp `FileReportStore` from in-memory results
   - Calls `anomaly.EnrichFromStore` through the same store path
   - Modifies the anomaly report in-place (no read-back needed)
   - Cleans up the temp store on return
3. **Legacy removal** — `EnrichFromReports`, `enrichFromReportsWithExtractors`,
   and `enrichAnomalyReport` deleted. All enrichment goes through `EnrichFromStore`.
4. **Deadcode whitelist cleanup** — Phase 8 entries removed (now reachable).

## Types

No new types. Uses existing:
- `anomaly.Analyzer` — for `WindowSize` and `Threshold` fields
- `analyze.FileReportStore` — for `Begin`/`Open`/`Close`
- `analyze.Report` — for reading/writing anomaly report

## Behavior

### enrichAnomalyFromStore Flow (plot pipeline)

1. Iterate `allAnalyzers`, find `*anomaly.Analyzer` by type assertion.
2. If not found → return nil (anomaly not enabled, nothing to do).
3. Open anomaly analyzer's store entry → deserialize legacy `"report"` kind.
4. Call `anomaly.EnrichFromStore(anomalyReport, store, windowSize, threshold)`.
5. Write enriched report back to store.

### enrichAnomalyFromResults Flow (non-plot pipeline)

1. Find `*anomaly.Analyzer` in leaves.
2. If not found → return nil.
3. Create temp `FileReportStore`, write all in-memory reports as legacy `"report"` kind.
4. Call `anomaly.EnrichFromStore` on the anomaly report using the temp store.
5. Clean up temp store directory.

### Memory Guarantee

Same as Phase 8: only one external analyzer's data lives in memory at a time
during enrichment. The anomaly report itself is bounded (single report).

## Acceptance Criteria

1. `executePlotPipeline` calls `enrichAnomalyFromStore` after streaming completes.
2. Non-plot formats call `enrichAnomalyFromResults` (same `EnrichFromStore` path).
3. `EnrichFromReports` and `enrichFromReportsWithExtractors` deleted.
4. `go test ./internal/analyzers/anomaly/... ./cmd/codefang/commands/...` passes.
5. `make lint` clean.
6. Phase 8 deadcode whitelist entries removed (functions now reachable).

## Non-Goals

- No store-native time series extractors for quality/sentiment in this phase.
- No changes to `EnrichFromStore` itself (Phase 8 code is stable).

## Implementation

### Files Modified
- `cmd/codefang/commands/run.go` — added `enrichAnomalyFromStore` (plot), `enrichAnomalyFromResults` (non-plot); removed `enrichAnomalyReport`
- `internal/analyzers/anomaly/enrich.go` — removed `EnrichFromReports`, `enrichFromReportsWithExtractors`; only `detectExternalAnomalies` remains
- `internal/analyzers/anomaly/enrich_test.go` — tests now cover `detectExternalAnomalies` directly (5 tests)
- `internal/analyzers/anomaly/enrich_store_test.go` — equivalence test uses `detectExternalAnomalies` as ground truth
- `.deadcode-whitelist` — removed 4 Phase 8 entries now reachable
