package filehistory

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

// generatePlot creates an interactive HTML bar chart from the file history analysis report.
func (h *Analyzer) generatePlot(report analyze.Report, writer io.Writer) error {
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
func (h *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	files, ok := report["Files"].(map[string]FileHistory)
	if !ok {
		return nil, errors.New("expected map[string]FileHistory for files") //nolint:err113 // descriptive error
	}

	if len(files) == 0 {
		return createFileHistoryEmptyChart(), nil
	}

	// Calculate scores (commit count) for each file.
	type fileScore struct {
		Name  string
		Score int
	}

	scores := make([]fileScore, 0, len(files))
	for name, hist := range files {
		scores = append(scores, fileScore{Name: name, Score: len(hist.Hashes)})
	}

	// Sort by score descending.
	sort.Slice(scores, func(i, j int) bool {
		return scores[i].Score > scores[j].Score
	})

	// Take top 20.
	topN := 20
	topN = min(topN, len(scores))
	scores = scores[:topN]

	// Prepare data.
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
			Title:    "Top Modified Files",
			Subtitle: "Files with most commits",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "File",
			AxisLabel: &opts.AxisLabel{
				Rotate: rotateDegrees, // Rotate labels to fit long filenames.
			},
		}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Commits"}),
	)
	bar.SetXAxis(xLabels)
	bar.AddSeries("Commits", barData)

	return bar, nil
}

func createFileHistoryEmptyChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Top Modified Files",
			Subtitle: "No data",
		}),
	)

	return bar
}
