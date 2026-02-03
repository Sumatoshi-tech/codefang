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
	if len(data.Metrics.Churn) == 0 {
		return nil
	}

	xLabels := make([]string, len(data.Metrics.Churn))
	added := make([]opts.BarData, len(data.Metrics.Churn))
	removed := make([]opts.BarData, len(data.Metrics.Churn))

	for i, cm := range data.Metrics.Churn {
		xLabels[i] = strconv.Itoa(cm.Tick)
		added[i] = opts.BarData{Value: cm.Added}
		removed[i] = opts.BarData{Value: -cm.Removed}
	}

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
