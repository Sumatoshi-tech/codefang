package devs

import (
	"io"
	"slices"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

type activityContent struct {
	chart *charts.Line
}

func createActivityTab(data *DashboardData) *activityContent {
	return &activityContent{chart: createActivityChart(data)}
}

// Render renders the activity content to the writer.
func (a *activityContent) Render(w io.Writer) error {
	if a.chart == nil {
		return plotpage.NewText("No activity data available").Render(w)
	}

	return plotpage.WrapChart(a.chart).Render(w)
}

func createActivityChart(data *DashboardData) *charts.Line {
	if len(data.Metrics.Activity) == 0 {
		return nil
	}

	topDevs := getTopDevIDs(data.Metrics.Developers, maxDevs)
	xLabels := make([]string, len(data.Metrics.Activity))

	for i, ad := range data.Metrics.Activity {
		xLabels[i] = strconv.Itoa(ad.Tick)
	}

	line := charts.NewLine()
	configureActivityChart(line)
	line.SetXAxis(xLabels)

	addDevSeriesTo(line, topDevs, data)
	addOthersSeriesTo(line, topDevs, data)

	return line
}

func getTopDevIDs(developers []DeveloperData, limit int) []int {
	ids := make([]int, 0, limit)

	for i, dev := range developers {
		if i >= limit {
			break
		}

		ids = append(ids, dev.ID)
	}

	return ids
}

func configureActivityChart(line *charts.Line) {
	co := plotpage.DefaultChartOpts()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", lineChartHeight)),
		charts.WithTitleOpts(co.Title("Developer Activity Over Time", "Stacked area showing contribution velocity (commits per tick)")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithGridOpts(co.Grid()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis("Commits")),
	)
}

func addDevSeriesTo(line *charts.Line, devIDs []int, data *DashboardData) {
	nameByID := make(map[int]string)

	for _, dev := range data.Metrics.Developers {
		nameByID[dev.ID] = dev.Name
	}

	for _, devID := range devIDs {
		seriesData := make([]opts.LineData, len(data.Metrics.Activity))
		for i, ad := range data.Metrics.Activity {
			seriesData[i] = opts.LineData{Value: ad.ByDeveloper[devID]}
		}

		line.AddSeries(nameByID[devID], seriesData,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacityNormal)}),
		)
	}
}

func addOthersSeriesTo(line *charts.Line, topDevs []int, data *DashboardData) {
	if len(data.Metrics.Developers) <= maxDevs {
		return
	}

	othersData := make([]opts.LineData, len(data.Metrics.Activity))

	for i, ad := range data.Metrics.Activity {
		total := 0

		for devID, commits := range ad.ByDeveloper {
			if !slices.Contains(topDevs, devID) {
				total += commits
			}
		}

		othersData[i] = opts.LineData{Value: total}
	}

	line.AddSeries("Others", othersData,
		charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacityOther)}),
	)
}
