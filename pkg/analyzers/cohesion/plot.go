package cohesion

import (
	"errors"
	"fmt"
	"io"
	"math"
	"path/filepath"
	"sort"
	"strings"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	emptyChartHeight   = "400px"
	pieRadius          = "60%"
	histogramBins      = 10
	midpointFactor     = 0.5
	minGroupSize       = 3
	maxDirectories     = 15
	maxPathComponents  = 3
	boxPlotLabelRotate = 30
	pQ1                = 0.25
	pMedian            = 0.50
	pQ3                = 0.75
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
	histogram, err := c.generateHistogram(report)
	if err != nil {
		return nil, err
	}

	pieChart := c.generatePieChart(report)
	boxPlot := c.generateBoxPlot(report)

	return []plotpage.Section{
		{
			Title:    "Cohesion Score Distribution",
			Subtitle: "Number of functions in each cohesion score range.",
			Chart:    histogram,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Left side (low scores)</strong> = functions with poor cohesion — refactoring candidates",
					"<strong>Right side (high scores)</strong> = functions with good cohesion — well-structured",
					"<strong>Red zone</strong> (< 0.3) = Poor — function is isolated, consider splitting",
					"<strong>Green zone</strong> (≥ 0.6) = Excellent — function shares most variables with the module",
					"<strong>Healthy codebase:</strong> most functions should cluster on the right side",
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
					"<strong>Excellent</strong> = Functions with cohesion ≥ 0.6",
					"<strong>Good</strong> = Functions with cohesion 0.4-0.6",
					"<strong>Fair</strong> = Functions with cohesion 0.3-0.4",
					"<strong>Poor</strong> = Functions with cohesion < 0.3",
					"<strong>Goal:</strong> Maximize the Excellent and Good segments",
				},
			},
		},
		{
			Title:    "Cohesion by Package",
			Subtitle: "Box plot showing cohesion score distribution per directory.",
			Chart:    boxPlot,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Box</strong> = Middle 50% of scores (interquartile range)",
					"<strong>Line inside box</strong> = Median cohesion score for the package",
					"<strong>Whiskers</strong> = Min and max cohesion scores in the package",
					"<strong>Sorted left-to-right</strong> by median (worst packages first)",
					"<strong>Goal:</strong> All boxes should cluster above 0.5",
				},
			},
		},
	}, nil
}

func (c *Analyzer) generateHistogram(report analyze.Report) (*charts.Bar, error) {
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

	scores := extractScores(functions)
	bins := binScores(scores)
	co := plotpage.DefaultChartOpts()

	return createHistogramChart(bins, co), nil
}

// extractScores extracts cohesion values from function maps.
func extractScores(functions []map[string]any) []float64 {
	scores := make([]float64, len(functions))
	for i, fn := range functions {
		scores[i] = getCohesionValue(fn)
	}

	return scores
}

func getCohesionValue(fn map[string]any) float64 {
	if val, ok := fn["cohesion"].(float64); ok {
		return val
	}

	return 0
}

// histogramBin holds the count and label for one histogram bucket.
type histogramBin struct {
	Label string
	Count int
	Color string
}

// binScores distributes scores into equal-width bins from 0.0 to 1.0.
func binScores(scores []float64) []histogramBin {
	binWidth := 1.0 / float64(histogramBins)
	counts := make([]int, histogramBins)

	for _, s := range scores {
		idx := int(s / binWidth)
		if idx >= histogramBins {
			idx = histogramBins - 1
		}

		counts[idx]++
	}

	bins := make([]histogramBin, histogramBins)
	for i := range histogramBins {
		lo := float64(i) * binWidth
		hi := lo + binWidth
		mid := lo + binWidth*midpointFactor

		bins[i] = histogramBin{
			Label: fmt.Sprintf("%.1f–%.1f", lo, hi),
			Count: counts[i],
			Color: getCohesionColor(mid),
		}
	}

	return bins
}

func getCohesionColor(cohesion float64) string {
	switch {
	case cohesion >= DistExcellentMin:
		return "#91cc75"
	case cohesion >= DistGoodMin:
		return "#fac858"
	case cohesion >= DistFairMin:
		return "#fd8c73"
	default:
		return "#ee6666"
	}
}

// createHistogramChart builds a vertical bar chart showing frequency of
// functions in each cohesion score bin.
func createHistogramChart(bins []histogramBin, co *plotpage.ChartOpts) *charts.Bar {
	bar := charts.NewBar()

	labels := make([]string, len(bins))
	barData := make([]opts.BarData, len(bins))

	for i, b := range bins {
		labels[i] = b.Label

		barData[i] = opts.BarData{
			Value:     b.Count,
			ItemStyle: &opts.ItemStyle{Color: b.Color},
		}
	}

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(opts.Tooltip{
			Show:    opts.Bool(true),
			Trigger: "axis",
		}),
		charts.WithGridOpts(co.Grid()),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Cohesion Score",
			AxisLabel: &opts.AxisLabel{
				Color: co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      "Number of Functions",
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			SplitLine: &opts.SplitLine{LineStyle: &opts.LineStyle{Color: co.GridColor()}},
		}),
	)

	bar.SetXAxis(labels)
	bar.AddSeries("Functions", barData)

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
		case cohesion >= DistExcellentMin:
			distribution["Excellent"]++
		case cohesion >= DistGoodMin:
			distribution["Good"]++
		case cohesion >= DistFairMin:
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

