package burndown

import (
	"errors"
	"fmt"
	"io"
	"strconv"
	"time"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// hoursPerDay is the number of hours in a day, used for time conversions.
const hoursPerDay = 24

// generatePlot creates an interactive HTML stacked area chart from the burndown analysis report.
func (b *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := b.GenerateChart(report)
	if err != nil {
		return fmt.Errorf("generate chart: %w", err)
	}

	if r, ok := chart.(interface{ Render(io.Writer) error }); ok {
		err = r.Render(writer)
		if err != nil {
			return fmt.Errorf("render chart: %w", err)
		}

		return nil
	}

	return errors.New("chart does not support Render") //nolint:err113 // dynamic error
}

// burndownChartParams holds extracted parameters for chart generation.
type burndownChartParams struct {
	globalHistory DenseHistory
	sampling      int
	granularity   int
	tickSize      time.Duration
}

// extractChartParams extracts and validates parameters from the report.
func extractChartParams(report analyze.Report) (*burndownChartParams, error) {
	globalHistory, ok := report["GlobalHistory"].(DenseHistory)
	if !ok {
		return nil, errors.New("expected DenseHistory for GlobalHistory") //nolint:err113 // descriptive error
	}

	sampling, ok := report["Sampling"].(int)
	if !ok {
		return nil, errors.New("expected int for Sampling") //nolint:err113 // descriptive error
	}

	granularity, ok := report["Granularity"].(int)
	if !ok {
		return nil, errors.New("expected int for Granularity") //nolint:err113 // descriptive error
	}

	tickSize, ok := report["TickSize"].(time.Duration)
	if !ok {
		tickSize = hoursPerDay * time.Hour
	}

	return &burndownChartParams{
		globalHistory: globalHistory,
		sampling:      sampling,
		granularity:   granularity,
		tickSize:      tickSize,
	}, nil
}

// GenerateChart creates the chart object from the report.
func (b *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	params, err := extractChartParams(report)
	if err != nil {
		return nil, err
	}

	if len(params.globalHistory) == 0 {
		return createBurndownEmptyChart(), nil
	}

	xLabels := buildXLabels(params.globalHistory, params.sampling, params.tickSize)
	line := createBurndownLineChart(xLabels)
	addBurndownSeries(line, params)

	return line, nil
}

func buildXLabels(globalHistory DenseHistory, sampling int, tickSize time.Duration) []string {
	xLabels := make([]string, len(globalHistory))

	for i := range globalHistory {
		ticks := i * sampling
		days := (time.Duration(ticks) * tickSize).Hours() / hoursPerDay
		xLabels[i] = strconv.Itoa(int(days)) + "d"
	}

	return xLabels
}

func createBurndownLineChart(xLabels []string) *charts.Line {
	const fullZoomPct = 100

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{Title: "Code Burndown History", Subtitle: "Lines of code by age band over time"}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithLegendOpts(opts.Legend{Show: opts.Bool(true), Type: "scroll", Top: "5px"}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{Name: "Time (days)"}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Lines of Code"}),
	)
	line.SetXAxis(xLabels)

	return line
}

func addBurndownSeries(line *charts.Line, params *burndownChartParams) {
	const opacity = 0.5

	numBands := 0
	if len(params.globalHistory) > 0 {
		numBands = len(params.globalHistory[0])
	}

	for bandIdx := range numBands {
		data := make([]opts.LineData, len(params.globalHistory))

		for sampleIdx, sample := range params.globalHistory {
			if bandIdx < len(sample) {
				data[sampleIdx] = opts.LineData{Value: sample[bandIdx]}
			} else {
				data[sampleIdx] = opts.LineData{Value: 0}
			}
		}

		startTicks := bandIdx * params.granularity
		endTicks := (bandIdx + 1) * params.granularity
		startDays := (time.Duration(startTicks) * params.tickSize).Hours() / hoursPerDay
		endDays := (time.Duration(endTicks) * params.tickSize).Hours() / hoursPerDay

		line.AddSeries(
			fmt.Sprintf("%.0f-%.0f days old", startDays, endDays),
			data,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(opacity)}),
		)
	}
}

func createBurndownEmptyChart() *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Code Burndown History",
			Subtitle: "No data",
		}),
	)
	line.SetXAxis([]string{})

	return line
}
