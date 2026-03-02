package commands

// FRD: specs/frds/FRD-20260228-plot-through-store.md.

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestRunCommand_ForwardsPlotOutputFlag(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	outputDir := t.TempDir()
	command.SetArgs([]string{
		"-a", "history/devs",
		"--format", "plot",
		"--output", outputDir,
	})

	err := command.Execute()
	require.NoError(t, err)
	require.Equal(t, outputDir, seenOptions.PlotOutput)
}

func TestRunCommand_ForwardsKeepStoreFlag(t *testing.T) {
	t.Parallel()

	var seenOptions HistoryRunOptions

	command := newRunCommandWithDeps(
		func(_ string, _ []string, _ string, _ bool, _ bool, _ io.Writer) error {
			return nil
		},
		func(_ context.Context, _ string, _ []string, _ string, _ bool, opts HistoryRunOptions, _ io.Writer) error {
			seenOptions = opts

			return nil
		},
		stubRunRegistry,
		noopObservabilityInit,
	)

	outputDir := t.TempDir()
	command.SetArgs([]string{
		"-a", "history/devs",
		"--format", "plot",
		"--output", outputDir,
		"--keep-store",
	})

	err := command.Execute()
	require.NoError(t, err)
	require.True(t, seenOptions.KeepStore)
}

func TestRenderFromStore_ProducesHTML(t *testing.T) {
	t.Parallel()

	registerTestPlotSections(t)

	storeDir := createTestStore(t)
	outputDir := filepath.Join(t.TempDir(), "html")

	err := renderFromStore(storeDir, outputDir)
	require.NoError(t, err)

	// Verify index.html exists.
	indexData, readErr := os.ReadFile(filepath.Join(outputDir, "index.html"))
	require.NoError(t, readErr, "index.html should exist")
	require.Contains(t, string(indexData), "cdn.tailwindcss.com")

	// Verify per-analyzer pages.
	for _, safeID := range []string{"history-alpha", "history-beta"} {
		pagePath := filepath.Join(outputDir, safeID+".html")

		pageData, pageErr := os.ReadFile(pagePath)
		require.NoError(t, pageErr, "page for %s should exist", safeID)
		require.Contains(t, string(pageData), "cdn.tailwindcss.com")
	}
}

func TestRenderFromStore_InvalidStoreDir(t *testing.T) {
	t.Parallel()

	outputDir := filepath.Join(t.TempDir(), "html")

	err := renderFromStore("/nonexistent/store", outputDir)
	require.Error(t, err)
}

func TestRenderFromStore_CreatesOutputDir(t *testing.T) {
	t.Parallel()

	registerTestPlotSections(t)

	storeDir := createTestStore(t)
	outputDir := filepath.Join(t.TempDir(), "deep", "nested", "dir")

	err := renderFromStore(storeDir, outputDir)
	require.NoError(t, err)

	_, statErr := os.Stat(filepath.Join(outputDir, "index.html"))
	require.NoError(t, statErr, "index.html should exist in nested output dir")
}

func TestPlotOutputRequired_WhenFormatPlot(t *testing.T) {
	t.Parallel()

	err := validatePlotFlags(analyze.FormatPlot, "")
	require.ErrorIs(t, err, ErrPlotOutputRequired)
}

func TestPlotOutputRequired_OtherFormatsIgnored(t *testing.T) {
	t.Parallel()

	err := validatePlotFlags(analyze.FormatJSON, "")
	require.NoError(t, err)
}
