package quality

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindTimeSeries = "time_series"
	KindAggregate  = "aggregate"
)

// WriteToStore implements analyze.StoreWriter.
// It extracts per-commit quality data from TICKs, computes metrics, and streams
// pre-computed results as individual records:
//   - "time_series": per-tick TimeSeriesEntry records (sorted by tick).
//   - "aggregate": single AggregateData record.
func (a *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	report := ticksToReport(ctx, ticks, nil)

	metrics, metricsErr := ComputeAllMetrics(report)
	if metricsErr != nil {
		return fmt.Errorf("compute metrics: %w", metricsErr)
	}

	for i := range metrics.TimeSeries {
		writeErr := w.Write(KindTimeSeries, metrics.TimeSeries[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindTimeSeries, writeErr)
		}
	}

	aggErr := w.Write(KindAggregate, metrics.Aggregate)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}
