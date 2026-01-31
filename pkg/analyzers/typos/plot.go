package typos

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

// generatePlot creates an interactive HTML bar chart from the typos analysis report.
func (t *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := t.GenerateChart(report)
	if err != nil {
		return fmt.Errorf("generate chart: %w", err)
	}

	if r, ok := chart.(interface{ Render(io.Writer) error }); ok {
		err = r.Render(writer)
		if err != nil {
			return fmt.Errorf("render chart: %w", err)
		}

		return nil
	}

	return errors.New("chart does not support Render") //nolint:err113 // dynamic error
}

// GenerateChart creates the chart object from the report.
func (t *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	typos, ok := report["typos"].([]Typo)
	if !ok {
		return nil, errors.New("expected []Typo for typos") //nolint:err113 // descriptive error
	}

	if len(typos) == 0 {
		return createTyposEmptyChart(), nil
	}

	// Count typos per file.
	fileCounts := make(map[string]int)
	for _, typo := range typos {
		fileCounts[typo.File]++
	}

	// Sort files by typo count.
	type fileScore struct {
		Name  string
		Count int
	}

	scores := make([]fileScore, 0, len(fileCounts))
	for name, count := range fileCounts {
		scores = append(scores, fileScore{Name: name, Count: count})
	}

	sort.Slice(scores, func(i, j int) bool {
		return scores[i].Count > scores[j].Count
	})

	// Take top 20.
	topN := 20
	topN = min(topN, len(scores))
	scores = scores[:topN]

	xLabels := make([]string, topN)
	barData := make([]opts.BarData, topN)

	for i, sc := range scores {
		xLabels[i] = sc.Name
		barData[i] = opts.BarData{Value: sc.Count}
	}

	const rotateDegrees = 45

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Typo-Prone Files",
			Subtitle: "Files with most fixed typos",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "File",
			AxisLabel: &opts.AxisLabel{
				Rotate: rotateDegrees,
			},
		}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Typo Count"}),
	)
	bar.SetXAxis(xLabels)
	bar.AddSeries("Typos", barData)

	return bar, nil
}

func createTyposEmptyChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Typo-Prone Files",
			Subtitle: "No data",
		}),
	)

	return bar
}
