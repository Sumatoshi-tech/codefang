package couples

import (
	"errors"
	"fmt"
	"io"
	"strings"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

const (
	labelRotate      = 60
	labelFontSize    = 10
	innerLabelSize   = 9
	emptyChartHeight = "400px"
	barChartHeight   = "500px"
	pieChartWidth    = "600px"
	pieChartHeight   = "400px"
	pieRadius        = "65%"
	maxFileCouples   = 20
	maxHeatmapDevs   = 20
	heatmapMinHeight = 400
	heatmapMaxHeight = 900
	heatmapPerDev    = 30
	heatmapPadding   = 200
	maxDevNameLen    = 30
)

// ErrInvalidMatrix indicates the report doesn't contain expected matrix data.
var ErrInvalidMatrix = errors.New("invalid couples report: expected []map[int]int64 for PeopleMatrix")

// ErrInvalidNames indicates the report doesn't contain expected names data.
var ErrInvalidNames = errors.New("invalid couples report: expected []string for ReversedPeopleDict")

func (c *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := c.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Couples Analysis",
		"File coupling, developer coupling, and ownership patterns from commit history",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (c *HistoryAnalyzer) GenerateSections(report analyze.Report) (sections []plotpage.Section, err error) {
	var result []plotpage.Section

	// Section 1: File coupling bar chart.
	fileCouplingChart := buildFileCouplingBarChart(report)
	if fileCouplingChart != nil {
		result = append(result, plotpage.Section{
			Title:    "Top File Couples",
			Subtitle: "Most frequently co-changed file pairs across commit history.",
			Chart:    plotpage.WrapChart(fileCouplingChart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = file pairs that frequently change together",
					"Cross-package coupling may indicate architectural issues",
					"Test files coupled with implementation is expected and healthy",
					"Action: Consider extracting shared logic or merging tightly coupled files",
				},
			},
		})
	}

	// Section 2: Developer coupling heatmap.
	heatmap, hmErr := c.buildChart(report)
	if hmErr != nil {
		return nil, hmErr
	}

	result = append(result, plotpage.Section{
		Title:    "Developer Coupling Heatmap",
		Subtitle: "Shows how often developers work on the same files in the same commits.",
		Chart:    plotpage.WrapChart(heatmap),
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"High values on diagonal = individual developer activity",
				"High off-diagonal values = developers frequently working on the same code",
				"Symmetric patterns = collaborative pairs who often commit together",
				"Look for: Isolated developers or tight clusters",
				"Action: High coupling may indicate knowledge sharing or ownership issues",
			},
		},
	})

	// Section 3: Ownership distribution pie chart.
	ownershipPie := buildOwnershipPieChart(report)
	if ownershipPie != nil {
		result = append(result, plotpage.Section{
			Title:    "File Ownership Distribution",
			Subtitle: "How files are distributed by number of contributors.",
			Chart:    plotpage.WrapChart(ownershipPie),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Single owner = bus factor risk if that person leaves",
					"Many owners = potential coordination overhead",
					"2-3 owners is often the healthy sweet spot",
					"Action: Review single-owner files for knowledge sharing opportunities",
				},
			},
		})
	}

	return result, nil
}

// buildChart creates a heatmap chart showing developer coupling.
func (c *HistoryAnalyzer) buildChart(report analyze.Report) (heatMap *charts.HeatMap, err error) {
	matrix, names, extractErr := extractCouplesData(report)
	if extractErr != nil {
		return nil, extractErr
	}

	if len(matrix) == 0 {
		return createEmptyHeatMap(), nil
	}

	// Limit to top-N developers by activity (diagonal value).
	matrix, names = FilterTopDevs(matrix, names, maxHeatmapDevs)

	// Shorten "name|email" labels to just the name part.
	shortNames := shortenDevNames(names)

	co := plotpage.DefaultChartOpts()
	maxVal := findMaxOffDiagonal(matrix)

	if maxVal == 0 {
		maxVal = findMaxValue(matrix)
	}

	data := buildHeatMapData(matrix, names)
	height := dynamicHeatmapHeight(len(shortNames))
	hm := createHeatMapChart(shortNames, maxVal, data, co, height)

	return hm, nil
}

