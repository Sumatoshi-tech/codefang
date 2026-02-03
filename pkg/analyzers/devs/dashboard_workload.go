package devs

import (
	"io"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

type workloadContent struct {
	chart *charts.TreeMap
}

func createWorkloadTab(data *DashboardData) *workloadContent {
	return &workloadContent{chart: createWorkloadTreemap(data)}
}

// Render implements the Renderable interface for the workload tab.
func (wc *workloadContent) Render(w io.Writer) error {
	if wc.chart == nil {
		return plotpage.NewText("No workload data available").Render(w)
	}

	return plotpage.WrapChart(wc.chart).Render(w)
}

func createWorkloadTreemap(data *DashboardData) *charts.TreeMap {
	if len(data.DevSummaries) == 0 {
		return nil
	}

	rootNodes := buildTreemapNodes(data)

	co := plotpage.DefaultChartOpts()

	tm := charts.NewTreeMap()
	tm.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", treemapHeight)),
		charts.WithTitleOpts(co.Title("Workload Distribution", "Developers grouped by primary language, sized by commits")),
		charts.WithTooltipOpts(co.Tooltip("item")),
	)

	tm.AddSeries("Workload", rootNodes, charts.WithTreeMapOpts(opts.TreeMapChart{
		Animation:      opts.Bool(true),
		Roam:           opts.Bool(true),
		LeafDepth:      treemapLeafDepth,
		ColorMappingBy: "value",
		Label:          &opts.Label{Show: opts.Bool(true), Formatter: "{b}"},
		UpperLabel:     &opts.UpperLabel{Show: opts.Bool(true)},
		Levels: &[]opts.TreeMapLevel{
			{
				ItemStyle:  &opts.ItemStyle{BorderColor: "#555", BorderWidth: borderWidth, GapWidth: gapWidth},
				UpperLabel: &opts.UpperLabel{Show: opts.Bool(true)},
			},
			{
				ItemStyle:       &opts.ItemStyle{BorderColor: "#999", BorderWidth: 1, GapWidth: 1},
				ColorSaturation: []float32{0.3, 0.6},
			},
		},
		Left: "2%", Right: "2%", Top: "15%", Bottom: "2%",
	}))

	return tm
}

func buildTreemapNodes(data *DashboardData) []opts.TreeMapNode {
	langDevs := make(map[string][]opts.TreeMapNode)
	langTotals := make(map[string]int)

	count := min(topDevsForTreemap, len(data.DevSummaries))

	for i := range count {
		ds := data.DevSummaries[i]
		primaryLang := findPrimaryLanguage(ds)

		langDevs[primaryLang] = append(langDevs[primaryLang], opts.TreeMapNode{
			Name:  ds.Name,
			Value: ds.Commits,
		})
		langTotals[primaryLang] += ds.Commits
	}

	rootNodes := make([]opts.TreeMapNode, 0, len(langDevs))

	for lang, devNodes := range langDevs {
		sort.Slice(devNodes, func(i, j int) bool {
			return devNodes[i].Value > devNodes[j].Value
		})

		rootNodes = append(rootNodes, opts.TreeMapNode{
			Name:     lang,
			Value:    langTotals[lang],
			Children: devNodes,
		})
	}

	sort.Slice(rootNodes, func(i, j int) bool {
		return rootNodes[i].Value > rootNodes[j].Value
	})

	return rootNodes
}

func findPrimaryLanguage(ds DeveloperSummary) string {
	primaryLang := langOther
	maxLines := 0

	for lang, stats := range ds.Languages {
		if stats.Added > maxLines {
			maxLines = stats.Added
			primaryLang = lang

			if primaryLang == "" {
				primaryLang = langOther
			}
		}
	}

	return primaryLang
}
