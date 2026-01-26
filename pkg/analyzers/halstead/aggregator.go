// Package halstead provides halstead functionality.
package halstead

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

const (
	magic100            = 100
	magic1000           = 1000
	volumeThresholdHigh = 5000
)

// Aggregator aggregates Halstead analysis results.
type Aggregator struct {
	*common.Aggregator //nolint:embeddedstructfieldcheck // embedded struct field is intentional.
	detailedFunctions  []map[string]any
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
			"name",
			messageBuilder,
			emptyResultBuilder,
		),
		detailedFunctions: make([]map[string]any, 0),
	}
}

// Aggregate overrides the base Aggregate method to collect detailed functions.
func (ha *Aggregator) Aggregate(results map[string]analyze.Report) {
	ha.collectDetailedFunctions(results)
	ha.Aggregator.Aggregate(results)
}

// GetResult overrides the base GetResult method to include detailed functions.
func (ha *Aggregator) GetResult() analyze.Report {
	result := ha.Aggregator.GetResult()
	ha.addDetailedFunctionsToResult(result)

	return result
}

// collectDetailedFunctions extracts detailed functions from all reports.
func (ha *Aggregator) collectDetailedFunctions(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		ha.extractFunctionsFromReport(report)
	}
}

// extractFunctionsFromReport extracts functions from a single report.
func (ha *Aggregator) extractFunctionsFromReport(report analyze.Report) {
	if functions, ok := report["functions"].([]map[string]any); ok {
		ha.detailedFunctions = append(ha.detailedFunctions, functions...)
	}
}

// addDetailedFunctionsToResult adds detailed functions to the result.
func (ha *Aggregator) addDetailedFunctionsToResult(result analyze.Report) {
	if len(ha.detailedFunctions) > 0 {
		result["functions"] = ha.detailedFunctions
	}
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

// buildHalsteadMessage creates a message based on the volume metric.
func buildHalsteadMessage(volume float64) string {
	switch {
	case volume >= volumeThresholdHigh:
		return "Very high Halstead complexity - significant refactoring recommended"
	case volume >= magic1000:
		return "High Halstead complexity - consider refactoring"
	case volume >= magic100:
		return "Moderate Halstead complexity - acceptable"
	default:
		return "Low Halstead complexity - well-structured code"
	}
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
