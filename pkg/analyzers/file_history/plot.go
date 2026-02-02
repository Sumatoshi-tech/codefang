package filehistory

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
	topFilesLimit    = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
)

// ErrInvalidFiles indicates the report doesn't contain expected files data.
var ErrInvalidFiles = errors.New("invalid file_history report: expected map[string]FileHistory for Files")

func (h *Analyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := h.GenerateChart(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"File History Analysis",
		"Identifying the most actively modified files in the repository",
	)
	page.Add(plotpage.Section{
		Title:    "Most Modified Files",
		Subtitle: "Files ranked by total number of commits touching them.",
		Chart:    chart,
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"Tall bars = frequently modified files (high churn)",
				"Configuration files = expected to change often",
				"Core business logic = may indicate instability or active development",
				"Look for: Files changing too frequently that should be stable",
				"Action: High-churn files benefit from better test coverage",
			},
		},
	})

	return page.Render(writer)
}

// GenerateChart creates a bar chart showing the most modified files.
func (h *Analyzer) GenerateChart(report analyze.Report) (*charts.Bar, error) {
	files, ok := report["Files"].(map[string]FileHistory)
	if !ok {
		return nil, ErrInvalidFiles
	}

	if len(files) == 0 {
		return createEmptyFileChart(), nil
	}

	type kv struct {
		k string
		v int
	}

	var items []kv

	for name, hist := range files {
		items = append(items, kv{name, len(hist.Hashes)})
	}

	sort.Slice(items, func(i, j int) bool { return items[i].v > items[j].v })

	if len(items) > topFilesLimit {
		items = items[:topFilesLimit]
	}

	labels := make([]string, len(items))
	data := make([]int, len(items))

	for i, item := range items {
		labels[i] = item.k
		data[i] = item.v
	}

	style := plotpage.DefaultStyle()

	return plotpage.NewBarChart(style).
		XAxis(labels, xAxisRotate).
		YAxis("Commits").
		Series("Commits", data, "#ee6666").
		Build(), nil
}

func createEmptyFileChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Top Modified Files", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return bar
}