// extractCouplesData extracts the people matrix and names from the report.
func extractCouplesData(report analyze.Report) (matrix []map[int]int64, names []string, err error) {
	matrix, matrixOK := report["PeopleMatrix"].([]map[int]int64)
	if !matrixOK {
		return nil, nil, ErrInvalidMatrix
	}

	names, namesOK := report["ReversedPeopleDict"].([]string)
	if !namesOK {
		return nil, nil, ErrInvalidNames
	}

	return matrix, names, nil
}

func findMaxValue(matrix []map[int]int64) (maxVal int64) {
	for _, row := range matrix {
		for _, val := range row {
			if val > maxVal {
				maxVal = val
			}
		}
	}

	return maxVal
}

// findMaxOffDiagonal returns the max value excluding diagonal cells (i==j).
// The diagonal represents self-activity which dominates the color scale.
func findMaxOffDiagonal(matrix []map[int]int64) (maxVal int64) {
	for i, row := range matrix {
		for j, val := range row {
			if i != j && val > maxVal {
				maxVal = val
			}
		}
	}

	return maxVal
}

// shortenDevNames extracts the name part from "name|email" format strings
// and truncates long names for chart readability.
func shortenDevNames(names []string) []string {
	short := make([]string, len(names))

	for i, name := range names {
		if before, _, found := strings.Cut(name, "|"); found {
			name = before
		}

		if len(name) > maxDevNameLen {
			name = name[:maxDevNameLen-3] + "..."
		}

		short[i] = name
	}

	return short
}

// dynamicHeatmapHeight computes a height string based on developer count.
func dynamicHeatmapHeight(devCount int) string {
	h := devCount*heatmapPerDev + heatmapPadding

	h = max(heatmapMinHeight, min(heatmapMaxHeight, h))

	return fmt.Sprintf("%dpx", h)
}

func buildHeatMapData(matrix []map[int]int64, names []string) (data []opts.HeatMapData) {
	for i, row := range matrix {
		for j, val := range row {
			if i < len(names) && j < len(names) {
				data = append(data, opts.HeatMapData{Value: []any{i, j, val}})
			}
		}
	}

	return data
}

func createHeatMapChart(names []string, maxVal int64, data []opts.HeatMapData, co *plotpage.ChartOpts, height string) *charts.HeatMap {
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init("100%", height)),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{Rotate: labelRotate, Interval: "0", FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
		charts.WithVisualMapOpts(opts.VisualMap{
			Calculable: opts.Bool(true), Min: 0, Max: float32(maxVal),
			InRange: &opts.VisualMapInRange{Color: []string{"#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"}},
			Orient:  "horizontal", Left: "center", Bottom: "2%",
			TextStyle: &opts.TextStyle{Color: co.TextMutedColor()},
		}),
		charts.WithGridOpts(opts.Grid{
			Left: "20%", Right: "5%", Top: "40", Bottom: "20%",
		}),
	)
	hm.AddSeries("Coupling", data, charts.WithLabelOpts(opts.Label{
		Show: opts.Bool(true), Position: "inside", Color: "black", FontSize: innerLabelSize,
	}))

	return hm
}

// RegisterPlotSections registers the couples plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterStorePlotSections("couples", GenerateStoreSections)
}

func createEmptyHeatMap() *charts.HeatMap {
	co := plotpage.DefaultChartOpts()
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Developer Coupling", "No data")),
	)

	return hm
}

