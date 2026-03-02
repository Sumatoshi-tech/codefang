package imports

import (
	"fmt"
	"slices"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed import usage data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	records, readErr := readImportUsageIfPresent(reader, kinds)
	if readErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindImportUsage, readErr)
	}

	return buildImportsStoreSections(records)
}

// readImportUsageIfPresent reads all import_usage records, returning nil if absent.
func readImportUsageIfPresent(reader analyze.ReportReader, kinds []string) ([]ImportUsageRecord, error) {
	if !slices.Contains(kinds, KindImportUsage) {
		return nil, nil
	}

	var result []ImportUsageRecord

	iterErr := reader.Iter(KindImportUsage, func(raw []byte) error {
		var record ImportUsageRecord

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// buildImportsStoreSections constructs the imports plot sections from pre-computed data.
func buildImportsStoreSections(records []ImportUsageRecord) ([]plotpage.Section, error) {
	if len(records) == 0 {
		return nil, nil
	}

	chart := buildBarChartFromUsageRecords(records)

	return []plotpage.Section{
		{
			Title:    "Top Imports Usage",
			Subtitle: "Most frequently added imports across the codebase.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = frequently used imports (core dependencies)",
					"External libraries = check for outdated or redundant dependencies",
					"Standard library imports = indicate code patterns",
					"Look for: Unexpected dependencies or duplicate functionality",
					"Action: Consider consolidating similar imports",
				},
			},
		},
	}, nil
}

// buildBarChartFromUsageRecords builds a bar chart from pre-sorted ImportUsageRecord data.
func buildBarChartFromUsageRecords(records []ImportUsageRecord) *charts.Bar {
	limit := min(len(records), topImportsLimit)
	top := records[:limit]

	labels := make([]string, limit)
	data := make([]int, limit)

	for i, rec := range top {
		labels[i] = rec.Import
		data[i] = int(rec.Count)
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	bar := createImportsBarChart(labels, data, co, palette)

	bar.SetGlobalOptions(
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
	)

	return bar
}
