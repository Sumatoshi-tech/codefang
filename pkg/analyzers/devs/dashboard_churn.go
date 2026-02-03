package devs

import (
	"io"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

type churnContent struct {
	chart *charts.Bar
}

func createChurnTab(data *DashboardData) *churnContent {
	return &churnContent{chart: createChurnChart(data)}
}

// Render implements the Renderable interface for the churn tab.
func (cc *churnContent) Render(w io.Writer) error {
	if cc.chart == nil {
		return plotpage.NewText("No churn data available").Render(w)
	}

	return plotpage.WrapChart(cc.chart).Render(w)
}

func createChurnChart(data *DashboardData) *charts.Bar {
	tickKeys := sortedKeys(data.Ticks)
	if len(tickKeys) == 0 {
		return nil
	}

	added, removed := computeChurnData(tickKeys, data.Ticks)
	xLabels := buildChurnLabels(tickKeys)

	co := plotpage.DefaultChartOpts()

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", churnChartHeight)),
		charts.WithTitleOpts(co.Title("Code Churn", "Lines added vs removed over time")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithGridOpts(co.GridCompact()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis("Lines")),
	)

	bar.SetXAxis(xLabels)

	bar.AddSeries("Added", added,
		charts.WithItemStyleOpts(opts.ItemStyle{Color: "#22c55e"}),
	)
	bar.AddSeries("Removed", removed,
		charts.WithItemStyleOpts(opts.ItemStyle{Color: "#ef4444"}),
	)

	return bar
}

func computeChurnData(tickKeys []int, ticks map[int]map[int]*DevTick) (added, removed []opts.BarData) {
	added = make([]opts.BarData, len(tickKeys))
	removed = make([]opts.BarData, len(tickKeys))

	for i, tick := range tickKeys {
		totalAdded, totalRemoved := 0, 0

		for _, dt := range ticks[tick] {
			totalAdded += dt.Added
			totalRemoved += dt.Removed
		}

		added[i] = opts.BarData{Value: totalAdded}
		removed[i] = opts.BarData{Value: -totalRemoved}
	}

	return added, removed
}

func buildChurnLabels(tickKeys []int) []string {
	labels := make([]string, len(tickKeys))

	for i, tick := range tickKeys {
		labels[i] = strconv.Itoa(tick)
	}

	return labels
}
