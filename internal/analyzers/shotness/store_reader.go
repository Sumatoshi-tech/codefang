package shotness

import (
	"fmt"
	"slices"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed shotness data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or recomputing the co-change matrix.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	records, readErr := readNodeDataIfPresent(reader, kinds)
	if readErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindNodeData, readErr)
	}

	if len(records) == 0 {
		return nil, nil
	}

	nodes := make([]NodeSummary, len(records))
	counters := make([]map[int]int, len(records))

	for i, rec := range records {
		nodes[i] = rec.Summary
		counters[i] = rec.Counter
	}

	chartOpts := plotpage.DefaultChartOpts()
	palette := plotpage.GetChartPalette(plotpage.ThemeDark)

	return []plotpage.Section{
		treeMapSection(nodes, counters, chartOpts),
		heatMapSection(nodes, counters, chartOpts),
		barChartSection(nodes, counters, chartOpts, palette),
	}, nil
}

// readNodeDataIfPresent reads all node_data records, returning nil if absent.
func readNodeDataIfPresent(reader analyze.ReportReader, kinds []string) ([]NodeStoreRecord, error) {
	if !slices.Contains(kinds, KindNodeData) {
		return nil, nil
	}

	var result []NodeStoreRecord

	iterErr := reader.Iter(KindNodeData, func(raw []byte) error {
		var record NodeStoreRecord

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}
