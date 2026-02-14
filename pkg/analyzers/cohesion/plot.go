package cohesion

import (
	"errors"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	topFunctionsLimit = 20
	xAxisRotate       = 45
	emptyChartHeight  = "400px"
	pieRadius         = "60%"
)

// ErrInvalidFunctions indicates the report doesn't contain expected functions data.
var ErrInvalidFunctions = errors.New("invalid cohesion report: expected []map[string]any for functions")

// RegisterPlotSections registers the cohesion plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("static/cohesion", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generateSections(report)
	})
}

// FormatReportPlot generates an HTML plot visualization for cohesion analysis.
func (c *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	sections, err := c.generateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Code Cohesion Analysis",
		"Function cohesion metrics and distribution",
	)

	page.Add(sections...)

	return page.Render(w)
}

func (c *Analyzer) generateSections(report analyze.Report) ([]plotpage.Section, error) {
	barChart, err := c.generateBarChart(report)
	if err != nil {
		return nil, err
	}

	pieChart := c.generatePieChart(report)

	return []plotpage.Section{
		{
			Title:    "Function Cohesion Scores",
			Subtitle: "Cohesion score per function (higher is better, 0.0-1.0 scale).",
			Chart:    barChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Green (≥0.8)</strong> = Excellent cohesion - function is well-focused",
					"<strong>Yellow (0.6-0.8)</strong> = Good cohesion with room for improvement",
					"<strong>Orange (0.3-0.6)</strong> = Fair cohesion - consider refactoring",
					"<strong>Red (<0.3)</strong> = Poor cohesion - function lacks focus",
					"<strong>Action:</strong> Functions with low cohesion should be split into smaller, focused units",
				},
			},
		},
		{
			Title:    "Cohesion Distribution",
			Subtitle: "Distribution of functions by cohesion category.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Excellent</strong> = Functions with cohesion ≥ 0.8",
					"<strong>Good</strong> = Functions with cohesion 0.6-0.8",
					"<strong>Fair</strong> = Functions with cohesion 0.3-0.6",
					"<strong>Poor</strong> = Functions with cohesion < 0.3",
					"<strong>Goal:</strong> Maximize the Excellent and Good segments",
				},
			},
		},
	}, nil
}

func (c *Analyzer) generateBarChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_cohesion")
	}

	if !ok {
		return nil, ErrInvalidFunctions
	}

	if len(functions) == 0 {
		return createEmptyCohesionChart(), nil
	}

	sorted := sortByCohesion(functions)
	if len(sorted) > topFunctionsLimit {
		sorted = sorted[:topFunctionsLimit]
	}

	labels, scores, colors := extractCohesionData(sorted)
	co := plotpage.DefaultChartOpts()

	return createCohesionBarChart(labels, scores, colors, co), nil
}

func sortByCohesion(functions []map[string]any) []map[string]any {
	sorted := make([]map[string]any, len(functions))
	copy(sorted, functions)

	sort.Slice(sorted, func(i, j int) bool {
		ci := getCohesionValue(sorted[i])
		cj := getCohesionValue(sorted[j])

		return ci > cj
	})

	return sorted
}

func getCohesionValue(fn map[string]any) float64 {
	if val, ok := fn["cohesion"].(float64); ok {
		return val
	}

	return 0
}

func extractCohesionData(functions []map[string]any) (labels []string, scores []float64, colors []string) {
	labels = make([]string, len(functions))
	scores = make([]float64, len(functions))
	colors = make([]string, len(functions))

	for i, fn := range functions {
		if name, ok := fn["name"].(string); ok {
			labels[i] = name
		} else {
			labels[i] = "unknown"
		}

		scores[i] = getCohesionValue(fn)
		colors[i] = getCohesionColor(scores[i])
	}

	return labels, scores, colors
}

func getCohesionColor(cohesion float64) string {
	switch {
	case cohesion >= cohesionThresholdHigh:
		return "#91cc75"
	case cohesion >= cohesionThresholdMedium:
		return "#fac858"
	case cohesion >= cohesionThresholdLow:
		return "#fd8c73"
	default:
		return "#ee6666"
	}
}

func createCohesionBarChart(labels []string, scores []float64, colors []string, co *plotpage.ChartOpts) *charts.Bar {
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithGridOpts(co.Grid()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      "Cohesion Score",
			Min:       0,
			Max:       1,
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			SplitLine: &opts.SplitLine{LineStyle: &opts.LineStyle{Color: co.GridColor()}},
		}),
	)

	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(scores))

	for i, score := range scores {
		barData[i] = opts.BarData{
			Value: score,
			ItemStyle: &opts.ItemStyle{
				Color: colors[i],
			},
		}
	}

	bar.AddSeries("Cohesion", barData)

	return bar
}

func (c *Analyzer) generatePieChart(report analyze.Report) *charts.Pie {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_cohesion")
	}

	if !ok || len(functions) == 0 {
		return createEmptyPieChart()
	}

	distribution := countCohesionDistribution(functions)

	return createCohesionPieChart(distribution)
}

func countCohesionDistribution(functions []map[string]any) map[string]int {
	distribution := map[string]int{
		"Excellent": 0,
		"Good":      0,
		"Fair":      0,
		"Poor":      0,
	}

	for _, fn := range functions {
		cohesion := getCohesionValue(fn)

		switch {
		case cohesion >= cohesionThresholdHigh:
			distribution["Excellent"]++
		case cohesion >= cohesionThresholdMedium:
			distribution["Good"]++
		case cohesion >= cohesionThresholdLow:
			distribution["Fair"]++
		default:
			distribution["Poor"]++
		}
	}

	return distribution
}

func createCohesionPieChart(distribution map[string]int) *charts.Pie {
	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init("600px", "400px")),
		charts.WithLegendOpts(opts.Legend{
			Show:      opts.Bool(true),
			Top:       "bottom",
			TextStyle: &opts.TextStyle{Color: co.TextMutedColor()},
		}),
	)

	pieData := []opts.PieData{
		{Name: "Excellent", Value: distribution["Excellent"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Good}},
		{Name: "Good", Value: distribution["Good"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Warning}},
		{Name: "Fair", Value: distribution["Fair"], ItemStyle: &opts.ItemStyle{Color: "#fd8c73"}},
		{Name: "Poor", Value: distribution["Poor"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Bad}},
	}

	pie.AddSeries("Cohesion", pieData).
		SetSeriesOptions(
			charts.WithLabelOpts(opts.Label{
				Show:      opts.Bool(true),
				Formatter: "{b}: {c} ({d}%)",
				Color:     co.TextMutedColor(),
			}),
			charts.WithPieChartOpts(opts.PieChart{
				Radius: pieRadius,
			}),
		)

	return pie
}

func createEmptyCohesionChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Function Cohesion", "No data")),
	)

	return bar
}

func createEmptyPieChart() *charts.Pie {
	co := plotpage.DefaultChartOpts()
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("600px", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Cohesion Distribution", "No data")),
	)

	return pie
}
