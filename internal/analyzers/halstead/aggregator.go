// Package halstead provides halstead functionality.
package halstead

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

const (
	magic100            = 100
	magic1000           = 1000
	volumeThresholdHigh = 5000
)

// Aggregator aggregates Halstead analysis results.
type Aggregator struct {
	*common.Aggregator
	common.PerFileRetainer
	detailed *common.DetailedDataCollector
}

// NewAggregator creates a new Halstead aggregator.
func NewAggregator() *Aggregator {
	numericKeys := getNumericKeys()
	countKeys := getCountKeys()
	messageBuilder := buildHalsteadMessage
	emptyResultBuilder := buildEmptyHalsteadResult

	return &Aggregator{
		Aggregator: common.NewAggregator(
			"halstead",
			numericKeys,
			countKeys,
			"functions",
			[]string{"_source_file", "name"},
			messageBuilder,
			emptyResultBuilder,
		),
		detailed: common.NewDetailedDataCollector("functions"),
	}
}

// SetAggregationMode propagates the mode to both the base aggregator and
// the detailed data collector.
func (ha *Aggregator) SetAggregationMode(mode analyze.AggregationMode) {
	ha.Aggregator.SetAggregationMode(mode)
	ha.detailed.SetAggregationMode(mode)
}

// Aggregate overrides the base Aggregate method to collect detailed functions.
func (ha *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		ha.Retain(report)
	}

	ha.detailed.CollectFromReports(results)
	ha.Aggregator.Aggregate(results)
}

// GetResult overrides the base GetResult method to include detailed functions.
func (ha *Aggregator) GetResult() analyze.Report {
	result := ha.Aggregator.GetResult()
	ha.detailed.AddToResult(result)

	return result
}

// getNumericKeys returns the numeric keys for Halstead aggregation.
func getNumericKeys() []string {
	return []string{
		"volume",
		"difficulty",
		"effort",
		"time_to_program",
		"delivered_bugs",
		"distinct_operators",
		"distinct_operands",
		"total_operators",
		"total_operands",
		"vocabulary",
		"length",
		"estimated_length",
	}
}

// getCountKeys returns the count keys for Halstead aggregation.
func getCountKeys() []string {
	return []string{"total_functions"}
}

var halsteadMessageLabeler = common.ThresholdLabeler{
	{Limit: volumeThresholdHigh, Label: "Very high Halstead complexity - significant refactoring recommended"},
	{Limit: magic1000, Label: "High Halstead complexity - consider refactoring"},
	{Limit: magic100, Label: "Moderate Halstead complexity - acceptable"},
	{Limit: 0, Label: "Low Halstead complexity - well-structured code"},
}

// buildHalsteadMessage creates a message based on the volume metric.
func buildHalsteadMessage(volume float64) string {
	return halsteadMessageLabeler.Label(volume)
}

// buildEmptyHalsteadResult creates an empty result with default Halstead values.
func buildEmptyHalsteadResult() analyze.Report {
	return analyze.Report{
		"total_functions":    0,
		"volume":             0.0,
		"difficulty":         0.0,
		"effort":             0.0,
		"time_to_program":    0.0,
		"delivered_bugs":     0.0,
		"distinct_operators": 0,
		"distinct_operands":  0,
		"total_operators":    0,
		"total_operands":     0,
		"vocabulary":         0,
		"length":             0,
		"estimated_length":   0.0,
		"message":            "No functions found",
	}
}
