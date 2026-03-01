package typos

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindFileTypos = "file_typos"
	KindAggregate = "aggregate"
)

// WriteToStore implements analyze.StoreWriter.
// It extracts typos from TICKs, deduplicates, computes per-file typo counts
// and aggregate statistics, and streams them as individual records:
//   - "file_typos": per-file FileTypoData records (sorted by typo count desc).
//   - "aggregate": single AggregateData record.
func (t *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	report := ticksToReport(ctx, ticks)

	input, parseErr := ParseReportData(report)
	if parseErr != nil {
		return fmt.Errorf("parse report: %w", parseErr)
	}

	fileTypos := computeFileTypos(input)

	for i := range fileTypos {
		writeErr := w.Write(KindFileTypos, fileTypos[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindFileTypos, writeErr)
		}
	}

	agg := computeAggregate(input)

	aggErr := w.Write(KindAggregate, agg)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}
