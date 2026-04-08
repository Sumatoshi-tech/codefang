# FRD: Static Plot Multi-Page HTML Output

**ID**: FRD-20260312-static-plot-multipage
**Date**: 2026-03-12

## Problem

When running `codefang run --analyzers 'static/*' --format plot --output .`, the static phase
ignores the `--output` flag and dumps raw HTML fragments to stdout. History analyzers produce
a combined multi-page HTML report in the output directory with an index page and per-analyzer
pages. Static analyzers should behave identically.

### Root Cause

`runStaticPhase` does not call `validatePlotFlags` (only `runHistoryPhase` does), so the
`--output` requirement is never enforced. The static executor passes `os.Stdout` as the
writer and calls `FormatPerAnalyzer`, which calls each analyzer's `FormatReportPlot` —
producing standalone HTML pages written sequentially to stdout.

The infrastructure for multi-page rendering already exists:
- Static analyzers register `SectionRendererFunc` via `RegisterPlotSections` (e.g.,
  `"static/complexity"`, `"static/cohesion"`, etc.)
- `plotpage.MultiPageRenderer` produces per-analyzer HTML pages + index.html
- `PlotSectionsFor(analyzerID)` retrieves the registered renderer

The missing piece is wiring: `runStaticPhase` needs to use this infrastructure when
format is `plot`.

## Decision

### 1. Validate plot flags for static phase

Call `validatePlotFlags(staticFormat, rc.plotOutput)` in `runStaticPhase`, same as history.

### 2. Add `FormatPlotPages` to `StaticService`

Add a method that takes analysis results and an output directory, and renders multi-page
HTML using `PlotSectionsFor` + `MultiPageRenderer`:

```go
func (svc *StaticService) FormatPlotPages(
    analyzerNames []string,
    results map[string]Report,
    outputDir string,
) error
```

For each analyzer:
1. Build the full analyzer ID (e.g., `"static/complexity"`)
2. Call `PlotSectionsFor(fullID)` to get the section renderer
3. Call renderer with the report to get sections
4. Pass sections to `MultiPageRenderer.RenderAnalyzerPage`

After all analyzers, call `MultiPageRenderer.RenderIndex`.

### 3. Wire in `runStaticPhase`

When format is plot:
- Skip the normal `staticExec` call (which writes to stdout)
- Run analysis directly via `StaticService.AnalyzeFolder`
- Call `StaticService.FormatPlotPages` with the output directory

Since `staticExec` is a function pointer that bundles analysis + formatting, and we need
to split these for plot, we add a new `staticPlotExec` function that the `runStaticPhase`
calls when format is plot.

### 4. Add `staticPlotExecutor` type

```go
type staticPlotExecutor func(
    path string,
    analyzerIDs []string,
    maxWorkers int,
    memoryBudget int64,
    outputDir string,
) error
```

Wire into `RunCommand` alongside `staticExec`. Default implementation calls
`StaticService.AnalyzeFolder` + `FormatPlotPages`.

## Contract

- `--format plot` without `--output` returns `ErrPlotOutputRequired` for both static and history
- Static `--format plot --output dir` produces `dir/index.html` + per-analyzer pages
- Each per-analyzer page uses the same `plotpage.MultiPageRenderer` as history
- Output HTML contains echarts + tailwind CDN references
- When format is not plot, behavior is unchanged

## Acceptance Criteria

- [x] `validatePlotFlags` called for static phase
- [x] `StaticService.FormatPlotPages` method implemented
- [x] `staticPlotExecutor` type and default implementation added
- [x] `runStaticPhase` uses plot executor when format is plot
- [x] Static plot produces index.html + per-analyzer HTML pages
- [x] Tests for validation, rendering, and end-to-end flow
- [x] `make lint` passes
- [x] `go test ./cmd/codefang/commands/...` passes
- [x] `go test ./internal/analyzers/analyze/...` passes

## Implementation

Files modified:
- `internal/analyzers/analyze/static.go` — `FormatPlotPages` method, `plotpage` import, constants
- `cmd/codefang/commands/run.go` — `staticPlotExecutor` type, `staticPlotExec` field, `runStaticPlotAnalyzers`, validation in `runStaticPhase`
- `internal/analyzers/analyze/static_test.go` — `TestStaticService_FormatPlotPages_ProducesHTML`, `_SkipsUnregisteredAnalyzers`
- `cmd/codefang/commands/run_plot_test.go` — `TestStaticPlot_RequiresOutputFlag`
