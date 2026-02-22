package shotness

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// --- Input Data Types ---.

// ReportData is the parsed input data for shotness metrics computation.
type ReportData struct {
	Nodes    []NodeSummary
	Counters []map[int]int
}

// ParseReportData extracts ReportData from an analyzer report.
func ParseReportData(report analyze.Report) (*ReportData, error) {
	data := &ReportData{}

	if v, ok := report["Nodes"].([]NodeSummary); ok {
		data.Nodes = v
	}

	if v, ok := report["Counters"].([]map[int]int); ok {
		data.Counters = v
	}

	return data, nil
}

// --- Output Data Types ---.

// NodeHotnessData contains hotness information for a code node.
type NodeHotnessData struct {
	Name         string  `json:"name"          yaml:"name"`
	Type         string  `json:"type"          yaml:"type"`
	File         string  `json:"file"          yaml:"file"`
	ChangeCount  int     `json:"change_count"  yaml:"change_count"`
	CoupledNodes int     `json:"coupled_nodes" yaml:"coupled_nodes"`
	HotnessScore float64 `json:"hotness_score" yaml:"hotness_score"`
}

// NodeCouplingData contains coupling between code nodes.
type NodeCouplingData struct {
	Node1Name string `json:"node1_name" yaml:"node1_name"`
	Node1File string `json:"node1_file" yaml:"node1_file"`
	Node2Name string `json:"node2_name" yaml:"node2_name"`
	Node2File string `json:"node2_file" yaml:"node2_file"`
	CoChanges int    `json:"co_changes" yaml:"co_changes"`
}

// HotspotNodeData identifies hot nodes that change frequently.
type HotspotNodeData struct {
	Name        string `json:"name"         yaml:"name"`
	Type        string `json:"type"         yaml:"type"`
	File        string `json:"file"         yaml:"file"`
	ChangeCount int    `json:"change_count" yaml:"change_count"`
	RiskLevel   string `json:"risk_level"   yaml:"risk_level"`
}

// AggregateData contains summary statistics.
type AggregateData struct {
	TotalNodes        int     `json:"total_nodes"          yaml:"total_nodes"`
	TotalChanges      int     `json:"total_changes"        yaml:"total_changes"`
	TotalCouplings    int     `json:"total_couplings"      yaml:"total_couplings"`
	AvgChangesPerNode float64 `json:"avg_changes_per_node" yaml:"avg_changes_per_node"`
	HotNodes          int     `json:"hot_nodes"            yaml:"hot_nodes"`
}

// Hotspot thresholds.
const (
	HotspotThresholdHigh   = 20
	HotspotThresholdMedium = 10

	// Coupling divisor for strength calculation.
	couplingDivisor = 2
)

// --- Pure Metric Functions ---.

// computeNodeHotness calculates node hotness data.
func computeNodeHotness(input *ReportData) []NodeHotnessData {
	result := make([]NodeHotnessData, 0, len(input.Nodes))

	// Find max change count for normalization.
	var maxChanges int
	for i, counters := range input.Counters {
		if selfCount, ok := counters[i]; ok && selfCount > maxChanges {
			maxChanges = selfCount
		}
	}

	for i, node := range input.Nodes {
		if i >= len(input.Counters) {
			continue
		}

		counters := input.Counters[i]

		changeCount := 0
		if selfCount, ok := counters[i]; ok {
			changeCount = selfCount
		}

		coupledNodes := len(counters) - 1 // Exclude self.

		var hotnessScore float64
		if maxChanges > 0 {
			hotnessScore = float64(changeCount) / float64(maxChanges)
		}

		result = append(result, NodeHotnessData{
			Name:         node.Name,
			Type:         node.Type,
			File:         node.File,
			ChangeCount:  changeCount,
			CoupledNodes: coupledNodes,
			HotnessScore: hotnessScore,
		})
	}

	// Sort by change count descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].ChangeCount > result[j].ChangeCount
	})

	return result
}

