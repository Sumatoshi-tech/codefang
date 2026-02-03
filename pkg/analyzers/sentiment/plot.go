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
	chart, err := s.generateChart(report)
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
	return s.generateChart(report)
}

// generateChart creates a line chart showing sentiment over time.
func (s *HistoryAnalyzer) generateChart(report analyze.Report) (*charts.Line, error) {
	emotions, ok := report["emotions_by_tick"].(map[int]float32)
	if !ok {
		return nil, ErrInvalidEmotions
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

func createEmptySentimentChart() *charts.Line {
	co := plotpage.DefaultChartOpts()
	line := charts.NewLine()
	line.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Sentiment History", "No data")),
	)

	return line
}
