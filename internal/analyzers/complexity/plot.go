package complexity

import (
	"errors"
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/stats"
)

const (
	topFunctionsLimit    = 20
	xAxisRotate          = 45
	emptyChartHeight     = "400px"
	pieRadius            = "60%"
	scatterSymbolSize    = 15
	nestingMultiplier    = 3
	cyclomaticYellowLine = 5
	cyclomaticRedLine    = 10
	cognitiveYellowLine  = 7
	cognitiveRedLine     = 15
	unknownName          = "unknown"

	// Plot display labels for complexity distribution.
	plotLabelSimple   = "Simple"
	plotLabelModerate = "Moderate"
	plotLabelComplex  = "Complex"
)

// ErrInvalidFunctionsData indicates the report doesn't contain expected functions data.
var ErrInvalidFunctionsData = errors.New("invalid complexity report: expected []map[string]any for functions")

// RegisterPlotSections registers the complexity plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("static/complexity", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generateSections(report)
	})
}

// FormatReportPlot generates an HTML plot visualization for complexity analysis.
func (c *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	sections, err := c.generateSections(report)
	if err != nil {
		return err
	}

	return plotpage.RenderAnalyzerPage(w,
		"Code Complexity Analysis",
		"Cyclomatic and cognitive complexity metrics",
		sections...,
	)
}

func (c *Analyzer) generateSections(report analyze.Report) ([]plotpage.Section, error) {
	barChart, err := c.generateComplexityBarChart(report)
	if err != nil {
		return nil, err
	}

	scatterChart, scatterErr := c.generateComplexityScatterChart(report)
	if scatterErr != nil {
		return nil, scatterErr
	}

	pieChart := c.generateComplexityPieChart(report)

	return []plotpage.Section{
		{
			Title:    "Top Complex Functions",
			Subtitle: "Functions ranked by cyclomatic complexity (higher = more complex).",
			Chart:    barChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Green (1-5)</strong> = Simple, easy to understand and test",
					"<strong>Yellow (6-10)</strong> = Moderate complexity, consider simplifying",
					"<strong>Red (>10)</strong> = High complexity, should be refactored",
					"<strong>Action:</strong> Break down complex functions into smaller units",
				},
			},
		},
		{
			Title:    "Cyclomatic vs Cognitive Complexity",
			Subtitle: "Scatter plot showing relationship between complexity measures.",
			Chart:    scatterChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Bottom-left</strong> = Simple functions (ideal)",
					"<strong>Top-right</strong> = Complex functions (need attention)",
					"<strong>High cyclomatic, low cognitive</strong> = Many simple branches",
					"<strong>Low cyclomatic, high cognitive</strong> = Deep nesting or recursion",
					"<strong>Bubble size</strong> = Nesting depth",
				},
			},
		},
		{
			Title:    "Complexity Distribution",
			Subtitle: "Distribution of functions by complexity category.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Simple (1-5)</strong> = Functions that are easy to maintain",
					"<strong>Moderate (6-10)</strong> = Functions that need careful review",
					"<strong>Complex (>10)</strong> = Functions that should be refactored",
					"<strong>Goal:</strong> Maximize Simple functions, minimize Complex ones",
				},
			},
		},
	}, nil
}

func (c *Analyzer) generateComplexityBarChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := analyze.ReportFunctionListWithFallback(report, "functions", "function_complexity")
	if !ok {
		return nil, ErrInvalidFunctionsData
	}

	if len(functions) == 0 {
		return createEmptyComplexityChart(), nil
	}

	sorted := mapx.SortAndLimit(functions, func(a, b map[string]any) bool {
		return getCyclomaticValue(a) > getCyclomaticValue(b)
	}, topFunctionsLimit)

	labels, cyclomatic, cognitive, colors := extractComplexityData(sorted)
	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createComplexityBarChart(labels, cyclomatic, cognitive, colors, co, palette), nil
}

func getCyclomaticValue(fn map[string]any) int {
	return reportutil.GetInt(fn, KeyFuncCyclomatic)
}

func getCognitiveValue(fn map[string]any) int {
	return reportutil.GetInt(fn, KeyFuncCognitive)
}

func getNestingValue(fn map[string]any) int {
	return reportutil.GetInt(fn, KeyFuncNesting)
}

func extractComplexityData(functions []map[string]any) (labels []string, cyclomatic, cognitive []int, colors []string) {
	labels = make([]string, len(functions))
	cyclomatic = make([]int, len(functions))
	cognitive = make([]int, len(functions))
	colors = make([]string, len(functions))

	for i, fn := range functions {
		if name, ok := fn["name"].(string); ok {
			labels[i] = name
		} else {
			labels[i] = unknownName
		}

		cyclomatic[i] = getCyclomaticValue(fn)
		cognitive[i] = getCognitiveValue(fn)
		colors[i] = getComplexityColor(cyclomatic[i])
	}

	return labels, cyclomatic, cognitive, colors
}

