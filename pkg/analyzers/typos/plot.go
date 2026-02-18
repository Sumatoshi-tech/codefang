package typos

import (
	"errors"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	topFilesLimit    = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
	chartHeight      = "500px"
	chartWidth       = "100%"
)

// ErrInvalidTypos indicates the report doesn't contain expected typos data.
var ErrInvalidTypos = errors.New("invalid typos report: expected []Typo for typos")

// RegisterPlotSections registers the typos plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/typos", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&HistoryAnalyzer{}).GenerateSections(report)
	})
}

func (t *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, genErr := t.GenerateSections(report)
	if genErr != nil {
		return genErr
	}

	page := plotpage.NewPage(
		"Typo Analysis",
		"Tracking typo corrections across the codebase",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (t *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, chartErr := t.buildChart(report)
	if chartErr != nil {
		return nil, chartErr
	}

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

// GenerateChart implements PlotGenerator interface.
func (t *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return t.buildChart(report)
}

// buildChart creates a bar chart showing typo-prone files.
func (t *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.Bar, error) {
	fileLabels, fileCounts, extractErr := extractTyposPlotData(report)
	if extractErr != nil {
		return nil, extractErr
	}

	if len(fileLabels) == 0 {
		return createEmptyTyposChart(), nil
	}

	chartOpts := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createTyposBarChart(fileLabels, fileCounts, chartOpts, palette), nil
}

// extractTyposPlotData extracts typo file labels and counts from the report,
// handling both in-memory and binary-decoded JSON key formats.
func extractTyposPlotData(report analyze.Report) (fileLabels []string, fileCounts []int, extractErr error) {
	fileLabels, fileCounts, extractErr = tryInMemoryTypos(report)
	if extractErr == nil {
		return fileLabels, fileCounts, nil
	}

	fileLabels, fileCounts, extractErr = tryFileTypos(report)
	if extractErr == nil {
		return fileLabels, fileCounts, nil
	}

	return tryTypoList(report)
}

// errTyposKeyNotFound indicates the "typos" key is missing or has wrong type.
var errTyposKeyNotFound = errors.New("typos key not found or wrong type")

// errFileTyposKeyNotFound indicates the "file_typos" key is missing.
var errFileTyposKeyNotFound = errors.New("file_typos key not found")

// errFileTyposNotList indicates "file_typos" is not a non-empty list.
var errFileTyposNotList = errors.New("file_typos is not a non-empty list")

// tryInMemoryTypos attempts to extract typos from the in-memory []Typo key.
func tryInMemoryTypos(report analyze.Report) (fileLabels []string, fileCounts []int, extractErr error) {
	typos, ok := report["typos"].([]Typo)
	if !ok {
		return nil, nil, errTyposKeyNotFound
	}

	if len(typos) == 0 {
		return nil, nil, nil
	}

	counts := countTyposPerFile(typos)
	fileLabels, fileCounts = topTypoFiles(counts, topFilesLimit)

	return fileLabels, fileCounts, nil
}

// tryFileTypos attempts to extract per-file typo counts from the "file_typos" key.
func tryFileTypos(report analyze.Report) (fileLabels []string, fileCounts []int, extractErr error) {
	rawFileTypos, exists := report["file_typos"]
	if !exists {
		return nil, nil, errFileTyposKeyNotFound
	}

	fileTypoList, isList := rawFileTypos.([]any)
	if !isList || len(fileTypoList) == 0 {
		return nil, nil, errFileTyposNotList
	}

	counts := make(map[string]int, len(fileTypoList))

	for _, item := range fileTypoList {
		entry, isMap := item.(map[string]any)
		if !isMap {
			continue
		}

		fileName, hasFile := entry["file"].(string)
		if !hasFile || fileName == "" {
			continue
		}

		counts[fileName] = typosToInt(entry["typo_count"])
	}

	fileLabels, fileCounts = topTypoFiles(counts, topFilesLimit)

	return fileLabels, fileCounts, nil
}

// tryTypoList attempts to extract typos from the "typo_list" key (binary-decoded fallback).
func tryTypoList(report analyze.Report) (fileLabels []string, fileCounts []int, extractErr error) {
	rawList, exists := report["typo_list"]
	if !exists {
		return nil, nil, ErrInvalidTypos
	}

	if rawList == nil {
		return nil, nil, nil
	}

	typoList, isList := rawList.([]any)
	if !isList {
		return nil, nil, ErrInvalidTypos
	}

	if len(typoList) == 0 {
		return nil, nil, nil
	}

	counts := make(map[string]int)

	for _, item := range typoList {
		entry, isMap := item.(map[string]any)
		if !isMap {
			continue
		}

		fileName, hasFile := entry["file"].(string)
		if !hasFile || fileName == "" {
			continue
		}

		counts[fileName]++
	}

	fileLabels, fileCounts = topTypoFiles(counts, topFilesLimit)

	return fileLabels, fileCounts, nil
}

// typosToInt converts a numeric value to int.
func typosToInt(val any) int {
	switch num := val.(type) {
	case float64:
		return int(num)
	case int:
		return num
	case int64:
		return int(num)
	default:
		return 0
	}
}

func createTyposBarChart(labels []string, data []int, chartOpts *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(chartOpts.Init(chartWidth, chartHeight)),
		charts.WithTooltipOpts(chartOpts.Tooltip("axis")),
		charts.WithGridOpts(chartOpts.Grid()),
		charts.WithDataZoomOpts(chartOpts.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    chartOpts.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: chartOpts.AxisColor()}},
		}),
		charts.WithYAxisOpts(chartOpts.YAxis("Typo Count")),
	)
	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(data))

	for idx, val := range data {
		barData[idx] = opts.BarData{Value: val}
	}

	bar.AddSeries("Typos", barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Warning}))

	return bar
}

func countTyposPerFile(typos []Typo) map[string]int {
	counts := make(map[string]int)

	for _, typo := range typos {
		counts[typo.File]++
	}

	return counts
}

func topTypoFiles(counts map[string]int, limit int) (labels []string, data []int) {
	type kv struct {
		key   string
		value int
	}

	var items []kv

	for name, count := range counts {
		items = append(items, kv{name, count})
	}

	sort.Slice(items, func(idx, jdx int) bool { return items[idx].value > items[jdx].value })

	if len(items) > limit {
		items = items[:limit]
	}

	labels = make([]string, len(items))
	data = make([]int, len(items))

	for idx, item := range items {
		labels[idx] = item.key
		data[idx] = item.value
	}

	return labels, data
}

func createEmptyTyposChart() *charts.Bar {
	chartOpts := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(chartOpts.Init(chartWidth, emptyChartHeight)),
		charts.WithTitleOpts(chartOpts.Title("Typo-Prone Files", "No data")),
	)

	return bar
}
