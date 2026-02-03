package shotness

import (
	"errors"
	"fmt"
	"io"
	"path/filepath"
	"sort"

	"github.com/go-echarts/go-echarts/v2/charts"
	"github.com/go-echarts/go-echarts/v2/components"
	"github.com/go-echarts/go-echarts/v2/opts"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const (
	topNNodes        = 20
	maxFiles         = 30
	rotateDegrees    = 60
	labelFontSize    = 10
	innerLabelSize   = 9
	treeMapHeight    = "550px"
	heatMapHeight    = "650px"
	emptyChartHeight = "400px"
	treeMapLeafDepth = 2
	borderWidth1     = 1
	borderWidth2     = 2
	minHeatMapNodes  = 2
)

// ErrInvalidNodes indicates the report doesn't contain expected nodes data.
var ErrInvalidNodes = errors.New("invalid shotness report: expected []NodeSummary for Nodes")

// ErrInvalidCounters indicates the report doesn't contain expected counters data.
var ErrInvalidCounters = errors.New("invalid shotness report: expected []map[int]int for Counters")

func (s *HistoryAnalyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := s.GenerateSections(report)
	if err != nil {
		return err
	}

	if len(sections) == 0 {
		renderErr := createEmptyChart().Render(writer)
		if renderErr != nil {
			return fmt.Errorf("render empty chart: %w", renderErr)
		}

		return nil
	}

	page := plotpage.NewPage(
		"Shotness Analysis",
		"Fine-grained analysis of code change patterns at the function/method level",
	)

	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (s *HistoryAnalyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	nodes, counters, err := extractShotnessData(report)
	if err != nil {
		return nil, err
	}

	if len(nodes) == 0 {
		return nil, nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return []plotpage.Section{
		treeMapSection(nodes, counters, co),
		heatMapSection(nodes, counters, co),
		barChartSection(nodes, counters, co, palette),
	}, nil
}

// GenerateChart creates a bar chart showing the hottest functions.
func (s *HistoryAnalyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	nodes, counters, err := extractShotnessData(report)
	if err != nil {
		return nil, err
	}

	if len(nodes) == 0 {
		return createEmptyChart(), nil
	}

	co := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createBarChart(nodes, counters, co, palette), nil
}

func extractShotnessData(report analyze.Report) ([]NodeSummary, []map[int]int, error) {
	nodes, ok := report["Nodes"].([]NodeSummary)
	if !ok {
		return nil, nil, ErrInvalidNodes
	}

	counters, ok := report["Counters"].([]map[int]int)
	if !ok {
		return nil, nil, ErrInvalidCounters
	}

	return nodes, counters, nil
}

func treeMapSection(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts) plotpage.Section {
	return plotpage.Section{
		Title:    "Code Hotness TreeMap",
		Subtitle: "Hierarchical view: Files -> Functions. Rectangle size = change frequency.",
		Chart:    plotpage.WrapChart(createTreeMap(nodes, counters, co)),
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"Large rectangles = frequently changed code (potential maintenance burden)",
				"Color intensity = relative hotness within the file",
				"Click on a file to drill down and see individual functions",
				"Look for: Small files with many hot functions",
			},
		},
	}
}

func heatMapSection(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts) plotpage.Section {
	return plotpage.Section{
		Title:    "Function Coupling Matrix",
		Subtitle: "Co-change frequency between functions. Diagonal = self, off-diagonal = coupled.",
		Chart:    plotpage.WrapChart(createHeatMap(nodes, counters, co)),
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"Diagonal (dark green) = how often each function changes independently",
				"Off-diagonal cells = functions that change together in same commits",
				"High off-diagonal = tight coupling (may indicate hidden dependency)",
				"Look for: Functions from different files changing together",
			},
		},
	}
}

func barChartSection(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) plotpage.Section {
	return plotpage.Section{
		Title:    "Top Hot Functions",
		Subtitle: "Ranking of most frequently changed functions with coupling information.",
		Chart:    plotpage.WrapChart(createBarChart(nodes, counters, co, palette)),
		Hint: plotpage.Hint{
			Title: "How to interpret:",
			Items: []string{
				"Blue bars (Self Changes) = direct modifications to this function",
				"Green bars (Coupled Changes) = changes alongside other functions",
				"High blue + low green = isolated changes (frequently bugfixed)",
				"High blue + high green = central/core function affecting many others",
				"Action: Top functions are candidates for additional test coverage",
			},
		},
	}
}

