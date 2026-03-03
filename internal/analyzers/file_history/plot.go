package filehistory

import (
	"errors"
	"fmt"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	topFilesLimit        = 20
	xAxisRotate          = 60
	emptyChartHeight     = "400px"
	compositionAreaAlpha = 0.5
)

// categoryColors maps each category to a chart color.
var categoryColors = map[Category]string{
	CategorySource:        "#4CAF50",
	CategoryDocumentation: "#2196F3",
	CategoryConfiguration: "#FF9800",
	CategoryVendor:        "#9C27B0",
	CategoryGenerated:     "#607D8B",
	CategoryDotFile:       "#795548",
	CategoryImage:         "#E91E63",
	CategoryBinary:        "#F44336",
}

// ErrInvalidFiles indicates the report doesn't contain expected files data.
var ErrInvalidFiles = errors.New("invalid file_history report: expected map[string]FileHistory for Files")

func (h *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := h.GenerateSections(report)
	if err != nil {
		return err
	}

	return plotpage.RenderAnalyzerPage(writer,
		"File History Analysis",
		"Identifying the most actively modified files in the repository",
		sections...,
	)
}

// GenerateSections returns the sections for combined reports.
func (h *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := h.buildChart(report)
	if err != nil {
		return nil, err
	}

	sections := []plotpage.Section{
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
	}

	if compChart := buildCompositionChart(report); compChart != nil {
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

// buildChart creates a bar chart showing the most modified files.
func (h *HistoryAnalyzer) buildChart(report analyze.Report) (chart *charts.Bar, buildErr error) {
	labels, data, err := extractFileHistoryData(report)
	if err != nil {
		return nil, err
	}

	if len(labels) == 0 {
		return createEmptyFileChart(), nil
	}

	cOpts := plotpage.DefaultChartOpts()

	// Convert int to any for SeriesData.
	seriesData := make([]plotpage.SeriesData, len(data))
	for i, v := range data {
		seriesData[i] = v
	}

	series := []plotpage.BarSeries{
		{
			Name:  "Commits",
			Data:  seriesData,
			Color: plotpage.GetChartPalette(plotpage.ThemeDark).Semantic.Bad,
		},
	}

	chart = plotpage.BuildBarChart(cOpts, labels, series, "Commits")

	// Apply custom X axis for rotated labels.
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

	return chart, nil
}

// fileChurnItem holds a file path and its commit count for sorting.
type fileChurnItem struct {
	path        string
	commitCount int
}

// extractFileHistoryData extracts file names and commit counts from the report.
func extractFileHistoryData(report analyze.Report) (labels []string, data []int, err error) {
	files, filesOK := report["Files"].(map[string]FileHistory)
	if !filesOK {
		return nil, nil, ErrInvalidFiles
	}

	var items []fileChurnItem

	for name, hist := range files {
		items = append(items, fileChurnItem{name, len(hist.Hashes)})
	}

	if len(items) == 0 {
		return nil, nil, nil
	}

	sort.Slice(items, func(i, j int) bool { return items[i].commitCount > items[j].commitCount })

	if len(items) > topFilesLimit {
		items = items[:topFilesLimit]
	}

	labels = make([]string, len(items))
	data = make([]int, len(items))

	for i, item := range items {
		labels[i] = item.path
		data[i] = item.commitCount
	}

	return labels, data, nil
}

// RegisterPlotSections registers the file-history plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterStorePlotSections("file-history", GenerateStoreSections)
}

// buildCompositionChart builds a stacked area chart from tick_composition data.
func buildCompositionChart(report analyze.Report) *charts.Line {
	tickComp, ok := report["tick_composition"].(map[int]*CategoryCounts)
	if !ok || len(tickComp) == 0 {
		return nil
	}

	// Sort ticks.
	ticks := make([]int, 0, len(tickComp))
	for t := range tickComp {
		ticks = append(ticks, t)
	}

	sort.Ints(ticks)

	// Build labels and per-category data.
	labels := make([]string, len(ticks))
	for i, t := range ticks {
		labels[i] = fmt.Sprintf("Tick %d", t)
	}

	var series []plotpage.LineSeries

	for _, cat := range AllCategories {
		data := make([]plotpage.SeriesData, len(ticks))
		hasData := false

		for i, t := range ticks {
			v := tickComp[t].Get(cat)
			data[i] = v

			if v > 0 {
				hasData = true
			}
		}

		if !hasData {
			continue
		}

		series = append(series, plotpage.LineSeries{
			Name:        string(cat),
			Data:        data,
			Color:       categoryColors[cat],
			Stack:       "total",
			AreaOpacity: compositionAreaAlpha,
		})
	}

	if len(series) == 0 {
		return nil
	}

	cOpts := plotpage.DefaultChartOpts()

	return plotpage.BuildLineChart(cOpts, labels, series, "Files")
}

// buildCompositionChartFromTS builds a stacked area chart from pre-computed time series.
func buildCompositionChartFromTS(ts []CompositionTimeSeriesEntry) *charts.Line {
	if len(ts) == 0 {
		return nil
	}

	labels := make([]string, len(ts))
	for i, entry := range ts {
		labels[i] = fmt.Sprintf("Tick %d", entry.Tick)
	}

	var series []plotpage.LineSeries

	for _, cat := range AllCategories {
		data := make([]plotpage.SeriesData, len(ts))
		hasData := false

		for i, entry := range ts {
			v := entry.Breakdown[string(cat)]
			data[i] = v

			if v > 0 {
				hasData = true
			}
		}

		if !hasData {
			continue
		}

		series = append(series, plotpage.LineSeries{
			Name:        string(cat),
			Data:        data,
			Color:       categoryColors[cat],
			Stack:       "total",
			AreaOpacity: compositionAreaAlpha,
		})
	}

	if len(series) == 0 {
		return nil
	}

	cOpts := plotpage.DefaultChartOpts()

	return plotpage.BuildLineChart(cOpts, labels, series, "Files")
}

func createEmptyFileChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Top Modified Files", "No data")),
	)

	return bar
}
