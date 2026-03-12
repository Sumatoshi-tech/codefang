package analyze

// AggregationMode controls whether per-item data is collected during aggregation.
type AggregationMode int

const (
	// AggregationModeFull collects all per-item data (default, zero value).
	AggregationModeFull AggregationMode = iota

	// AggregationModeSummaryOnly skips per-item data collection.
	// MetricsProcessor continues normally; SpillableDataCollector and DetailedDataCollector become no-ops.
	AggregationModeSummaryOnly
)

// AggregationModeAware is implemented by aggregators that support mode switching.
type AggregationModeAware interface {
	SetAggregationMode(mode AggregationMode)
}
