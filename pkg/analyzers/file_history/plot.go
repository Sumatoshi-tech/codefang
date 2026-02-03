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
	chart, err := h.generateChart(report)
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
	return h.generateChart(report)
}

// generateChart creates a bar chart showing the most modified files.
func (h *Analyzer) generateChart(report analyze.Report) (*charts.Bar, error) {
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

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createFileHistoryBarChart(labels, data, co, palette), nil
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

func createEmptyFileChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Top Modified Files", "No data")),
	)

	return bar
}
