package sentiment

import (
	"errors"
	"io"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	areaOpacity      = 0.3
	emptyChartHeight = "400px"
)

// ErrInvalidEmotions indicates the report doesn't contain expected emotions data.
var ErrInvalidEmotions = errors.New("invalid sentiment report: expected map[int]float32 for emotions_by_tick")

func (s *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := s.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Commit Sentiment Analysis",
		"Emotional tone of commit messages over project lifetime",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (s *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := s.buildChart(report)
	if err != nil {
		return nil, err
	}

	return []plotpage.Section{
		{
			Title:    "Sentiment History Over Time",
			Subtitle: "Average sentiment score extracted from commit messages per time interval.",
			Chart:    plotpage.WrapChart(chart),
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
		},
	}, nil
}

// GenerateChart implements PlotGenerator interface.
func (s *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return s.buildChart(report)
}

// buildChart creates a line chart showing sentiment over time.
func (s *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.Line, error) {
	emotions, ok := report["emotions_by_tick"].(map[int]float32)
	if !ok {
		// Fallback: after binary encode -> JSON decode, "emotions_by_tick"
		// becomes "time_series" ([]any of map[string]any with tick/sentiment).
		emotions = extractEmotionsFromBinary(report)
		if emotions == nil {
			return nil, ErrInvalidEmotions
		}
	}

	if len(emotions) == 0 {
		return createEmptySentimentChart(), nil
	}

	ticks := sortedTicks(emotions)
	labels, data := buildSentimentData(ticks, emotions)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	line := createSentimentChart(labels, data, co, palette)

	return line, nil
}

// extractEmotionsFromBinary converts binary-decoded time_series data back to
// map[int]float32. Each entry has "tick" and "sentiment" fields.
func extractEmotionsFromBinary(report analyze.Report) map[int]float32 {
	rawTS, rawOK := report["time_series"]
	if !rawOK {
		return nil
	}

	if rawTS == nil {
		return map[int]float32{}
	}

	tsList, listOK := rawTS.([]any)
	if !listOK {
		return nil
	}

	if len(tsList) == 0 {
		return map[int]float32{}
	}

	emotions := make(map[int]float32, len(tsList))

	for _, item := range tsList {
		m, mOK := item.(map[string]any)
		if !mOK {
			continue
		}

		tick := sentimentToInt(m["tick"])
		sentiment := sentimentToFloat32(m["sentiment"])
		emotions[tick] = sentiment
	}

	return emotions
}

// sentimentToInt converts a numeric value to int.
func sentimentToInt(v any) int {
	switch n := v.(type) {
	case float64:
		return int(n)
	case int:
		return n
	default:
		return 0
	}
}

// sentimentToFloat32 converts a numeric value to float32.
func sentimentToFloat32(v any) float32 {
	switch n := v.(type) {
	case float64:
		return float32(n)
	case float32:
		return n
	default:
		return 0
	}
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

func createSentimentChart(labels []string, data []opts.LineData, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Line {
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(co.XAxis("Time (tick)")),
		charts.WithYAxisOpts(co.YAxis("Sentiment Score")),
		charts.WithGridOpts(co.Grid()),
	)
	line.SetXAxis(labels)
	line.AddSeries("Sentiment", data,
		charts.WithLineChartOpts(opts.LineChart{Smooth: opts.Bool(true)}),
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}),
		charts.WithAreaStyleOpts(opts.AreaStyle{Opacity: opts.Float(areaOpacity)}),
	)

	return line
}

func init() { //nolint:gochecknoinits // registration pattern
	analyze.RegisterPlotSections("history/sentiment", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&HistoryAnalyzer{}).GenerateSections(report)
	})
}

func createEmptySentimentChart() *charts.Line {
	co := plotpage.DefaultChartOpts()
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Sentiment History", "No data")),
	)

	return line
}
