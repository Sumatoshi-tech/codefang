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
func (h *Analyzer) buildChart(report analyze.Report) (chart *charts.Bar, buildErr error) {
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

// fileChurnItem holds a file path and its commit count for sorting.
type fileChurnItem struct {
	path        string
	commitCount int
}

// extractFileHistoryData extracts file names and commit counts from the report,
// handling both in-memory and binary-decoded JSON key formats.
func extractFileHistoryData(report analyze.Report) (labels []string, data []int, err error) {
	var items []fileChurnItem

	// Try in-memory key first.
	if files, filesOK := report["Files"].(map[string]FileHistory); filesOK {
		for name, hist := range files {
			items = append(items, fileChurnItem{name, len(hist.Hashes)})
		}
	} else {
		items, err = extractFileChurnFromBinary(report)
		if err != nil {
			return nil, nil, err
		}
	}

	if len(items) == 0 {
		return nil, nil, nil
	}

	sort.Slice(items, func(i, j int) bool { return items[i].commitCount > items[j].commitCount })

	if len(items) > topFilesLimit {
		items = items[:topFilesLimit]
	}

	labels = make([]string, len(items))
	data = make([]int, len(items))

	for i, item := range items {
		labels[i] = item.path
		data[i] = item.commitCount
	}

	return labels, data, nil
}

// extractFileChurnFromBinary extracts file churn data from the binary-decoded report format.
// The "file_churn" key contains []any of map[string]any with "path" and "commit_count" fields.
func extractFileChurnFromBinary(report analyze.Report) (items []fileChurnItem, err error) {
	rawChurn, churnOK := report["file_churn"]
	if !churnOK {
		return nil, ErrInvalidFiles
	}

	if rawChurn == nil {
		return nil, nil
	}

	churnList, listOK := rawChurn.([]any)
	if !listOK {
		return nil, ErrInvalidFiles
	}

	if len(churnList) == 0 {
		return nil, nil
	}

	for _, item := range churnList {
		m, mOK := item.(map[string]any)
		if !mOK {
			continue
		}

		filePath, _ := m["path"].(string) //nolint:errcheck // type assertion, not error
		count := fileHistoryToInt(m["commit_count"])

		if filePath != "" {
			items = append(items, fileChurnItem{filePath, count})
		}
	}

	return items, nil
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

func init() { //nolint:gochecknoinits // registration pattern
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
