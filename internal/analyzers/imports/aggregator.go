// Package imports provides imports functionality.
package imports

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Aggregator aggregates import analysis results across multiple files.
type Aggregator struct {
	allImports map[string]int // Import path -> count.
	totalFiles int
}

// NewAggregator creates a new Aggregator.
func NewAggregator() *Aggregator {
	return &Aggregator{
		allImports: make(map[string]int),
	}
}

// Aggregate combines results from multiple files.
func (a *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		a.totalFiles++

		if imports, ok := report["imports"].([]string); ok {
			for _, imp := range imports {
				a.allImports[imp]++
			}
		}
	}
}

// GetResult returns the aggregated result.
func (a *Aggregator) GetResult() analyze.Report {
	imports := make([]string, 0, len(a.allImports))
	for imp := range a.allImports {
		imports = append(imports, imp)
	}

	return analyze.Report{
		"imports":       imports,
		"import_counts": a.allImports,
		"count":         len(a.allImports),
		"total_files":   a.totalFiles,
	}
}
