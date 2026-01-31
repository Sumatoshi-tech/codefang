package sentiment

import (
	"errors"
	"fmt"
	"io"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// generatePlot creates an interactive HTML line chart from the sentiment analysis report.
func (s *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := s.GenerateChart(report)
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

// GenerateChart creates the chart object from the report.
func (s *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	const fullZoomPct = 100

	emotions, ok := report["emotions_by_tick"].(map[int]float32)
	if !ok {
		return nil, errors.New("expected map[int]float32 for emotions") //nolint:err113 // descriptive error
	}

	if len(emotions) == 0 {
		return createSentimentEmptyChart(), nil
	}

	ticks := make([]int, 0, len(emotions))
	for tick := range emotions {
		ticks = append(ticks, tick)
	}

	sort.Ints(ticks)

	xLabels := make([]string, len(ticks))
	lineData := make([]opts.LineData, len(ticks))

	for i, tick := range ticks {
		xLabels[i] = strconv.Itoa(tick)
		lineData[i] = opts.LineData{Value: emotions[tick]}
	}

	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Sentiment History",
			Subtitle: "Average sentiment score per tick",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{Name: "Time (tick)"}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Sentiment Score"}),
	)
	line.SetXAxis(xLabels)
	line.AddSeries("Sentiment", lineData)

	return line, nil
}

func createSentimentEmptyChart() *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Sentiment History",
			Subtitle: "No data",
		}),
	)

	return line
}
