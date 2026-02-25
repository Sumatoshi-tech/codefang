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

// RegisterPlotSections registers the shotness plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterPlotSections("history/shotness", func(report analyze.Report) ([]plotpage.Section, error) {
		return (&Analyzer{}).GenerateSections(report)
	})
}

func (s *Analyzer) generatePlot(report analyze.Report, writer io.Writer) error {
	sections, err := s.GenerateSections(report)
	if err != nil {
		return fmt.Errorf("generate sections: %w", err)
	}

	page := plotpage.NewPage("Shotness Analysis", "Function-level change frequency and coupling")
	page.Add(sections...)

	return page.Render(writer)
}

// GenerateSections returns the sections for combined reports.
func (s *Analyzer) GenerateSections(report analyze.Report) ([]plotpage.Section, error) {
	nodes, counters, err := extractShotnessData(report)
	if err != nil {
		return nil, err
	}

	if len(nodes) == 0 {
		return nil, nil
	}

	chartOpts := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return []plotpage.Section{
		treeMapSection(nodes, counters, chartOpts),
		heatMapSection(nodes, counters, chartOpts),
		barChartSection(nodes, counters, chartOpts, palette),
	}, nil
}

// GenerateChart creates a bar chart showing the hottest functions.
func (s *Analyzer) GenerateChart(report analyze.Report) (components.Charter, error) {
	nodes, counters, err := extractShotnessData(report)
	if err != nil {
		return nil, err
	}

	if len(nodes) == 0 {
		return createEmptyChart(), nil
	}

	chartOpts := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return createBarChart(nodes, counters, chartOpts, palette), nil
}

func extractShotnessData(report analyze.Report) ([]NodeSummary, []map[int]int, error) {
	nodes, nodesOK := report["Nodes"].([]NodeSummary)
	counters, countersOK := report["Counters"].([]map[int]int)

	if nodesOK && countersOK {
		return nodes, counters, nil
	}

	return extractShotnessFromJSON(report, nodesOK)
}

// extractShotnessFromJSON handles the fallback path where, after binary encode -> JSON decode,
// "Nodes" becomes "node_hotness" and "Counters" becomes "node_coupling".
func extractShotnessFromJSON(report analyze.Report, nodesOK bool) ([]NodeSummary, []map[int]int, error) {
	rawHotness, hotnessOK := report["node_hotness"]
	if !hotnessOK {
		if !nodesOK {
			return nil, nil, ErrInvalidNodes
		}

		return nil, nil, ErrInvalidCounters
	}

	if rawHotness == nil {
		return nil, nil, nil
	}

	hotnessList, listOK := rawHotness.([]any)
	if !listOK {
		return nil, nil, ErrInvalidNodes
	}

	if len(hotnessList) == 0 {
		return nil, nil, nil
	}

	nodes, counters, nameToIdx := buildNodesFromHotness(hotnessList)
	applyCouplingData(report, counters, nameToIdx)

	return nodes, counters, nil
}

// buildNodesFromHotness builds NodeSummary slices and self-counts from the node_hotness list.
func buildNodesFromHotness(hotnessList []any) (nodes []NodeSummary, counters []map[int]int, nameToIdx map[string]int) {
	nodes = make([]NodeSummary, len(hotnessList))
	counters = make([]map[int]int, len(hotnessList))
	nameToIdx = make(map[string]int, len(hotnessList))

	for idx, item := range hotnessList {
		entry, entryOK := item.(map[string]any)
		if !entryOK {
			continue
		}

		nodeName, _ := assertString(entry, "name")
		nodeType, _ := assertString(entry, "type")
		nodeFile, _ := assertString(entry, "file")
		changeCount := shotnessToInt(entry["change_count"])

		nodes[idx] = NodeSummary{Name: nodeName, Type: nodeType, File: nodeFile}
		counters[idx] = map[int]int{idx: changeCount}
		nameToIdx[nodeName] = idx
	}

	return nodes, counters, nameToIdx
}

// assertString safely extracts a string value from a map entry.
func assertString(entry map[string]any, key string) (string, bool) {
	val, valOK := entry[key].(string)

	return val, valOK
}

// applyCouplingData fills cross-node coupling from the node_coupling report entry.
func applyCouplingData(report analyze.Report, counters []map[int]int, nameToIdx map[string]int) {
	rawCoupling, couplingExists := report["node_coupling"]
	if !couplingExists {
		return
	}

	couplingList, couplingOK := rawCoupling.([]any)
	if !couplingOK {
		return
	}

	for _, item := range couplingList {
		entry, entryOK := item.(map[string]any)
		if !entryOK {
			continue
		}

		node1Name, _ := assertString(entry, "node1_name")
		node2Name, _ := assertString(entry, "node2_name")
		coChanges := shotnessToInt(entry["co_changes"])

		idx1, found1 := nameToIdx[node1Name]
		idx2, found2 := nameToIdx[node2Name]

		if found1 && found2 && coChanges > 0 {
			counters[idx1][idx2] = coChanges
			counters[idx2][idx1] = coChanges
		}
	}
}

