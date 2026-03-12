package devs

import (
	"io"
	"slices"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
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

	nameByID := make(map[int]string)
	for _, dev := range data.Metrics.Developers {
		nameByID[dev.ID] = dev.Name
	}

	series := make([]plotpage.LineSeries, 0, len(topDevs)+1)

	series = append(series, buildTopDevSeries(data, topDevs, nameByID)...)

	if len(data.Metrics.Developers) > maxDevs {
		series = append(series, buildOthersSeries(data, topDevs))
	}

	co := plotpage.DefaultChartOpts()
	line := plotpage.BuildLineChart(co, xLabels, series, "Commits")

	// Set specific title overrides.
	line.SetGlobalOptions(
		charts.WithTitleOpts(co.Title("Developer Activity Over Time", "Stacked area showing contribution velocity (commits per tick)")),
	)

	return line
}

func buildTopDevSeries(data *DashboardData, topDevs []int, nameByID map[int]string) []plotpage.LineSeries {
	series := make([]plotpage.LineSeries, 0, len(topDevs))

	for _, devID := range topDevs {
		seriesData := make([]plotpage.SeriesData, len(data.Metrics.Activity))
		for i, ad := range data.Metrics.Activity {
			seriesData[i] = ad.ByDeveloper[devID]
		}

		series = append(series, plotpage.LineSeries{
			Name:        nameByID[devID],
			Data:        seriesData,
			Stack:       "total",
			AreaOpacity: float32(areaOpacityNormal),
		})
	}

	return series
}

func buildOthersSeries(data *DashboardData, topDevs []int) plotpage.LineSeries {
	othersData := make([]plotpage.SeriesData, len(data.Metrics.Activity))

	for i, ad := range data.Metrics.Activity {
		total := 0

		for devID, commits := range ad.ByDeveloper {
			if !slices.Contains(topDevs, devID) {
				total += commits
			}
		}

		othersData[i] = total
	}

	return plotpage.LineSeries{
		Name:        "Others",
		Data:        othersData,
		Stack:       "total",
		AreaOpacity: float32(areaOpacityOther),
	}
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
