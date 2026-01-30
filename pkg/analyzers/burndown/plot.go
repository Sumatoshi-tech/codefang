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

// generatePlot creates an interactive HTML stacked area chart from the burndown analysis report.
func (b *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := b.GenerateChart(report)
	if err != nil {
		return fmt.Errorf("generate chart: %w", err)
	}

	if r, ok := chart.(interface{ Render(io.Writer) error }); ok {
		if err := r.Render(writer); err != nil {
			return fmt.Errorf("render chart: %w", err)
		}

		return nil
	}

	return errors.New("chart does not support Render") //nolint:err113 // dynamic error
}

// GenerateChart creates the chart object from the report.
//
//nolint:ireturn // interface needed for generic plotting
func (b *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
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
		// fallback if missing
		tickSize = 24 * time.Hour
	}

	if len(globalHistory) == 0 {
		return createBurndownEmptyChart(), nil
	}

	// Prepare X-axis (time samples).
	xLabels := make([]string, len(globalHistory))
	for i := range globalHistory {
		ticks := i * sampling
		days := (time.Duration(ticks) * tickSize).Hours() / 24
		xLabels[i] = strconv.Itoa(int(days)) + "d"
	}

	// GlobalHistory is [sample][band] -> lines.
	numBands := 0
	if len(globalHistory) > 0 {
		numBands = len(globalHistory[0])
	}

	const (
		opacity     = 0.5
		fullZoomPct = 100
	)

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Code Burndown History",
			Subtitle: "Lines of code by age band over time",
		}),
		charts.WithTooltipOpts(opts.Tooltip{
			Show:    opts.Bool(true),
			Trigger: "axis",
		}),
		charts.WithLegendOpts(opts.Legend{Show: opts.Bool(true), Type: "scroll", Top: "5px"}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Time (days)",
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name: "Lines of Code",
		}),
	)
	line.SetXAxis(xLabels)

	for bandIdx := range numBands {
		data := make([]opts.LineData, len(globalHistory))
		for sampleIdx, sample := range globalHistory {
			if bandIdx < len(sample) {
				data[sampleIdx] = opts.LineData{Value: sample[bandIdx]}
			} else {
				data[sampleIdx] = opts.LineData{Value: 0}
			}
		}

		startTicks := bandIdx * granularity
		endTicks := (bandIdx + 1) * granularity
		startDays := (time.Duration(startTicks) * tickSize).Hours() / 24
		endDays := (time.Duration(endTicks) * tickSize).Hours() / 24

		line.AddSeries(
			fmt.Sprintf("%.0f-%.0f days old", startDays, endDays),
			data,
			charts.WithLineChartOpts(opts.LineChart{Stack: "total"}),
			charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(opacity)}),
		)
	}

	return line, nil
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
