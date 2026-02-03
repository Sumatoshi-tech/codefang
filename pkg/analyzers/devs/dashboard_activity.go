package devs

import (
	"io"
	"slices"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

var chartOpts = plotpage.DefaultChartOpts()

type activityContent struct {
	chart *charts.Line
}

func createActivityTab(data *DashboardData) *activityContent {
	return &activityContent{chart: createActivityChart(data)}
}

// Render implements the Renderable interface for the activity tab.
func (a *activityContent) Render(w io.Writer) error {
	if a.chart == nil {
		return plotpage.NewText("No activity data available").Render(w)
	}

	return plotpage.WrapChart(a.chart).Render(w)
}

func createActivityChart(data *DashboardData) *charts.Line {
	tickKeys := sortedKeys(data.Ticks)
	if len(tickKeys) == 0 {
		return nil
	}

	topDevs := getTopDevIDs(data.DevSummaries, maxDevs)
	xLabels := ticksToLabels(tickKeys)

	line := charts.NewLine()
	configureActivityChart(line)
	line.SetXAxis(xLabels)

	addDevSeriesTo(line, topDevs, tickKeys, data)
	addOthersSeriesTo(line, topDevs, tickKeys, data)

	return line
}

func getTopDevIDs(summaries []DeveloperSummary, limit int) []int {
	ids := make([]int, 0, limit)

	for i, ds := range summaries {
		if i >= limit {
			break
		}

		ids = append(ids, ds.ID)
	}

	return ids
}

func ticksToLabels(tickKeys []int) []string {
	labels := make([]string, len(tickKeys))

	for i, tick := range tickKeys {
		labels[i] = strconv.Itoa(tick)
	}

	return labels
}

func configureActivityChart(line *charts.Line) {
	line.SetGlobalOptions(
		charts.WithInitializationOpts(chartOpts.Init("100%", lineChartHeight)),
		charts.WithTitleOpts(chartOpts.Title("Developer Activity Over Time", "Stacked area showing contribution velocity (commits per tick)")),
		charts.WithTooltipOpts(chartOpts.Tooltip("axis")),
		charts.WithLegendOpts(chartOpts.Legend()),
		charts.WithGridOpts(chartOpts.Grid()),
		charts.WithDataZoomOpts(chartOpts.DataZoom()...),
		charts.WithXAxisOpts(chartOpts.XAxis("Time (tick)")),
		charts.WithYAxisOpts(chartOpts.YAxis("Commits")),
	)
}

func addDevSeriesTo(line *charts.Line, devIDs, tickKeys []int, data *DashboardData) {
	for _, devID := range devIDs {
		seriesData := make([]opts.LineData, len(tickKeys))

		for i, tick := range tickKeys {
			val := 0
			if devTick := data.Ticks[tick][devID]; devTick != nil {
				val = devTick.Commits
			}

			seriesData[i] = opts.LineData{Value: val}
		}

		line.AddSeries(devName(devID, data.Names), seriesData,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacityNormal)}),
		)
	}
}

func addOthersSeriesTo(line *charts.Line, topDevs, tickKeys []int, data *DashboardData) {
	if len(data.DevSummaries) <= maxDevs {
		return
	}

	othersData := make([]opts.LineData, len(tickKeys))

	for i, tick := range tickKeys {
		total := 0

		for devID, dt := range data.Ticks[tick] {
			if !slices.Contains(topDevs, devID) {
				total += dt.Commits
			}
		}

		othersData[i] = opts.LineData{Value: total}
	}

	line.AddSeries("Others", othersData,
		charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacityOther)}),
	)
}
