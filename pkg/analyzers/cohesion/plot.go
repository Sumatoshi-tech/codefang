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

// FormatReportPlot generates an HTML plot visualization for cohesion analysis.
func (c *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	barChart, err := c.generateBarChart(report)
	if err != nil {
		return err
	}

	pieChart := c.generatePieChart(report)

	page := plotpage.NewPage(
		"Code Cohesion Analysis",
		"Function cohesion metrics and distribution",
	)

	page.Add(
		plotpage.Section{
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
		plotpage.Section{
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
	)

	return page.Render(w)
}

func (c *Analyzer) generateBarChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := report["functions"].([]map[string]any)
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
	style := plotpage.DefaultStyle()

	return createCohesionBarChart(labels, scores, colors, style), nil
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

func createCohesionBarChart(labels []string, scores []float64, colors []string, style plotpage.Style) *charts.Bar {
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "axis"}),
		charts.WithInitializationOpts(opts.Initialization{Width: style.Width, Height: style.Height}),
		charts.WithGridOpts(opts.Grid{
			Left: style.GridLeft, Right: style.GridRight,
			Top: style.GridTop, Bottom: style.GridBottom,
			ContainLabel: opts.Bool(true),
		}),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{Rotate: xAxisRotate, Interval: "0"},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name: "Cohesion Score",
			Min:  0,
			Max:  1,
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
	functions, ok := report["functions"].([]map[string]any)
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
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithInitializationOpts(opts.Initialization{Width: "600px", Height: "400px"}),
		charts.WithLegendOpts(opts.Legend{Show: opts.Bool(true), Top: "bottom"}),
	)

	pieData := []opts.PieData{
		{Name: "Excellent", Value: distribution["Excellent"], ItemStyle: &opts.ItemStyle{Color: "#91cc75"}},
		{Name: "Good", Value: distribution["Good"], ItemStyle: &opts.ItemStyle{Color: "#fac858"}},
		{Name: "Fair", Value: distribution["Fair"], ItemStyle: &opts.ItemStyle{Color: "#fd8c73"}},
		{Name: "Poor", Value: distribution["Poor"], ItemStyle: &opts.ItemStyle{Color: "#ee6666"}},
	}

	pie.AddSeries("Cohesion", pieData).
		SetSeriesOptions(
			charts.WithLabelOpts(opts.Label{
				Show:      opts.Bool(true),
				Formatter: "{b}: {c} ({d}%)",
			}),
			charts.WithPieChartOpts(opts.PieChart{
				Radius: pieRadius,
			}),
		)

	return pie
}

func createEmptyCohesionChart() *charts.Bar {
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Function Cohesion", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return bar
}

func createEmptyPieChart() *charts.Pie {
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Cohesion Distribution", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "600px", Height: emptyChartHeight}),
	)

	return pie
}
