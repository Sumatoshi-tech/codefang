package sentiment

import (
	"errors"
	"io"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	dataZoomEnd      = 100
	labelFontSize    = 10
	areaOpacity      = 0.3
	emptyChartHeight = "400px"
)

// ErrInvalidEmotions indicates the report doesn't contain expected emotions data.
var ErrInvalidEmotions = errors.New("invalid sentiment report: expected map[int]float32 for emotions_by_tick")

func (s *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := s.GenerateChart(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Commit Sentiment Analysis",
		"Emotional tone of commit messages over project lifetime",
	)
	page.Add(plotpage.Section{
		Title:    "Sentiment History Over Time",
		Subtitle: "Average sentiment score extracted from commit messages per time interval.",
		Chart:    chart,
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"Positive values = generally positive/constructive commit messages",
				"Negative values = frustration, urgency, or negative sentiment",
				"Sudden drops = may indicate stressful periods or difficult bugs",
				"Stable positive trend = healthy team communication",
				"Look for: Correlation with release dates or team changes",
				"Action: Investigate periods of sustained negative sentiment",
			},
		},
	})

	return page.Render(writer)
}

// GenerateChart creates a line chart showing sentiment over time.
func (s *HistoryAnalyzer) GenerateChart(report analyze.Report) (*charts.Line, error) {
	emotions, ok := report["emotions_by_tick"].(map[int]float32)
	if !ok {
		return nil, ErrInvalidEmotions
	}

	if len(emotions) == 0 {
		return createEmptySentimentChart(), nil
	}

	ticks := sortedTicks(emotions)
	labels, data := buildSentimentData(ticks, emotions)

	style := plotpage.DefaultStyle()
	line := createSentimentChart(labels, data, style)

	return line, nil
}

func sortedTicks(emotions map[int]float32) []int {
	ticks := make([]int, 0, len(emotions))

	for t := range emotions {
		ticks = append(ticks, t)
	}

	sort.Ints(ticks)

	return ticks
}

func buildSentimentData(ticks []int, emotions map[int]float32) ([]string, []opts.LineData) {
	labels := make([]string, len(ticks))
	data := make([]opts.LineData, len(ticks))

	for i, tick := range ticks {
		labels[i] = strconv.Itoa(tick)
		data[i] = opts.LineData{Value: emotions[tick]}
	}

	return labels, data
}

func createSentimentChart(labels []string, data []opts.LineData, style plotpage.Style) *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithInitializationOpts(opts.Initialization{Width: style.Width, Height: style.Height}),
		charts.WithDataZoomOpts(
			opts.DataZoom{Type: "slider", Start: 0, End: dataZoomEnd},
			opts.DataZoom{Type: "inside"},
		),
		charts.WithXAxisOpts(opts.XAxis{
			Name:      "Time (tick)",
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize},
		}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Sentiment Score"}),
		charts.WithGridOpts(opts.Grid{
			Left: style.GridLeft, Right: style.GridRight,
			Top: style.GridTop, Bottom: style.GridBottom,
			ContainLabel: opts.Bool(true),
		}),
	)
	line.SetXAxis(labels)
	line.AddSeries("Sentiment", data,
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(true)}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: "#91cc75"}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacity)}),
	)

	return line
}

func createEmptySentimentChart() *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Sentiment History", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return line
}
