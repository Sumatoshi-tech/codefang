package typos

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

// ErrInvalidTypos indicates the report doesn't contain expected typos data.
var ErrInvalidTypos = errors.New("invalid typos report: expected []Typo for typos")

func (t *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	chart, err := t.GenerateChart(report)
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Typo Analysis",
		"Tracking typo corrections across the codebase",
	)
	page.Add(plotpage.Section{
		Title:    "Typo-Prone Files",
		Subtitle: "Files ranked by number of typo fixes detected in commit history.",
		Chart:    chart,
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
	})

	return page.Render(writer)
}

// GenerateChart creates a bar chart showing typo-prone files.
func (t *HistoryAnalyzer) GenerateChart(report analyze.Report) (*charts.Bar, error) {
	typos, ok := report["typos"].([]Typo)
	if !ok {
		return nil, ErrInvalidTypos
	}

	if len(typos) == 0 {
		return createEmptyTyposChart(), nil
	}

	counts := countTyposPerFile(typos)
	labels, data := topTypoFiles(counts, topFilesLimit)

	style := plotpage.DefaultStyle()

	return plotpage.NewBarChart(style).
		XAxis(labels, xAxisRotate).
		YAxis("Typo Count").
		Series("Typos", data, "#fac858").
		Build(), nil
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
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithTitleOpts(opts.Title{
			Title: "Typo-Prone Files", Subtitle: "No data", Left: "center",
		}),
		charts.WithInitializationOpts(opts.Initialization{Width: "1200px", Height: emptyChartHeight}),
	)

	return bar
}
