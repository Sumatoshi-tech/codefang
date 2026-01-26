// Package cohesion provides cohesion functionality.
package cohesion

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

const (
	scoreThresholdHigh   = 0.8
	scoreThresholdLow    = 0.3
	scoreThresholdMedium = 0.6
)

// Aggregator aggregates results from multiple cohesion analyses.
type Aggregator struct {
	*common.Aggregator
}

// NewAggregator creates a new Aggregator.
func NewAggregator() *Aggregator {
	config := buildAggregatorConfig()

	return &Aggregator{
		Aggregator: common.NewAggregator(
			"cohesion",
			config.numericKeys,
			config.countKeys,
			"functions",
			"name",
			config.messageBuilder,
			config.emptyResultBuilder,
		),
	}
}

// aggregatorConfig holds the configuration for the aggregator.
type aggregatorConfig struct {
	messageBuilder     func(float64) string
	emptyResultBuilder func() analyze.Report
	numericKeys        []string
	countKeys          []string
}

// buildAggregatorConfig creates the configuration for the cohesion aggregator.
func buildAggregatorConfig() aggregatorConfig {
	return aggregatorConfig{
		numericKeys:        getNumericKeys(),
		countKeys:          getCountKeys(),
		messageBuilder:     buildMessageBuilder(),
		emptyResultBuilder: buildEmptyResultBuilder(),
	}
}

// getNumericKeys returns the numeric keys for aggregation.
func getNumericKeys() []string {
	return []string{"lcom", "cohesion_score", "function_cohesion"}
}

// getCountKeys returns the count keys for aggregation.
func getCountKeys() []string {
	return []string{"total_functions"}
}

// buildMessageBuilder creates the message builder function.
func buildMessageBuilder() func(float64) string {
	return getCohesionMessage
}

// getCohesionMessage returns a message based on the cohesion score.
func getCohesionMessage(score float64) string {
	if score >= scoreThresholdHigh {
		return "Excellent overall cohesion across all analyzed code"
	}

	if score >= scoreThresholdMedium {
		return "Good overall cohesion with room for improvement"
	}

	if score >= scoreThresholdLow {
		return "Fair overall cohesion - consider refactoring some functions"
	}

	return "Poor overall cohesion - significant refactoring recommended"
}

// buildEmptyResultBuilder creates the empty result builder function.
func buildEmptyResultBuilder() func() analyze.Report {
	return createEmptyResult
}

// createEmptyResult creates an empty result when no functions are found.
func createEmptyResult() analyze.Report {
	return analyze.Report{
		"total_functions":   0,
		"lcom":              0.0,
		"cohesion_score":    1.0,
		"function_cohesion": 1.0,
		"message":           "No functions found",
	}
}
