package shotness

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindNodeData  = "node_data"
	KindAggregate = "aggregate"
)

// NodeStoreRecord holds a single node's summary and co-change counter map.
// Counter keys are node indices into the ordered node list; counter values
// are co-change counts; counter[self] is the node's self-change count.
type NodeStoreRecord struct {
	Summary NodeSummary
	Counter map[int]int
}

// WriteToStore implements analyze.StoreWriter.
// It merges per-tick node data, builds the co-change matrix, and streams
// pre-computed results as individual records:
//   - "node_data": per-node NodeStoreRecord records (sorted by node key).
//   - "aggregate": single AggregateData record.
func (s *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	report := ticksToReport(ctx, ticks)

	nodes, counters, extractErr := extractShotnessData(report)
	if extractErr != nil {
		return fmt.Errorf("extract data: %w", extractErr)
	}

	for i := range nodes {
		rec := NodeStoreRecord{
			Summary: nodes[i],
			Counter: counters[i],
		}

		writeErr := w.Write(KindNodeData, rec)
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindNodeData, writeErr)
		}
	}

	input := &ReportData{Nodes: nodes, Counters: counters}
	agg := computeAggregate(input)

	aggErr := w.Write(KindAggregate, agg)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}
