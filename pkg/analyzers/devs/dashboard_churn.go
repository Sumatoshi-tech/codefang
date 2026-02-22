package devs

import (
	"io"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"

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
	added := make([]plotpage.SeriesData, len(data.Metrics.Churn))
	removed := make([]plotpage.SeriesData, len(data.Metrics.Churn))

	for i, cm := range data.Metrics.Churn {
		xLabels[i] = strconv.Itoa(cm.Tick)
		added[i] = cm.Added
		removed[i] = -cm.Removed
	}

	series := []plotpage.BarSeries{
		{Name: "Added", Data: added, Color: "#22c55e"},
		{Name: "Removed", Data: removed, Color: "#ef4444"},
	}

	co := plotpage.DefaultChartOpts()
	bar := plotpage.BuildBarChart(co, xLabels, series, "Lines")

	// Add specific titles to the generated chart.
	bar.SetGlobalOptions(
		charts.WithTitleOpts(co.Title("Code Churn", "Lines added vs removed over time")),
	)

	return bar
}
