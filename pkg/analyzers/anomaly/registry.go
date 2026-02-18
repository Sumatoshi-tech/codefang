package anomaly

import "github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"

// TimeSeriesExtractor extracts tick-indexed dimensions from a finalized report.
// It returns sorted tick indices and a map of dimension_name -> values (parallel
// to ticks). Returning nil ticks signals that no data is available.
type TimeSeriesExtractor func(report analyze.Report) (ticks []int, dimensions map[string][]float64)

// timeSeriesExtractors maps analyzer flag names to their time series extractors.
var timeSeriesExtractors = make(map[string]TimeSeriesExtractor)

// RegisterTimeSeriesExtractor registers an extractor for the given analyzer flag.
// Analyzers call this in their init() to opt into cross-analyzer anomaly detection.
func RegisterTimeSeriesExtractor(analyzerFlag string, fn TimeSeriesExtractor) {
	timeSeriesExtractors[analyzerFlag] = fn
}

// TimeSeriesExtractorFor returns the registered extractor for the given flag, or nil.
func TimeSeriesExtractorFor(analyzerFlag string) TimeSeriesExtractor {
	return timeSeriesExtractors[analyzerFlag]
}
