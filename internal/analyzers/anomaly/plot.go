package anomaly

import (
	"fmt"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	lineWidth = 2
)

func buildChartData(ticks []int, input *ReportData) (labels []string, churnData, anomalyData []opts.LineData) {
	anomalySet := make(map[int]bool, len(input.Anomalies))
	for _, a := range input.Anomalies {
		anomalySet[a.Tick] = true
	}

	labels = make([]string, len(ticks))
	churnData = make([]opts.LineData, len(ticks))
	anomalyData = make([]opts.LineData, len(ticks))

	for i, tick := range ticks {
		labels[i] = strconv.Itoa(tick)
		tm := input.TickMetrics[tick]
		churnData[i] = opts.LineData{Value: tm.NetChurn}

		if anomalySet[tick] {
			anomalyData[i] = opts.LineData{
				Value:  tm.NetChurn,
				Symbol: "circle",
			}
		} else {
			anomalyData[i] = opts.LineData{Value: "-"}
		}
	}

	return labels, churnData, anomalyData
}

func createChurnChart(
	labels []string,
	churnData, anomalyData []opts.LineData,
	co *plotpage.ChartOpts,
	palette plotpage.ChartPalette,
) *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis("Net Churn (lines)")),
		charts.WithGridOpts(co.Grid()),
	)
	line.SetXAxis(labels)
	line.AddSeries("Net Churn", churnData,
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(true)}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}),
		charts.WithLineStyleOpts(opts.LineStyle{Width: lineWidth}),
	)
	line.AddSeries("Anomalies", anomalyData,
		charts.WithLineChartOpts(opts.LineChart{
			Step: "",
		}),
		charts.WithItemStyleOpts(opts.ItemStyle{
			Color: palette.Semantic.Bad,
		}),
		charts.WithLineStyleOpts(opts.LineStyle{Width: 0, Opacity: opts.Float(0)}),
	)

	return line
}

// Threshold constants for anomaly rate badge coloring.
const (
	anomalyRateWarningThreshold = 10.0
	anomalyRateErrorThreshold   = 25.0
	maxStatsColumns             = 4
)

func buildExternalAnomalySection(input *ReportData) (plotpage.Section, bool) {
	if len(input.ExternalSummaries) == 0 {
		return plotpage.Section{}, false
	}

	var stats []plotpage.Renderable

	for _, summary := range input.ExternalSummaries {
		label := summary.Source + " / " + summary.Dimension
		value := strconv.Itoa(summary.Anomalies)

		stat := plotpage.NewStat(label, value)

		if summary.Anomalies > 0 {
			zStr := fmt.Sprintf("peak Z=%.1f", summary.HighestZ)
			stat = stat.WithTrend(zStr, plotpage.BadgeWarning)
		}

		stats = append(stats, stat)
	}

	grid := plotpage.NewGrid(maxStatsColumns, stats...)

	return plotpage.Section{
		Title:    "Cross-Analyzer Anomaly Detection",
		Subtitle: "Anomalies detected on time series from other history analyzers.",
		Chart:    grid,
	}, true
}

// RegisterPlotSections registers the anomaly plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterStorePlotSections("anomaly", GenerateStoreSections)
}
