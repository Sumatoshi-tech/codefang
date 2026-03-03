package common

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// DetailedDataCollector collects detailed per-item data from individual file
// reports and merges them into the aggregated result. Unlike DataCollector,
// it appends all items without deduplication.
type DetailedDataCollector struct {
	collections map[string][]map[string]any
	keys        []string
}

// NewDetailedDataCollector creates a collector for the given report keys.
func NewDetailedDataCollector(keys ...string) *DetailedDataCollector {
	collections := make(map[string][]map[string]any, len(keys))
	for _, k := range keys {
		collections[k] = make([]map[string]any, 0)
	}

	return &DetailedDataCollector{
		collections: collections,
		keys:        keys,
	}
}

// CollectFromReports extracts data for all keys from all non-nil reports.
func (d *DetailedDataCollector) CollectFromReports(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		d.extractFromReport(report)
	}
}

// AddToResult adds all non-empty collections to the result report.
func (d *DetailedDataCollector) AddToResult(result analyze.Report) {
	for _, key := range d.keys {
		if len(d.collections[key]) > 0 {
			result[key] = d.collections[key]
		}
	}
}

// extractFromReport extracts data for all configured keys from a single report.
func (d *DetailedDataCollector) extractFromReport(report analyze.Report) {
	for _, key := range d.keys {
		if items, ok := report[key].([]map[string]any); ok {
			d.collections[key] = append(d.collections[key], items...)
		}
	}
}
