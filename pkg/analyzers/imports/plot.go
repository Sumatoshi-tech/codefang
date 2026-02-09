package imports

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
	topImportsLimit  = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
)

// ErrInvalidImports indicates the report doesn't contain expected imports data.
var ErrInvalidImports = errors.New("invalid imports report: expected Map for imports")

func (h *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := h.GenerateSections(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Import Usage Analysis",
		"Tracking dependency usage patterns over project history",
	)
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (h *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	chart, err := h.buildChart(report)
	if err != nil {
		return nil, err
	}

	return []plotpage.Section{
		{
			Title:    "Top Imports Usage",
			Subtitle: "Most frequently added imports across the codebase.",
			Chart:    plotpage.WrapChart(chart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = frequently used imports (core dependencies)",
					"External libraries = check for outdated or redundant dependencies",
					"Standard library imports = indicate code patterns",
					"Look for: Unexpected dependencies or duplicate functionality",
					"Action: Consider consolidating similar imports",
				},
			},
		},
	}, nil
}

// GenerateChart implements PlotGenerator interface.
func (h *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	return h.buildChart(report)
}

// buildChart creates a bar chart showing top imports by usage.
func (h *HistoryAnalyzer) buildChart(report analyze.Report) (*charts.Bar, error) {
	labels, data, err := extractImportsPlotData(report)
	if err != nil {
		return nil, err
	}

	if len(labels) == 0 {
		return createEmptyImportsChart(), nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createImportsBarChart(labels, data, co, palette), nil
}

// extractImportsPlotData extracts import labels and counts from the report,
// handling both in-memory and binary-decoded JSON key formats.
func extractImportsPlotData(report analyze.Report) ([]string, []int, error) {
	// Try in-memory key first (direct Map type).
	if imports, ok := report["imports"].(Map); ok {
		if len(imports) == 0 {
			return nil, nil, nil
		}
		counts := aggregateImportCounts(imports)
		labels, data := topImports(counts, topImportsLimit)
		return labels, data, nil
	}

	// Fallback: binary-decoded "import_list" from ComputedMetrics path.
	if rawList, ok := report["import_list"]; ok {
		return extractFromImportList(rawList)
	}

	// Fallback: binary-decoded raw history report where "imports" is a
	// JSON-decoded nested map with string keys (not int keys).
	if rawImports, ok := report["imports"]; ok && rawImports != nil {
		if counts := aggregateImportCountsFromJSON(rawImports); len(counts) > 0 {
			labels, data := topImports(counts, topImportsLimit)
			return labels, data, nil
		}
		// Raw history report present but empty — return empty data, not error.
		return nil, nil, nil
	}

	return nil, nil, ErrInvalidImports
}

// extractFromImportList handles the ComputedMetrics "import_list" key.
func extractFromImportList(rawList any) ([]string, []int, error) {
	if rawList == nil {
		return nil, nil, nil
	}
	importList, ok := rawList.([]any)
	if !ok {
		return nil, nil, ErrInvalidImports
	}
	if len(importList) == 0 {
		return nil, nil, nil
	}

	counts := make(map[string]int64)
	for _, item := range importList {
		m, ok := item.(map[string]any)
		if !ok {
			continue
		}
		path, _ := m["path"].(string)
		if path != "" {
			counts[path]++
		}
	}

	labels, data := topImports(counts, topImportsLimit)
	return labels, data, nil
}

// aggregateImportCountsFromJSON walks the JSON-decoded raw history imports map.
// After JSON round-trip, Map (map[int]map[string]map[string]map[int]int64) becomes
// map[string]any with string keys at every level and float64 leaf values.
func aggregateImportCountsFromJSON(raw any) map[string]int64 {
	authorMap, ok := raw.(map[string]any)
	if !ok {
		return nil
	}

	counts := make(map[string]int64)
	for _, langVal := range authorMap {
		langMap, ok := langVal.(map[string]any)
		if !ok {
			continue
		}
		for _, impVal := range langMap {
			impMap, ok := impVal.(map[string]any)
			if !ok {
				continue
			}
			for impName, tickVal := range impMap {
				tickMap, ok := tickVal.(map[string]any)
				if !ok {
					continue
				}
				for _, countVal := range tickMap {
					if c, ok := countVal.(float64); ok {
						counts[impName] += int64(c)
					}
				}
			}
		}
	}

	return counts
}

func createImportsBarChart(labels []string, data []int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
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
		charts.WithYAxisOpts(co.YAxis("Usage Count")),
	)
	bar.SetXAxis(labels)

	barData := make([]opts.BarData, len(data))
	for i, v := range data {
		barData[i] = opts.BarData{Value: v}
	}

	bar.AddSeries("Usage", barData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}))

	return bar
}

func aggregateImportCounts(imports Map) map[string]int64 {
	counts := make(map[string]int64)

	for _, langMap := range imports {
		for _, impMap := range langMap {
			for name, tickMap := range impMap {
				for _, count := range tickMap {
					counts[name] += count
				}
			}
		}
	}

	return counts
}

func topImports(counts map[string]int64, limit int) (labels []string, data []int) {
	type kv struct {
		k string
		v int64
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
		data[i] = int(item.v)
	}

	return labels, data
}

func init() {
	analyze.RegisterPlotSections("history/imports", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&HistoryAnalyzer{}).GenerateSections(report)
	})
}

func createEmptyImportsChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Top Imports", "No data")),
	)

	return bar
}
