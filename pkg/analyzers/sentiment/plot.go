package sentiment

import (
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	areaOpacity = 0.3
)

// RegisterPlotSections registers the sentiment plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/sentiment", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}

// GenerateSections returns the sections for combined reports.
func (s *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
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
func (s *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return s.buildChart(report)
}

// buildChart creates a line chart showing sentiment over time.
func (s *Analyzer) buildChart(report analyze.Report) (*charts.Line, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, err
	}

	if len(metrics.TimeSeries) == 0 {
		return plotpage.BuildLineChart(
			nil,
			nil,
			nil,
			"Sentiment Score",
		), nil
	}

	labels := make([]string, len(metrics.TimeSeries))
	lineData := make([]plotpage.SeriesData, 0, len(metrics.TimeSeries))

	for i, ts := range metrics.TimeSeries {
		labels[i] = strconv.Itoa(ts.Tick)
		lineData = append(lineData, float64(ts.Sentiment))
	}

	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	series := []plotpage.LineSeries{
		{
			Name:        "Sentiment",
			Data:        lineData,
			Color:       palette.Semantic.Good,
			AreaOpacity: areaOpacity,
		},
	}

	return plotpage.BuildLineChart(
		nil,
		labels,
		series,
		"Sentiment Score",
	), nil
}
