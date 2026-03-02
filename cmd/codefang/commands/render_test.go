package commands

// FRD: specs/frds/FRD-20260228-render-command.md.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	testAnalyzerAlpha = "history/alpha"
	testAnalyzerBeta  = "history/beta"
)

// registerTestPlotSections registers stub store section renderers for testing.
func registerTestPlotSections(t *testing.T) {
	t.Helper()

	analyze.RegisterStorePlotSections(testAnalyzerAlpha, func(_ analyze.ReportReader) ([]plotpage.Section, error) {
		return []plotpage.Section{
			{Title: "Alpha Chart", Subtitle: "alpha data"},
		}, nil
	})

	analyze.RegisterStorePlotSections(testAnalyzerBeta, func(_ analyze.ReportReader) ([]plotpage.Section, error) {
		return []plotpage.Section{
			{Title: "Beta Chart", Subtitle: "beta data"},
		}, nil
	})
}

// createTestStore creates a FileReportStore with two analyzer entries.
func createTestStore(t *testing.T) string {
	t.Helper()

	storeDir := filepath.Join(t.TempDir(), "store")

	store := analyze.NewFileReportStore(storeDir)

	writeAnalyzerEntry(t, store, testAnalyzerAlpha)
	writeAnalyzerEntry(t, store, testAnalyzerBeta)

	closeErr := store.Close()
	require.NoError(t, closeErr)

	return storeDir
}

func writeAnalyzerEntry(t *testing.T, store *analyze.FileReportStore, id string) {
	t.Helper()

	w, beginErr := store.Begin(id, analyze.ReportMeta{AnalyzerID: id})
	require.NoError(t, beginErr)

	// Write a dummy record so the analyzer shows up in AnalyzerIDs().
	writeErr := w.Write("dummy", struct{}{})
	require.NoError(t, writeErr)

	closeErr := w.Close()
	require.NoError(t, closeErr)
}

func TestRenderCommand_ProducesHTMLFiles(t *testing.T) {
	t.Parallel()

	registerTestPlotSections(t)

	storeDir := createTestStore(t)
	outputDir := filepath.Join(t.TempDir(), "html")

	cmd := buildRenderCommand()
	cmd.SetArgs([]string{storeDir, "--output", outputDir})

	err := cmd.Execute()
	require.NoError(t, err)

	// Verify index.html exists.
	indexPath := filepath.Join(outputDir, "index.html")
	indexData, readErr := os.ReadFile(indexPath)
	require.NoError(t, readErr, "index.html should exist")

	indexHTML := string(indexData)
	require.Contains(t, indexHTML, "cdn.tailwindcss.com")

	// Verify per-analyzer HTML files.
	for _, id := range []string{testAnalyzerAlpha, testAnalyzerBeta} {
		safeID := strings.ReplaceAll(id, "/", "-")
		pagePath := filepath.Join(outputDir, safeID+".html")

		pageData, pageErr := os.ReadFile(pagePath)
		require.NoError(t, pageErr, "page for %s should exist", id)

		pageHTML := string(pageData)
		require.Contains(t, pageHTML, "cdn.tailwindcss.com")
		require.Contains(t, pageHTML, "index.html", "page should link to index")
	}
}

func TestRenderCommand_IndexLinksToAllPages(t *testing.T) {
	t.Parallel()

	registerTestPlotSections(t)

	storeDir := createTestStore(t)
	outputDir := filepath.Join(t.TempDir(), "html")

	cmd := buildRenderCommand()
	cmd.SetArgs([]string{storeDir, "--output", outputDir})

	err := cmd.Execute()
	require.NoError(t, err)

	indexData, readErr := os.ReadFile(filepath.Join(outputDir, "index.html"))
	require.NoError(t, readErr)

	indexHTML := string(indexData)

	// Both analyzer pages should be linked.
	require.Contains(t, indexHTML, "history-alpha.html")
	require.Contains(t, indexHTML, "history-beta.html")
}

func TestRenderCommand_MissingStoreDir(t *testing.T) {
	t.Parallel()

	cmd := buildRenderCommand()
	cmd.SetArgs([]string{"/nonexistent/store/dir", "--output", t.TempDir()})

	err := cmd.Execute()
	require.Error(t, err)
}

func TestRenderCommand_CreatesOutputDir(t *testing.T) {
	t.Parallel()

	registerTestPlotSections(t)

	storeDir := createTestStore(t)
	outputDir := filepath.Join(t.TempDir(), "new", "nested", "dir")

	cmd := buildRenderCommand()
	cmd.SetArgs([]string{storeDir, "--output", outputDir})

	err := cmd.Execute()
	require.NoError(t, err)

	_, statErr := os.Stat(filepath.Join(outputDir, "index.html"))
	require.NoError(t, statErr, "index.html should exist in created output dir")
}

func TestRenderCommand_SkipsUnregisteredAnalyzers(t *testing.T) {
	t.Parallel()

	// Create store with an analyzer that has no registered section renderer.
	storeDir := filepath.Join(t.TempDir(), "store")
	store := analyze.NewFileReportStore(storeDir)

	writeAnalyzerEntry(t, store, "history/unknown")

	closeErr := store.Close()
	require.NoError(t, closeErr)

	outputDir := filepath.Join(t.TempDir(), "html")

	cmd := buildRenderCommand()
	cmd.SetArgs([]string{storeDir, "--output", outputDir})

	// Should succeed even with no renderable analyzers.
	err := cmd.Execute()
	require.NoError(t, err)

	// Index should still exist (possibly empty).
	_, statErr := os.Stat(filepath.Join(outputDir, "index.html"))
	require.NoError(t, statErr, "index.html should still be created")
}
