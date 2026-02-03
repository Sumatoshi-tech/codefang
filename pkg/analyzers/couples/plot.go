package couples

import (
	"errors"
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	heatMapHeight    = "650px"
	labelRotate      = 60
	labelFontSize    = 10
	innerLabelSize   = 9
	emptyChartHeight = "400px"
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
		"Developer Coupling Analysis",
		"Co-occurrence patterns between developers based on commit history",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (c *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := c.buildChart(report)
	if err != nil {
		return nil, err
	}

	return []plotpage.Section{
		{
			Title:    "Developer Coupling Heatmap",
			Subtitle: "Shows how often developers work on the same files in the same commits.",
			Chart:    plotpage.WrapChart(chart),
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
		},
	}, nil
}

// GenerateChart implements PlotGenerator interface.
func (c *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return c.buildChart(report)
}

// buildChart creates a heatmap chart showing developer coupling.
func (c *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.HeatMap, error) {
	matrix, ok := report["PeopleMatrix"].([]map[int]int64)
	if !ok {
		return nil, ErrInvalidMatrix
	}

	names, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil, ErrInvalidNames
	}

	if len(matrix) == 0 {
		return createEmptyHeatMap(), nil
	}

	co := plotpage.DefaultChartOpts()
	maxVal := findMaxValue(matrix)
	data := buildHeatMapData(matrix, names)
	hm := createHeatMapChart(names, maxVal, data, co)

	return hm, nil
}

func findMaxValue(matrix []map[int]int64) int64 {
	var maxVal int64

	for _, row := range matrix {
		for _, val := range row {
			if val > maxVal {
				maxVal = val
			}
		}
	}

	return maxVal
}

func buildHeatMapData(matrix []map[int]int64, names []string) []opts.HeatMapData {
	var data []opts.HeatMapData

	for i, row := range matrix {
		for j, val := range row {
			if i < len(names) && j < len(names) {
				data = append(data, opts.HeatMapData{Value: []any{i, j, val}})
			}
		}
	}

	return data
}

func createHeatMapChart(names []string, maxVal int64, data []opts.HeatMapData, co *plotpage.ChartOpts) *charts.HeatMap {
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init("100%", heatMapHeight)),
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

func createEmptyHeatMap() *charts.HeatMap {
	co := plotpage.DefaultChartOpts()
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Developer Coupling", "No data")),
	)

	return hm
}
