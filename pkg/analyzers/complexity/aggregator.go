// Package complexity provides complexity functionality.
package complexity

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

const (
	magic7p0           = 7.0
	scoreThresholdHigh = 3.0
)

const msgGoodComplexity = "Good complexity - functions have reasonable complexity"

// ComplexityAggregator aggregates results from multiple complexity analyses.
type ComplexityAggregator struct {
	*common.Aggregator //nolint:embeddedstructfieldcheck // embedded struct field is intentional.
	detailedFunctions  []map[string]any
	maxComplexity      int
}

// NewComplexityAggregator creates a new ComplexityAggregator.
func NewComplexityAggregator() *ComplexityAggregator {
	numericKeys := getNumericKeys()
	countKeys := getCountKeys()
	messageBuilder := buildComplexityMessage
	emptyResultBuilder := buildEmptyComplexityResult

	return &ComplexityAggregator{
		Aggregator: common.NewAggregator(
			"complexity",
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

// Aggregate overrides the base Aggregate method to collect detailed functions
// and track the true maximum complexity across all files.
func (ca *ComplexityAggregator) Aggregate(results map[string]analyze.Report) {
	ca.collectDetailedFunctions(results)
	ca.trackMaxComplexity(results)
	ca.Aggregator.Aggregate(results)
}

// GetResult overrides the base GetResult method to include detailed functions
// and compute derived metrics (average_complexity, max_complexity, message).
func (ca *ComplexityAggregator) GetResult() analyze.Report {
	result := ca.Aggregator.GetResult()
	ca.addDetailedFunctionsToResult(result)
	// Only add derived metrics when we actually aggregated reports;
	// otherwise the empty result builder already set correct defaults.
	if ca.GetMetricsProcessor().GetReportCount() > 0 {
		ca.addDerivedMetrics(result)
	}

	return result
}

// collectDetailedFunctions extracts detailed functions from all reports.
func (ca *ComplexityAggregator) collectDetailedFunctions(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		ca.extractFunctionsFromReport(report)
	}
}

// extractFunctionsFromReport extracts functions from a single report.
func (ca *ComplexityAggregator) extractFunctionsFromReport(report analyze.Report) {
	if functions, ok := report["functions"].([]map[string]any); ok {
		ca.detailedFunctions = append(ca.detailedFunctions, functions...)
	}
}

// addDetailedFunctionsToResult adds detailed functions to the result.
func (ca *ComplexityAggregator) addDetailedFunctionsToResult(result analyze.Report) {
	if len(ca.detailedFunctions) > 0 {
		result["functions"] = ca.detailedFunctions
	}
}

// trackMaxComplexity tracks the true maximum complexity across all files.
func (ca *ComplexityAggregator) trackMaxComplexity(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		if val, ok := extractIntFromReport(report, "max_complexity"); ok {
			if val > ca.maxComplexity {
				ca.maxComplexity = val
			}
		}
	}
}

// addDerivedMetrics computes average_complexity, max_complexity, and a
// deterministic message from the aggregated totals.
func (ca *ComplexityAggregator) addDerivedMetrics(result analyze.Report) {
	totalComplexity := 0
	if v, ok := extractIntFromReport(result, "total_complexity"); ok {
		totalComplexity = v
	}

	totalFunctions := 0
	if v, ok := extractIntFromReport(result, "total_functions"); ok {
		totalFunctions = v
	}

	var avgComplexity float64
	if totalFunctions > 0 {
		avgComplexity = float64(totalComplexity) / float64(totalFunctions)
	}

	result["average_complexity"] = avgComplexity
	result["max_complexity"] = ca.maxComplexity
	result["message"] = buildComplexityMessage(avgComplexity)
}

// extractIntFromReport safely extracts an int from a report value.
func extractIntFromReport(report analyze.Report, key string) (int, bool) {
	val, ok := report[key]
	if !ok {
		return 0, false
	}

	switch v := val.(type) {
	case int:
		return v, true
	case int64:
		return int(v), true
	case float64:
		return int(v), true
	default:
		return 0, false
	}
}

// getNumericKeys returns the numeric keys for complexity analysis
// Note: average_complexity is excluded because it's a derived metric
// computed as total_complexity / total_functions in GetResult().
func getNumericKeys() []string {
	return []string{"cognitive_complexity", "nesting_depth"}
}

// getCountKeys returns the count keys for complexity analysis
// Note: max_complexity is excluded because it needs max-tracking (not summing).
// It is tracked separately in ComplexityAggregator.maxComplexity.
func getCountKeys() []string {
	return []string{"total_functions", "total_complexity", "decision_points"}
}

// buildComplexityMessage creates a message based on the complexity score.
func buildComplexityMessage(score float64) string {
	switch {
	case score <= 1.0:
		return "Excellent complexity - functions are simple and maintainable"
	case score <= scoreThresholdHigh:
		return msgGoodComplexity
	case score <= magic7p0:
		return "Fair complexity - some functions could be simplified"
	default:
		return "High complexity - functions are complex and should be refactored"
	}
}

// buildEmptyComplexityResult creates an empty result with default values.
func buildEmptyComplexityResult() analyze.Report {
	return analyze.Report{
		"total_functions":    0,
		"average_complexity": 0.0,
		"max_complexity":     0,
		"total_complexity":   0,
		"message":            "No functions found",
	}
}
