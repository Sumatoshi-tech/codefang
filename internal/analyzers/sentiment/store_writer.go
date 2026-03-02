package sentiment

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindTimeSeries = "time_series"
	KindTrend      = "trend"
	KindAggregate  = "aggregate"
)

// WriteToStore implements analyze.StoreWriter.
// It extracts comment data from TICKs, computes sentiment scores, and streams
// pre-computed metrics as individual records:
//   - "time_series": per-tick TimeSeriesData records (sorted by tick).
//   - "trend": single TrendData record.
//   - "aggregate": single AggregateData record.
func (s *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
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

	trendErr := w.Write(KindTrend, metrics.Trend)
	if trendErr != nil {
		return fmt.Errorf("write %s: %w", KindTrend, trendErr)
	}

	aggErr := w.Write(KindAggregate, metrics.Aggregate)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}
