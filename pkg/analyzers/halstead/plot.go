package halstead

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
	scatterSymbolSize = 15
	maxSymbolSize     = 50
	bugsMultiplier    = 10
	volumeLow         = 100
	volumeMedium      = 1000
	volumeHigh        = 5000
	effortLow         = 1000
	effortMedium      = 10000
	difficultyLow     = 5
	difficultyMedium  = 15
)

// ErrInvalidFunctionsData indicates the report doesn't contain expected functions data.
var ErrInvalidFunctionsData = errors.New("invalid halstead report: expected []map[string]any for functions")

// FormatReportPlot generates an HTML plot visualization for Halstead analysis.
func (h *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	effortChart, err := h.generateEffortBarChart(report)
	if err != nil {
		return err
	}

	scatterChart, scatterErr := h.generateVolumeVsDifficultyChart(report)
	if scatterErr != nil {
		return scatterErr
	}

	pieChart := h.generateVolumePieChart(report)

	page := plotpage.NewPage(
		"Halstead Complexity Analysis",
		"Program volume, difficulty, and effort metrics",
	)

	page.Add(
		plotpage.Section{
			Title:    "Top Functions by Effort",
			Subtitle: "Functions ranked by programming effort required (higher = more effort).",
			Chart:    effortChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Effort</strong> = Volume × Difficulty (mental effort to understand code)",
					"<strong>Green bars</strong> = Low effort functions",
					"<strong>Yellow bars</strong> = Medium effort functions",
					"<strong>Red bars</strong> = High effort functions",
					"<strong>Action:</strong> Prioritize refactoring high-effort functions",
				},
			},
		},
		plotpage.Section{
			Title:    "Volume vs Difficulty",
			Subtitle: "Scatter plot showing relationship between code volume and difficulty.",
			Chart:    scatterChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Volume</strong> = Code size (operators and operands)",
					"<strong>Difficulty</strong> = How hard to write/understand",
					"<strong>Bottom-left</strong> = Simple, easy functions (ideal)",
					"<strong>Top-right</strong> = Complex, hard functions (refactor)",
					"<strong>Bubble size</strong> = Estimated bugs (larger = more bugs)",
				},
			},
		},
		plotpage.Section{
			Title:    "Volume Distribution",
			Subtitle: "Distribution of functions by code volume category.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Low (≤100)</strong> = Small, well-structured functions",
					"<strong>Medium (101-1000)</strong> = Medium-sized functions",
					"<strong>High (1001-5000)</strong> = Large functions, consider splitting",
					"<strong>Very High (>5000)</strong> = Very large, definitely split",
				},
			},
		},
	)

	return page.Render(w)
}

func (h *Analyzer) generateEffortBarChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := report["functions"].([]map[string]any)
	if !ok {
		return nil, ErrInvalidFunctionsData
	}

	if len(functions) == 0 {
		return createEmptyHalsteadChart(), nil
	}

	sorted := sortByEffort(functions)
	if len(sorted) > topFunctionsLimit {
		sorted = sorted[:topFunctionsLimit]
	}

	labels, efforts, colors := extractEffortData(sorted)
	co := plotpage.DefaultChartOpts()

	return createEffortBarChart(labels, efforts, colors, co), nil
}

func sortByEffort(functions []map[string]any) []map[string]any {
	sorted := make([]map[string]any, len(functions))
	copy(sorted, functions)

	sort.Slice(sorted, func(i, j int) bool {
		ei := getEffortValue(sorted[i])
		ej := getEffortValue(sorted[j])

		return ei > ej
	})

	return sorted
}

func getEffortValue(fn map[string]any) float64 {
	if val, ok := fn["effort"].(float64); ok {
		return val
	}

	return 0
}

func getVolumeValue(fn map[string]any) float64 {
	if val, ok := fn["volume"].(float64); ok {
		return val
	}

	return 0
}

func getDifficultyValue(fn map[string]any) float64 {
	if val, ok := fn["difficulty"].(float64); ok {
		return val
	}

	return 0
}

func getDeliveredBugsValue(fn map[string]any) float64 {
	if val, ok := fn["delivered_bugs"].(float64); ok {
		return val
	}

	return 0
}

func extractEffortData(functions []map[string]any) (labels []string, efforts []float64, colors []string) {
	labels = make([]string, len(functions))
	efforts = make([]float64, len(functions))
	colors = make([]string, len(functions))

	for i, fn := range functions {
		if name, ok := fn["name"].(string); ok {
			labels[i] = name
		} else {
			labels[i] = "unknown"
		}

		efforts[i] = getEffortValue(fn)
		colors[i] = getEffortColor(efforts[i])
	}

	return labels, efforts, colors
}

