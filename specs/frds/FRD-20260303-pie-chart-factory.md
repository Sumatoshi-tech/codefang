# FRD: Add BuildPieChart factory to plotpage (Roadmap F4.2)

**ID**: FRD-20260303-pie-chart-factory
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F4.2
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 4: Visualization boilerplate reduction

## Problem

8 analyzers create pie charts with `charts.NewPie()`. Of these, 5 follow a nearly
identical pattern: 600x400 dimensions, tooltip "item", bottom legend with themed
text, radius "60%", label formatter "{b}: {c} ({d}%)" — totaling ~20 lines of
`SetGlobalOptions` + `SetSeriesOptions` boilerplate per chart.

`BuildBarChart` and `BuildLineChart` factories already exist in `plotpage/builders.go`,
but no pie chart factory exists.

## Feature

Add `BuildPieChart(co *ChartOpts, seriesName string, data []opts.PieData) *charts.Pie`
to `plotpage/builders.go` with sensible defaults. Then migrate the 5 consistent pie
chart creation sites.

### Design Decisions

- **Pre-built PieData**: The caller constructs `[]opts.PieData` (with custom per-item
  colors). The factory handles global options and series options only.
- **Sensible defaults**: Width "600px", height "400px", bottom legend, radius "60%",
  label formatter "{b}: {c} ({d}%)". These match the 5 identical implementations.
- **No scatter factory**: The 2 scatter chart implementations (complexity, halstead)
  are too different (different axes, mark lines, risk bucketing) to benefit from a factory.
- **Bar charts unchanged**: The remaining bar charts not using `BuildBarChart` need
  per-item colors which the existing factory doesn't support. Extending the bar factory
  is out of scope.

### Migration Scope

| File | Status |
|------|--------|
| comments/plot.go | Migrate (standard pattern) |
| cohesion/plot.go | Migrate (standard pattern) |
| halstead/plot.go | Migrate (standard pattern) |
| complexity/plot.go | Migrate (standard pattern) |
| couples/plot.go | Migrate (radius "65%" — pass via option) |
| imports/static_plot.go | Skip (100% width, custom legend) |
| clones/plot.go | Skip (no tooltip, no legend) |
| sentiment/plot.go | Skip (donut style, different formatter) |

## Acceptance Criteria

- [x] `BuildPieChart` exists in `plotpage/builders.go`
- [x] Unit tests in `plotpage/builders_test.go` (3 cases)
- [x] 5 pie chart creation sites migrated
- [x] All existing tests pass
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/common/plotpage/builders.go` — added `BuildPieChart` function with pie default constants
- `internal/analyzers/common/plotpage/builders_test.go` — added 3 pie chart tests, fixed `opts` import shadow
- `internal/analyzers/cohesion/plot.go` — migrated `createCohesionPieChart` to `BuildPieChart`
- `internal/analyzers/complexity/plot.go` — migrated `createComplexityDistributionPie` to `BuildPieChart`
- `internal/analyzers/comments/plot.go` — migrated `createDocumentationPieChart` to `BuildPieChart`
- `internal/analyzers/halstead/plot.go` — migrated `createVolumeDistributionPie` to `BuildPieChart`
- `internal/analyzers/couples/plot.go` — migrated `buildOwnershipPieChartFromData` to `BuildPieChart`, deleted unused `pieChartWidth`/`pieChartHeight` constants
