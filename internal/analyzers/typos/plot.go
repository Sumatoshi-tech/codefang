package typos

import (
	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	topFilesLimit = 20
)

// RegisterPlotSections registers the typos plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/typos", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}

// GenerateSections returns the sections for combined reports.
func (t *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, chartErr := t.buildChart(report)
	if chartErr != nil {
		return nil, chartErr
	}

	return []plotpage.Section{
		{
			Title:    "Typo-Prone Files",
			Subtitle: "Files ranked by number of typo fixes detected in commit history.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = files where typos are frequently fixed",
					"Documentation files = expected to have more text-related fixes",
					"Code files = typos may indicate hasty commits",
					"Look for: Code files with unusually high typo rates",
					"Action: Consider adding spell-checking to pre-commit hooks",
				},
			},
		},
	}, nil
}

// GenerateChart implements PlotGenerator interface.
func (t *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return t.buildChart(report)
}

// buildChart creates a bar chart showing typo-prone files.
func (t *Analyzer) buildChart(report analyze.Report) (*charts.Bar, error) {
	metrics, err := ComputeAllMetrics(report)
	if err != nil {
		return nil, err
	}

	if len(metrics.FileTypos) == 0 {
		return plotpage.BuildBarChart(
			nil,
			nil,
			nil,
			"Typo-Prone Files",
		), nil
	}

	limit := min(topFilesLimit, len(metrics.FileTypos))

	labels := make([]string, limit)
	barData := make([]plotpage.SeriesData, 0, limit)

	for i := range limit {
		labels[i] = metrics.FileTypos[i].File
		barData = append(barData, metrics.FileTypos[i].TypoCount)
	}

	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	series := []plotpage.BarSeries{
		{
			Name:  "Typos",
			Data:  barData,
			Color: palette.Semantic.Warning,
		},
	}

	return plotpage.BuildBarChart(
		nil,
		labels,
		series,
		"Typo Count",
	), nil
}
