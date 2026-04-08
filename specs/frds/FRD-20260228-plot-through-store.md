# FRD: Wire `--format plot` Through Store (Phase 5)

**ID**: FRD-20260228-plot-through-store
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 5.1, 5.2
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 5
**Depends on**: [FRD-20260228-render-command](FRD-20260228-render-command.md) (Phase 4)

## Problem

`codefang run --format plot` currently calls `OutputHistoryResults` which materializes
all analyzer reports in memory and renders a single monolithic HTML page. This is the
OOM path — all reports must fit in memory simultaneously. Phase 2 introduced
`FinalizeToStore` (one analyzer at a time, nil-ing aggregators), and Phase 4 introduced
`codefang render` (reads store, renders multi-page HTML). Phase 5 wires these together:
`--format plot` now finalizes to a temp store, then renders from it.

## Feature

1. **Store-backed plot output** — `executeHistoryPipeline` creates a temp `FileReportStore`
   when format=plot, sets `streamConfig.ReportStore`, and renders from store after analysis.
2. **`--output` flag** — Output directory for multi-page HTML (required when format=plot).
3. **`--keep-store` flag** — Preserves the temp store directory and prints its path.
4. **Legacy plot path removal** — `outputCombinedPlot` and its helpers removed from `output.go`.

## CLI Interface

```
codefang run -a 'history/*' --format plot --output ./html [path]
codefang run -a burndown --format plot --output ./html --keep-store [path]
```

### New Flags
- `--output` / `-o` (string): Output directory for plot HTML files. Required when `--format plot`.
- `--keep-store` (bool): Keep the temp ReportStore directory after rendering. Prints store path.

## Behavior

### Plot-Through-Store Flow

1. `executeHistoryPipeline` detects `normalizedFormat == FormatPlot`.
2. Creates temp dir via `os.MkdirTemp("", "codefang-store-*")`.
3. Creates `FileReportStore(tempDir)`.
4. Sets `streamConfig.ReportStore = store`.
5. `RunStreaming` dispatches to `FinalizeToStore` (Phase 2) — one analyzer at a time.
6. After `RunStreaming`: `store.Close()`.
7. Calls `renderFromStore(storePath, outputDir)` — reuses render.go's `runRender` logic.
8. If `--keep-store`: logs store path. Otherwise: `os.RemoveAll(tempDir)`.
9. Returns — does NOT call `renderReport`/`OutputHistoryResults`.

### Legacy Plot Path Removal

From `output.go`, the following are removed:
- `outputCombinedPlot` — single-page combined HTML renderer
- `buildCombinedPage` — creates `plotpage.Page` from leaves
- `addLeafToPage` — dispatches to SectionGenerator/PlotGenerator
- `addSectionsToPage` — adds sections from SectionGenerator
- `addChartToPage` — adds chart from PlotGenerator
- `PlotGenerator` interface — no longer used (individual analyzers keep their methods)
- `SectionGenerator` interface — no longer used
- `FormatPlot` handling in `OutputHistoryResults`

### Error Handling

- If `--format plot` without `--output`: return clear error.
- If temp store creation fails: return error before analysis starts.
- If rendering fails after analysis: return error (store preserved if `--keep-store`).

## Acceptance Criteria

1. `codefang run -a burndown --format plot --output ./html .` produces multi-page HTML.
2. `--keep-store` preserves store dir and logs its path.
3. Without `--keep-store`, temp store is cleaned up.
4. `--format plot` without `--output` returns clear error.
5. Legacy `outputCombinedPlot` removed — no single-page plot path remains.
6. Other formats (json, yaml, timeseries, ndjson, binary, text) unaffected.
7. `go test ./cmd/codefang/commands/...` passes.
8. `make lint` clean.

## Non-Goals

- No `StoreWriter` (chunked) analyzer path yet — all use legacy gob fallback.
- No changes to `WriteConvertedOutput` plot path (input conversion).
- No changes to individual analyzer `Serialize` or `GenerateChart` methods.

## Implementation

### Files Modified
- `cmd/codefang/commands/run.go` — `PlotOutput`/`KeepStore` on `HistoryRunOptions`, `--output`/`--keep-store` flags, `executePlotPipeline`, `validatePlotFlags`, `renderFromStore`
- `internal/analyzers/analyze/output.go` — removed `outputCombinedPlot`, `buildCombinedPage`, `addLeafToPage`, `addSectionsToPage`, `addChartToPage`, `PlotGenerator`, `SectionGenerator`, `FormatPlot` from `rawOutput` check; removed unused imports `strings`, `components`, `plotpage`

### Files Created
- `cmd/codefang/commands/run_plot_test.go` — 7 tests: ForwardsPlotOutputFlag, ForwardsKeepStoreFlag, RenderFromStore_ProducesHTML, RenderFromStore_InvalidStoreDir, RenderFromStore_CreatesOutputDir, PlotOutputRequired_WhenFormatPlot, PlotOutputRequired_OtherFormatsIgnored
