// Package comments provides comments functionality.
package comments

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

const (
	scoreThresholdHigh   = 0.8
	scoreThresholdLow    = 0.4
	scoreThresholdMedium = 0.6
)

// Aggregator aggregates results from multiple comment analyses.
type Aggregator struct {
	*common.Aggregator
	common.PerFileRetainer
	detailed *common.DetailedDataCollector
}

// NewAggregator creates a new Aggregator.
func NewAggregator() *Aggregator {
	numericKeys := []string{"overall_score", "good_comments_ratio", "documentation_coverage"}
	countKeys := []string{
		"total_comments", "good_comments", "bad_comments",
		"total_functions", "documented_functions", "total_comment_details",
	}

	messageBuilder := buildMessage
	emptyResultBuilder := buildEmptyResult

	return &Aggregator{
		Aggregator: common.NewAggregator(
			"comments",
			numericKeys,
			countKeys,
			"comments",
			[]string{"line"},
			messageBuilder,
			emptyResultBuilder,
		),
		detailed: common.NewDetailedDataCollector("comments", "functions"),
	}
}

// SetAggregationMode propagates the mode to both the base aggregator and
// the detailed data collector.
func (ca *Aggregator) SetAggregationMode(mode analyze.AggregationMode) {
	ca.Aggregator.SetAggregationMode(mode)
	ca.detailed.SetAggregationMode(mode)
}

// Aggregate overrides the base Aggregate method to collect detailed comments and functions.
func (ca *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		ca.Retain(report)
	}

	ca.detailed.CollectFromReports(results)
	ca.Aggregator.Aggregate(results)
}

// GetResult overrides the base GetResult method to include detailed comments and functions.
func (ca *Aggregator) GetResult() analyze.Report {
	result := ca.Aggregator.GetResult()
	ca.detailed.AddToResult(result)

	return result
}

var commentMessageLabeler = common.ThresholdLabeler{
	{Limit: scoreThresholdHigh, Label: "Excellent comment quality and placement"},
	{Limit: scoreThresholdMedium, Label: msgGoodCommentQuality},
	{Limit: scoreThresholdLow, Label: "Fair comment quality - consider improving placement"},
	{Limit: 0, Label: "Poor comment quality - significant improvement needed"},
}

// buildMessage creates a message based on the overall score.
func buildMessage(score float64) string {
	return commentMessageLabeler.Label(score)
}

// buildEmptyResult creates an empty result with default values.
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
