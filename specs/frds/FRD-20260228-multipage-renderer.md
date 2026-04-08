# FRD: Multi-Page Renderer (Phase 3)

**ID**: FRD-20260228-multipage-renderer
**Roadmap**: [specs/perf3/ROADMAP.md](../perf3/ROADMAP.md) — Steps 3.1, 3.2
**Spec**: [specs/perf3/SPEC.md](../perf3/SPEC.md) — Step 3
**Depends on**: [FRD-20260228-runner-integration](FRD-20260228-runner-integration.md) (Phase 2)

## Problem

The current plot output (`outputCombinedPlot`) renders all analyzers into a single
monolithic HTML page. This requires all report data to be in memory simultaneously
to generate one huge page. With the new `ReportStore` architecture (Phases 1-2),
data is written per-analyzer. The renderer must match — produce one HTML file per
analyzer, plus an index page with navigation between them.

## Feature

1. **PageMeta type** — lightweight metadata for an analyzer page: ID, title, and
   optional description, used by the index page to generate navigation cards.
2. **MultiPageRenderer** — writes standalone per-analyzer HTML pages and an index
   page. Reuses existing `Page`/`HTMLRenderer` for individual pages. Index uses a
   new `templates/index.html` template for navigation cards.
3. **Navigation** — each analyzer page includes a "back to index" link; the index
   page includes cards linking to each analyzer page.

## Types

### PageMeta

```go
// PageMeta carries metadata about a rendered analyzer page for the index.
type PageMeta struct {
    ID          string // Filename stem, e.g. "devs", "couples".
    Title       string // Display title, e.g. "Developer Contributions".
    Description string // Short description for the index card.
}
```

### MultiPageRenderer

```go
// MultiPageRenderer produces per-analyzer HTML pages plus an index.
type MultiPageRenderer struct {
    OutputDir string // Directory to write HTML files into.
    Title     string // Project/report title shown on every page.
    Theme     Theme  // ThemeDark or ThemeLight.
}
```

## Behavior

### RenderAnalyzerPage(id, title string, sections []Section) error

1. Creates a `Page` with the given title, theme, and sections.
2. Prepends navigation back to `index.html`.
3. Writes the page to `<OutputDir>/<id>.html`.
4. Returns any write/render error.

### RenderIndex(pages []PageMeta) error

1. Renders `templates/index.html` with navigation cards — one per PageMeta.
2. Each card links to `<id>.html` and shows the title and description.
3. Wraps content in the standard page layout (header, tailwind, echarts CDN).
4. Writes to `<OutputDir>/index.html`.

### Template: templates/index.html

Navigation cards rendered as a responsive grid. Each card shows:
- Title as heading
- Description as subtitle
- Link to the analyzer page

## Acceptance Criteria

1. `MultiPageRenderer.RenderAnalyzerPage` creates `<id>.html` containing:
   - Standalone HTML with echarts + tailwind CDN
   - All provided sections
   - Navigation link back to `index.html`
2. `MultiPageRenderer.RenderIndex` creates `index.html` containing:
   - Navigation cards for every `PageMeta`
   - Links to `<id>.html` for each analyzer
   - Standalone HTML with tailwind CDN
3. Each page renders identically to the existing `Page.Render` output
   (same theme, same CSS, same scripts).
4. `go test ./internal/analyzers/common/plotpage/...` passes.
5. `make lint` clean.

## Non-Goals

- No server, no JavaScript routing — static HTML files only.
- No bundling of echarts — CDN links as today.
- No changes to existing single-page rendering path.

## Implementation

### Files Created
- `internal/analyzers/common/plotpage/multipage.go` — `MultiPageRenderer`, `PageMeta`, `rawHTML`
- `internal/analyzers/common/plotpage/multipage_test.go` — 6 tests
- `internal/analyzers/common/plotpage/templates/index.html` — responsive card grid
- `internal/analyzers/common/plotpage/templates/nav.html` — back-to-index navigation

### Files Modified
- `.deadcode-whitelist` — added `MultiPageRenderer.RenderAnalyzerPage`, `MultiPageRenderer.RenderIndex`, `rawHTML.Render`
