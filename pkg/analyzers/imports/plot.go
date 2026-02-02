package imports

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
	topImportsLimit  = 20
	xAxisRotate      = 60
	emptyChartHeight = "400px"
)

// ErrInvalidImports indicates the report doesn't contain expected imports data.
var ErrInvalidImports = errors.New("invalid imports report: expected Map for imports")

func (h *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := h.GenerateChart(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Import Usage Analysis",
		"Tracking dependency usage patterns over project history",
	)
	page.Add(plotpage.Section{
		Title:    "Top Imports Usage",
		Subtitle: "Most frequently added imports across the codebase.",
		Chart:    chart,
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
	})

	return page.Render(writer)
}

// GenerateChart creates a bar chart showing top imports by usage.
func (h *HistoryAnalyzer) GenerateChart(report analyze.Report) (*charts.Bar, error) {
	imports, ok := report["imports"].(Map)
	if !ok {
		return nil, ErrInvalidImports
	}

	if len(imports) == 0 {
		return createEmptyImportsChart(), nil
	}

	counts := aggregateImportCounts(imports)
	labels, data := topImports(counts, topImportsLimit)

	style := plotpage.DefaultStyle()

	return plotpage.NewBarChart(style).
		XAxis(labels, xAxisRotate).
		YAxis("Usage Count").
		Series("Usage", data, "#5470c6").
		Build(), nil
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

func createEmptyImportsChart() *charts.Bar {
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Top Imports", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return bar
}
