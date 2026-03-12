// Package common provides common functionality.
package common

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Compile-time interface checks.
var (
	_ analyze.SpillThresholdSetter = (*Aggregator)(nil)
	_ analyze.StateSizer           = (*Aggregator)(nil)
)

// Aggregator provides generic aggregation capabilities for analyzers.
type Aggregator struct {
	metricsProcessor   *MetricsProcessor
	dataCollector      *SpillableDataCollector
	resultBuilder      *ResultBuilder
	messageBuilder     func(float64) string
	emptyResultBuilder func() analyze.Report
	analyzerName       string
}

// NewAggregator creates a new Aggregator with configurable components.
// identifierKeys specifies the key(s) used for deduplication. When multiple keys
// are provided, they form a composite dedup key (e.g., ["_source_file", "name"])
// to prevent cross-file overwrites of items with the same primary name.
func NewAggregator(
	analyzerName string,
	numericKeys, countKeys []string,
	collectionKey string,
	identifierKeys []string,
	messageBuilder func(float64) string,
	emptyResultBuilder func() analyze.Report,
) *Aggregator {
	var dc *SpillableDataCollector
	if len(identifierKeys) == 1 {
		dc = NewSpillableDataCollector(collectionKey, identifierKeys[0], defaultSpillThreshold)
	} else {
		dc = NewSpillableDataCollectorComposite(collectionKey, identifierKeys, defaultSpillThreshold)
	}

	return &Aggregator{
		metricsProcessor:   NewMetricsProcessor(numericKeys, countKeys),
		dataCollector:      dc,
		resultBuilder:      NewResultBuilder(),
		analyzerName:       analyzerName,
		messageBuilder:     messageBuilder,
		emptyResultBuilder: emptyResultBuilder,
	}
}

// Aggregate combines multiple analysis results.
func (a *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		a.metricsProcessor.ProcessReport(report)
		a.dataCollector.CollectFromReport(report)
	}
}

// GetResult returns the aggregated analysis result.
func (a *Aggregator) GetResult() analyze.Report {
	if a.metricsProcessor.GetReportCount() == 0 {
		if a.emptyResultBuilder != nil {
			return a.emptyResultBuilder()
		}

		return a.resultBuilder.BuildEmptyResult(a.analyzerName)
	}

	averages := a.metricsProcessor.CalculateAverages()
	counts := a.metricsProcessor.GetCounts()
	collectedData := a.dataCollector.GetSortedData()

	// Build metrics map.
	metrics := make(map[string]any)
	for key, value := range averages {
		metrics[key] = value
	}

	for key, value := range counts {
		metrics[key] = value
	}

	// Build message.
	var message string

	if a.messageBuilder != nil {
		// Use the first numeric metric for message building (can be customized).
		for _, value := range averages {
			message = a.messageBuilder(value)

			break
		}
	}

	if message == "" {
		message = "Analysis completed"
	}

	return a.resultBuilder.BuildCollectionResult(
		a.analyzerName,
		a.dataCollector.GetCollectionKey(),
		collectedData,
		metrics,
		message,
	)
}

// GetMetricsProcessor returns the metrics processor.
func (a *Aggregator) GetMetricsProcessor() *MetricsProcessor {
	return a.metricsProcessor
}

// GetDataCollector returns the data collector.
func (a *Aggregator) GetDataCollector() *SpillableDataCollector {
	return a.dataCollector
}

// SetSpillThreshold configures the spill threshold on the data collector.
// A threshold of 0 disables spilling.
func (a *Aggregator) SetSpillThreshold(threshold int) {
	a.dataCollector.spillThreshold = threshold
}

// GetResultBuilder returns the result builder.
func (a *Aggregator) GetResultBuilder() *ResultBuilder {
	return a.resultBuilder
}

// EstimatedStateSize returns the estimated in-memory state size in bytes.
// Sums MetricsProcessor and SpillableDataCollector estimates.
func (a *Aggregator) EstimatedStateSize() int64 {
	return a.metricsProcessor.EstimatedStateBytes() + a.dataCollector.EstimatedBufferBytes()
}

// SetAggregationMode sets the aggregation mode on the data collector.
// In [analyze.AggregationModeSummaryOnly], per-item data collection is disabled.
func (a *Aggregator) SetAggregationMode(mode analyze.AggregationMode) {
	a.dataCollector.SetAggregationMode(mode)
}
