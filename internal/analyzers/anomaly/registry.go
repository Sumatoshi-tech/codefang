package anomaly

import (
	"maps"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// TimeSeriesExtractor extracts tick-indexed dimensions from a finalized report.
// It returns sorted tick indices and a map of dimension_name -> values (parallel
// to ticks). Returning nil ticks signals that no data is available.
type TimeSeriesExtractor func(report analyze.Report) (ticks []int, dimensions map[string][]float64)

// timeSeriesExtractors maps analyzer flag names to their time series extractors.
var (
	timeSeriesExtractorsMu sync.RWMutex
	timeSeriesExtractors   = make(map[string]TimeSeriesExtractor)
)

// RegisterTimeSeriesExtractor registers an extractor for the given analyzer flag.
// Analyzers call this in their init() to opt into cross-analyzer anomaly detection.
func RegisterTimeSeriesExtractor(analyzerFlag string, fn TimeSeriesExtractor) {
	timeSeriesExtractorsMu.Lock()
	defer timeSeriesExtractorsMu.Unlock()

	timeSeriesExtractors[analyzerFlag] = fn
}

// snapshotExtractors returns a shallow copy of the registered extractors under the read lock.
func snapshotExtractors() map[string]TimeSeriesExtractor {
	timeSeriesExtractorsMu.RLock()
	defer timeSeriesExtractorsMu.RUnlock()

	snap := make(map[string]TimeSeriesExtractor, len(timeSeriesExtractors))
	maps.Copy(snap, timeSeriesExtractors)

	return snap
}
