package filehistory

import (
	"errors"
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
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
	sections, err := h.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"File History Analysis",
		"Identifying the most actively modified files in the repository",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (h *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := h.buildChart(report)
	if err != nil {
		return nil, err
	}

	return []plotpage.Section{
		{
			Title:    "Most Modified Files",
			Subtitle: "Files ranked by total number of commits touching them.",
			Chart:    plotpage.WrapChart(chart),
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
		},
	}, nil
}

// GenerateChart creates a bar chart showing the most modified files (implements PlotGenerator).
func (h *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return h.buildChart(report)
}

// buildChart creates a bar chart showing the most modified files.
func (h *Analyzer) buildChart(report analyze.Report) (*charts.Bar, error) {
	labels, data, err := extractFileHistoryData(report)
	if err != nil {
		return nil, err
	}

	if len(labels) == 0 {
		return createEmptyFileChart(), nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createFileHistoryBarChart(labels, data, co, palette), nil
}

// extractFileHistoryData extracts file names and commit counts from the report,
// handling both in-memory and binary-decoded JSON key formats.
func extractFileHistoryData(report analyze.Report) ([]string, []int, error) {
	type kv struct {
		k string
		v int
	}

	var items []kv

	// Try in-memory key first.
	if files, ok := report["Files"].(map[string]FileHistory); ok {
		for name, hist := range files {
			items = append(items, kv{name, len(hist.Hashes)})
		}
	} else {
		// Fallback: binary-decoded "file_churn" is []any of map[string]any
		// with "path" and "commit_count" fields.
		rawChurn, ok := report["file_churn"]
		if !ok {
			return nil, nil, ErrInvalidFiles
		}
		if rawChurn == nil {
			return nil, nil, nil
		}
		churnList, ok := rawChurn.([]any)
		if !ok {
			return nil, nil, ErrInvalidFiles
		}
		if len(churnList) == 0 {
			return nil, nil, nil
		}
		for _, item := range churnList {
			m, ok := item.(map[string]any)
			if !ok {
				continue
			}
			path, _ := m["path"].(string)
			commitCount := fileHistoryToInt(m["commit_count"])
			if path != "" {
				items = append(items, kv{path, commitCount})
			}
		}
	}

	if len(items) == 0 {
		return nil, nil, nil
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

	return labels, data, nil
}

// fileHistoryToInt converts a numeric value (int, float64) to int.
func fileHistoryToInt(v any) int {
	switch n := v.(type) {
	case float64:
		return int(n)
	case int:
		return n
	case int64:
		return int(n)
	default:
		return 0
	}
}

func createFileHistoryBarChart(labels []string, data []int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
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
		charts.WithYAxisOpts(co.YAxis("Commits")),
	)
	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(data))
	for i, v := range data {
		barData[i] = opts.BarData{Value: v}
	}

	bar.AddSeries("Commits", barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Bad}))

	return bar
}

func init() {
	analyze.RegisterPlotSections("history/file-history", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}

func createEmptyFileChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Top Modified Files", "No data")),
	)

	return bar
}
