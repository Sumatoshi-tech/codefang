package quality

import (
	"fmt"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	lineWidth        = 2
	lineWidthThin    = 1
	emptyChartHeight = "400px"
	maxStatsColumns  = 4
)

// chartSeries defines a single line series to plot.
type chartSeries struct {
	Name      string
	ValueFunc func(TickStats) float64
	Color     string
	Width     float32
	Dashed    bool
}

func buildComplexityChart(computed *ComputedMetrics) *charts.Line {
	return buildDistributionChart(computed,
		"Complexity Over Time", "Complexity",
		func(s TickStats) float64 { return s.ComplexityMedian },
		func(s TickStats) float64 { return s.ComplexityMean },
		func(s TickStats) float64 { return s.ComplexityP95 },
		func(p plotpage.ChartPalette) string { return p.Semantic.Good },
	)
}

func buildHalsteadChart(computed *ComputedMetrics) *charts.Line {
	return buildDistributionChart(computed,
		"Halstead Volume Over Time", "Halstead Volume",
		func(s TickStats) float64 { return s.HalsteadVolMedian },
		func(s TickStats) float64 { return s.HalsteadVolMean },
		func(s TickStats) float64 { return s.HalsteadVolP95 },
		func(p plotpage.ChartPalette) string { return p.Semantic.Warning },
	)
}

func buildDistributionChart(
	computed *ComputedMetrics,
	title, yAxisLabel string,
	medianFunc, meanFunc, p95Func func(TickStats) float64,
	medianColorFunc func(plotpage.ChartPalette) string,
) *charts.Line {
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return buildMultiSeriesChart(computed, title, yAxisLabel, []chartSeries{
		{
			Name: "Median", ValueFunc: medianFunc,
			Color: medianColorFunc(palette), Width: lineWidth,
		},
		{
			Name: "Mean", ValueFunc: meanFunc,
			Color: palette.Primary[0], Width: lineWidthThin, Dashed: true,
		},
		{
			Name: "P95", ValueFunc: p95Func,
			Color: palette.Semantic.Bad, Width: lineWidthThin, Dashed: true,
		},
	})
}

func buildMultiSeriesChart(
	computed *ComputedMetrics,
	title, yAxisLabel string,
	series []chartSeries,
) *charts.Line {
	if len(computed.TimeSeries) == 0 {
		return createEmptyChart(title)
	}

	labels := make([]string, len(computed.TimeSeries))

	for i, entry := range computed.TimeSeries {
		labels[i] = strconv.Itoa(entry.Tick)
	}

	co := plotpage.DefaultChartOpts()

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis(yAxisLabel)),
		charts.WithGridOpts(co.Grid()),
	)
	line.SetXAxis(labels)

	for _, s := range series {
		data := make([]opts.LineData, len(computed.TimeSeries))

		for i, entry := range computed.TimeSeries {
			data[i] = opts.LineData{Value: s.ValueFunc(entry.Stats)}
		}

		lineStyle := opts.LineStyle{Width: s.Width}

		if s.Dashed {
			lineStyle.Type = "dashed"
		}

		line.AddSeries(s.Name, data,
			charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(true)}),
			charts.WithItemStyleOpts(opts.ItemStyle{Color: s.Color}),
			charts.WithLineStyleOpts(lineStyle),
		)
	}

	return line
}

func buildQualityStatsSection(computed *ComputedMetrics) plotpage.Section {
	medianComplexity := fmt.Sprintf("%.2f", computed.Aggregate.ComplexityMedianMean)
	p95Complexity := fmt.Sprintf("%.2f", computed.Aggregate.ComplexityP95Mean)
	medianHalstead := fmt.Sprintf("%.1f", computed.Aggregate.HalsteadVolMedianMean)
	totalBugs := fmt.Sprintf("%.1f", computed.Aggregate.TotalDeliveredBugs)
	minComment := fmt.Sprintf("%.2f", computed.Aggregate.MinCommentScore)
	minCohesion := fmt.Sprintf("%.2f", computed.Aggregate.MinCohesion)
	totalFiles := strconv.Itoa(computed.Aggregate.TotalFilesAnalyzed)

	grid := plotpage.NewGrid(
		maxStatsColumns,
		plotpage.NewStat("Median Complexity", medianComplexity),
		plotpage.NewStat("P95 Complexity", p95Complexity),
		plotpage.NewStat("Median Halstead Vol", medianHalstead),
		plotpage.NewStat("Total Delivered Bugs", totalBugs),
		plotpage.NewStat("Min Comment Score", minComment),
		plotpage.NewStat("Min Cohesion", minCohesion),
		plotpage.NewStat("Total Files Analyzed", totalFiles),
	)

	return plotpage.Section{
		Title:    "Code Quality Summary",
		Subtitle: "Aggregate statistics from code quality analysis across commit history.",
		Chart:    grid,
	}
}

func createEmptyChart(title string) *charts.Line {
	co := plotpage.DefaultChartOpts()
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title(title, "No data")),
	)

	return line
}

// RegisterPlotSections registers the quality plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterStorePlotSections("quality", GenerateStoreSections)
}
