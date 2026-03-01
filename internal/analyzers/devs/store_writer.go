package devs

import (
	"context"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindDeveloper = "developer"
	KindLanguage  = "language"
	KindBusFactor = "bus_factor"
	KindActivity  = "activity"
	KindChurn     = "churn"
	KindAggregate = "aggregate"
)

// WriteToStore implements analyze.StoreWriter.
// It extracts per-commit dev data from TICKs, computes all metrics, and streams
// pre-computed results as individual records:
//   - "developer": per-developer DeveloperData records (sorted by commits desc).
//   - "language": per-language LanguageData records (sorted by total lines desc).
//   - "bus_factor": per-language BusFactorData records (sorted by risk).
//   - "activity": per-tick ActivityData records (sorted by tick).
//   - "churn": per-tick ChurnData records (sorted by tick).
//   - "aggregate": single AggregateData record.
func (a *Analyzer) WriteToStore(ctx context.Context, ticks []analyze.TICK, w analyze.ReportWriter) error {
	report := ticksToReport(ctx, ticks, a.commitsByTick, a.getReversedPeopleDict(), a.tickSize, a.Anonymize)

	metrics, metricsErr := ComputeAllMetrics(report)
	if metricsErr != nil {
		return fmt.Errorf("compute metrics: %w", metricsErr)
	}

	return writeMetrics(w, metrics)
}

// writeMetrics streams all computed metric records to the report writer.
func writeMetrics(w analyze.ReportWriter, metrics *ComputedMetrics) error {
	devErr := writeSliceKind(w, KindDeveloper, metrics.Developers)
	if devErr != nil {
		return devErr
	}

	langErr := writeSliceKind(w, KindLanguage, metrics.Languages)
	if langErr != nil {
		return langErr
	}

	busErr := writeSliceKind(w, KindBusFactor, metrics.BusFactor)
	if busErr != nil {
		return busErr
	}

	actErr := writeSliceKind(w, KindActivity, metrics.Activity)
	if actErr != nil {
		return actErr
	}

	churnErr := writeSliceKind(w, KindChurn, metrics.Churn)
	if churnErr != nil {
		return churnErr
	}

	aggErr := w.Write(KindAggregate, metrics.Aggregate)
	if aggErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, aggErr)
	}

	return nil
}

// writeSliceKind writes each element of a typed slice as a separate record.
func writeSliceKind[T any](w analyze.ReportWriter, kind string, records []T) error {
	for i := range records {
		writeErr := w.Write(kind, records[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", kind, writeErr)
		}
	}

	return nil
}