func getEffortColor(effort float64) string {
	switch {
	case effort <= effortLow:
		return "#91cc75"
	case effort <= effortMedium:
		return "#fac858"
	default:
		return "#ee6666"
	}
}

func createEffortBarChart(labels []string, efforts []float64, colors []string, co *plotpage.ChartOpts) *charts.Bar {
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
		charts.WithYAxisOpts(co.YAxis("Effort")),
	)

	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(efforts))

	for i, effort := range efforts {
		barData[i] = opts.BarData{
			Value: effort,
			ItemStyle: &opts.ItemStyle{
				Color: colors[i],
			},
		}
	}

	bar.AddSeries("Effort", barData)

	return bar
}

func (h *Analyzer) generateVolumeVsDifficultyChart(report analyze.Report) (*charts.Scatter, error) {
	functions, ok := report["functions"].([]map[string]any)
	if !ok {
		return nil, ErrInvalidFunctionsData
	}

	if len(functions) == 0 {
		return createEmptyHalsteadScatter(), nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createVolumeVsDifficultyChart(functions, co, palette), nil
}

func createVolumeVsDifficultyChart(functions []map[string]any, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Scatter {
	scatter := charts.NewScatter()

	scatter.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithXAxisOpts(opts.XAxis{
			Name:      "Volume",
			Type:      "value",
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			AxisLine:  &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Name:      "Difficulty",
			Type:      "value",
			AxisLabel: &opts.AxisLabel{Color: co.TextMutedColor()},
			SplitLine: &opts.SplitLine{LineStyle: &opts.LineStyle{Color: co.GridColor()}},
		}),
		charts.WithGridOpts(co.Grid()),
	)

	scatterData := make([]opts.ScatterData, len(functions))

	for i, fn := range functions {
		volume := getVolumeValue(fn)
		difficulty := getDifficultyValue(fn)
		bugs := getDeliveredBugsValue(fn)
		name := "unknown"

		if n, ok := fn["name"].(string); ok {
			name = n
		}

		symbolSize := min(scatterSymbolSize+int(bugs*bugsMultiplier), maxSymbolSize)

		scatterData[i] = opts.ScatterData{
			Value:      []any{volume, difficulty, name},
			SymbolSize: symbolSize,
		}
	}

	scatter.AddSeries("Functions", scatterData,
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}),
	)

	return scatter
}

func (h *Analyzer) generateVolumePieChart(report analyze.Report) *charts.Pie {
	functions, ok := report["functions"].([]map[string]any)
	if !ok || len(functions) == 0 {
		return createEmptyHalsteadPie()
	}

	distribution := countVolumeDistribution(functions)

	return createVolumeDistributionPie(distribution)
}

func countVolumeDistribution(functions []map[string]any) map[string]int {
	distribution := map[string]int{
		"Low":       0,
		"Medium":    0,
		"High":      0,
		"Very High": 0,
	}

	for _, fn := range functions {
		volume := getVolumeValue(fn)

		switch {
		case volume <= volumeLow:
			distribution["Low"]++
		case volume <= volumeMedium:
			distribution["Medium"]++
		case volume <= volumeHigh:
			distribution["High"]++
		default:
			distribution["Very High"]++
		}
	}

	return distribution
}

func createVolumeDistributionPie(distribution map[string]int) *charts.Pie {
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
		{Name: "Low (≤100)", Value: distribution["Low"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Good}},
		{Name: "Medium (101-1000)", Value: distribution["Medium"], ItemStyle: &opts.ItemStyle{Color: palette.Primary[1]}},
		{Name: "High (1001-5000)", Value: distribution["High"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Warning}},
		{Name: "Very High (>5000)", Value: distribution["Very High"], ItemStyle: &opts.ItemStyle{Color: palette.Semantic.Bad}},
	}

	pie.AddSeries("Volume", pieData).
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

func createEmptyHalsteadChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()

	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Function Effort", "No data")),
	)

	return bar
}

func createEmptyHalsteadScatter() *charts.Scatter {
	co := plotpage.DefaultChartOpts()
	scatter := charts.NewScatter()

	scatter.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Volume vs Difficulty", "No data")),
	)

	return scatter
}

func createEmptyHalsteadPie() *charts.Pie {
	co := plotpage.DefaultChartOpts()
	pie := charts.NewPie()

	pie.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("600px", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Volume Distribution", "No data")),
	)

	return pie
}
