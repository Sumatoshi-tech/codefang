package devs

import (
	"errors"
	"fmt"
	"io"
	"sort"
	"strconv"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
)

const (
	plotStackName = "total"
	fullZoomPct   = 100
	maxDevs       = 20
)

// GeneratePlot writes an interactive HTML bar chart from a devs analysis report.
// It is used by both HistoryAnalyzer and the fast devs path (Libgit2Analyzer).
func GeneratePlot(report analyze.Report, writer io.Writer) error {
	chart, err := GenerateChart(report)
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
func GenerateChart(report analyze.Report) (components.Charter, error) {
	ticks, ok := report["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return nil, errors.New("expected map[int]map[int]*DevTick for ticks") //nolint:err113 // descriptive error
	}

	reversedPeopleDict, ok := report["ReversedPeopleDict"].([]string)
	if !ok {
		return nil, errors.New("expected []string for reversedPeopleDict") //nolint:err113 // descriptive error
	}

	tickKeys := getTickKeys(ticks)
	if len(tickKeys) == 0 {
		return createEmptyChart(), nil
	}

	devIDs := collectDevIDs(ticks)

	// Filter and sort top N developers.
	sortedDevs := sortDevsByTotalCommits(devIDs, ticks)
	topDevs := sortedDevs
	hasOthers := false

	if len(sortedDevs) > maxDevs {
		topDevs = sortedDevs[:maxDevs]
		hasOthers = true
	}

	xLabels := make([]string, len(tickKeys))
	for i, t := range tickKeys {
		xLabels[i] = strconv.Itoa(t)
	}

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Developer Activity History",
			Subtitle: "Commits per developer over time (Top 20)",
			Left:     "2%",
		}),
		charts.WithTooltipOpts(opts.Tooltip{Show: opts.Bool(true)}),
		charts.WithLegendOpts(opts.Legend{
			Show: opts.Bool(true),
			Type: "scroll",
			Top:  "5px",
			Left: "40%",
		}),
		charts.WithGridOpts(opts.Grid{
			Top:    "15%",
			Bottom: "10%",
			Left:   "5%",
			Right:  "5%",
		}),
		charts.WithDataZoomOpts(opts.DataZoom{Type: "slider", Start: 0, End: fullZoomPct}, opts.DataZoom{Type: "inside"}),
		charts.WithXAxisOpts(opts.XAxis{Name: "Time (tick)"}),
		charts.WithYAxisOpts(opts.YAxis{Name: "Commits"}),
	)
	bar.SetXAxis(xLabels)

	addTopDevsSeries(bar, topDevs, reversedPeopleDict, tickKeys, ticks)

	// Add "Others" series if needed.
	if hasOthers {
		addOthersSeries(bar, sortedDevs, tickKeys, ticks)
	}

	return bar, nil
}

func getTickKeys(ticks map[int]map[int]*DevTick) []int {
	tickKeys := make([]int, 0, len(ticks))
	for tick := range ticks {
		tickKeys = append(tickKeys, tick)
	}

	sort.Ints(tickKeys)

	return tickKeys
}

func addTopDevsSeries(
	bar *charts.Bar, topDevs []int, reversedPeopleDict []string, tickKeys []int, ticks map[int]map[int]*DevTick,
) {
	for _, devID := range topDevs {
		name := devName(devID, reversedPeopleDict)
		data := make([]opts.BarData, len(tickKeys))

		for i, tick := range tickKeys {
			devTick := ticks[tick][devID]
			val := 0

			if devTick != nil {
				val = devTick.Commits
			}

			data[i] = opts.BarData{Value: val}
		}

		bar.AddSeries(name, data, charts.WithBarChartOpts(opts.BarChart{Stack: plotStackName}))
	}
}

func addOthersSeries(bar *charts.Bar, sortedDevs, tickKeys []int, ticks map[int]map[int]*DevTick) {
	data := make([]opts.BarData, len(tickKeys))
	otherDevs := sortedDevs[maxDevs:]

	for i, tick := range tickKeys {
		total := 0

		for _, devID := range otherDevs {
			devTick := ticks[tick][devID]
			if devTick != nil {
				total += devTick.Commits
			}
		}

		data[i] = opts.BarData{Value: total}
	}

	bar.AddSeries("Others", data, charts.WithBarChartOpts(opts.BarChart{Stack: plotStackName}))
}

func sortDevsByTotalCommits(devIDs []int, ticks map[int]map[int]*DevTick) []int {
	totals := make(map[int]int)

	for _, devID := range devIDs {
		sum := 0

		for _, tickMap := range ticks {
			if dt, ok := tickMap[devID]; ok {
				sum += dt.Commits
			}
		}

		totals[devID] = sum
	}

	sorted := make([]int, len(devIDs))
	copy(sorted, devIDs)

	sort.Slice(sorted, func(i, j int) bool {
		return totals[sorted[i]] > totals[sorted[j]]
	})

	return sorted
}

func collectDevIDs(ticks map[int]map[int]*DevTick) []int {
	seen := make(map[int]bool)

	for _, devTick := range ticks {
		for devID := range devTick {
			seen[devID] = true
		}
	}

	ids := make([]int, 0, len(seen))
	for id := range seen {
		ids = append(ids, id)
	}

	return ids
}

func devName(devID int, reversedPeopleDict []string) string {
	if devID == identity.AuthorMissing {
		return identity.AuthorMissingName
	}

	if devID >= 0 && devID < len(reversedPeopleDict) {
		return reversedPeopleDict[devID]
	}

	return fmt.Sprintf("dev_%d", devID)
}

func (d *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	return GeneratePlot(report, writer)
}

// GenerateChart creates the chart object from the report.
func (d *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return GenerateChart(report)
}

func createEmptyChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title:    "Developer Activity History",
			Subtitle: "No data (empty repository or time range)",
		}),
	)
	bar.SetXAxis([]string{})

	return bar
}
