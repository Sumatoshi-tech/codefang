package comments

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

// ErrInvalidFunctionsData indicates the report doesn't contain expected functions data.
var ErrInvalidFunctionsData = errors.New("invalid comments report: expected []map[string]any for functions")

func init() { //nolint:gochecknoinits // registration pattern
	analyze.RegisterPlotSections("static/comments", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generateSections(report)
	})
}

// FormatReportPlot generates an HTML plot visualization for comments analysis.
func (c *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	sections, err := c.generateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Code Comments Analysis",
		"Documentation coverage and comment quality metrics",
	)

	page.Add(sections...)

	return page.Render(w)
}

func (c *Analyzer) generateSections(report analyze.Report) ([]plotpage.Section, error) {
	barChart, err := c.generateFunctionCoverageChart(report)
	if err != nil {
		return nil, err
	}

	pieChart := c.generateDocumentationPieChart(report)
	gaugeChart := c.generateOverallScoreGauge(report)

	return []plotpage.Section{
		{
			Title:    "Overall Documentation Score",
			Subtitle: "Combined score based on comment quality and placement.",
			Chart:    gaugeChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Green (≥80%)</strong> = Excellent documentation quality",
					"<strong>Yellow (60-80%)</strong> = Good quality with room for improvement",
					"<strong>Orange (40-60%)</strong> = Fair quality - improvements needed",
					"<strong>Red (<40%)</strong> = Poor quality - significant improvements needed",
				},
			},
		},
		{
			Title:    "Function Documentation Status",
			Subtitle: "Documentation status for each function (sorted by lines of code).",
			Chart:    barChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Green bars</strong> = Well-documented functions",
					"<strong>Red bars</strong> = Functions without documentation",
					"<strong>Taller bars</strong> = Larger functions (more lines)",
					"<strong>Action:</strong> Prioritize documenting larger undocumented functions",
				},
			},
		},
		{
			Title:    "Documentation Coverage",
			Subtitle: "Distribution of documented vs undocumented functions.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Documented</strong> = Functions with properly placed comments",
					"<strong>Undocumented</strong> = Functions missing documentation",
					"<strong>Goal:</strong> Maximize the Documented segment",
				},
			},
		},
	}, nil
}

// reportValue looks up a key in the report, falling back to the "aggregate" sub-map
// that appears after binary encode -> JSON decode round-trip.
func reportValue(report analyze.Report, key string) (any, bool) {
	if val, found := report[key]; found {
		return val, true
	}

	if agg, aggOK := report["aggregate"].(map[string]any); aggOK {
		if aggVal, aggFound := agg[key]; aggFound {
			return aggVal, true
		}
	}

	return nil, false
}

func (c *Analyzer) generateFunctionCoverageChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_documentation")
	}

	if !ok {
		return nil, ErrInvalidFunctionsData
	}

	if len(functions) == 0 {
		return createEmptyCommentsChart(), nil
	}

	sorted := sortByLines(functions)
	if len(sorted) > topFunctionsLimit {
		sorted = sorted[:topFunctionsLimit]
	}

	labels, lines, colors := extractFunctionData(sorted)
	co := plotpage.DefaultChartOpts()

	return createFunctionCoverageBarChart(labels, lines, colors, co), nil
}

func sortByLines(functions []map[string]any) []map[string]any {
	sorted := make([]map[string]any, len(functions))
	copy(sorted, functions)

	sort.Slice(sorted, func(i, j int) bool {
		li := getLinesValue(sorted[i])
		lj := getLinesValue(sorted[j])

		return li > lj
	})

	return sorted
}

func getLinesValue(fn map[string]any) int {
	switch val := fn["lines"].(type) {
	case int:
		return val
	case float64:
		return int(val)
	default:
		return 0
	}
}

func isDocumented(fn map[string]any) bool {
	// In-memory report uses "assessment" field.
	if assessment, assessOK := fn["assessment"].(string); assessOK {
		return assessment == "✅ Well Documented"
	}

	// Binary-decoded ComputedMetrics uses "is_documented" bool.
	if documented, docOK := fn["is_documented"].(bool); docOK {
		return documented
	}

	// Binary-decoded ComputedMetrics uses "status" string.
	if status, statusOK := fn["status"].(string); statusOK {
		return status == "Well Documented"
	}

	return false
}

// getFunctionName returns the function name from a function record,
// handling both in-memory ("function") and binary-decoded ("name") keys.
func getFunctionName(fn map[string]any) string {
	if fnName, fnOK := fn["function"].(string); fnOK {
		return fnName
	}

	if altName, altOK := fn["name"].(string); altOK {
		return altName
	}

	return "unknown"
}

func extractFunctionData(functions []map[string]any) (labels []string, lines []int, colors []string) {
	labels = make([]string, len(functions))
	lines = make([]int, len(functions))
	colors = make([]string, len(functions))

	for i, fn := range functions {
		labels[i] = getFunctionName(fn)
		lines[i] = getLinesValue(fn)

		if isDocumented(fn) {
			colors[i] = "#91cc75"
		} else {
			colors[i] = "#ee6666"
		}
	}

	return labels, lines, colors
}

func createFunctionCoverageBarChart(labels []string, lines []int, colors []string, co *plotpage.ChartOpts) *charts.Bar {
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
		charts.WithYAxisOpts(co.YAxis("Lines of Code")),
	)

	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(lines))

	for i, lineCount := range lines {
		barData[i] = opts.BarData{
			Value: lineCount,
			ItemStyle: &opts.ItemStyle{
				Color: colors[i],
			},
		}
	}

	bar.AddSeries("Lines", barData)

	return bar
}

func (c *Analyzer) generateDocumentationPieChart(report analyze.Report) *charts.Pie {
	documented := 0
	undocumented := 0

	docVal, _ := reportValue(report, "documented_functions")

	switch val := docVal.(type) {
	case int:
		documented = val
	case float64:
		documented = int(val)
	}

	totalVal, _ := reportValue(report, "total_functions")

	switch total := totalVal.(type) {
	case int:
		undocumented = total - documented
	case float64:
		undocumented = int(total) - documented
	}

	if documented == 0 && undocumented == 0 {
		return createEmptyCommentsPie()
	}

	return createDocumentationPieChart(documented, undocumented)
}

func createDocumentationPieChart(documented, undocumented int) *charts.Pie {
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
		{Name: "Documented", Value: documented, ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Good}},
		{Name: "Undocumented", Value: undocumented, ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Bad}},
	}

	pie.AddSeries("Documentation", pieData).
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

func (c *Analyzer) generateOverallScoreGauge(report analyze.Report) *charts.Liquid {
	score := 0.0

	if val, ok := reportValue(report, "overall_score"); ok {
		if f, isFloat := val.(float64); isFloat {
			score = f
		}
	}

	return createScoreLiquid(score)
}

func createScoreLiquid(score float64) *charts.Liquid {
	liquid := charts.NewLiquid()

	liquid.SetGlobalOptions(
		charts.WithInitializationOpts(opts.Initialization{Width: "400px", Height: "400px"}),
	)

	liquid.AddSeries("Score", []opts.LiquidData{
		{Value: score},
	})

	return liquid
}

func createEmptyCommentsChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Function Documentation", "No data")),
	)

	return bar
}

func createEmptyCommentsPie() *charts.Pie {
	co := plotpage.DefaultChartOpts()
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("600px", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Documentation Coverage", "No data")),
	)

	return pie
}
