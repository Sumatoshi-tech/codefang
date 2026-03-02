package halstead

import (
	"errors"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	topFunctionsLimit = 12
	xAxisRotate       = 45
	emptyChartHeight  = "400px"
	pieRadius         = "60%"
	scatterSymbolSize = 12
	maxSymbolSize     = 45
	bugsMultiplier    = 10
	volumeLow         = 100
	volumeMedium      = 1000
	volumeHigh        = 5000
	effortLow         = 1000
	effortMedium      = 10000
	difficultyLow     = 5
	difficultyMedium  = 15
	difficultyHigh    = 30
)

// ErrInvalidFunctionsData indicates the report doesn't contain expected functions data.
var ErrInvalidFunctionsData = errors.New("invalid halstead report: expected []map[string]any for functions")

// RegisterPlotSections registers the halstead plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("static/halstead", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).generateSections(report)
	})
}

// FormatReportPlot generates an HTML plot visualization for Halstead analysis.
func (h *Analyzer) FormatReportPlot(report analyze.Report, w io.Writer) error {
	sections, err := h.generateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Halstead Complexity Analysis",
		"Program volume, difficulty, and effort metrics",
	)

	page.Add(sections...)

	return page.Render(w)
}

func (h *Analyzer) generateSections(report analyze.Report) ([]plotpage.Section, error) {
	effortChart, err := h.generateEffortBarChart(report)
	if err != nil {
		return nil, err
	}

	scatterChart, scatterErr := h.generateVolumeVsDifficultyChart(report)
	if scatterErr != nil {
		return nil, scatterErr
	}

	pieChart := h.generateVolumePieChart(report)

	return []plotpage.Section{
		{
			Title:    "Top Functions by Effort",
			Subtitle: "Most expensive functions first; start review from the top.",
			Chart:    effortChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Effort</strong> = Volume × Difficulty (higher means harder to maintain)",
					"<strong>Green</strong> = monitor, <strong>Yellow</strong> = schedule cleanup, <strong>Red</strong> = refactor now",
					"<strong>Tip:</strong> Start with red bars to reduce risk fastest",
				},
			},
		},
		{
			Title:    "Volume vs Difficulty",
			Subtitle: "Risk map by size (x), difficulty (y), and bug estimate (bubble size).",
			Chart:    scatterChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Bottom-left</strong> points are healthiest",
					"<strong>Top-right</strong> points are highest risk",
					"<strong>Bubble size</strong> reflects estimated bugs",
					"<strong>Color</strong> reflects risk zone (green/yellow/red)",
				},
			},
		},
		{
			Title:    "Volume Distribution",
			Subtitle: "Portfolio split by Halstead volume buckets.",
			Chart:    pieChart,
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"<strong>Low (≤100)</strong> = usually easy to maintain",
					"<strong>High / Very High</strong> concentration means decomposition debt",
				},
			},
		},
	}, nil
}

func (h *Analyzer) generateEffortBarChart(report analyze.Report) (*charts.Bar, error) {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_halstead")
	}

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
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_halstead")
	}

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

	lowRiskData := make([]opts.ScatterData, 0, len(functions))
	mediumRiskData := make([]opts.ScatterData, 0, len(functions))
	highRiskData := make([]opts.ScatterData, 0, len(functions))

	for _, fn := range functions {
		volume := getVolumeValue(fn)
		difficulty := getDifficultyValue(fn)
		bugs := getDeliveredBugsValue(fn)
		name := "unknown"

		if n, ok := fn["name"].(string); ok {
			name = n
		}

		symbolSize := min(scatterSymbolSize+int(bugs*bugsMultiplier), maxSymbolSize)
		point := opts.ScatterData{
			Value:      []any{volume, difficulty, name},
			SymbolSize: symbolSize,
		}

		switch classifyScatterRisk(volume, difficulty, bugs) {
		case riskHigh:
			highRiskData = append(highRiskData, point)
		case riskMedium:
			mediumRiskData = append(mediumRiskData, point)
		case riskLow:
			lowRiskData = append(lowRiskData, point)
		}
	}

	if len(lowRiskData) > 0 {
		scatter.AddSeries("Low risk", lowRiskData,
			charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}),
		)
	}

	if len(mediumRiskData) > 0 {
		scatter.AddSeries("Medium risk", mediumRiskData,
			charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Warning}),
		)
	}

	if len(highRiskData) > 0 {
		scatter.AddSeries("High risk", highRiskData,
			charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Bad}),
		)
	}

	return scatter
}

type scatterRisk int

const (
	riskLow scatterRisk = iota
	riskMedium
	riskHigh
)

func classifyScatterRisk(volume, difficulty, bugs float64) scatterRisk {
	switch {
	case volume >= volumeHigh || difficulty >= difficultyHigh || bugs >= 1.0:
		return riskHigh
	case volume >= volumeMedium || difficulty >= difficultyMedium || bugs >= 0.3:
		return riskMedium
	default:
		return riskLow
	}
}

func (h *Analyzer) generateVolumePieChart(report analyze.Report) *charts.Pie {
	functions, ok := analyze.ReportFunctionList(report, "functions")
	if !ok {
		functions, ok = analyze.ReportFunctionList(report, "function_halstead")
	}

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
