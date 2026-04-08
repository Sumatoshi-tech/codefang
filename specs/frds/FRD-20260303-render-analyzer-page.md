# FRD: Add plotpage.RenderAnalyzerPage helper (Roadmap F4.1)

**ID**: FRD-20260303-render-analyzer-page
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F4.1
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 4: Visualization boilerplate reduction

## Problem

13 plot files follow the identical three-step pattern:
```go
page := plotpage.NewPage(title, desc)
page.Add(sections...)
return page.Render(w)
```

This is 3-5 lines of boilerplate per file (depending on line wrapping). None of the
callers modify page properties between `NewPage` and `Render`.

## Feature

Add `RenderAnalyzerPage(w io.Writer, title, desc string, sections ...Section) error`
to the `plotpage` package. This one-liner replaces the three-step pattern.

### Design Decisions

- **Variadic sections**: Uses `...Section` to match the existing `Add()` signature.
- **No return of Page**: Callers never need the `*Page` after rendering, so a
  functional helper that takes writer + data and returns error is sufficient.
- **Default settings**: Uses the same defaults as `NewPage()` (ThemeDark, DefaultStyle,
  ShowThemeToggle: true). No configuration needed.
- **Selective migration**: Only migrate files that follow the exact `NewPage → Add →
  Render` pattern with no intermediate modifications. The `unified_model.go` file
  builds sections in a loop and adds them incrementally — it can still use the
  `NewPage` API directly.

### Migration Scope

| File | Lines saved |
|------|-------------|
| burndown/plot.go | 2 |
| couples/plot.go | 4 |
| complexity/plot.go | 4 |
| halstead/plot.go | 4 |
| cohesion/plot.go | 4 |
| comments/plot.go | 4 |
| shotness/plot.go | 2 |
| sentiment/analyzer.go | 2 |
| clones/plot.go | 4 |
| imports/static_plot.go | 4 |
| devs/dashboard.go | 4 |
| file_history/plot.go | 2 |

Not migrated: `common/renderer/unified_model.go` (incrementally adds sections in loop).

## Acceptance Criteria

- [x] `plotpage.RenderAnalyzerPage(w, title, desc, sections...)` exists in `plotpage/plotpage.go`
- [x] Unit test in `plotpage/plotpage_test.go`
- [x] 12 plot files migrated to use `RenderAnalyzerPage`
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/common/plotpage/plotpage.go` — added `RenderAnalyzerPage` function
- `internal/analyzers/common/plotpage/plotpage_test.go` — added `TestRenderAnalyzerPage`
- `internal/analyzers/burndown/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/couples/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/complexity/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/halstead/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/cohesion/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/comments/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/shotness/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/sentiment/analyzer.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/clones/plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/imports/static_plot.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/devs/dashboard.go` — migrated to `RenderAnalyzerPage`
- `internal/analyzers/file_history/plot.go` — migrated to `RenderAnalyzerPage`
