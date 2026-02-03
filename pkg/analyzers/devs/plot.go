package devs

import (
	"fmt"
	"io"
	"slices"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
)

const (
	maxDevs          = 20
	chartHeight      = "550px"
	emptyChartHeight = "400px"
	xAxisRotate      = 45
	labelFontSize    = 10
)

// GeneratePlot creates a full dashboard page for developer analytics.
// This is the main entry point for visualization, providing a comprehensive
// multi-tab dashboard with activity trends, workload distribution, language
// expertise, bus factor analysis, and code churn metrics.
func GeneratePlot(report analyze.Report, writer io.Writer) error {
	return GenerateDashboard(report, writer)
}

// GenerateChart creates a stacked bar chart showing developer activity over time.
func GenerateChart(report analyze.Report) (components.Charter, error) {
	ticks, ok := report["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return nil, ErrInvalidTicks
	}

	names, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil, ErrInvalidPeopleDict
	}

	tickKeys := sortedKeys(ticks)
	if len(tickKeys) == 0 {
		return createEmptyBar(), nil
	}

	co := plotpage.DefaultChartOpts()
	devTotals := computeDevTotals(ticks)
	topDevs := topNByValue(devTotals, maxDevs)
	xLabels := buildXLabels(tickKeys)

	bar := createBarChart(co)
	bar.SetXAxis(xLabels)
	addDevSeries(bar, topDevs, tickKeys, ticks, names)

	if len(devTotals) > maxDevs {
		addOthersSeries(bar, topDevs, tickKeys, ticks)
	}

	return bar, nil
}

func (d *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	return GenerateDashboard(report, writer)
}

// GenerateChart creates a chart for the history analyzer.
func (d *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return GenerateChart(report)
}

// GenerateSections returns the dashboard sections for combined reports.
func (d *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	return GenerateSections(report)
}

// GenerateDashboardForAnalyzer creates the full dashboard for this analyzer.
func (d *HistoryAnalyzer) GenerateDashboardForAnalyzer(report analyze.Report, writer io.Writer) error {
	return GenerateDashboard(report, writer)
}

func computeDevTotals(ticks map[int]map[int]*DevTick) map[int]int {
	devTotals := make(map[int]int)

	for _, tickMap := range ticks {
		for devID, devTick := range tickMap {
			devTotals[devID] += devTick.Commits
		}
	}

	return devTotals
}

func buildXLabels(tickKeys []int) []string {
	xLabels := make([]string, len(tickKeys))
	for i, t := range tickKeys {
		xLabels[i] = strconv.Itoa(t)
	}

	return xLabels
}

func createBarChart(co *plotpage.ChartOpts) *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", chartHeight)),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithGridOpts(co.Grid()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			Name: "Time (tick)",
			AxisLabel: &opts.AxisLabel{
				Rotate:   xAxisRotate,
				FontSize: labelFontSize,
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(co.YAxis("Commits")),
	)

	return bar
}

func addDevSeries(bar *charts.Bar, topDevs, tickKeys []int, ticks map[int]map[int]*DevTick, names []string) {
	for _, devID := range topDevs {
		data := make([]opts.BarData, len(tickKeys))

		for i, tick := range tickKeys {
			val := 0
			if devTick := ticks[tick][devID]; devTick != nil {
				val = devTick.Commits
			}

			data[i] = opts.BarData{Value: val}
		}

		bar.AddSeries(devName(devID, names), data, charts.WithBarChartOpts(opts.BarChart{Stack: "total"}))
	}
}

func addOthersSeries(bar *charts.Bar, topDevs, tickKeys []int, ticks map[int]map[int]*DevTick) {
	others := make([]opts.BarData, len(tickKeys))

	for i, tick := range tickKeys {
		total := 0

		for devID, devTick := range ticks[tick] {
			if !slices.Contains(topDevs, devID) {
				total += devTick.Commits
			}
		}

		others[i] = opts.BarData{Value: total}
	}

	bar.AddSeries("Others", others, charts.WithBarChartOpts(opts.BarChart{Stack: "total"}))
}

func sortedKeys(m map[int]map[int]*DevTick) []int {
	keys := make([]int, 0, len(m))

	for k := range m {
		keys = append(keys, k)
	}

	sort.Ints(keys)

	return keys
}

func topNByValue(totals map[int]int, count int) []int {
	type kv struct {
		k, v int
	}

	var items []kv

	for k, v := range totals {
		items = append(items, kv{k, v})
	}

	sort.Slice(items, func(i, j int) bool { return items[i].v > items[j].v })

	if len(items) > count {
		items = items[:count]
	}

	result := make([]int, len(items))

	for i, item := range items {
		result[i] = item.k
	}

	return result
}

func devName(id int, names []string) string {
	if id == identity.AuthorMissing {
		return identity.AuthorMissingName
	}

	if id >= 0 && id < len(names) {
		return names[id]
	}

	return fmt.Sprintf("dev_%d", id)
}

func createEmptyBar() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Developer Activity", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)
	bar.SetXAxis([]string{})

	return bar
}