func createTreeMap(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts) *charts.TreeMap {
	fileMap, fileTotals := buildFileHierarchy(nodes, counters)
	rootNodes := buildRootNodes(fileMap, fileTotals)

	tm := charts.NewTreeMap()
	tm.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init("100%", treeMapHeight)),
	)
	tm.AddSeries("Hotness", rootNodes, charts.WithTreeMapOpts(opts.TreeMapChart{
		Animation:      opts.Bool(true),
		Roam:           opts.Bool(true),
		LeafDepth:      treeMapLeafDepth,
		ColorMappingBy: "value",
		Label:          &opts.Label{Show: opts.Bool(true), Formatter: "{b}", Color: co.TextColor()},
		UpperLabel:     &opts.UpperLabel{Show: opts.Bool(true), Color: co.TextColor()},
		Levels: &[]opts.TreeMapLevel{
			{
				ItemStyle:  &opts.ItemStyle{BorderColor: co.GridColor(), BorderWidth: borderWidth2, GapWidth: borderWidth2},
				UpperLabel: &opts.UpperLabel{Show: opts.Bool(true)},
			},
			{
				ItemStyle:       &opts.ItemStyle{BorderColor: co.AxisColor(), BorderWidth: borderWidth1, GapWidth: borderWidth1},
				ColorSaturation: []float32{0.3, 0.6},
			},
		},
		Left: "2%", Right: "2%", Top: "10", Bottom: "2%",
	}))

	return tm
}

func buildFileHierarchy(
	nodes []NodeSummary,
	counters []map[int]int,
) (fileMap map[string][]opts.TreeMapNode, fileTotals map[string]int) {
	fileMap = make(map[string][]opts.TreeMapNode)
	fileTotals = make(map[string]int)

	for i, node := range nodes {
		count := counters[i][i]
		fileMap[node.File] = append(fileMap[node.File], opts.TreeMapNode{
			Name:  node.Name,
			Value: count,
		})
		fileTotals[node.File] += count
	}

	return fileMap, fileTotals
}

func buildRootNodes(fileMap map[string][]opts.TreeMapNode, fileTotals map[string]int) []opts.TreeMapNode {
	var rootNodes []opts.TreeMapNode

	for file, children := range fileMap {
		sort.Slice(children, func(i, j int) bool {
			return children[i].Value > children[j].Value
		})
		rootNodes = append(rootNodes, opts.TreeMapNode{
			Name:     filepath.Base(file),
			Value:    fileTotals[file],
			Children: children,
		})
	}

	sort.Slice(rootNodes, func(i, j int) bool {
		return rootNodes[i].Value > rootNodes[j].Value
	})

	if len(rootNodes) > maxFiles {
		rootNodes = rootNodes[:maxFiles]
	}

	return rootNodes
}

func createHeatMap(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts) *charts.HeatMap {
	actives := getActiveNodes(nodes, counters)
	if len(actives) < minHeatMapNodes {
		return nil
	}

	names := extractNames(actives)
	data, maxVal := buildHeatMapData(actives, counters)

	hm := charts.NewHeatMap()
	hm.SetGlobalOptions(
		charts.WithTooltipOpts(co.Tooltip("item")),
		charts.WithInitializationOpts(co.Init("100%", heatMapHeight)),
		charts.WithXAxisOpts(opts.XAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{Rotate: rotateDegrees, Interval: "0", FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize, Color: co.TextMutedColor()},
		}),
		charts.WithVisualMapOpts(opts.VisualMap{
			Calculable: opts.Bool(true), Min: 0, Max: float32(maxVal),
			InRange: &opts.VisualMapInRange{Color: []string{"#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"}},
			Orient:  "horizontal", Left: "center", Bottom: "2%",
			TextStyle: &opts.TextStyle{Color: co.TextMutedColor()},
		}),
		charts.WithGridOpts(opts.Grid{
			Left: "20%", Right: "5%", Top: "40", Bottom: "20%",
		}),
	)
	hm.AddSeries("Coupling", data, charts.WithLabelOpts(opts.Label{
		Show: opts.Bool(true), Position: "inside", Color: "black", FontSize: innerLabelSize,
	}))

	return hm
}

