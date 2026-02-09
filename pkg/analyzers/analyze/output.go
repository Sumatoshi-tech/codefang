package analyze

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/go-echarts/go-echarts/v2/components"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// PlotGenerator interface for analyzers that can generate plots.
type PlotGenerator interface {
	GenerateChart(report Report) (components.Charter, error)
}

// SectionGenerator interface for analyzers that can generate page sections.
type SectionGenerator interface {
	GenerateSections(report Report) ([]plotpage.Section, error)
}

// OutputHistoryResults outputs the results for all selected history leaves.
func OutputHistoryResults(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
	format string,
	writer io.Writer,
) error {
	if writer == nil {
		writer = os.Stdout
	}

	rawOutput := format == FormatJSON || format == FormatPlot || format == FormatBinary
	if !rawOutput {
		PrintHeader(writer)
	}

	if format == FormatPlot && len(leaves) > 1 {
		return outputCombinedPlot(leaves, results, writer)
	}

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		if !rawOutput {
			fmt.Fprintf(writer, "%s:\n", leaf.Name())
		}

		serializeErr := leaf.Serialize(res, format, writer)
		if serializeErr != nil {
			return fmt.Errorf("serialization error for %s: %w", leaf.Name(), serializeErr)
		}
	}

	return nil
}

func outputCombinedPlot(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
	writer io.Writer,
) error {
	page := buildCombinedPage(leaves)

	for _, leaf := range leaves {
		res := results[leaf]
		if res == nil {
			continue
		}

		err := addLeafToPage(page, leaf, res)
		if err != nil {
			return err
		}
	}

	err := page.Render(writer)
	if err != nil {
		return fmt.Errorf("render page: %w", err)
	}

	return nil
}

func buildCombinedPage(leaves []HistoryAnalyzer) *plotpage.Page {
	names := make([]string, 0, len(leaves))
	for _, leaf := range leaves {
		names = append(names, leaf.Name())
	}

	return plotpage.NewPage(
		"Combined Analysis Report",
		fmt.Sprintf("Analysis results for: %s", strings.Join(names, ", ")),
	)
}

func addLeafToPage(page *plotpage.Page, leaf HistoryAnalyzer, res Report) error {
	if sectionGen, ok := leaf.(SectionGenerator); ok {
		return addSectionsToPage(page, sectionGen, leaf.Name(), res)
	}

	if plotter, ok := leaf.(PlotGenerator); ok {
		return addChartToPage(page, plotter, leaf.Name(), res)
	}

	return nil
}

func addSectionsToPage(page *plotpage.Page, gen SectionGenerator, name string, res Report) error {
	sections, err := gen.GenerateSections(res)
	if err != nil {
		return fmt.Errorf("failed to generate sections for %s: %w", name, err)
	}

	page.Add(sections...)

	return nil
}

func addChartToPage(page *plotpage.Page, plotter PlotGenerator, name string, res Report) error {
	chart, err := plotter.GenerateChart(res)
	if err != nil {
		return fmt.Errorf("failed to generate chart for %s: %w", name, err)
	}

	if renderable, ok := chart.(plotpage.Renderable); ok {
		page.Add(plotpage.Section{
			Title:    name,
			Subtitle: fmt.Sprintf("Results from %s analyzer", name),
			Chart:    plotpage.WrapChart(renderable),
		})
	}

	return nil
}

// PrintHeader prints the codefang version header.
func PrintHeader(writer io.Writer) {
	fmt.Fprintln(writer, "codefang (v2):")
	fmt.Fprintf(writer, "  version: %d\n", version.Binary)
	fmt.Fprintln(writer, "  hash:", version.BinaryGitHash)
}
