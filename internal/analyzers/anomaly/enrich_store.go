package anomaly

import (
	"errors"
	"fmt"
	"math"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// EnrichAndRewrite reads the anomaly analyzer's structured store kinds,
// detects cross-analyzer anomalies from other analyzers' store data,
// then rewrites all anomaly kinds (original + enrichment) to the store.
func EnrichAndRewrite(
	store analyze.ReportStore,
	analyzerID string,
	windowSize int,
	threshold float64,
) error {
	// Read existing structured kinds.
	reader, openErr := store.Open(analyzerID)
	if openErr != nil {
		return fmt.Errorf("open anomaly: %w", openErr)
	}

	kinds := reader.Kinds()

	timeSeries, tsErr := ReadTimeSeriesIfPresent(reader, kinds)
	if tsErr != nil {
		reader.Close()

		return fmt.Errorf("read %s: %w", KindTimeSeries, tsErr)
	}

	anomalies, anomErr := ReadAnomaliesIfPresent(reader, kinds)
	if anomErr != nil {
		reader.Close()

		return fmt.Errorf("read %s: %w", KindAnomalyRecord, anomErr)
	}

	agg, aggErr := ReadAggregateIfPresent(reader, kinds)
	if aggErr != nil {
		reader.Close()

		return fmt.Errorf("read %s: %w", KindAggregate, aggErr)
	}

	reader.Close()

	// Detect cross-analyzer anomalies.
	extractors := snapshotStoreExtractors()
	externalAnomalies, externalSummaries := runStoreEnrichment(
		store, windowSize, threshold, extractors,
	)

	// Rewrite all kinds (original + enrichment).
	meta := analyze.ReportMeta{AnalyzerID: analyzerID}

	w, beginErr := store.Begin(analyzerID, meta)
	if beginErr != nil {
		return fmt.Errorf("begin anomaly write: %w", beginErr)
	}

	for i := range timeSeries {
		writeErr := w.Write(KindTimeSeries, timeSeries[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindTimeSeries, writeErr)
		}
	}

	for i := range anomalies {
		writeErr := w.Write(KindAnomalyRecord, anomalies[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", KindAnomalyRecord, writeErr)
		}
	}

	writeErr := w.Write(KindAggregate, agg)
	if writeErr != nil {
		return fmt.Errorf("write %s: %w", KindAggregate, writeErr)
	}

	enrichErr := WriteEnrichmentToStore(w, externalAnomalies, externalSummaries)
	if enrichErr != nil {
		return enrichErr
	}

	return w.Close()
}

// runStoreEnrichment is the core enrichment logic with an explicit extractor map.
// Tests inject custom extractors; production code passes snapshotStoreExtractors().
func runStoreEnrichment(
	store analyze.ReportStore,
	windowSize int,
	threshold float64,
	storeExtractors map[string]StoreTimeSeriesExtractor,
) ([]ExternalAnomaly, []ExternalSummary) {
	var allAnomalies []ExternalAnomaly

	var allSummaries []ExternalSummary

	for _, analyzerID := range store.AnalyzerIDs() {
		// Skip the anomaly analyzer itself.
		if analyzerID == analyzerNameAnomaly {
			continue
		}

		ticks, dimensions, extractErr := extractFromStoreOnly(analyzerID, store, storeExtractors)
		if extractErr != nil {
			continue
		}

		if len(ticks) == 0 || len(dimensions) == 0 {
			continue
		}

		anomalies, summaries := detectExternalAnomalies(analyzerID, ticks, dimensions, windowSize, threshold)
		allAnomalies = append(allAnomalies, anomalies...)
		allSummaries = append(allSummaries, summaries...)
	}

	// Sort anomalies by absolute Z-score descending.
	sort.Slice(allAnomalies, func(i, j int) bool {
		return math.Abs(allAnomalies[i].ZScore) > math.Abs(allAnomalies[j].ZScore)
	})

	// Sort summaries by source then dimension for stable output.
	sort.Slice(allSummaries, func(i, j int) bool {
		if allSummaries[i].Source != allSummaries[j].Source {
			return allSummaries[i].Source < allSummaries[j].Source
		}

		return allSummaries[i].Dimension < allSummaries[j].Dimension
	})

	return allAnomalies, allSummaries
}

// extractFromStoreOnly uses store-based extractors to read time series data.
func extractFromStoreOnly(
	analyzerID string,
	store analyze.ReportStore,
	storeExtractors map[string]StoreTimeSeriesExtractor,
) (ticks []int, dimensions map[string][]float64, err error) {
	storeExt, ok := storeExtractors[analyzerID]
	if !ok {
		return nil, nil, errNoExtractor
	}

	reader, openErr := store.Open(analyzerID)
	if openErr != nil {
		return nil, nil, fmt.Errorf("open %s: %w", analyzerID, openErr)
	}

	defer reader.Close()

	ticks, dimensions = storeExt(reader)

	return ticks, dimensions, nil
}

// errNoExtractor indicates no extractor is registered for an analyzer.
var errNoExtractor = errors.New("no extractor registered")
