package filehistory

import (
	"fmt"
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

	compositionTS, compErr := readCompositionIfPresent(reader, kinds)
	if compErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindComposition, compErr)
	}

	return buildStoreSections(churnData, compositionTS)
}

// readFileChurnIfPresent reads all file_churn records, returning nil if absent.
func readFileChurnIfPresent(reader analyze.ReportReader, kinds []string) ([]FileChurnData, error) {
	return analyze.ReadRecordsIfPresent[FileChurnData](reader, kinds, KindFileChurn)
}

// readCompositionIfPresent reads all composition records, returning nil if absent.
func readCompositionIfPresent(reader analyze.ReportReader, kinds []string) ([]CompositionTimeSeriesEntry, error) {
	return analyze.ReadRecordsIfPresent[CompositionTimeSeriesEntry](reader, kinds, KindComposition)
}

// buildStoreSections constructs the file history plot sections from pre-computed data.
func buildStoreSections(churnData []FileChurnData, compositionTS []CompositionTimeSeriesEntry) ([]plotpage.Section, error) {
	if len(churnData) == 0 && len(compositionTS) == 0 {
		return nil, nil
	}

	var sections []plotpage.Section

	if len(churnData) > 0 {
		chart := buildBarChartFromChurnData(churnData)
		sections = append(sections, plotpage.Section{
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
		})
	}

	if compChart := buildCompositionChartFromTS(compositionTS); compChart != nil {
		sections = append(sections, plotpage.Section{
			Title:    "File Composition Over Time",
			Subtitle: "Distribution of changed files by category across analysis ticks.",
			Chart:    plotpage.WrapChart(compChart),
			Hint: plotpage.Hint{
				Title: "Categories:",
				Items: []string{
					"Source = project code (first-party)",
					"Documentation = docs, README, LICENSE, examples",
					"Configuration = YAML, JSON, TOML, XML, Makefile",
					"Vendor = third-party dependencies (node_modules, vendor/)",
					"Generated = protobuf, code generators, minified bundles",
					"DotFile = .gitignore, .editorconfig, etc.",
					"Image = PNG, JPG, GIF",
					"Binary = files with binary content",
				},
			},
		})
	}

	return sections, nil
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
