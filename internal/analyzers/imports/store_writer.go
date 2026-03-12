package imports

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindImportUsage = "import_usage"
)

// ImportUsageRecord holds pre-computed import usage count for a single import path.
type ImportUsageRecord struct {
	Import string
	Count  int64
}

// WriteToStore implements analyze.StoreWriter.
// It extracts import data from TICKs, aggregates usage counts across
// all authors/languages/ticks, and streams them as individual records:
//   - "import_usage": per-import ImportUsageRecord records (sorted by count desc).
func (h *HistoryAnalyzer) WriteToStore(_ context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	merged := Map{}

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		mergeImportMaps(merged, td.Imports)
	}

	counts := aggregateImportCounts(merged)
	labels, data := topImports(counts)

	for i := range labels {
		writeErr := w.Write(KindImportUsage, ImportUsageRecord{
			Import: labels[i],
			Count:  int64(data[i]),
		})
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindImportUsage, writeErr)
		}
	}

	return nil
}
