package common

// FRD: specs/frds/FRD-20260311-typed-report-items.md.

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// DetailedDataCollector collects detailed per-item data from individual file
// reports and merges them into the aggregated result. Unlike [SpillableDataCollector],
// it appends all items without deduplication.
//
// Supports both legacy []map[string]any collections and [analyze.TypedCollection]
// wrappers. TypedCollection items are stored as-is and converted to maps only
// in [DetailedDataCollector.AddToResult].
type DetailedDataCollector struct {
	collections      map[string][]map[string]any
	typedCollections map[string][]analyze.TypedCollection
	keys             []string
	mode             analyze.AggregationMode
}

// NewDetailedDataCollector creates a collector for the given report keys.
func NewDetailedDataCollector(keys ...string) *DetailedDataCollector {
	collections := make(map[string][]map[string]any, len(keys))
	typedCollections := make(map[string][]analyze.TypedCollection, len(keys))

	for _, k := range keys {
		collections[k] = make([]map[string]any, 0)
		typedCollections[k] = make([]analyze.TypedCollection, 0)
	}

	return &DetailedDataCollector{
		collections:      collections,
		typedCollections: typedCollections,
		keys:             keys,
	}
}

// SetAggregationMode sets the aggregation mode.
// In [analyze.AggregationModeSummaryOnly], CollectFromReports becomes a no-op.
func (d *DetailedDataCollector) SetAggregationMode(mode analyze.AggregationMode) {
	d.mode = mode
}

// CollectFromReports extracts data for all keys from all non-nil reports.
// In [analyze.AggregationModeSummaryOnly] mode, this is a no-op.
func (d *DetailedDataCollector) CollectFromReports(results map[string]analyze.Report) {
	if d.mode == analyze.AggregationModeSummaryOnly {
		return
	}

	for _, report := range results {
		if report == nil {
			continue
		}

		d.extractFromReport(report)
	}
}

// AddToResult adds all non-empty collections to the result report.
// TypedCollection items are converted to []map[string]any at this point.
func (d *DetailedDataCollector) AddToResult(result analyze.Report) {
	for _, key := range d.keys {
		items := d.buildItems(key)
		if len(items) > 0 {
			result[key] = items
		}
	}
}

// buildItems merges typed and legacy collections for a given key.
func (d *DetailedDataCollector) buildItems(key string) []map[string]any {
	typed := d.typedCollections[key]
	legacy := d.collections[key]

	if len(typed) == 0 {
		return legacy
	}

	// Estimate capacity: count typed items + legacy items.
	capacity := len(legacy)

	for _, tc := range typed {
		capacity += typedCollectionLen(tc)
	}

	items := make([]map[string]any, 0, capacity)

	for _, tc := range typed {
		items = append(items, tc.ToMaps(tc.Items, tc.SourceFile)...)
	}

	items = append(items, legacy...)

	return items
}

// typedCollectionLen returns the length of a TypedCollection's Items slice
// using a type switch for known slice types, falling back to 0.
func typedCollectionLen(tc analyze.TypedCollection) int {
	if s, ok := tc.Items.(interface{ Len() int }); ok {
		return s.Len()
	}

	return 0
}

// extractFromReport extracts data for all configured keys from a single report.
// Handles both [analyze.TypedCollection] and legacy []map[string]any values.
func (d *DetailedDataCollector) extractFromReport(report analyze.Report) {
	for _, key := range d.keys {
		val := report[key]
		if val == nil {
			continue
		}

		switch v := val.(type) {
		case analyze.TypedCollection:
			d.typedCollections[key] = append(d.typedCollections[key], v)
		case []map[string]any:
			d.collections[key] = append(d.collections[key], v...)
		}
	}
}
