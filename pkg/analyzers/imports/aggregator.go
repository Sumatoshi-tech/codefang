// Package imports provides imports functionality.
package imports

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// ImportsAggregator aggregates import analysis results across multiple files.
type ImportsAggregator struct {
	allImports map[string]int // Import path -> count.
	totalFiles int
}

// NewImportsAggregator creates a new ImportsAggregator.
func NewImportsAggregator() *ImportsAggregator {
	return &ImportsAggregator{
		allImports: make(map[string]int),
	}
}

// Aggregate combines results from multiple files.
func (a *ImportsAggregator) Aggregate(results map[string]analyze.Report) {
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
func (a *ImportsAggregator) GetResult() analyze.Report {
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
