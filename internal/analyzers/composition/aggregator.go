package composition

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
)

// Aggregator report keys.
const (
	keyBreakdown  = "breakdown"
	keyPercentage = "percentages"
	keyTotalFiles = "total_files"

	percentMultiplier = 100.0
)

// Aggregator aggregates file composition results across multiple files.
type Aggregator struct {
	common.PerFileRetainer

	counts     filehistory.CategoryCounts
	totalFiles int
}

// NewAggregator creates a new composition Aggregator.
func NewAggregator() *Aggregator {
	return &Aggregator{}
}

// Aggregate accumulates per-file classification results.
func (a *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		a.Retain(report)
		a.totalFiles++

		cat, ok := report[keyCategory].(string)
		if !ok {
			continue
		}

		a.counts.Increment(filehistory.Category(cat))
	}
}

// GetResult builds the aggregated composition report.
func (a *Aggregator) GetResult() analyze.Report {
	breakdown := make(map[string]int, len(filehistory.AllCategories))
	percentages := make(map[string]float64, len(filehistory.AllCategories))

	for _, cat := range filehistory.AllCategories {
		count := a.counts.Get(cat)
		breakdown[string(cat)] = count

		if a.totalFiles > 0 {
			percentages[string(cat)] = float64(count) / float64(a.totalFiles) * percentMultiplier
		}
	}

	return analyze.Report{
		keyBreakdown:  breakdown,
		keyPercentage: percentages,
		keyTotalFiles: a.totalFiles,
	}
}