// directoryGroup holds cohesion scores for one directory.
type directoryGroup struct {
	Label  string
	Scores []float64 // Sorted ascending.
}

func (c *Analyzer) generateBoxPlot(report analyze.Report) *charts.BoxPlot {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_cohesion")
	}

	if !ok || len(functions) == 0 {
		return createEmptyBoxPlot()
	}

	groups := groupByDirectory(functions)
	if len(groups) == 0 {
		return createEmptyBoxPlot()
	}

	return buildBoxPlotChart(groups)
}

// groupByDirectory groups functions by their source file directory,
// filters small groups, sorts by median ascending (worst first), and caps at maxDirectories.
func groupByDirectory(functions []map[string]any) []directoryGroup {
	grouped := make(map[string][]float64)

	for _, fn := range functions {
		filePath, ok := fn["_source_file"].(string)
		if !ok || filePath == "" {
			continue
		}

		dir := shortenDirectory(filepath.Dir(filePath))
		grouped[dir] = append(grouped[dir], getCohesionValue(fn))
	}

	var result []directoryGroup

	for dir, scores := range grouped {
		if len(scores) < minGroupSize {
			continue
		}

		sort.Float64s(scores)
		result = append(result, directoryGroup{Label: dir, Scores: scores})
	}

	sort.Slice(result, func(i, j int) bool {
		return median(result[i].Scores) < median(result[j].Scores)
	})

	if len(result) > maxDirectories {
		result = result[:maxDirectories]
	}

	return result
}

// shortenDirectory keeps the last maxPathComponents non-empty components of a path.
func shortenDirectory(dir string) string {
	allParts := strings.Split(filepath.ToSlash(dir), "/")

	parts := make([]string, 0, len(allParts))

	for _, p := range allParts {
		if p != "" {
			parts = append(parts, p)
		}
	}

	if len(parts) > maxPathComponents {
		parts = parts[len(parts)-maxPathComponents:]
	}

	return strings.Join(parts, "/")
}

// boxStats computes [min, Q1, median, Q3, max] for a sorted slice.
func boxStats(sorted []float64) [5]float64 {
	if len(sorted) == 0 {
		return [5]float64{}
	}

	return [5]float64{
		sorted[0],
		percentile(sorted, pQ1),
		percentile(sorted, pMedian),
		percentile(sorted, pQ3),
		sorted[len(sorted)-1],
	}
}

// percentile computes the p-th percentile of a sorted slice using linear interpolation.
func percentile(sorted []float64, p float64) float64 {
	if len(sorted) == 0 {
		return 0
	}

	if len(sorted) == 1 {
		return sorted[0]
	}

	idx := p * float64(len(sorted)-1)
	lower := int(math.Floor(idx))
	upper := int(math.Ceil(idx))

	if lower == upper {
		return sorted[lower]
	}

	frac := idx - float64(lower)

	return sorted[lower]*(1-frac) + sorted[upper]*frac
}

func median(sorted []float64) float64 {
	return percentile(sorted, pMedian)
}

func buildBoxPlotChart(groups []directoryGroup) *charts.BoxPlot {
	co := plotpage.DefaultChartOpts()
	bp := charts.NewBoxPlot()

	labels := make([]string, len(groups))
	data := make([]opts.BoxPlotData, len(groups))

	for i, g := range groups {
		labels[i] = g.Label
		stats := boxStats(g.Scores)
		data[i] = opts.BoxPlotData{
			Name:  g.Label,
			Value: stats[:],
		}
	}

	bp.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true), Trigger: "item"}),
		charts.WithGridOpts(co.Grid()),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Package / Directory",
			AxisLabel: &opts.AxisLabel{
				Color:  co.TextMutedColor(),
				Rotate: boxPlotLabelRotate,
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      "Cohesion Score",
			Min:       0,
			Max:       1.0,
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			SplitLine: &opts.SplitLine{LineStyle: &opts.LineStyle{Color: co.GridColor()}},
		}),
	)

	bp.SetXAxis(labels)
	bp.AddSeries("Cohesion", data)

	return bp
}

func createEmptyBoxPlot() *charts.BoxPlot {
	co := plotpage.DefaultChartOpts()
	bp := charts.NewBoxPlot()

	bp.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Cohesion by Package", "No package data available")),
	)

	return bp
}
