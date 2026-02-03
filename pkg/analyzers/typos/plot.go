package typos

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

// ErrInvalidTypos indicates the report doesn't contain expected typos data.
var ErrInvalidTypos = errors.New("invalid typos report: expected []Typo for typos")

func (t *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := t.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Typo Analysis",
		"Tracking typo corrections across the codebase",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (t *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := t.generateChart(report)
	if err != nil {
		return nil, err
	}

	return []plotpage.Section{
		{
			Title:    "Typo-Prone Files",
			Subtitle: "Files ranked by number of typo fixes detected in commit history.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = files where typos are frequently fixed",
					"Documentation files = expected to have more text-related fixes",
					"Code files = typos may indicate hasty commits",
					"Look for: Code files with unusually high typo rates",
					"Action: Consider adding spell-checking to pre-commit hooks",
				},
			},
		},
	}, nil
}

// GenerateChart implements PlotGenerator interface.
func (t *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return t.generateChart(report)
}

// generateChart creates a bar chart showing typo-prone files.
func (t *HistoryAnalyzer) generateChart(report analyze.Report) (*charts.Bar, error) {
	typos, ok := report["typos"].([]Typo)
	if !ok {
		return nil, ErrInvalidTypos
	}

	if len(typos) == 0 {
		return createEmptyTyposChart(), nil
	}

	counts := countTyposPerFile(typos)
	labels, data := topTypoFiles(counts, topFilesLimit)

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createTyposBarChart(labels, data, co, palette), nil
}

func createTyposBarChart(labels []string, data []int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
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
		charts.WithYAxisOpts(co.YAxis("Typo Count")),
	)
	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(data))
	for i, v := range data {
		barData[i] = opts.BarData{Value: v}
	}

	bar.AddSeries("Typos", barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Warning}))

	return bar
}

func countTyposPerFile(typos []Typo) map[string]int {
	counts := make(map[string]int)
	for _, typo := range typos {
		counts[typo.File]++
	}

	return counts
}

func topTypoFiles(counts map[string]int, limit int) (labels []string, data []int) {
	type kv struct {
		k string
		v int
	}

	var items []kv

	for k, v := range counts {
		items = append(items, kv{k, v})
	}

	sort.Slice(items, func(i, j int) bool { return items[i].v > items[j].v })

	if len(items) > limit {
		items = items[:limit]
	}

	labels = make([]string, len(items))
	data = make([]int, len(items))

	for i, item := range items {
		labels[i] = item.k
		data[i] = item.v
	}

	return labels, data
}

func createEmptyTyposChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Typo-Prone Files", "No data")),
	)

	return bar
}
