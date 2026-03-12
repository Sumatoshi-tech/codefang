package commands

import (
	"errors"
	"fmt"
	"log/slog"
	"os"
	"strings"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/anomaly"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/clones"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/quality"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/typos"
)

const (
	analyzerIDSepOld   = "/"
	analyzerIDSepSafe  = "-"
	renderDirPerm      = 0o750
	renderCmdUse       = "render <store-dir>"
	renderCmdShort     = "Render stored analysis results as multi-page HTML"
	renderArgCount     = 1
	renderOutputFlag   = "output"
	renderOutputShort  = "o"
	renderOutputUsage  = "output directory for HTML files"
	renderProjectTitle = "Codefang"
)

// ErrNoOutputDir is returned when the --output flag is not set.
var ErrNoOutputDir = errors.New("output directory is required (use --output)")

// ErrEmptyStore is returned when the store contains no analyzer data.
var ErrEmptyStore = errors.New("no analyzer data found in store")

// ErrNoSectionRenderer is returned when no store section renderer is registered.
var ErrNoSectionRenderer = errors.New("no section renderer registered")

// NewRenderCommand creates the render subcommand.
func NewRenderCommand() *cobra.Command {
	registerAllPlotSections()

	return buildRenderCommand()
}

func buildRenderCommand() *cobra.Command {
	var outputDir string

	cmd := &cobra.Command{
		Use:   renderCmdUse,
		Short: renderCmdShort,
		Args:  cobra.ExactArgs(renderArgCount),
		RunE: func(_ *cobra.Command, args []string) error {
			if outputDir == "" {
				return ErrNoOutputDir
			}

			return runRender(args[0], outputDir)
		},
	}

	cmd.Flags().StringVarP(&outputDir, renderOutputFlag, renderOutputShort, "", renderOutputUsage)

	return cmd
}

func registerAllPlotSections() {
	anomaly.RegisterPlotSections()
	burndown.RegisterPlotSections()
	clones.RegisterPlotSections()
	cohesion.RegisterPlotSections()
	comments.RegisterPlotSections()
	complexity.RegisterPlotSections()
	couples.RegisterPlotSections()
	devs.RegisterDevPlotSections()
	filehistory.RegisterPlotSections()
	halstead.RegisterPlotSections()
	imports.RegisterPlotSections()
	quality.RegisterPlotSections()
	sentiment.RegisterPlotSections()
	shotness.RegisterPlotSections()
	typos.RegisterPlotSections()
}

func runRender(storeDir, outputDir string) error {
	mkErr := os.MkdirAll(outputDir, renderDirPerm)
	if mkErr != nil {
		return fmt.Errorf("create output dir: %w", mkErr)
	}

	store := analyze.NewFileReportStore(storeDir)

	defer store.Close()

	analyzerIDs := store.AnalyzerIDs()
	if len(analyzerIDs) == 0 {
		return ErrEmptyStore
	}

	renderer := &plotpage.MultiPageRenderer{
		OutputDir: outputDir,
		Title:     renderProjectTitle,
		Theme:     plotpage.ThemeDark,
	}

	pages := make([]plotpage.PageMeta, 0, len(analyzerIDs))

	for _, id := range analyzerIDs {
		meta, renderErr := renderOneAnalyzer(store, renderer, id)
		if renderErr != nil {
			slog.Default().Warn("skipping analyzer", "id", id, "error", renderErr)

			continue
		}

		pages = append(pages, meta)
	}

	indexErr := renderer.RenderIndex(pages)
	if indexErr != nil {
		return fmt.Errorf("render index: %w", indexErr)
	}

	return nil
}

func renderOneAnalyzer(
	store *analyze.FileReportStore,
	renderer *plotpage.MultiPageRenderer,
	id string,
) (plotpage.PageMeta, error) {
	sections, genErr := generateSectionsForAnalyzer(store, id)
	if genErr != nil {
		return plotpage.PageMeta{}, genErr
	}

	safeID := safeAnalyzerID(id)

	pageErr := renderer.RenderAnalyzerPage(safeID, id, sections)
	if pageErr != nil {
		return plotpage.PageMeta{}, fmt.Errorf("render page %s: %w", id, pageErr)
	}

	return plotpage.PageMeta{
		ID:    safeID,
		Title: id,
	}, nil
}

// generateSectionsForAnalyzer uses the store-aware section renderer.
// All analyzers write structured kinds to the store.
func generateSectionsForAnalyzer(store *analyze.FileReportStore, id string) ([]plotpage.Section, error) {
	storeFn := analyze.StorePlotSectionsFor(id)
	if storeFn == nil {
		return nil, fmt.Errorf("%w: %s", ErrNoSectionRenderer, id)
	}

	reader, openErr := store.Open(id)
	if openErr != nil {
		return nil, fmt.Errorf("open %s: %w", id, openErr)
	}

	defer reader.Close()

	return storeFn(reader)
}

// safeAnalyzerID converts analyzer IDs like "history/burndown" to "history-burndown"
// for use as filenames.
func safeAnalyzerID(id string) string {
	return strings.ReplaceAll(id, analyzerIDSepOld, analyzerIDSepSafe)
}
