package typos

import (
	"fmt"
	"slices"

	"github.com/go-echarts/go-echarts/v2/charts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed typo data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	fileTypos, readErr := readFileTyposIfPresent(reader, kinds)
	if readErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindFileTypos, readErr)
	}

	return buildStoreSections(fileTypos)
}

// readFileTyposIfPresent reads all file_typos records, returning nil if absent.
func readFileTyposIfPresent(reader analyze.ReportReader, kinds []string) ([]FileTypoData, error) {
	if !slices.Contains(kinds, KindFileTypos) {
		return nil, nil
	}

	var result []FileTypoData

	iterErr := reader.Iter(KindFileTypos, func(raw []byte) error {
		var record FileTypoData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// buildStoreSections constructs the typos plot sections from pre-computed data.
func buildStoreSections(fileTypos []FileTypoData) ([]plotpage.Section, error) {
	if len(fileTypos) == 0 {
		return nil, nil
	}

	chart := buildBarChartFromFileTypos(fileTypos)

	return []plotpage.Section{
		{
			Title:    "Typo-Prone Files",
			Subtitle: "Files ranked by number of typo fixes detected in commit history.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = files where typos are frequently fixed",
					"Documentation files = expected to have more text-related fixes",
					"Code files = typos may indicate hasty commits",
					"Look for: Code files with unusually high typo rates",
					"Action: Consider adding spell-checking to pre-commit hooks",
				},
			},
		},
	}, nil
}

// buildBarChartFromFileTypos builds a bar chart from pre-sorted FileTypoData.
func buildBarChartFromFileTypos(fileTypos []FileTypoData) *charts.Bar {
	limit := min(len(fileTypos), topFilesLimit)
	top := fileTypos[:limit]

	labels := make([]string, limit)
	barData := make([]plotpage.SeriesData, limit)

	for i, item := range top {
		labels[i] = item.File
		barData[i] = item.TypoCount
	}

	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	series := []plotpage.BarSeries{
		{
			Name:  "Typos",
			Data:  barData,
			Color: palette.Semantic.Warning,
		},
	}

	return plotpage.BuildBarChart(
		nil,
		labels,
		series,
		"Typo Count",
	)
}
