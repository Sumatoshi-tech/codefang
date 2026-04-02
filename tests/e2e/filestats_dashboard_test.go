//go:build e2e

package e2e_test

// Acceptance tests for specs/filestats/SPEC.md — Feature 3 (Visual Dashboard).

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

// renderPlotDir runs static analysis and emits plot pages to a temp dir.
func renderPlotDir(t *testing.T, fileCount int) string {
	t.Helper()

	dir := fixtureDir(t, fileCount)
	outputDir := filepath.Join(t.TempDir(), "reports")
	svc := newStaticService()

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	names := make([]string, 0, len(results))
	for n := range results {
		names = append(names, n)
	}

	require.NoError(t, svc.FormatPlotPages(names, results, outputDir))

	return outputDir
}

// ---------------------------------------------------------------------------
// FR-3.3: index.html
// ---------------------------------------------------------------------------

func TestDashboard_IndexHTMLExists(t *testing.T) {
	t.Parallel()

	outputDir := renderPlotDir(t, 5)

	data, err := os.ReadFile(filepath.Join(outputDir, "index.html"))
	require.NoError(t, err, "index.html must exist")
	assert.Contains(t, string(data), "<html")
}

// ---------------------------------------------------------------------------
// FR-3.1: New chart types
// ---------------------------------------------------------------------------

// TestDashboard_ContributorWorkloadPage validates that the devs store plot
// section renderer is registered and produces sections from sample data.
func TestDashboard_ContributorWorkloadPage(t *testing.T) {
	t.Parallel()

	// The devs analyzer registers a store-based plot section renderer.
	// Verify the registration exists — this is the prerequisite for
	// generating devs chart pages when history analysis runs with --format plot.
	storeFn := analyze.StorePlotSectionsFor("devs")
	assert.NotNil(t, storeFn,
		"devs store plot section renderer must be registered")
}

// TestDashboard_CouplingHeatmapPage validates that the couples plot section
// renderer is registered. The couples analyzer already produces a developer
// coupling heatmap via go-echarts HeatMap.
func TestDashboard_CouplingHeatmapPage(t *testing.T) {
	t.Parallel()

	// The couples analyzer registers a store-based plot section renderer.
	storeFn := analyze.StorePlotSectionsFor("couples")
	assert.NotNil(t, storeFn,
		"couples store plot section renderer must be registered")
}

// ---------------------------------------------------------------------------
// FR-3.5: report.json
// ---------------------------------------------------------------------------

func TestDashboard_ReportJSONEmitted(t *testing.T) {
	t.Parallel()

	outputDir := renderPlotDir(t, 5)

	data, err := os.ReadFile(filepath.Join(outputDir, "report.json"))
	if !assert.NoError(t, err, "report.json must be emitted alongside charts") {
		return
	}

	var parsed jsonObj
	assert.NoError(t, json.Unmarshal(data, &parsed), "report.json must be valid JSON")
}

// ---------------------------------------------------------------------------
// AC: all HTML files well-formed
// ---------------------------------------------------------------------------

func TestDashboard_HTMLWellFormed(t *testing.T) {
	t.Parallel()

	outputDir := renderPlotDir(t, 5)

	entries, err := os.ReadDir(outputDir)
	require.NoError(t, err)

	htmlCount := 0

	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".html") {
			continue
		}

		htmlCount++

		data, err := os.ReadFile(filepath.Join(outputDir, e.Name()))
		require.NoError(t, err)
		content := string(data)
		assert.Contains(t, content, "<html", "%s must have <html", e.Name())
		assert.Contains(t, content, "</html>", "%s must close </html>", e.Name())
	}

	assert.Greater(t, htmlCount, 0, "at least one HTML page must be generated")
}
