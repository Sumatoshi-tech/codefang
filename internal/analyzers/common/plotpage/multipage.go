package plotpage

import (
	"fmt"
	"html/template"
	"io"
	"os"
	"path/filepath"
)

const (
	indexFileName    = "index.html"
	indexTitle       = "Analysis Report"
	indexDescription = "Select an analyzer to view its report."
)

// PageMeta carries metadata about a rendered analyzer page for the index.
type PageMeta struct {
	ID          string // Filename stem, e.g. "devs", "couples".
	Title       string // Display title, e.g. "Developer Contributions".
	Description string // Short description for the index card.
}

// MultiPageRenderer produces per-analyzer HTML pages plus an index page.
type MultiPageRenderer struct {
	OutputDir string // Directory to write HTML files into.
	Title     string // Project/report title shown on every page.
	Theme     Theme  // ThemeDark or ThemeLight.
}

// RenderAnalyzerPage renders a single analyzer page to <OutputDir>/<id>.html.
// Each page is standalone HTML with echarts + tailwind CDN and a navigation
// link back to index.html.
func (r *MultiPageRenderer) RenderAnalyzerPage(id, title string, sections []Section) error {
	page := NewPage(title, "")
	page.Theme = r.Theme
	page.ProjectName = r.Title

	navHTML, err := renderTemplate("nav.html", nil)
	if err != nil {
		return fmt.Errorf("render nav: %w", err)
	}

	renderer := HTMLRenderer{
		ExtraCSS: "",
	}

	// Prepend navigation as a section with no title (just the nav HTML).
	navSection := Section{
		Chart: rawHTML(navHTML),
	}
	page.Sections = append([]Section{navSection}, sections...)

	outPath := filepath.Join(r.OutputDir, id+".html")

	f, err := os.Create(outPath)
	if err != nil {
		return fmt.Errorf("create %s: %w", outPath, err)
	}
	defer f.Close()

	renderErr := renderer.Render(f, page)
	if renderErr != nil {
		return fmt.Errorf("render %s: %w", id, renderErr)
	}

	return nil
}

// RenderIndex renders an index page with navigation cards to <OutputDir>/index.html.
func (r *MultiPageRenderer) RenderIndex(pages []PageMeta) error {
	page := NewPage(indexTitle, indexDescription)
	page.Theme = r.Theme
	page.ProjectName = r.Title

	indexContent, err := renderTemplate("index.html", indexData{Pages: pages})
	if err != nil {
		return fmt.Errorf("render index content: %w", err)
	}

	page.Sections = []Section{
		{Chart: rawHTML(indexContent)},
	}

	renderer := HTMLRenderer{}

	outPath := filepath.Join(r.OutputDir, indexFileName)

	f, err := os.Create(outPath)
	if err != nil {
		return fmt.Errorf("create %s: %w", outPath, err)
	}
	defer f.Close()

	renderErr := renderer.Render(f, page)
	if renderErr != nil {
		return fmt.Errorf("render index: %w", renderErr)
	}

	return nil
}

// indexData holds template data for index.html.
type indexData struct {
	Pages []PageMeta
}

// rawHTML is a Renderable that writes pre-rendered HTML.
type rawHTML template.HTML

// Render writes the raw HTML content.
func (r rawHTML) Render(w io.Writer) error {
	_, err := w.Write([]byte(r))
	if err != nil {
		return fmt.Errorf("write raw html: %w", err)
	}

	return nil
}
