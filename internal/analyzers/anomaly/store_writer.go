package anomaly

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindTimeSeries      = "time_series"
	KindAnomalyRecord   = "anomaly_record"
	KindAggregate       = "aggregate"
	KindExternalAnomaly = "external_anomaly"
	KindExternalSummary = "external_summary"
)

// WriteToStore implements analyze.StoreWriter.
// It converts ticks to a report, computes all metrics, and streams
// pre-computed results as individual records:
//   - "time_series": per-tick TimeSeriesEntry records (sorted by tick).
//   - "anomaly_record": per-anomaly Record entries (sorted by Z-score desc).
//   - "aggregate": single AggregateData record.
func (h *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	report := ticksToReport(ctx, ticks, h.Threshold, h.WindowSize, h.commitsByTick)

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

	for i := range metrics.Anomalies {
		writeErr := w.Write(KindAnomalyRecord, metrics.Anomalies[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindAnomalyRecord, writeErr)
		}
	}

	aggErr := w.Write(KindAggregate, metrics.Aggregate)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}

// WriteEnrichmentToStore writes external anomaly and summary records to the writer.
// Called by the enrichment pipeline after cross-analyzer anomaly detection.
func WriteEnrichmentToStore(
	w analyze.ReportWriter,
	externalAnomalies []ExternalAnomaly,
	externalSummaries []ExternalSummary,
) error {
	for i := range externalAnomalies {
		writeErr := w.Write(KindExternalAnomaly, externalAnomalies[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindExternalAnomaly, writeErr)
		}
	}

	for i := range externalSummaries {
		writeErr := w.Write(KindExternalSummary, externalSummaries[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindExternalSummary, writeErr)
		}
	}

	return nil
}