// buildFileCouplingBarChart creates a horizontal bar chart of top coupled file pairs.
func buildFileCouplingBarChart(report analyze.Report) *charts.Bar {
	input, err := ParseReportData(report)
	if err != nil || len(input.Files) == 0 || len(input.FilesMatrix) == 0 {
		return nil
	}

	metric := NewFileCouplingMetric()
	couples := metric.Compute(input)

	return buildFileCouplingBarChartFromData(couples)
}

// buildOwnershipPieChart creates a pie chart showing file ownership distribution.
func buildOwnershipPieChart(report analyze.Report) *charts.Pie {
	input, err := ParseReportData(report)
	if err != nil || len(input.Files) == 0 {
		return nil
	}

	metric := NewFileOwnershipMetric()
	ownership := metric.Compute(input)

	return buildOwnershipPieChartFromData(ownership)
}

// truncatePath shortens a file path for chart labels.
func truncatePath(path string) string {
	const maxLen = 30

	if len(path) <= maxLen {
		return path
	}

	return "..." + path[len(path)-maxLen+3:]
}

// buildFileCouplingBarChartFromData creates a bar chart from pre-computed FileCouplingData.
func buildFileCouplingBarChartFromData(couples []FileCouplingData) *charts.Bar {
	if len(couples) == 0 {
		return nil
	}

	shown := min(len(couples), maxFileCouples)
	labels := make([]string, shown)
	values := make([]opts.BarData, shown)

	for i, cp := range couples[:shown] {
		labels[shown-1-i] = truncatePath(cp.File1) + " \u2194 " + truncatePath(cp.File2) // ↔
		values[shown-1-i] = opts.BarData{Value: cp.CoChanges}
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithInitializationOpts(co.Init("100%", barChartHeight)),
		charts.WithGridOpts(opts.Grid{
			Left: "35%", Right: "5%", Top: "40", Bottom: "10%",
		}),
		charts.WithXAxisOpts(opts.XAxis{
			Type:      "value",
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type: "category", Data: labels,
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
	)

	bar.AddSeries("Co-changes", values,
		charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[0]}),
		charts.WithLabelOpts(opts.Label{
			Show:     opts.Bool(true),
			Position: "right",
			Color:    co.TextMutedColor(),
			FontSize: innerLabelSize,
		}),
	)

	return bar
}

// buildHeatmapFromMatrix creates a heatmap chart from a pre-computed dev matrix.
func buildHeatmapFromMatrix(matrix []map[int]int64, names []string) *charts.HeatMap {
	if len(matrix) == 0 {
		return createEmptyHeatMap()
	}

	shortNames := shortenDevNames(names)

	co := plotpage.DefaultChartOpts()
	maxVal := findMaxOffDiagonal(matrix)

	if maxVal == 0 {
		maxVal = findMaxValue(matrix)
	}

	data := buildHeatMapData(matrix, names)
	height := dynamicHeatmapHeight(len(shortNames))

	return createHeatMapChart(shortNames, maxVal, data, co, height)
}

// buildOwnershipPieChartFromData creates a pie chart from pre-computed FileOwnershipData.
func buildOwnershipPieChartFromData(ownership []FileOwnershipData) *charts.Pie {
	if len(ownership) == 0 {
		return nil
	}

	buckets := BucketOwnership(ownership)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	pie := charts.NewPie()
	pie.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init(pieChartWidth, pieChartHeight)),
		charts.WithLegendOpts(opts.Legend{
			Show:      opts.Bool(true),
			Top:       "bottom",
			TextStyle: &opts.TextStyle{Color: co.TextMutedColor()},
		}),
	)

	bucketColors := []string{
		palette.Semantic.Bad,
		palette.Semantic.Good,
		palette.Semantic.Warning,
		palette.Primary[0],
	}

	pieData := make([]opts.PieData, len(buckets))
	for i, b := range buckets {
		pieData[i] = opts.PieData{
			Name:      b.Label,
			Value:     b.Count,
			ItemStyle: &opts.ItemStyle{Color: bucketColors[i]},
		}
	}

	pie.AddSeries("Ownership", pieData).
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