// shotnessToInt converts a numeric value (int, float64) to int.
func shotnessToInt(val any) int {
	switch num := val.(type) {
	case float64:
		return int(num)
	case int:
		return num
	case int64:
		return int(num)
	default:
		return 0
	}
}

func treeMapSection(nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts) plotpage.Section {
	return plotpage.Section{
		Title:    "Code Hotness TreeMap",
		Subtitle: "Hierarchical view: Files -> Functions. Rectangle size = change frequency.",
		Chart:    plotpage.WrapChart(createTreeMap(nodes, counters, chartOpts)),
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

func heatMapSection(nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts) plotpage.Section {
	return plotpage.Section{
		Title:    "Function Coupling Matrix",
		Subtitle: "Co-change frequency between functions. Diagonal = self, off-diagonal = coupled.",
		Chart:    plotpage.WrapChart(createHeatMap(nodes, counters, chartOpts)),
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

func barChartSection(
	nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts, palette plotpage.ChartPalette,
) plotpage.Section {
	return plotpage.Section{
		Title:    "Top Hot Functions",
		Subtitle: "Ranking of most frequently changed functions with coupling information.",
		Chart:    plotpage.WrapChart(createBarChart(nodes, counters, chartOpts, palette)),
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

func createTreeMap(nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts) *charts.TreeMap {
	fileMap, fileTotals := buildFileHierarchy(nodes, counters)
	rootNodes := buildRootNodes(fileMap, fileTotals)

	treeMap := charts.NewTreeMap()
	treeMap.SetGlobalOptions(
		charts.WithTooltipOpts(chartOpts.Tooltip("item")),
		charts.WithInitializationOpts(chartOpts.Init("100%", treeMapHeight)),
	)
	treeMap.AddSeries("Hotness", rootNodes, charts.WithTreeMapOpts(opts.TreeMapChart{
		Animation:      opts.Bool(true),
		Roam:           opts.Bool(true),
		LeafDepth:      treeMapLeafDepth,
		ColorMappingBy: "value",
		Label:          &opts.Label{Show: opts.Bool(true), Formatter: "{b}", Color: chartOpts.TextColor()},
		UpperLabel:     &opts.UpperLabel{Show: opts.Bool(true), Color: chartOpts.TextColor()},
		Levels: &[]opts.TreeMapLevel{
			{
				ItemStyle:  &opts.ItemStyle{BorderColor: chartOpts.GridColor(), BorderWidth: borderWidth2, GapWidth: borderWidth2},
				UpperLabel: &opts.UpperLabel{Show: opts.Bool(true)},
			},
			{
				ItemStyle:       &opts.ItemStyle{BorderColor: chartOpts.AxisColor(), BorderWidth: borderWidth1, GapWidth: borderWidth1},
				ColorSaturation: []float32{0.3, 0.6},
			},
		},
		Left: "2%", Right: "2%", Top: "10", Bottom: "2%",
	}))

	return treeMap
}

func buildFileHierarchy(
	nodes []NodeSummary,
	counters []map[int]int,
) (fileMap map[string][]opts.TreeMapNode, fileTotals map[string]int) {
	fileMap = make(map[string][]opts.TreeMapNode)
	fileTotals = make(map[string]int)

	for idx, node := range nodes {
		count := counters[idx][idx]
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
		sort.Slice(children, func(idx1, idx2 int) bool {
			return children[idx1].Value > children[idx2].Value
		})

		rootNodes = append(rootNodes, opts.TreeMapNode{
			Name:     filepath.Base(file),
			Value:    fileTotals[file],
			Children: children,
		})
	}

	sort.Slice(rootNodes, func(idx1, idx2 int) bool {
		return rootNodes[idx1].Value > rootNodes[idx2].Value
	})

	if len(rootNodes) > maxFiles {
		rootNodes = rootNodes[:maxFiles]
	}

	return rootNodes
}

func createHeatMap(nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts) *charts.HeatMap {
	actives := getActiveNodes(nodes, counters)

	if len(actives) < minHeatMapNodes {
		return nil
	}

	names := extractNames(actives)
	data, maxVal := buildHeatMapData(actives, counters)

	heatMap := charts.NewHeatMap()
	heatMap.SetGlobalOptions(
		charts.WithTooltipOpts(chartOpts.Tooltip("item")),
		charts.WithInitializationOpts(chartOpts.Init("100%", heatMapHeight)),
		charts.WithXAxisOpts(opts.XAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{Rotate: rotateDegrees, Interval: "0", FontSize: labelFontSize, Color: chartOpts.TextMutedColor()},
		}),
		charts.WithYAxisOpts(opts.YAxis{
			Type: "category", Data: names,
			SplitArea: &opts.SplitArea{Show: opts.Bool(true)},
			AxisLabel: &opts.AxisLabel{FontSize: labelFontSize, Color: chartOpts.TextMutedColor()},
		}),
		charts.WithVisualMapOpts(opts.VisualMap{
			Calculable: opts.Bool(true), Min: 0, Max: float32(maxVal),
			InRange: &opts.VisualMapInRange{Color: []string{"#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"}},
			Orient:  "horizontal", Left: "center", Bottom: "2%",
			TextStyle: &opts.TextStyle{Color: chartOpts.TextMutedColor()},
		}),
		charts.WithGridOpts(opts.Grid{
			Left: "20%", Right: "5%", Top: "40", Bottom: "20%",
		}),
	)
	heatMap.AddSeries("Coupling", data, charts.WithLabelOpts(opts.Label{
		Show: opts.Bool(true), Position: "inside", Color: "black", FontSize: innerLabelSize,
	}))

	return heatMap
}

type activeNode struct {
	idx   int
	name  string
	count int
}

func getActiveNodes(nodes []NodeSummary, counters []map[int]int) []activeNode {
	var actives []activeNode

	for idx, counter := range counters {
		if counter[idx] > 0 {
			actives = append(actives, activeNode{idx, nodes[idx].Name, counter[idx]})
		}
	}

	sort.Slice(actives, func(idx1, idx2 int) bool { return actives[idx1].count > actives[idx2].count })

	if len(actives) > topNNodes {
		actives = actives[:topNNodes]
	}

	return actives
}

func extractNames(actives []activeNode) []string {
	names := make([]string, len(actives))

	for idx, active := range actives {
		names[idx] = active.name
	}

	return names
}

func buildHeatMapData(
	actives []activeNode,
	counters []map[int]int,
) (data []opts.HeatMapData, maxVal float64) {
	data = make([]opts.HeatMapData, 0, len(actives)*len(actives))

	for row, rowActive := range actives {
		for col, colActive := range actives {
			var val int

			if row == col {
				val = counters[rowActive.idx][rowActive.idx]
			} else {
				val = counters[rowActive.idx][colActive.idx]
			}

			data = append(data, opts.HeatMapData{Value: []any{row, col, val}})

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

func createBarChart(nodes []NodeSummary, counters []map[int]int, chartOpts *plotpage.ChartOpts, palette plotpage.ChartPalette) *charts.Bar {
	scores := computeScores(nodes, counters)
	labels, selfData, coupledData := buildBarData(scores)

	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(chartOpts.Init("100%", "500px")),
		charts.WithTooltipOpts(chartOpts.Tooltip("axis")),
		charts.WithLegendOpts(chartOpts.Legend()),
		charts.WithGridOpts(chartOpts.Grid()),
		charts.WithDataZoomOpts(chartOpts.DataZoom()...),
		charts.WithXAxisOpts(opts.XAxis{
			AxisLabel: &opts.AxisLabel{
				Rotate:   rotateDegrees,
				Interval: "0",
				Color:    chartOpts.TextMutedColor(),
			},
			AxisLine: &opts.AxisLine{LineStyle: &opts.LineStyle{Color: chartOpts.AxisColor()}},
		}),
		charts.WithYAxisOpts(chartOpts.YAxis("Count")),
	)
	bar.SetXAxis(labels)

	selfBarData := make([]opts.BarData, len(selfData))

	for idx, value := range selfData {
		selfBarData[idx] = opts.BarData{Value: value}
	}

	coupledBarData := make([]opts.BarData, len(coupledData))

	for idx, value := range coupledData {
		coupledBarData[idx] = opts.BarData{Value: value}
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

	for idx, counter := range counters {
		var coupled int

		for other, val := range counter {
			if other != idx && val > 0 {
				coupled += val
			}
		}

		scores[idx] = nodeScore{nodes[idx].Name, counter[idx], coupled}
	}

	sort.Slice(scores, func(idx1, idx2 int) bool { return scores[idx1].self > scores[idx2].self })

	if len(scores) > topNNodes {
		scores = scores[:topNNodes]
	}

	return scores
}

func buildBarData(scores []nodeScore) (labels []string, selfData, coupledData []int) {
	labels = make([]string, len(scores))
	selfData = make([]int, len(scores))
	coupledData = make([]int, len(scores))

	for idx, score := range scores {
		labels[idx] = score.name
		selfData[idx] = score.self
		coupledData[idx] = score.coupled
	}

	return labels, selfData, coupledData
}

func createEmptyChart() *charts.Bar {
	chartOpts := plotpage.DefaultChartOpts()
	bar := charts.NewBar()
	bar.SetGlobalOptions(
		charts.WithInitializationOpts(chartOpts.Init("100%", emptyChartHeight)),
		charts.WithTitleOpts(chartOpts.Title("Shotness Analysis", "No data - ensure UAST parsing is configured")),
	)

	return bar
}
