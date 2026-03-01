package filehistory

import (
	"fmt"
	"slices"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed file history data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	churnData, churnErr := readFileChurnIfPresent(reader, kinds)
	if churnErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindFileChurn, churnErr)
	}

	return buildStoreSections(churnData)
}

// readFileChurnIfPresent reads all file_churn records, returning nil if absent.
func readFileChurnIfPresent(reader analyze.ReportReader, kinds []string) ([]FileChurnData, error) {
	if !slices.Contains(kinds, KindFileChurn) {
		return nil, nil
	}

	var result []FileChurnData

	iterErr := reader.Iter(KindFileChurn, func(raw []byte) error {
		var record FileChurnData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// buildStoreSections constructs the file history plot sections from pre-computed data.
func buildStoreSections(churnData []FileChurnData) ([]plotpage.Section, error) {
	if len(churnData) == 0 {
		return nil, nil
	}

	chart := buildBarChartFromChurnData(churnData)

	return []plotpage.Section{
		{
			Title:    "Most Modified Files",
			Subtitle: "Files ranked by total number of commits touching them.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = frequently modified files (high churn)",
					"Configuration files = expected to change often",
					"Core business logic = may indicate instability or active development",
					"Look for: Files changing too frequently that should be stable",
					"Action: High-churn files benefit from better test coverage",
				},
			},
		},
	}, nil
}

// buildBarChartFromChurnData builds a bar chart from pre-sorted FileChurnData.
func buildBarChartFromChurnData(churnData []FileChurnData) *charts.Bar {
	// Sort by commit count descending and take top N.
	sort.Slice(churnData, func(i, j int) bool {
		return churnData[i].CommitCount > churnData[j].CommitCount
	})

	limit := min(len(churnData), topFilesLimit)
	top := churnData[:limit]

	labels := make([]string, limit)
	seriesData := make([]plotpage.SeriesData, limit)

	for i, item := range top {
		labels[i] = item.Path
		seriesData[i] = item.CommitCount
	}

	cOpts := plotpage.DefaultChartOpts()

	series := []plotpage.BarSeries{
		{
			Name:  "Commits",
			Data:  seriesData,
			Color: plotpage.GetChartPalette(plotpage.ThemeDark).Semantic.Bad,
		},
	}

	chart := plotpage.BuildBarChart(cOpts, labels, series, "Commits")

	chart.SetGlobalOptions(
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    cOpts.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: cOpts.AxisColor()}},
		}),
	)

	return chart
}
