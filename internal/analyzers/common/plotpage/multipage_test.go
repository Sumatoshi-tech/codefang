package plotpage

// FRD: specs/frds/FRD-20260228-multipage-renderer.md.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRenderAnalyzerPage_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	renderer := &MultiPageRenderer{
		OutputDir: dir,
		Title:     "Test Project",
		Theme:     ThemeDark,
	}

	sections := []Section{
		{Title: "Section One", Subtitle: "sub1"},
		{Title: "Section Two", Subtitle: "sub2"},
	}

	err := renderer.RenderAnalyzerPage("devs", "Developer Analysis", sections)
	if err != nil {
		t.Fatalf("RenderAnalyzerPage: %v", err)
	}

	outPath := filepath.Join(dir, "devs.html")

	data, readErr := os.ReadFile(outPath)
	if readErr != nil {
		t.Fatalf("Expected file %s to exist: %v", outPath, readErr)
	}

	html := string(data)

	// Verify standalone HTML with echarts + tailwind CDN.
	if !strings.Contains(html, "cdn.tailwindcss.com") {
		t.Error("Expected Tailwind CDN")
	}

	if !strings.Contains(html, "echarts.min.js") {
		t.Error("Expected ECharts CDN")
	}

	// Verify sections appear.
	if !strings.Contains(html, "Section One") {
		t.Error("Expected section title 'Section One'")
	}

	if !strings.Contains(html, "Section Two") {
		t.Error("Expected section title 'Section Two'")
	}

	// Verify page title.
	if !strings.Contains(html, "Developer Analysis") {
		t.Error("Expected page title")
	}

	// Verify back-to-index navigation.
	if !strings.Contains(html, "index.html") {
		t.Error("Expected navigation link to index.html")
	}
}

func TestRenderAnalyzerPage_DarkTheme(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	renderer := &MultiPageRenderer{
		OutputDir: dir,
		Title:     "Project",
		Theme:     ThemeDark,
	}

	err := renderer.RenderAnalyzerPage("test", "Test", []Section{
		{Title: "S1"},
	})
	if err != nil {
		t.Fatalf("RenderAnalyzerPage: %v", err)
	}

	data, readErr := os.ReadFile(filepath.Join(dir, "test.html"))
	if readErr != nil {
		t.Fatalf("ReadFile: %v", readErr)
	}

	html := string(data)

	if !strings.Contains(html, `class="dark"`) {
		t.Error("Expected dark theme class")
	}
}

func TestRenderIndex_CreatesFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	renderer := &MultiPageRenderer{
		OutputDir: dir,
		Title:     "My Report",
		Theme:     ThemeDark,
	}

	pages := []PageMeta{
		{ID: "devs", Title: "Developer Analysis", Description: "Who wrote what"},
		{ID: "couples", Title: "Coupling Analysis", Description: "File co-changes"},
		{ID: "burndown", Title: "Burndown", Description: "Code age tracking"},
	}

	err := renderer.RenderIndex(pages)
	if err != nil {
		t.Fatalf("RenderIndex: %v", err)
	}

	outPath := filepath.Join(dir, "index.html")

	data, readErr := os.ReadFile(outPath)
	if readErr != nil {
		t.Fatalf("Expected index.html to exist: %v", readErr)
	}

	html := string(data)

	// Verify standalone HTML.
	if !strings.Contains(html, "cdn.tailwindcss.com") {
		t.Error("Expected Tailwind CDN")
	}

	// Verify all analyzer links.
	if !strings.Contains(html, "devs.html") {
		t.Error("Expected link to devs.html")
	}

	if !strings.Contains(html, "couples.html") {
		t.Error("Expected link to couples.html")
	}

	if !strings.Contains(html, "burndown.html") {
		t.Error("Expected link to burndown.html")
	}

	// Verify titles appear.
	if !strings.Contains(html, "Developer Analysis") {
		t.Error("Expected 'Developer Analysis' title")
	}

	if !strings.Contains(html, "Coupling Analysis") {
		t.Error("Expected 'Coupling Analysis' title")
	}

	// Verify descriptions appear.
	if !strings.Contains(html, "Who wrote what") {
		t.Error("Expected description 'Who wrote what'")
	}

	// Verify report title appears.
	if !strings.Contains(html, "My Report") {
		t.Error("Expected report title")
	}
}

func TestMultiPageRenderer_ThreePages(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	renderer := &MultiPageRenderer{
		OutputDir: dir,
		Title:     "Full Report",
		Theme:     ThemeDark,
	}

	pages := []PageMeta{
		{ID: "devs", Title: "Devs"},
		{ID: "couples", Title: "Couples"},
		{ID: "burndown", Title: "Burndown"},
	}

	// Render all 3 analyzer pages.
	for _, p := range pages {
		renderErr := renderer.RenderAnalyzerPage(p.ID, p.Title, []Section{
			{Title: p.Title + " Section"},
		})
		if renderErr != nil {
			t.Fatalf("RenderAnalyzerPage(%s): %v", p.ID, renderErr)
		}
	}

	// Render index.
	err := renderer.RenderIndex(pages)
	if err != nil {
		t.Fatalf("RenderIndex: %v", err)
	}

	// Verify all 4 files exist (3 pages + index).
	expectedFiles := []string{"devs.html", "couples.html", "burndown.html", "index.html"}
	for _, name := range expectedFiles {
		fpath := filepath.Join(dir, name)

		_, statErr := os.Stat(fpath)
		if os.IsNotExist(statErr) {
			t.Errorf("Expected file %s to exist", name)
		}
	}
}

func TestRenderIndex_CardLinksAreRelative(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	renderer := &MultiPageRenderer{
		OutputDir: dir,
		Title:     "Report",
		Theme:     ThemeDark,
	}

	pages := []PageMeta{
		{ID: "devs", Title: "Devs"},
	}

	err := renderer.RenderIndex(pages)
	if err != nil {
		t.Fatalf("RenderIndex: %v", err)
	}

	data, readErr := os.ReadFile(filepath.Join(dir, "index.html"))
	if readErr != nil {
		t.Fatalf("ReadFile: %v", readErr)
	}

	html := string(data)

	// Links should be relative (no leading slash or protocol).
	if strings.Contains(html, `href="/devs.html"`) {
		t.Error("Links should be relative, not absolute")
	}

	if !strings.Contains(html, `href="devs.html"`) {
		t.Error("Expected relative link href=\"devs.html\"")
	}
}

func TestRenderAnalyzerPage_InvalidDir(t *testing.T) {
	t.Parallel()

	renderer := &MultiPageRenderer{
		OutputDir: "/nonexistent/path/that/does/not/exist",
		Title:     "Test",
		Theme:     ThemeDark,
	}

	err := renderer.RenderAnalyzerPage("test", "Test", nil)
	if err == nil {
		t.Error("Expected error for nonexistent output directory")
	}
}
