package comments

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common"
)

// CommentsAggregator aggregates results from multiple comment analyses
type CommentsAggregator struct {
	*common.Aggregator
	detailedComments  []map[string]interface{}
	detailedFunctions []map[string]interface{}
}

// NewCommentsAggregator creates a new CommentsAggregator
func NewCommentsAggregator() *CommentsAggregator {
	numericKeys := []string{"overall_score", "good_comments_ratio", "documentation_coverage"}
	countKeys := []string{"total_comments", "good_comments", "bad_comments", "total_functions", "documented_functions", "total_comment_details"}

	messageBuilder := buildMessage
	emptyResultBuilder := buildEmptyResult

	return &CommentsAggregator{
		Aggregator: common.NewAggregator(
			"comments",
			numericKeys,
			countKeys,
			"comments",
			"line",
			messageBuilder,
			emptyResultBuilder,
		),
		detailedComments:  make([]map[string]interface{}, 0),
		detailedFunctions: make([]map[string]interface{}, 0),
	}
}

// Aggregate overrides the base Aggregate method to collect detailed comments and functions
func (ca *CommentsAggregator) Aggregate(results map[string]analyze.Report) {
	ca.collectDetailedData(results)
	ca.Aggregator.Aggregate(results)
}

// GetResult overrides the base GetResult method to include detailed comments and functions
func (ca *CommentsAggregator) GetResult() analyze.Report {
	result := ca.Aggregator.GetResult()
	ca.addDetailedDataToResult(result)
	return result
}

// collectDetailedData extracts detailed comments and functions from all reports
func (ca *CommentsAggregator) collectDetailedData(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		ca.extractCommentsFromReport(report)
		ca.extractFunctionsFromReport(report)
	}
}

// extractCommentsFromReport extracts comments from a single report
func (ca *CommentsAggregator) extractCommentsFromReport(report analyze.Report) {
	if comments, ok := report["comments"].([]map[string]interface{}); ok {
		ca.detailedComments = append(ca.detailedComments, comments...)
	}
}

// extractFunctionsFromReport extracts functions from a single report
func (ca *CommentsAggregator) extractFunctionsFromReport(report analyze.Report) {
	if functions, ok := report["functions"].([]map[string]interface{}); ok {
		ca.detailedFunctions = append(ca.detailedFunctions, functions...)
	}
}

// addDetailedDataToResult adds detailed comments and functions to the result
func (ca *CommentsAggregator) addDetailedDataToResult(result analyze.Report) {
	if len(ca.detailedComments) > 0 {
		result["comments"] = ca.detailedComments
	}

	if len(ca.detailedFunctions) > 0 {
		result["functions"] = ca.detailedFunctions
	}
}

// buildMessage creates a message based on the overall score
func buildMessage(score float64) string {
	switch {
	case score >= 0.8:
		return "Excellent comment quality and placement"
	case score >= 0.6:
		return "Good comment quality with room for improvement"
	case score >= 0.4:
		return "Fair comment quality - consider improving placement"
	default:
		return "Poor comment quality - significant improvement needed"
	}
}

// buildEmptyResult creates an empty result with default values
func buildEmptyResult() analyze.Report {
	return analyze.Report{
		"total_comments":        0,
		"good_comments":         0,
		"bad_comments":          0,
		"overall_score":         0.0,
		"total_functions":       0,
		"documented_functions":  0,
		"total_comment_details": 0,
		"message":               "No comments found",
	}
}
