package couples

import (
	"errors"
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	heatMapHeight    = "650px"
	dataZoomEnd      = 100
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
	chart, err := c.GenerateChart(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Developer Coupling Analysis",
		"Co-occurrence patterns between developers based on commit history",
	)
	page.Add(plotpage.Section{
		Title:    "Developer Coupling Heatmap",
		Subtitle: "Shows how often developers work on the same files in the same commits.",
		Chart:    chart,
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

	return page.Render(writer)
}

// GenerateChart creates a heatmap chart showing developer coupling.
func (c *HistoryAnalyzer) GenerateChart(report analyze.Report) (*charts.HeatMap, error) {
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

	style := plotpage.DefaultStyle()
	maxVal := findMaxValue(matrix)
	data := buildHeatMapData(matrix, names)
	hm := createHeatMapChart(names, maxVal, data, style)

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

func createHeatMapChart(names []string, maxVal int64, data []opts.HeatMapData, style plotpage.Style) *charts.HeatMap {
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithInitializationOpts(opts.Initialization{Width: style.Width, Height: heatMapHeight}),
		charts.WithDataZoomOpts(
			opts.DataZoom{Type: "slider", Start: 0, End: dataZoomEnd},
			opts.DataZoom{Type: "inside"},
		),
		charts.WithXAxisOpts(opts.XAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{Rotate: labelRotate, Interval: "0", FontSize: labelFontSize},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize},
		}),
		charts.WithVisualMapOpts(opts.VisualMap{
			Calculable: opts.Bool(true), Min: 0, Max: float32(maxVal),
			InRange: &opts.VisualMapInRange{Color: []string{"#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"}},
			Orient:  "horizontal", Left: "center", Bottom: "2%",
		}),
		charts.WithGridOpts(opts.Grid{
			Left: "20%", Right: style.GridRight, Top: style.GridTop, Bottom: "20%",
		}),
	)
	hm.AddSeries("Coupling", data, charts.WithLabelOpts(opts.Label{
		Show: opts.Bool(true), Position: "inside", Color: "black", FontSize: innerLabelSize,
	}))

	return hm
}

func createEmptyHeatMap() *charts.HeatMap {
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Developer Coupling", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return hm
}
