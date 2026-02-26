package clones

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

// Aggregator wraps common.Aggregator for clone detection results.
type Aggregator struct {
	*common.Aggregator
}

// NewAggregator creates a new clone detection aggregator.
func NewAggregator() *Aggregator {
	return &Aggregator{
		Aggregator: common.NewAggregator(
			analyzerName,
			[]string{keyCloneRatio},
			[]string{keyTotalFunctions, keyTotalClonePairs},
			keyClonePairs,
			"func_a",
			aggregatorMessage,
			aggregatorEmptyResult,
		),
	}
}

// aggregatorMessage returns a message based on the aggregated clone ratio.
func aggregatorMessage(cloneRatio float64) string {
	if cloneRatio <= 0 {
		return msgNoClones
	}

	if cloneRatio <= thresholdCloneRatioYellow {
		return msgLowClones
	}

	if cloneRatio <= thresholdCloneRatioRed {
		return msgModClones
	}

	return msgHighClones
}

// aggregatorEmptyResult returns the empty result for aggregation.
func aggregatorEmptyResult() analyze.Report {
	return buildEmptyReport(msgNoFunctions)
}