// computeNodeCoupling calculates node coupling data.
func computeNodeCoupling(input *ReportData) []NodeCouplingData {
	var result []NodeCouplingData

	for i, counters := range input.Counters {
		if i >= len(input.Nodes) {
			continue
		}

		node1 := input.Nodes[i]

		for j, coChanges := range counters {
			if j <= i || j >= len(input.Nodes) {
				continue // Skip self and lower triangle.
			}

			if coChanges == 0 {
				continue
			}

			node2 := input.Nodes[j]

			result = append(result, NodeCouplingData{
				Node1Name: node1.Name,
				Node1File: node1.File,
				Node2Name: node2.Name,
				Node2File: node2.File,
				CoChanges: coChanges,
			})
		}
	}

	// Sort by co-changes descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].CoChanges > result[j].CoChanges
	})

	return result
}

func classifyChangeRisk(changeCount int) string {
	switch {
	case changeCount >= HotspotThresholdHigh:
		return "HIGH"
	case changeCount >= HotspotThresholdMedium:
		return "MEDIUM"
	default:
		return ""
	}
}

// computeHotspotNodes identifies hotspot nodes.
func computeHotspotNodes(input *ReportData) []HotspotNodeData {
	var result []HotspotNodeData

	for i, node := range input.Nodes {
		if i >= len(input.Counters) {
			continue
		}

		counters := input.Counters[i]

		changeCount := 0
		if selfCount, ok := counters[i]; ok {
			changeCount = selfCount
		}

		riskLevel := classifyChangeRisk(changeCount)
		if riskLevel == "" {
			continue // Skip low-risk nodes.
		}

		result = append(result, HotspotNodeData{
			Name:        node.Name,
			Type:        node.Type,
			File:        node.File,
			ChangeCount: changeCount,
			RiskLevel:   riskLevel,
		})
	}

	// Sort by change count descending.
	sort.Slice(result, func(i, j int) bool {
		return result[i].ChangeCount > result[j].ChangeCount
	})

	return result
}

// computeAggregate calculates aggregate statistics.
func computeAggregate(input *ReportData) AggregateData {
	agg := AggregateData{
		TotalNodes: len(input.Nodes),
	}

	var totalChanges, totalCouplings, hotNodes int

	for i, counters := range input.Counters {
		if selfCount, ok := counters[i]; ok {
			totalChanges += selfCount
			if selfCount >= HotspotThresholdMedium {
				hotNodes++
			}
		}

		// Count couplings (non-self entries).
		for j := range counters {
			if j != i {
				totalCouplings++
			}
		}
	}

	agg.TotalChanges = totalChanges
	agg.TotalCouplings = totalCouplings / couplingDivisor // Divide by 2 since counted twice.
	agg.HotNodes = hotNodes

	if agg.TotalNodes > 0 {
		agg.AvgChangesPerNode = float64(totalChanges) / float64(agg.TotalNodes)
	}

	return agg
}

// --- Computed Metrics ---.

// ComputedMetrics holds all computed metric results for the shotness analyzer.
type ComputedMetrics struct {
	NodeHotness  []NodeHotnessData  `json:"node_hotness"  yaml:"node_hotness"`
	NodeCoupling []NodeCouplingData `json:"node_coupling" yaml:"node_coupling"`
	HotspotNodes []HotspotNodeData  `json:"hotspot_nodes" yaml:"hotspot_nodes"`
	Aggregate    AggregateData      `json:"aggregate"     yaml:"aggregate"`
}

const analyzerNameShotness = "shotness"

// AnalyzerName returns the analyzer identifier.
func (m *ComputedMetrics) AnalyzerName() string {
	return analyzerNameShotness
}

// ToJSON returns the metrics in JSON-serializable format.
func (m *ComputedMetrics) ToJSON() any {
	return m
}

// ToYAML returns the metrics in YAML-serializable format.
func (m *ComputedMetrics) ToYAML() any {
	return m
}

// ComputeAllMetrics runs all shotness metrics and returns the results.
func ComputeAllMetrics(report analyze.Report) (*ComputedMetrics, error) {
	input, err := ParseReportData(report)
	if err != nil {
		return nil, err
	}

	return &ComputedMetrics{
		NodeHotness:  computeNodeHotness(input),
		NodeCoupling: computeNodeCoupling(input),
		HotspotNodes: computeHotspotNodes(input),
		Aggregate:    computeAggregate(input),
	}, nil
}
