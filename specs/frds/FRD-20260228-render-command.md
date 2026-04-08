# FRD: Render Command (Phase 4)

**ID**: FRD-20260228-render-command
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 4.1, 4.2
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 4
**Depends on**: [FRD-20260228-multipage-renderer](FRD-20260228-multipage-renderer.md) (Phase 3)

## Problem

After analysis, reports live in a `FileReportStore` directory. A separate command
is needed to read from the store and produce multi-page HTML output — fully decoupled
from the analysis step. This enables re-rendering without re-analyzing and keeps
peak memory bounded (one analyzer's data at a time).

## Feature

1. **`codefang render` command** — Cobra subcommand that reads a ReportStore directory
   and writes multi-page HTML using `MultiPageRenderer`.
2. **Plot section registration** — Registers all analyzer plot sections so the render
   command can generate charts from stored report data.
3. **Legacy report reading** — For non-StoreWriter analyzers, reads the gob-encoded
   `Report` under the `"report"` kind and passes it to the registered
   `SectionRendererFunc`.

## CLI Interface

```
codefang render <store-dir> --output <dir>
```

### Arguments
- `store-dir` (positional, required): path to a `FileReportStore` directory.

### Flags
- `--output` / `-o` (required): directory to write HTML files into. Created if missing.

## Behavior

### Render Flow

1. Open `FileReportStore(storeDir)`.
2. Read `store.AnalyzerIDs()` — ordered list of analyzer IDs.
3. For each analyzer ID:
   a. Look up `analyze.PlotSectionsFor(id)` — get the `SectionRendererFunc`.
   b. If no renderer registered, skip (log warning).
   c. `store.Open(id)` → `ReportReader`.
   d. Read the `"report"` kind via `reader.Iter("report", ...)` → gob-decode into `Report`.
   e. Call `renderer(report)` → `[]plotpage.Section`.
   f. `MultiPageRenderer.RenderAnalyzerPage(id, title, sections)`.
   g. `reader.Close()` — data goes out of scope, bounded memory.
   h. Collect `PageMeta` for this analyzer.
4. `MultiPageRenderer.RenderIndex(allPageMeta)`.
5. `store.Close()`.

### Plot Section Registration

The render command must call all `RegisterPlotSections()` functions before rendering:
- All 14 history analyzer registrations (same as `NewRunCommand`).
- `devs.RegisterDevPlotSections()` explicitly.

### Error Handling

- If store directory does not exist or manifest is invalid: return clear error.
- If a single analyzer fails to render: log error, continue with remaining analyzers.
- Return error only if no analyzers could be rendered at all.

## Acceptance Criteria

1. `codefang render <store-dir> --output <dir>` produces per-analyzer HTML + index.html.
2. Each analyzer page contains the registered plot sections.
3. Data goes out of scope after each analyzer — bounded memory.
4. Command is registered in `cmd/codefang/main.go`.
5. `go test ./cmd/codefang/commands/...` passes.
6. `make lint` clean.

## Non-Goals

- No `StoreWriter` (chunked) reader path yet — that comes in Phase 6+.
  For now, all analyzers use the legacy `"report"` gob path.
- No `--format` flag — HTML is the only output format.
- No incremental re-render — always renders all analyzers.

## Implementation

### Files Created
- `cmd/codefang/commands/render.go` — `NewRenderCommand`, `buildRenderCommand`, `runRender`, `renderOneAnalyzer`, `readLegacyReport`, `safeAnalyzerID`
- `cmd/codefang/commands/render_test.go` — 5 tests

### Files Modified
- `cmd/codefang/main.go` — added `commands.NewRenderCommand()` registration
- `cmd/codefang/commands/run_test.go` — added `registerGobTypesForTest()` to `TestMain`
- `internal/analyzers/analyze/report_store_file.go` — added `loadManifest()` to `NewFileReportStore` for read scenarios
