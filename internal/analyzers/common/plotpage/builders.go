package plotpage

import (
	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"
)

// SeriesData represents a single numeric value in a chart series.
// We use any to allow both int and float64 (to map to opts.BarData/opts.LineData).
type SeriesData any

// BarSeries defines the properties and data for a single bar chart series.
type BarSeries struct {
	Name  string
	Data  []SeriesData
	Color string // Optional, uses theme if empty.
	Stack string // Optional, stack grouping.
}

// LineSeries defines the properties and data for a single line chart series.
type LineSeries struct {
	Name        string
	Data        []SeriesData
	Color       string  // Optional, uses theme if empty.
	Stack       string  // Optional, stack grouping.
	AreaOpacity float32 // Optional, area opacity for area charts.
}

// BuildBarChart constructs a fully configured go-echarts Bar chart using ChartOpts.
// If cOpts is nil, DefaultChartOpts() is used.
func BuildBarChart(cOpts *ChartOpts, labels []string, series []BarSeries, yAxisLabel string) *charts.Bar {
	if cOpts == nil {
		cOpts = DefaultChartOpts()
	}

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(cOpts.Init("100%", "500px")),
		charts.WithTooltipOpts(cOpts.Tooltip("axis")),
		charts.WithDataZoomOpts(cOpts.DataZoom()...),
		charts.WithXAxisOpts(cOpts.XAxis("")),
		charts.WithYAxisOpts(cOpts.YAxis(yAxisLabel)),
		charts.WithLegendOpts(cOpts.Legend()),
	)

	bar.SetXAxis(labels)

	for _, s := range series {
		barData := make([]opts.BarData, len(s.Data))
		for i, v := range s.Data {
			barData[i] = opts.BarData{Value: v}
		}

		var seriesOpts []charts.SeriesOpts
		if s.Color != "" {
			seriesOpts = append(seriesOpts, charts.WithItemStyleOpts(opts.ItemStyle{Color: s.Color}))
		}

		bar.AddSeries(s.Name, barData, seriesOpts...)
	}

	return bar
}

// BuildLineChart constructs a fully configured go-echarts Line chart using ChartOpts.
// If cOpts is nil, DefaultChartOpts() is used.
func BuildLineChart(cOpts *ChartOpts, labels []string, series []LineSeries, yAxisLabel string) *charts.Line {
	if cOpts == nil {
		cOpts = DefaultChartOpts()
	}

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(cOpts.Init("100%", "500px")),
		charts.WithTooltipOpts(cOpts.Tooltip("axis")),
		charts.WithDataZoomOpts(cOpts.DataZoom()...),
		charts.WithXAxisOpts(cOpts.XAxis("")),
		charts.WithYAxisOpts(cOpts.YAxis(yAxisLabel)),
		charts.WithLegendOpts(cOpts.Legend()),
	)

	line.SetXAxis(labels)

	for _, s := range series {
		lineData := make([]opts.LineData, len(s.Data))
		for i, v := range s.Data {
			lineData[i] = opts.LineData{Value: v}
		}

		var seriesOpts []charts.SeriesOpts
		if s.Color != "" {
			seriesOpts = append(seriesOpts,
				charts.WithItemStyleOpts(opts.ItemStyle{Color: s.Color}),
				charts.WithLineStyleOpts(opts.LineStyle{Color: s.Color}),
			)
		}

		if s.Stack != "" {
			seriesOpts = append(seriesOpts, charts.WithLineChartOpts(opts.LineChart{Stack: s.Stack}))
		}

		if s.AreaOpacity > 0 {
			seriesOpts = append(seriesOpts, charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(s.AreaOpacity)}))
		}

		line.AddSeries(s.Name, lineData, seriesOpts...)
	}

	return line
}