type activeNode struct {
	idx   int
	name  string
	count int
}

func getActiveNodes(nodes []NodeSummary, counters []map[int]int) []activeNode {
	var actives []activeNode

	for i, counter := range counters {
		if counter[i] > 0 {
			actives = append(actives, activeNode{i, nodes[i].Name, counter[i]})
		}
	}

	sort.Slice(actives, func(i, j int) bool { return actives[i].count > actives[j].count })

	if len(actives) > topNNodes {
		actives = actives[:topNNodes]
	}

	return actives
}

func extractNames(actives []activeNode) []string {
	names := make([]string, len(actives))
	for i, a := range actives {
		names[i] = a.name
	}

	return names
}

func buildHeatMapData(
	actives []activeNode,
	counters []map[int]int,
) (data []opts.HeatMapData, maxVal float64) {
	data = make([]opts.HeatMapData, 0, len(actives)*len(actives))

	for i, ai := range actives {
		for j, aj := range actives {
			var val int

			if i == j {
				val = counters[ai.idx][ai.idx]
			} else {
				val = counters[ai.idx][aj.idx]
			}

			data = append(data, opts.HeatMapData{Value: []any{i, j, val}})

			if float64(val) > maxVal {
				maxVal = float64(val)
			}
		}
	}

	if maxVal == 0 {
		maxVal = 1
	}

	return data, maxVal
}

func createBarChart(nodes []NodeSummary, counters []map[int]int, co *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
	scores := computeScores(nodes, counters)
	labels, selfData, coupledData := buildBarData(scores)

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", "500px")),
		charts.WithTooltipOpts(co.Tooltip("axis")),
		charts.WithLegendOpts(co.Legend()),
		charts.WithGridOpts(co.Grid()),
		charts.WithDataZoomOpts(co.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   rotateDegrees,
				Interval: "0",
				Color:    co.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: co.AxisColor()}},
		}),
		charts.WithYAxisOpts(co.YAxis("Count")),
	)
	bar.SetXAxis(labels)

	selfBarData := make([]opts.BarData, len(selfData))
	for i, v := range selfData {
		selfBarData[i] = opts.BarData{Value: v}
	}

	coupledBarData := make([]opts.BarData, len(coupledData))
	for i, v := range coupledData {
		coupledBarData[i] = opts.BarData{Value: v}
	}

	bar.AddSeries("Self Changes", selfBarData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Primary[1]}))
	bar.AddSeries("Coupled Changes", coupledBarData, charts.WithItemStyleOpts(opts.ItemStyle{Color: palette.Semantic.Good}))

	return bar
}

type nodeScore struct {
	name    string
	self    int
	coupled int
}

func computeScores(nodes []NodeSummary, counters []map[int]int) []nodeScore {
	scores := make([]nodeScore, len(nodes))

	for i, counter := range counters {
		var coupled int

		for j, val := range counter {
			if j != i && val > 0 {
				coupled += val
			}
		}

		scores[i] = nodeScore{nodes[i].Name, counter[i], coupled}
	}

	sort.Slice(scores, func(i, j int) bool { return scores[i].self > scores[j].self })

	if len(scores) > topNNodes {
		scores = scores[:topNNodes]
	}

	return scores
}

func buildBarData(scores []nodeScore) (labels []string, selfData, coupledData []int) {
	labels = make([]string, len(scores))
	selfData = make([]int, len(scores))
	coupledData = make([]int, len(scores))

	for i, score := range scores {
		labels[i] = score.name
		selfData[i] = score.self
		coupledData[i] = score.coupled
	}

	return labels, selfData, coupledData
}

func createEmptyChart() *charts.Bar {
	co := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(co.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(co.Title("Shotness Analysis", "No data - ensure UAST parsing is configured")),
	)

	return bar
}
