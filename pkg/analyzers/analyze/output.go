package analyze

import (
	"fmt"
	"io"
	"os"
	"sort"
	"strings"

	"github.com/go-echarts/go-echarts/v2/components"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
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

	if format == FormatTimeSeries {
		return outputMergedTimeSeries(leaves, results, writer)
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

// outputMergedTimeSeries builds and writes a unified time-series from all analyzer reports.
func outputMergedTimeSeries(
	leaves []HistoryAnalyzer,
	results map[HistoryAnalyzer]Report,
	writer io.Writer,
) error {
	reports := make(map[string]Report, len(leaves))
	for _, leaf := range leaves {
		if res := results[leaf]; res != nil {
			reports[leaf.Flag()] = res
		}
	}

	// Build commit metadata from commitsByTick (available in most reports).
	commitMeta := buildCommitMetaFromReports(reports)

	ts := BuildMergedTimeSeries(reports, commitMeta, 0)

	return WriteMergedTimeSeries(ts, writer)
}

// buildCommitMetaFromReports extracts an ordered list of CommitMeta from
// the commitsByTick data present in analyzer reports.
func buildCommitMetaFromReports(reports map[string]Report) []CommitMeta {
	// Try to find commitsByTick from any report.
	var commitsByTick map[int][]gitlib.Hash

	for _, report := range reports {
		if cbt, ok := report["commits_by_tick"].(map[int][]gitlib.Hash); ok && len(cbt) > 0 {
			commitsByTick = cbt

			break
		}
	}

	if len(commitsByTick) == 0 {
		return nil
	}

	// Sort ticks to get chronological order.
	ticks := make([]int, 0, len(commitsByTick))
	for tick := range commitsByTick {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	var meta []CommitMeta

	for _, tick := range ticks {
		for _, hash := range commitsByTick[tick] {
			meta = append(meta, CommitMeta{
				Hash: hash.String(),
				Tick: tick,
			})
		}
	}

	return meta
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