func getComplexityColor(complexity int) string {
	switch {
	case complexity <= cyclomaticYellowLine:
		return "#91cc75"
	case complexity <= cyclomaticRedLine:
		return "#fac858"
	default:
		return "#ee6666"
	}
}

func createComplexityBarChart(
	labels []string, cyclomatic, cognitive []int, colors []string,
	co *plotpage.ChartOpts, palette plotpage.ChartPalette,
) *charts.Bar {
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithGridOpts(opts.Grid{
			Left: "5%", Right: "5%",
			Top: "25%", Bottom: "15%",
			ContainLabel: opts.Bool(true),
		}),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				Interval: "0",
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(co.YAxis("Complexity")),
	)

	bar.SetXAxis(labels)

	cyclomaticData := make([]opts.BarData, len(cyclomatic))

	for i, val := range cyclomatic {
		cyclomaticData[i] = opts.BarData{
			Value: val,
			ItemStyle: &opts.ItemStyle{
				Color: colors[i],
			},
		}
	}

	cognitiveData := make([]opts.BarData, len(cognitive))

	for i, val := range cognitive {
		cognitiveData[i] = opts.BarData{Value: val}
	}

	bar.AddSeries("Cyclomatic", cyclomaticData)
	bar.AddSeries("Cognitive", cognitiveData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}))

	return bar
}

func (c *Analyzer) generateComplexityScatterChart(report analyze.Report) (*charts.Scatter, error) {
	functions, ok := analyze.ReportFunctionListWithFallback(report, "functions", "function_complexity")
	if !ok {
		return nil, ErrInvalidFunctionsData
	}

	if len(functions) == 0 {
		return createEmptyScatterChart(), nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createComplexityScatterChart(functions, co, palette), nil
}

func createComplexityScatterChart(functions []map[string]any, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Scatter {
	scatter := charts.NewScatter()

	scatter.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithXAxisOpts(opts.XAxis{
			Name:      "Cyclomatic Complexity",
			Type:      "value",
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      "Cognitive Complexity",
			Type:      "value",
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			SplitLine: &opts.SplitLine{LineStyle: &opts.LineStyle{Color: co.GridColor()}},
		}),
		charts.WithGridOpts(co.Grid()),
	)

	scatterData := make([]opts.ScatterData, len(functions))

	for i, fn := range functions {
		cyclomatic := getCyclomaticValue(fn)
		cognitive := getCognitiveValue(fn)
		nesting := getNestingValue(fn)
		name := unknownName

		if n, ok := fn["name"].(string); ok {
			name = n
		}

		symbolSize := scatterSymbolSize + nesting*nestingMultiplier

		scatterData[i] = opts.ScatterData{
			Value:      []any{cyclomatic, cognitive, name},
			SymbolSize: symbolSize,
		}
	}

	scatter.AddSeries("Functions", scatterData,
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}),
		charts.WithMarkLineNameXAxisItemOpts(opts.MarkLineNameXAxisItem{
			Name:  "Cyclomatic warning",
			XAxis: cyclomaticRedLine,
		}),
		charts.WithMarkLineNameYAxisItemOpts(opts.MarkLineNameYAxisItem{
			Name:  "Cognitive warning",
			YAxis: cognitiveRedLine,
		}),
	)

	return scatter
}

func (c *Analyzer) generateComplexityPieChart(report analyze.Report) *charts.Pie {
	functions, ok := analyze.ReportFunctionListWithFallback(report, "functions", "function_complexity")
	if !ok || len(functions) == 0 {
		return createEmptyComplexityPie()
	}

	distribution := stats.Distribution(functions, classifyComplexityForPlot)

	return createComplexityDistributionPie(distribution)
}

// classifyComplexityForPlot assigns a display label to a function map entry.
func classifyComplexityForPlot(fn map[string]any) string {
	complexity := getCyclomaticValue(fn)

	switch {
	case complexity <= cyclomaticYellowLine:
		return plotLabelSimple
	case complexity <= cyclomaticRedLine:
		return plotLabelModerate
	default:
		return plotLabelComplex
	}
}

func createComplexityDistributionPie(distribution map[string]int) *charts.Pie {
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	pieData := []opts.PieData{
		{Name: "Simple (1-5)", Value: distribution[plotLabelSimple], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Good}},
		{Name: "Moderate (6-10)", Value: distribution[plotLabelModerate], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Warning}},
		{Name: "Complex (>10)", Value: distribution[plotLabelComplex], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Bad}},
	}

	return plotpage.BuildPieChart(nil, "Complexity", pieData, pieRadius)
}

func createEmptyComplexityChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Function Complexity", "No data")),
	)

	return bar
}

func createEmptyScatterChart() *charts.Scatter {
	co := plotpage.DefaultChartOpts()
	scatter := charts.NewScatter()

	scatter.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Complexity Scatter", "No data")),
	)

	return scatter
}

func createEmptyComplexityPie() *charts.Pie {
	co := plotpage.DefaultChartOpts()
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("600px", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Complexity Distribution", "No data")),
	)

	return pie
}
