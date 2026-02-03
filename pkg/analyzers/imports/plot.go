package imports

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
	topImportsLimit  = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
)

// ErrInvalidImports indicates the report doesn't contain expected imports data.
var ErrInvalidImports = errors.New("invalid imports report: expected Map for imports")

func (h *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := h.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Import Usage Analysis",
		"Tracking dependency usage patterns over project history",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (h *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := h.buildChart(report)
	if err != nil {
		return nil, err
	}

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

// GenerateChart implements PlotGenerator interface.
func (h *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return h.buildChart(report)
}

// buildChart creates a bar chart showing top imports by usage.
func (h *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.Bar, error) {
	imports, ok := report["imports"].(Map)
	if !ok {
		return nil, ErrInvalidImports
	}

	if len(imports) == 0 {
		return createEmptyImportsChart(), nil
	}

	counts := aggregateImportCounts(imports)
	labels, data := topImports(counts, topImportsLimit)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createImportsBarChart(labels, data, co, palette), nil
}

func createImportsBarChart(labels []string, data []int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithGridOpts(co.Grid()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(co.YAxis("Usage Count")),
	)
	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(data))
	for i, v := range data {
		barData[i] = opts.BarData{Value: v}
	}

	bar.AddSeries("Usage", barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}))

	return bar
}

func aggregateImportCounts(imports Map) map[string]int64 {
	counts := make(map[string]int64)

	for _, langMap := range imports {
		for _, impMap := range langMap {
			for name, tickMap := range impMap {
				for _, count := range tickMap {
					counts[name] += count
				}
			}
		}
	}

	return counts
}

func topImports(counts map[string]int64, limit int) (labels []string, data []int) {
	type kv struct {
		k string
		v int64
	}

	var items []kv

	for k, v := range counts {
		items = append(items, kv{k, v})
	}

	sort.Slice(items, func(i, j int) bool { return items[i].v > items[j].v })

	if len(items) > limit {
		items = items[:limit]
	}

	labels = make([]string, len(items))
	data = make([]int, len(items))

	for i, item := range items {
		labels[i] = item.k
		data[i] = int(item.v)
	}

	return labels, data
}

func createEmptyImportsChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Top Imports", "No data")),
	)

	return bar
}
