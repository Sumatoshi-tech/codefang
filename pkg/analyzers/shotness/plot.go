package shotness

import (
	"errors"
	"fmt"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// generatePlot creates an interactive HTML bar chart from the shotness analysis report.
func (s *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := s.GenerateChart(report)
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
func (s *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	nodes, ok := report["Nodes"].([]NodeSummary)
	if !ok {
		return nil, errors.New("expected []NodeSummary for nodes") //nolint:err113 // descriptive error
	}

	counters, ok := report["Counters"].([]map[int]int)
	if !ok {
		return nil, errors.New("expected []map[int]int for counters") //nolint:err113 // descriptive error
	}

	if len(nodes) == 0 {
		return createShotnessEmptyChart(), nil
	}

	// Calculate "hotness" score for each node (sum of couplings or just the count).
	// Counters[i] has [i] -> count (self count), and [j] -> coupling count.
	// We'll use the self count (counters[i][i]) as the metric for "Shotness".
	type nodeScore struct {
		Name  string
		Score int
	}

	scores := make([]nodeScore, len(nodes))
	for i, counter := range counters {
		scores[i] = nodeScore{Name: nodes[i].Name, Score: counter[i]}
	}

	// Sort by score descending.
	sort.Slice(scores, func(i, j int) bool {
		return scores[i].Score > scores[j].Score
	})

	// Take top 20.
	topN := 20
	topN = min(topN, len(scores))
	scores = scores[:topN]

	xLabels := make([]string, topN)
	barData := make([]opts.BarData, topN)

	for i, sc := range scores {
		xLabels[i] = sc.Name
		barData[i] = opts.BarData{Value: sc.Score}
	}

	const rotateDegrees = 45

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Shot Nodes",
			Subtitle: "Most frequently changed functions/structures",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Node",
			AxisLabel: &opts.AxisLabel{
				Rotate: rotateDegrees,
			},
		}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Change Count"}),
	)
	bar.SetXAxis(xLabels)
	bar.AddSeries("Changes", barData)

	return bar, nil
}

func createShotnessEmptyChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Shot Nodes",
			Subtitle: "No data",
		}),
	)

	return bar
}
