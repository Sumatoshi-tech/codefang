package couples

import (
	"errors"
	"fmt"
	"io"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// generatePlot creates an interactive HTML heatmap from the couples analysis report.
func (c *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := c.GenerateChart(report)
	if err != nil {
		return fmt.Errorf("generate chart: %w", err)
	}

	if r, ok := chart.(interface{ Render(io.Writer) error }); ok {
		if err := r.Render(writer); err != nil {
			return fmt.Errorf("render chart: %w", err)
		}

		return nil
	}

	return errors.New("chart does not support Render") //nolint:err113 // dynamic error
}

// GenerateChart creates the chart object from the report.
//
//nolint:ireturn // interface needed for generic plotting
func (c *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	const fullZoomPct = 100

	peopleMatrix, ok := report["PeopleMatrix"].([]map[int]int64)
	if !ok {
		return nil, errors.New("expected []map[int]int64 for peopleMatrix") //nolint:err113 // descriptive error
	}

	reversedPeopleDict, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil, errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error
	}

	if len(peopleMatrix) == 0 {
		return createCouplesEmptyChart(), nil
	}

	// X and Y axis labels are developer names.
	// Filter out developers who have no interactions if needed, but for matrix it's easier to keep all.
	// Or we can just use the provided dictionary.
	names := reversedPeopleDict

	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Developer Coupling Heatmap",
			Subtitle: "Co-occurrence in commits",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{
			Type:      "category",
			Data:      names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type:      "category",
			Data:      names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
		}),
		charts.WithVisualMapOpts(opts.VisualMap{
			Calculable: opts.Bool(true),
			Min:        0,
			Max:        findMaxCoupling(peopleMatrix),
			InRange:    &opts.VisualMapInRange{Color: []string{"#f6efa6", "#d88273", "#bf444c"}},
		}),
	)

	// Prepare data: [x, y, value].
	data := make([]opts.HeatMapData, 0)

	for i, row := range peopleMatrix {
		for j, val := range row {
			if i >= len(names) || j >= len(names) {
				continue
			}
			// HeatMapData value is [x, y, value].
			data = append(data, opts.HeatMapData{Value: []any{i, j, val}})
		}
	}

	hm.AddSeries("Coupling", data)

	return hm, nil
}

func createCouplesEmptyChart() *charts.HeatMap {
	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Developer Coupling Heatmap",
			Subtitle: "No data",
		}),
	)

	return hm
}

func findMaxCoupling(matrix []map[int]int64) float32 {
	var maxVal int64

	for _, row := range matrix {
		for _, val := range row {
			if val > maxVal {
				maxVal = val
			}
		}
	}

	return float32(maxVal)
}
