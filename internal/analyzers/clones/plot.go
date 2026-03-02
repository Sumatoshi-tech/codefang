package clones

import (
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// Plot display constants.
const (
	plotChartHeight = "400px"
	plotPieRadius   = "60%"
)

// RegisterPlotSections registers the clone detection plot section renderer.
func RegisterPlotSections() {
	analyze.RegisterPlotSections(analyzerID, func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generatePlotSections(report)
	})
}

// generatePlotSections creates plot sections for the clone detection report.
func (a *Analyzer) generatePlotSections(report analyze.Report) ([]plotpage.Section, error) {
	pieChart := a.generateCloneTypePieChart(report)

	sections := []plotpage.Section{
		{
			Title:    "Clone Type Distribution",
			Subtitle: "Distribution of detected clones by type.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Type-1 (Exact)</strong> = identical AST structure and tokens",
					"<strong>Type-2 (Renamed)</strong> = identical structure, different variable names",
					"<strong>Type-3 (Near-miss)</strong> = similar but modified structure",
				},
			},
		},
	}

	return sections, nil
}

// generateCloneTypePieChart creates a pie chart of clone types.
func (a *Analyzer) generateCloneTypePieChart(report analyze.Report) *charts.Pie {
	pairs := extractClonePairs(report)
	counts := categorizeClonePairs(pairs)

	pie := charts.NewPie()
	pie.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{Title: "Clone Types"}),
		charts.WithInitializationOpts(opts.Initialization{Height: plotChartHeight}),
	)

	pieData := []opts.PieData{
		{Name: distLabelType1, Value: counts.type1},
		{Name: distLabelType2, Value: counts.type2},
		{Name: distLabelType3, Value: counts.type3},
	}

	pie.AddSeries("Clone Types", pieData).
		SetSeriesOptions(
			charts.WithPieChartOpts(opts.PieChart{Radius: plotPieRadius}),
			charts.WithLabelOpts(opts.Label{Show: opts.Bool(true), Formatter: "{b}: {c}"}),
		)

	return pie
}

// renderPlotSections renders plot sections to an HTML page.
func renderPlotSections(sections []plotpage.Section, w io.Writer) error {
	page := plotpage.NewPage(
		"Clone Detection Analysis",
		"Code clone detection using MinHash and LSH",
	)

	page.Add(sections...)

	return page.Render(w)
}
