package imports

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

// generatePlot creates an interactive HTML bar chart from the imports analysis report.
func (h *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := h.GenerateChart(report)
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
func (h *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	imports, ok := report["imports"].(Map)
	if !ok {
		return nil, errors.New("expected Map for imports") //nolint:err113 // type assertion error
	}

	if len(imports) == 0 {
		return createImportsEmptyChart(), nil
	}

	// Aggregate counts per import.
	importCounts := aggregateImportCounts(imports)

	// Sort by count descending.
	scores := sortImportScores(importCounts)

	// Take top 20.
	topN := 20
	topN = min(topN, len(scores))
	scores = scores[:topN]

	xLabels := make([]string, topN)
	barData := make([]opts.BarData, topN)

	for i, s := range scores {
		xLabels[i] = s.Name
		barData[i] = opts.BarData{Value: s.Score}
	}

	const rotateDegrees = 45

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Imports Usage",
			Subtitle: "Most frequently added imports",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Import",
			AxisLabel: &opts.AxisLabel{
				Rotate: rotateDegrees,
			},
		}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Usage Count"}),
	)
	bar.SetXAxis(xLabels)
	bar.AddSeries("Usage", barData)

	return bar, nil
}

type importScore struct {
	Name  string
	Score int64
}

func aggregateImportCounts(imports Map) map[string]int64 {
	// Map = map[int]map[string]map[string]map[int]int64 (author -> lang -> import -> tick -> count).
	importCounts := make(map[string]int64)

	for _, langMap := range imports {
		for _, impMap := range langMap {
			for impName, tickMap := range impMap {
				for _, count := range tickMap {
					importCounts[impName] += count
				}
			}
		}
	}

	return importCounts
}

func sortImportScores(importCounts map[string]int64) []importScore {
	scores := make([]importScore, 0, len(importCounts))
	for name, score := range importCounts {
		scores = append(scores, importScore{Name: name, Score: score})
	}

	sort.Slice(scores, func(i, j int) bool {
		return scores[i].Score > scores[j].Score
	})

	return scores
}

func createImportsEmptyChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Imports Usage",
			Subtitle: "No data",
		}),
	)

	return bar
}
