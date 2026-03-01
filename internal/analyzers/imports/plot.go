package imports

import (
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	topImportsLimit  = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
)

// registerHistoryPlotSections registers the history/imports plot section renderer at package load time.
var _ = registerHistoryPlotSections()

func registerHistoryPlotSections() bool {
	analyze.RegisterStorePlotSections("imports-per-dev", GenerateStoreSections)

	return true
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

func topImports(counts map[string]int64) (labels []string, data []int) {
	type kv struct {
		k string
		v int64
	}

	var items []kv

	for k, v := range counts {
		items = append(items, kv{k, v})
	}

	sort.Slice(items, func(i, j int) bool { return items[i].v > items[j].v })

	if len(items) > topImportsLimit {
		items = items[:topImportsLimit]
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
