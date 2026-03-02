package anomaly

import (
	"maps"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// StoreTimeSeriesExtractor extracts tick-indexed dimensions from a store reader.
// It is used by analyzers that write structured store kinds.
type StoreTimeSeriesExtractor func(reader analyze.ReportReader) (ticks []int, dimensions map[string][]float64)

var (
	storeTimeSeriesExtractorsMu sync.RWMutex
	storeTimeSeriesExtractors   = make(map[string]StoreTimeSeriesExtractor)
)

// RegisterStoreTimeSeriesExtractor registers a store-based extractor for the given
// analyzer flag.
func RegisterStoreTimeSeriesExtractor(analyzerFlag string, fn StoreTimeSeriesExtractor) {
	storeTimeSeriesExtractorsMu.Lock()
	defer storeTimeSeriesExtractorsMu.Unlock()

	storeTimeSeriesExtractors[analyzerFlag] = fn
}

// snapshotStoreExtractors returns a shallow copy of the registered store extractors.
func snapshotStoreExtractors() map[string]StoreTimeSeriesExtractor {
	storeTimeSeriesExtractorsMu.RLock()
	defer storeTimeSeriesExtractorsMu.RUnlock()

	snap := make(map[string]StoreTimeSeriesExtractor, len(storeTimeSeriesExtractors))
	maps.Copy(snap, storeTimeSeriesExtractors)

	return snap
}
